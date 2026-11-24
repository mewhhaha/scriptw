use super::*;
use scriptw_ir::JS_STRING_SURROGATE_SENTINEL;

#[test]
fn descriptor_rows_keep_only_program_local_slots_and_utf16_spans() {
    let mut pool = StringPool::default();
    pool.intern_string("runtime\0\u{10000}");
    pool.intern_string("name");
    pool.compiler_owned = pool.boundary();
    let runtime = pool.pooled_code_unit_bytes.clone();
    let lone = format!("{JS_STRING_SURROGATE_SENTINEL}D800X{JS_STRING_SURROGATE_SENTINEL}DC00");
    let sources = [
        "".to_owned(),
        "\0program".to_owned(),
        "Ā\u{10000}".to_owned(),
        "zz-long-text".to_owned(),
        lone,
    ];
    for source in &sources {
        pool.intern_string(source);
    }
    pool.intern_string("name");
    let image = pool.program_string_descriptor_image();
    assert_eq!(image.count(), 5);
    assert_eq!(
        &pool.pooled_code_unit_bytes[..runtime.len()],
        runtime.as_slice()
    );
    let program = &pool.pooled_code_unit_bytes[runtime.len()..];
    let mut decoded = BTreeMap::new();
    for (ordinal, row) in image
        .bytes
        .chunks_exact(ProgramStringDescriptorWord::ROW_BYTES as usize)
        .enumerate()
    {
        let read = |word: ProgramStringDescriptorWord| {
            let start = word.byte_offset() as usize;
            u32::from_le_bytes(row[start..start + 4].try_into().unwrap())
        };
        let ordinal = ordinal as u32;
        let start = read(ProgramStringDescriptorWord::Utf16ByteOffset) as usize;
        let end = start + read(ProgramStringDescriptorWord::UnitLength) as usize * 2;
        assert!(decoded.insert(ordinal, &program[start..end]).is_none());
    }
    for source in &sources {
        let expected = StringPool::runtime_code_units_for_string(source)
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(
            decoded[&ProgramLiteralIndex::for_pool(
                &pool,
                pool.pooled_string_index(source).unwrap()
            )
            .unwrap()
            .ordinal()],
            expected
        );
    }
    assert_eq!(
        decoded.keys().copied().collect::<Vec<_>>(),
        (0..5).collect::<Vec<_>>()
    );
    assert!(
        ProgramLiteralIndex::for_pool(&pool, pool.pooled_string_index("name").unwrap()).is_err()
    );
}

#[test]
fn runtime_dedup_only_program_has_a_complete_empty_descriptor_image() {
    let mut pool = StringPool::default();
    pool.intern_string("runtime only");
    pool.compiler_owned = pool.boundary();
    let prefix = pool.pooled_code_unit_bytes.clone();
    pool.intern_string("runtime only");
    let image = pool.program_string_descriptor_image();
    assert_eq!(image.count(), 0);
    assert!(image.bytes.is_empty());
    assert_eq!(pool.pooled_code_unit_bytes, prefix);
}

#[test]
fn independent_program_images_reuse_local_slots_without_sharing_bytes() {
    fn program(source: &str) -> StringPool {
        let mut pool = StringPool::default();
        pool.intern_string("same runtime prefix");
        pool.compiler_owned = pool.boundary();
        pool.intern_string(source);
        pool
    }
    fn local_literal(pool: &StringPool, image: &ProgramStringDescriptorImage) -> Vec<u8> {
        assert_eq!(image.count(), 1);
        let word = |word: ProgramStringDescriptorWord| {
            let start = word.byte_offset() as usize;
            u32::from_le_bytes(image.bytes[start..start + 4].try_into().unwrap())
        };
        assert_eq!(
            image.bytes.len(),
            ProgramStringDescriptorWord::ROW_BYTES as usize
        );
        assert_eq!(word(ProgramStringDescriptorWord::Utf16ByteOffset), 0);
        let start = pool.compiler_owned_boundary().code_unit_bytes;
        let end = start + word(ProgramStringDescriptorWord::UnitLength) as usize * 2;
        pool.pooled_code_unit_bytes[start..end].to_vec()
    }
    let lone = format!("first\0{JS_STRING_SURROGATE_SENTINEL}D800");
    let first = program(&lone);
    let first_image = first.program_string_descriptor_image();
    let first_bytes = local_literal(&first, &first_image);
    let second = program("second\0\u{10000}");
    let second_image = second.program_string_descriptor_image();
    let second_bytes = local_literal(&second, &second_image);
    assert_ne!(first_bytes, second_bytes);
    assert_eq!(local_literal(&first, &first_image), first_bytes);
    assert_eq!(
        first_bytes,
        StringPool::runtime_code_units_for_string(&lone)
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        second_bytes,
        StringPool::runtime_code_units_for_string("second\0\u{10000}")
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    );
}
