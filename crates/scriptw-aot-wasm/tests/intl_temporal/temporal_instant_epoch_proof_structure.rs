use std::fs;
use std::path::Path;

const EPOCH_SOURCE: &str = include_str!("../../src/builtins/temporal/epoch.rs");
const INSTANT_SOURCE: &str = include_str!("../../src/builtins/temporal_instant.rs");
fn quoted_literal_end(source: &str, quote_start: usize, quote: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut offset = quote_start + 1;
    let mut escaped = false;
    while offset < bytes.len() {
        let byte = bytes[offset];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == quote {
            return Some(offset + 1);
        }
        offset += 1;
    }
    None
}

fn character_literal_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let value_start = start + 1;
    let value_end = if bytes.get(value_start) == Some(&b'\\') {
        let mut offset = value_start + 2;
        if bytes.get(value_start + 1) == Some(&b'u') && bytes.get(offset) == Some(&b'{') {
            offset += 1;
            while bytes.get(offset).is_some_and(|byte| *byte != b'}') {
                offset += 1;
            }
            offset + 1
        } else if bytes.get(value_start + 1) == Some(&b'x') {
            offset + 2
        } else {
            offset
        }
    } else {
        value_start + source.get(value_start..)?.chars().next()?.len_utf8()
    };
    (bytes.get(value_end) == Some(&b'\'')).then_some(value_end + 1)
}

fn raw_literal_end(source: &str, start: usize, prefix_len: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut quote_start = start + prefix_len;
    while bytes.get(quote_start) == Some(&b'#') {
        quote_start += 1;
    }
    if bytes.get(quote_start) != Some(&b'"') {
        return None;
    }
    let hashes = quote_start - start - prefix_len;
    let mut offset = quote_start + 1;
    while offset < bytes.len() {
        if bytes[offset] == b'"'
            && bytes
                .get(offset + 1..offset + 1 + hashes)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
        {
            return Some(offset + 1 + hashes);
        }
        offset += 1;
    }
    None
}

fn literal_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    match bytes.get(start).copied()? {
        b'"' => quoted_literal_end(source, start, b'"'),
        b'\'' => character_literal_end(source, start),
        b'b' if bytes.get(start + 1) == Some(&b'\'') => character_literal_end(source, start + 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'"') => {
            quoted_literal_end(source, start + 1, b'"')
        }
        b'r' => raw_literal_end(source, start, 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'r') => raw_literal_end(source, start, 2),
        _ => None,
    }
}

fn rust_code(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut code = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = literal_end(source, offset) {
            code.push(' ');
            offset = end;
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"//") {
            code.push(' ');
            offset += 2;
            while bytes.get(offset).is_some_and(|byte| *byte != b'\n') {
                offset += 1;
            }
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"/*") {
            code.push(' ');
            offset += 2;
            let mut depth = 1;
            while offset < bytes.len() && depth != 0 {
                if bytes.get(offset..offset + 2) == Some(b"/*") {
                    depth += 1;
                    offset += 2;
                } else if bytes.get(offset..offset + 2) == Some(b"*/") {
                    depth -= 1;
                    offset += 2;
                } else {
                    offset += 1;
                }
            }
            assert_eq!(depth, 0, "unterminated block comment in Rust source");
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"r#")
            && source[offset + 2..]
                .chars()
                .next()
                .is_some_and(|character| character == '_' || character.is_alphabetic())
        {
            offset += 2;
            continue;
        }
        let character = source[offset..].chars().next().unwrap();
        code.push(character);
        offset += character.len_utf8();
    }
    code
}

fn compact_rust(source: &str) -> String {
    rust_code(source)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn exact_identifier_count(source: &str, identifier: &str) -> usize {
    source
        .match_indices(identifier)
        .filter(|(offset, _)| {
            let before = source[..*offset].chars().next_back();
            let after = source[*offset + identifier.len()..].chars().next();
            [before, after].into_iter().all(|edge| {
                edge.map(|character| !character.is_alphanumeric() && character != '_')
                    .unwrap_or(true)
            })
        })
        .count()
}

fn rust_sources(dir: &Path) -> Vec<String> {
    let mut paths = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .flat_map(|path| {
            if path.is_dir() {
                return rust_sources(&path);
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return Vec::new();
            }
            vec![fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))]
        })
        .collect()
}

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

#[test]
fn validated_epoch_is_one_private_non_copy_proof() {
    let lexical_probe = rust_code(
        r###"
        // EpochNanoseconds
        EpochNanoseconds /* EpochNanoseconds */;
        "EpochNanoseconds"; b"EpochNanoseconds"; c"EpochNanoseconds";
        r"EpochNanoseconds"; br##"EpochNanoseconds"##;
        cr#"EpochNanoseconds"#; 'E'; b'E'; 'lifetime;
        "###,
    );
    assert_eq!(
        exact_identifier_count(&lexical_probe, "EpochNanoseconds"),
        1
    );

    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mentions = rust_sources(&source_root)
        .iter()
        .map(|source| exact_identifier_count(&rust_code(source), "TemporalEpochNanoseconds"))
        .sum::<usize>();
    assert_eq!(mentions, 18, "review every new completed GC epoch observer");
    let declaration = compact_rust(bounded(
        EPOCH_SOURCE,
        "pub(in crate::builtins) struct TemporalEpochNanoseconds {",
        "impl TemporalEpochNanoseconds {",
    ));
    assert_eq!(
        declaration,
        "value:GcLocal<BigIntValue>,seconds:I64Local,nanosecond:I64Local,}"
    );
    assert!(!EPOCH_SOURCE.contains("#[derive"));
    for capability in ["Clone", "Copy", "Default"] {
        assert!(!EPOCH_SOURCE.contains(&format!("impl {capability} for TemporalEpochNanoseconds")));
    }
}

