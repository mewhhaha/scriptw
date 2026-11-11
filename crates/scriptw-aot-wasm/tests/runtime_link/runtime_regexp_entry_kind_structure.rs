const DATA_SOURCE: &str = include_str!("../../src/data.rs");
const OWNER_SOURCE: &str = include_str!("../../src/data/runtime_regexp_entry_kind.rs");
const EXPRESSIONS_SOURCE: &str = include_str!("../../src/expressions.rs");
const REGEXP_SOURCE: &str = include_str!("../../src/expressions/regexp_program.rs");

struct NormalizedRust {
    identifiers: String,
    routes: String,
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
    if value_start >= bytes.len() {
        return None;
    }
    let value_end = if bytes[value_start] == b'\\' {
        let mut offset = value_start + 1;
        if offset >= bytes.len() {
            return None;
        }
        if bytes[offset] == b'u' && bytes.get(offset + 1) == Some(&b'{') {
            offset += 2;
            while bytes.get(offset).is_some_and(|byte| *byte != b'}') {
                offset += 1;
            }
            if bytes.get(offset) != Some(&b'}') {
                return None;
            }
            offset + 1
        } else if bytes[offset] == b'x'
            && bytes
                .get(offset + 1..offset + 3)
                .is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit))
        {
            offset + 3
        } else {
            offset + 1
        }
    } else {
        value_start + source[value_start..].chars().next()?.len_utf8()
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

fn normalize_rust(source: &str) -> NormalizedRust {
    let bytes = source.as_bytes();
    let mut identifiers = String::new();
    let mut routes = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = literal_end(source, offset) {
            identifiers.push(' ');
            routes.push('L');
            offset = end;
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"//") {
            identifiers.push(' ');
            offset += 2;
            while bytes.get(offset).is_some_and(|byte| *byte != b'\n') {
                offset += 1;
            }
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"/*") {
            identifiers.push(' ');
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
        if character.is_whitespace() {
            identifiers.push(' ');
        } else {
            identifiers.push(character);
            routes.push(character);
        }
        offset += character.len_utf8();
    }
    NormalizedRust {
        identifiers,
        routes,
    }
}

#[test]
fn runtime_regexp_entry_kind_has_one_private_capability_free_owner() {
    assert_eq!(
        DATA_SOURCE
            .matches("\nmod runtime_regexp_entry_kind;\n")
            .count(),
        1
    );
    assert_eq!(
        DATA_SOURCE
            .matches("pub(crate) use runtime_regexp_entry_kind::RuntimeRegExpEntryKind;")
            .count(),
        1
    );
    assert!(!DATA_SOURCE.contains("\npub mod runtime_regexp_entry_kind;\n"));
    assert_eq!(
        normalize_rust(OWNER_SOURCE).routes,
        concat!(
            "usesuper::*;pub(crate)enumRuntimeRegExpEntryKind{Program,Rejected,Unsupported,}",
            "implRuntimeRegExpEntryKind{",
            "pub(crate)constALL:[Self;3]=[Self::Program,Self::Rejected,Self::Unsupported];",
            "pub(crate)constfnword(&self)->u64{matchself{",
            "Self::Program=>RUNTIME_REGEXP_ENTRY_KIND_PROGRAM,",
            "Self::Rejected=>RUNTIME_REGEXP_ENTRY_KIND_REJECTED,",
            "Self::Unsupported=>RUNTIME_REGEXP_ENTRY_KIND_UNSUPPORTED,}}",
            "pub(crate)constfnthrows_syntax_error(&self)->bool{matchself{",
            "Self::Program|Self::Unsupported=>false,Self::Rejected=>true,}}}"
        )
    );
}

#[test]
fn runtime_regexp_entry_kind_preserves_exact_writer_and_wire_policies() {
    let normalized_data = normalize_rust(DATA_SOURCE).routes;
    for (constant, word) in [
        ("RUNTIME_REGEXP_ENTRY_KIND_PROGRAM", 0),
        ("RUNTIME_REGEXP_ENTRY_KIND_REJECTED", 1),
        ("RUNTIME_REGEXP_ENTRY_KIND_UNSUPPORTED", 2),
    ] {
        assert_eq!(
            normalized_data
                .matches(&format!("pub(crate)const{constant}:u64={word};"))
                .count(),
            1,
            "{constant} declaration"
        );
    }

    let writer = bounded(
        DATA_SOURCE,
        "    fn append_runtime_regexp_program_table(&mut self) {",
        "    fn append_regexp_programs(&mut self) {",
    );
    let normalized_writer = normalize_rust(writer).routes;
    for forbidden in ["==", "!=", "default()", "unwrap_or"] {
        assert!(!normalized_writer.contains(forbidden), "{forbidden}");
    }
    // Selection may discard non-program rows; the actual wire writer below
    // remains exhaustive over every entry kind.
    assert_eq!(normalized_writer.matches("_=>").count(), 1);
    assert!(normalized_writer.contains(concat!(
        "filter_map(|(_,_,entry)|matchentry{",
        "RuntimeRegExpEntry::Program(reference)=>Some(*reference),_=>None,})"
    )));
    assert!(normalized_writer.contains("ProgramMetadataSpan::for_pool(self,copied_address,length)"));
    assert!(normalized_writer.contains("ProgramMetadataSpan::for_pool(self,address,length)"));
    assert!(normalized_writer.contains("descriptors.insert(reference.payload(),span);"));
    let typed_assignment =
        "            record[RUNTIME_REGEXP_RECORD_ENTRY_KIND_WORD] = match entry {";
    assert_eq!(writer.matches(typed_assignment).count(), 1);
    let writer_tail = writer
        .split_once(typed_assignment)
        .expect("typed entry-kind assignment")
        .1;
    assert!(!normalize_rust(writer_tail).routes.contains("_=>"));
    let expected_writer_tail = r#"
                RuntimeRegExpEntry::Program(program) => {
                    record[RUNTIME_REGEXP_RECORD_PROGRAM_PAYLOAD_WORD] =
                        descriptors[&program.payload()].packed();
                    record[RUNTIME_REGEXP_RECORD_PROGRAM_DOMAIN_WORD] =
                        RUNTIME_REGEXP_DESCRIPTOR_PROGRAM_DOMAIN;
                    RuntimeRegExpEntryKind::Program.word()
                }
                RuntimeRegExpEntry::Rejected => RuntimeRegExpEntryKind::Rejected.word(),
                RuntimeRegExpEntry::Unsupported => RuntimeRegExpEntryKind::Unsupported.word(),
            };
            for value in record {
                self.bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
    }

"#;
    assert_eq!(
        normalize_rust(writer_tail).routes,
        normalize_rust(expected_writer_tail).routes,
        "the typed entry-kind assignment must flow directly into exact record serialization"
    );
}

