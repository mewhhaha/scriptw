use super::*;

#[test]
fn candidate_rows_preserve_gc_string_ordinals_and_all_closed_outcomes() {
    let mut pool = StringPool::default();
    let source = "Ā\0𝄞";
    let flags = "u";
    pool.intern_string(source);
    pool.intern_string(flags);
    let program = RegExpProgram::compile(source, flags).unwrap();
    pool.queue_regexp_program(&program);
    pool.append_regexp_programs();
    let reference = pool.regexp_programs[&RegExpProgramStaticKey::from_program(&program)];
    pool.runtime_regexp_programs = vec![
        (
            source.into(),
            flags.into(),
            RuntimeRegExpEntry::Program(reference),
        ),
        (source.into(), flags.into(), RuntimeRegExpEntry::Rejected),
        (source.into(), flags.into(), RuntimeRegExpEntry::Unsupported),
    ];
    pool.append_runtime_regexp_program_table();
    let table = pool.runtime_regexp_table().expect("nonempty checked table");
    assert_eq!(table.count(), 3, "no outcome may disappear from the table");
    assert_eq!(table.address % 8, 0);
    let offset = (table.address - STATIC_DATA_OFFSET) as usize;
    let rows = pool.bytes[offset..]
        .chunks_exact(RUNTIME_REGEXP_RECORD_SIZE as usize)
        .map(|row| {
            row.chunks_exact(8)
                .map(|word| u64::from_le_bytes(word.try_into().unwrap()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 3);
    for (row, kind) in rows.iter().zip(RuntimeRegExpEntryKind::ALL) {
        assert_eq!(
            row[RUNTIME_REGEXP_RECORD_SOURCE_WORD],
            PooledLiteralOperand::for_pool(&pool, pool.pooled_string_index(source).unwrap())
                .unwrap()
                .word()
        );
        assert_eq!(
            row[RUNTIME_REGEXP_RECORD_FLAGS_WORD],
            PooledLiteralOperand::for_pool(&pool, pool.pooled_string_index(flags).unwrap())
                .unwrap()
                .word()
        );
        assert_eq!(row[RUNTIME_REGEXP_RECORD_ENTRY_KIND_WORD], kind.word());
    }
    assert_eq!(
        rows[0][RUNTIME_REGEXP_RECORD_PROGRAM_PAYLOAD_WORD],
        ProgramMetadataSpan::for_pool(
            &pool,
            (reference.payload() >> 32) as u32,
            reference.payload() as u32
        )
        .unwrap()
        .packed()
    );
    assert_eq!(rows[1][RUNTIME_REGEXP_RECORD_PROGRAM_PAYLOAD_WORD], 0);
    assert_eq!(rows[2][RUNTIME_REGEXP_RECORD_PROGRAM_PAYLOAD_WORD], 0);
    let descriptor = rows[0][RUNTIME_REGEXP_RECORD_PROGRAM_PAYLOAD_WORD];
    assert_eq!(
        rows[0][RUNTIME_REGEXP_RECORD_PROGRAM_DOMAIN_WORD],
        RUNTIME_REGEXP_DESCRIPTOR_PROGRAM_DOMAIN
    );
    assert_eq!(rows[1][RUNTIME_REGEXP_RECORD_PROGRAM_DOMAIN_WORD], 0);
    assert_eq!(rows[2][RUNTIME_REGEXP_RECORD_PROGRAM_DOMAIN_WORD], 0);
    let start = pool.compiler_owned_boundary().static_len() + (descriptor >> 32) as usize;
    let length = descriptor as u32 as usize;
    assert_eq!(
        &pool.bytes[start..start + length],
        ValidatedRegExpProgram::from_program(&program)
            .unwrap()
            .bytes()
    );
}

#[test]
fn candidate_cache_compiles_semantic_units_instead_of_string_pool_marker_text() {
    let mut pool = StringPool::default();
    let patterns = [
        vec![0xd83d, 92, 117, 68, 67, 51, 56],
        vec![92, 117, 68, 56, 51, 68, 0xdc38],
        vec![91, 0xd83d, 92, 117, 68, 67, 51, 56, 93],
        "\u{F0000}D83D".encode_utf16().collect(),
    ];
    for units in &patterns {
        pool.runtime_regexp_candidate_literals
            .insert(scriptw_ir::encode_js_string_utf16(units));
    }
    pool.script_string_literals.insert("u".into());
    pool.queue_runtime_regexp_programs();
    for units in &patterns {
        let source = scriptw_ir::encode_js_string_utf16(units);
        for flags in ["", "u"] {
            let (_, _, entry) = pool
                .runtime_regexp_programs
                .iter()
                .find(|(candidate, candidate_flags, _)| {
                    candidate == &source && candidate_flags == flags
                })
                .expect("every source/flags pair must have a physical candidate row");
            let RuntimeRegExpEntry::Program(reference) = entry else {
                panic!("valid raw unit pattern was not compiled: {units:?}/{flags}: {entry:?}");
            };
            let expected = ValidatedRegExpProgram::from_program(
                &RegExpProgram::compile_utf16(units, flags).unwrap(),
            )
            .unwrap();
            let descriptor = reference.payload();
            let start = (descriptor >> 32) as usize - STATIC_DATA_OFFSET as usize;
            let length = descriptor as u32 as usize;
            assert_eq!(
                &pool.bytes[start..start + length],
                expected.bytes(),
                "{units:?}/{flags}"
            );
        }
    }
}

#[test]
fn candidate_runtime_descriptor_dedup_copies_into_program_relative_metadata() {
    let mut pool = StringPool::default();
    let source = "(?<name>Ā)";
    let program = RegExpProgram::compile(source, "u").unwrap();
    pool.intern_string(source);
    pool.intern_string("u");
    pool.queue_regexp_program(&program);
    pool.append_regexp_programs();
    let reference = pool.regexp_program(&program);
    pool.compiler_owned = pool.boundary();
    let runtime_bytes = pool.bytes.clone();
    let prefix = pool.compiler_owned.static_len();
    pool.intern_string("a different P string");
    pool.runtime_regexp_programs = vec![(
        source.into(),
        "u".into(),
        RuntimeRegExpEntry::Program(reference),
    )];
    pool.append_runtime_regexp_program_table();
    let table = pool.runtime_regexp_table().unwrap();
    let table_span = table.metadata_span(&pool).unwrap();
    let offset = prefix + table_span.start() as usize;
    let word = |index| {
        u64::from_le_bytes(
            pool.bytes[offset + index * 8..offset + index * 8 + 8]
                .try_into()
                .unwrap(),
        )
    };
    let descriptor = word(RUNTIME_REGEXP_RECORD_PROGRAM_PAYLOAD_WORD);
    let start = prefix + (descriptor >> 32) as usize;
    let length = descriptor as u32 as usize;
    assert_eq!(
        &pool.bytes[..prefix],
        runtime_bytes.as_slice(),
        "R bytes remain immutable"
    );
    assert_eq!(
        word(RUNTIME_REGEXP_RECORD_PROGRAM_DOMAIN_WORD),
        RUNTIME_REGEXP_DESCRIPTOR_PROGRAM_DOMAIN
    );
    assert!(start >= prefix && start + length <= offset);
    assert_eq!(
        &pool.bytes[start..start + length],
        ValidatedRegExpProgram::from_program(&program)
            .unwrap()
            .bytes()
    );
    assert_eq!(
        word(RUNTIME_REGEXP_RECORD_SOURCE_WORD),
        PooledLiteralOperand::for_pool(&pool, pool.pooled_string_index(source).unwrap())
            .unwrap()
            .word()
    );
    assert_eq!(
        word(RUNTIME_REGEXP_RECORD_FLAGS_WORD),
        PooledLiteralOperand::for_pool(&pool, pool.pooled_string_index("u").unwrap())
            .unwrap()
            .word()
    );
}

/// The emitted lookup binary-searches the key column, so the writer must store
/// every row's key of its actual runtime code units, in unsigned order.
#[test]
fn candidate_rows_are_sorted_by_the_code_unit_key_the_lookup_recomputes() {
    let mut pool = StringPool::default();
    for literal in ["b", "a", "ab", "Ā\u{1F600}", "[a-z]+", "(", ""] {
        pool.runtime_regexp_candidate_literals
            .insert(literal.to_string());
    }
    for flags in ["g", "iu", "dgimsuy"] {
        pool.script_string_literals.insert(flags.into());
    }
    pool.queue_runtime_regexp_programs();
    let table = pool.runtime_regexp_table().expect("nonempty checked table");
    assert_eq!(table.count() as usize, pool.runtime_regexp_programs.len());
    let offset = (table.address - STATIC_DATA_OFFSET) as usize;
    let keys = pool.bytes
        [offset..offset + table.count() as usize * RUNTIME_REGEXP_RECORD_SIZE as usize]
        .chunks_exact(RUNTIME_REGEXP_RECORD_SIZE as usize)
        .map(|row| {
            let at = RUNTIME_REGEXP_RECORD_KEY_WORD * 8;
            u64::from_le_bytes(row[at..at + 8].try_into().unwrap())
        })
        .collect::<Vec<_>>();
    assert!(
        keys.windows(2).all(|pair| pair[0] <= pair[1]),
        "rows must be in unsigned key order: {keys:x?}"
    );
    for ((source, flags, _), key) in pool.runtime_regexp_programs.iter().zip(&keys) {
        assert_eq!(
            *key,
            runtime_regexp_candidate_key(
                &scriptw_ir::decode_js_string_utf16(source),
                &scriptw_ir::decode_js_string_utf16(flags),
            ),
            "{source:?}/{flags:?}"
        );
    }
    // Rejected and program rows both survive the sort.
    assert!(pool
        .runtime_regexp_programs
        .iter()
        .any(|(source, _, entry)| source == "(" && matches!(entry, RuntimeRegExpEntry::Rejected)));
    assert!(pool
        .runtime_regexp_programs
        .iter()
        .any(|(source, flags, entry)| source == "[a-z]+"
            && flags == "g"
            && matches!(entry, RuntimeRegExpEntry::Program(_))));
}

#[test]
fn candidate_key_separates_source_from_flags_and_mixes_whole_code_units() {
    let units = |text: &str| text.encode_utf16().collect::<Vec<_>>();
    assert_ne!(
        runtime_regexp_candidate_key(&units("ag"), &units("")),
        runtime_regexp_candidate_key(&units("a"), &units("g"))
    );
    assert_ne!(
        runtime_regexp_candidate_key(&[0x0100], &[]),
        runtime_regexp_candidate_key(&[0x0001, 0x0000], &[])
    );
    // FNV-1a of the empty pair is the separator step alone.
    assert_eq!(
        runtime_regexp_candidate_key(&[], &[]),
        (RUNTIME_REGEXP_CANDIDATE_KEY_OFFSET_BASIS ^ RUNTIME_REGEXP_CANDIDATE_KEY_SEPARATOR)
            .wrapping_mul(RUNTIME_REGEXP_CANDIDATE_KEY_PRIME)
    );
}