#[test]
fn range_validation_and_completed_views_are_the_only_proof_constructors() {
    let constructor = compact_rust(bounded(
        EPOCH_SOURCE,
        "fn emit_temporal_instant_validated_epoch(",
        "fn emit_temporal_instant_range_error(",
    ));
    assert!(constructor.contains("input:&GcLocal<BigIntValue>"));
    assert_eq!(
        constructor
            .matches("self.emit_temporal_instant_range_error(function)?;")
            .count(),
        2
    );
    for bound in [
        "TEMPORAL_INSTANT_LIMIT_HIGH_LIMB",
        "TEMPORAL_INSTANT_LIMIT_LOW_LIMB",
    ] {
        assert!(constructor.contains(bound));
    }
    assert!(
        constructor
            .rfind("self.emit_temporal_instant_range_error(function)?;")
            .unwrap()
            < constructor
                .find("Ok(TemporalEpochNanoseconds{value,seconds,nanosecond,})")
                .unwrap()
    );
    let relative = bounded(
        EPOCH_SOURCE,
        "fn emit_temporal_epoch_from_relative_view(",
        "fn emit_temporal_epoch_from_string_rounding(",
    );
    assert!(
        relative.contains("input: &crate::builtins::temporal_zone_provider::RelativeEpochView<'_>")
    );
    let rounding = bounded(
        EPOCH_SOURCE,
        "fn emit_temporal_epoch_from_string_rounding(",
        "fn emit_temporal_instant_validated_epoch(",
    );
    assert!(rounding.contains(
        "input: &super::super::temporal_zoned_arithmetic::CompletedTemporalStringRoundingLocals"
    ));
    for completed in [relative, rounding] {
        assert!(completed.contains("input.floor_seconds().load(function)"));
        assert!(completed.contains("input.nanosecond().load(function)"));
        assert_eq!(
            completed.matches("TemporalEpochNanoseconds {").count(),
            2,
            "typed return plus one owned construction"
        );
    }
    assert_eq!(
        EPOCH_SOURCE
            .matches("        TemporalEpochNanoseconds {")
            .count(),
        2
    );
    assert_eq!(
        EPOCH_SOURCE
            .matches("Ok(TemporalEpochNanoseconds {")
            .count(),
        1
    );
}

#[test]
fn allocation_borrows_only_a_completed_epoch_and_release_consumes_it() {
    let consumer = compact_rust(bounded(
        EPOCH_SOURCE,
        "fn emit_alloc_temporal_instant(",
        "\n}",
    ));
    assert!(consumer.contains("epoch:&TemporalEpochNanoseconds,"));
    assert!(consumer.contains("schema.struct_type::<TemporalInstantObject>().construct((GcOperand::reference(&header,schema),GcOperand::reference(epoch.value(),schema),),function,)"));
    assert!(!consumer.contains("input:&GcLocal<BigIntValue>"));
    let proof = compact_rust(bounded(
        EPOCH_SOURCE,
        "impl TemporalEpochNanoseconds {",
        "impl FunctionBuilder<'_> {",
    ));
    assert!(proof.contains("fnvalue(&self)->&GcLocal<BigIntValue>"));
    assert!(proof.contains("fnclear(self,builder:&mutFunctionBuilder<'_>,function:&mutFunction,)"));
    assert!(proof.contains("release_i64_local(self.nanosecond,function)"));
    assert!(proof.contains("release_i64_local(self.seconds,function)"));
    assert!(proof.contains("self.value.clear(function)"));
}

#[test]
fn both_epoch_builtins_follow_validate_then_allocate() {
    let from_value = compact_rust(bounded(
        INSTANT_SOURCE,
        "fn emit_temporal_epoch_from_value(",
        "pub(crate) fn emit_temporal_instant_from_epoch_nanoseconds(",
    ));
    assert!(
        from_value
            .find("self.emit_value_to_bigint_locals(")
            .unwrap()
            < from_value
                .find("self.emit_temporal_instant_validated_epoch(&value,f)?")
                .unwrap()
    );
    for (start, end, admission) in [
        (
            "pub(crate) fn emit_temporal_instant_from_epoch_nanoseconds(",
            "pub(in crate::builtins) fn emit_temporal_epoch_milliseconds_to_epoch_nanoseconds(",
            "self.emit_temporal_epoch_from_value(&input,f)?",
        ),
        (
            "pub(crate) fn emit_temporal_instant_from_epoch_milliseconds(",
            "pub(crate) fn emit_temporal_instant_compare(",
            "self.emit_temporal_instant_validated_epoch(&value,f)?",
        ),
    ] {
        let builtin = compact_rust(bounded(INSTANT_SOURCE, start, end));
        let validate = builtin.find(admission).unwrap();
        let allocate = builtin
            .find("self.emit_alloc_temporal_instant(&epoch,TemporalPrototypeSource::Intrinsic,f)?;")
            .unwrap();
        let release = builtin.find("epoch.clear(self,f);").unwrap();
        assert!(validate < allocate && allocate < release);
    }
}