#[test]
fn literal_allocation_and_static_slot_publication_require_a_compiled_program() {
    let slots = normalize_rust(bounded(
        REGEXP_SOURCE,
        "    pub(crate) fn emit_regexp_program_slots(",
        "    pub(super) fn compile_regexp_literal_to_value(",
    ))
    .routes;
    assert!(slots.contains("program:&RegExpProgram,"));
    assert!(!slots.contains("Option<"));
    assert!(!slots.contains("unwrap_or("));

    let literal = normalize_rust(bounded(
        REGEXP_SOURCE,
        "    pub(super) fn compile_regexp_literal_to_value(",
        "\n}",
    ))
    .routes;
    assert!(literal.contains("program:&RegExpProgram,"));
    assert!(!literal.contains("Option<"));

    let admission = normalize_rust(bounded(
        EXPRESSIONS_SOURCE,
        "            ExprIr::RegExpLiteral {",
        "            ExprIr::FunctionValue(",
    ))
    .routes;
    for route in [
        "Some(StaticRegExpCompilation::Program(program))",
        "Some(compilation@StaticRegExpCompilation::InvalidSyntax{..})",
        "None=>{",
        "RuntimeSemanticGap::RegExpRuntimePatternCompilation",
    ] {
        assert!(admission.contains(route), "literal route: {route}");
    }
    assert!(!admission.contains("_=>"));
}
