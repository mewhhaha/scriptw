use std::fs;
use std::path::Path;

const ERROR_SOURCE: &str = include_str!("../../src/builtins/errors.rs");
const STANDARD_SOURCE: &str = include_str!("../../src/builtins/standard.rs");
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

fn rust_code(source: &str, retain_literals: bool) -> String {
    let bytes = source.as_bytes();
    let mut code = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = literal_end(source, offset) {
            if retain_literals {
                code.push_str(&source[offset..end]);
            } else {
                code.push(' ');
            }
            offset = end;
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"//") {
            offset += 2;
            while bytes.get(offset).is_some_and(|byte| *byte != b'\n') {
                offset += 1;
            }
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"/*") {
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
        if !character.is_whitespace() {
            code.push(character);
        } else if !retain_literals {
            code.push(' ');
        }
        offset += character.len_utf8();
    }
    code
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

fn count_identifier_in_rust_sources(dir: &Path, identifier: &str) -> usize {
    fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .map(|path| {
            if path.is_dir() {
                return count_identifier_in_rust_sources(&path, identifier);
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return 0;
            }
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            exact_identifier_count(&rust_code(&source, false), identifier)
        })
        .sum()
}

#[test]
fn error_builtin_is_the_exact_non_capability_dispatch_authority() {
    let lexical_probe = rust_code(
        r###"
        // ErrorBuiltin::IsError
        ErrorBuiltin /* nested /* ignored */ comment */ :: r#Constructor;
        "ErrorBuiltin"; b"ErrorBuiltin"; c"ErrorBuiltin";
        r"ErrorBuiltin"; br##"ErrorBuiltin"##; cr#"ErrorBuiltin"#;
        'E'; b'E'; 'lifetime;
        "###,
        false,
    );
    assert_eq!(exact_identifier_count(&lexical_probe, "ErrorBuiltin"), 1);

    let declaration = rust_code(
        bounded(ERROR_SOURCE, "mod stack;", "enum ErrorCauseOptionsArgument"),
        true,
    );
    assert_eq!(
        declaration,
        "enumErrorBuiltin{IsError,Constructor(NativeErrorKind),PrototypeToString,}"
    );

    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        count_identifier_in_rust_sources(&source_root, "ErrorBuiltin"),
        16
    );
    for capability in ["Clone", "Copy", "Debug", "Default", "PartialEq", "Eq"] {
        assert!(!ERROR_SOURCE.contains(&format!("impl {capability} for ErrorBuiltin")));
    }
}

#[test]
fn sole_error_emitter_consumes_every_dispatch_and_constructor_family() {
    let consumer = rust_code(
        bounded(
            ERROR_SOURCE,
            "fn emit_error_builtin(",
            "pub(super) fn emit_error_constructor_builtin(",
        ),
        true,
    );
    assert_eq!(consumer.matches("matchbuiltin{").count(), 1);
    assert_eq!(consumer.matches("matchkind{").count(), 1);
    for route in [
        "ErrorBuiltin::IsError=>{",
        "ErrorBuiltin::Constructor(kind)=>matchkind{",
        "ErrorBuiltin::PrototypeToString=>{",
    ] {
        assert_eq!(consumer.matches(route).count(), 1, "route `{route}`");
    }
    for kind in [
        "AggregateError",
        "SuppressedError",
        "Error",
        "EvalError",
        "RangeError",
        "ReferenceError",
        "SyntaxError",
        "TypeError",
        "URIError",
    ] {
        assert_eq!(
            consumer
                .matches(&format!("NativeErrorKind::{kind}"))
                .count(),
            // SuppressedError allocates directly here; AggregateError's
            // typed allocation belongs to its preparation child module.
            if kind == "SuppressedError" { 2 } else { 1 },
            "constructor family `{kind}`"
        );
    }
    for forbidden in ["_=>", "builtin.clone()", "builtin==", "builtin!="] {
        assert!(!consumer.contains(forbidden), "found `{forbidden}`");
    }
}

#[test]
fn eleven_fixed_error_entries_own_every_raw_authority() {
    let standard = rust_code(STANDARD_SOURCE, true);
    let dispatcher_mappings = [
        "StandardBuiltinId::ErrorConstructor=>self.emit_error_constructor_builtin(function)?,",
        "StandardBuiltinId::ErrorIsError=>self.emit_error_is_error_builtin(function)?,",
        "StandardBuiltinId::EvalErrorConstructor=>{self.emit_eval_error_constructor_builtin(function)?}",
        "StandardBuiltinId::AggregateErrorConstructor=>{self.emit_aggregate_error_constructor_builtin(function)?}",
        "StandardBuiltinId::SuppressedErrorConstructor=>{self.emit_suppressed_error_constructor_builtin(function)?}",
        "StandardBuiltinId::RangeErrorConstructor=>{self.emit_range_error_constructor_builtin(function)?}",
        "StandardBuiltinId::SyntaxErrorConstructor=>{self.emit_syntax_error_constructor_builtin(function)?}",
        "StandardBuiltinId::TypeErrorConstructor=>{self.emit_type_error_constructor_builtin(function)?}",
        "StandardBuiltinId::URIErrorConstructor=>{self.emit_uri_error_constructor_builtin(function)?}",
        "StandardBuiltinId::ReferenceErrorConstructor=>{self.emit_reference_error_constructor_builtin(function)?}",
        "StandardBuiltinId::ErrorPrototypeToString=>{self.emit_error_prototype_to_string_builtin(function)?}",
    ];
    for mapping in dispatcher_mappings {
        assert_eq!(standard.matches(&mapping).count(), 1, "mapping `{mapping}`");
    }
    assert!(!standard.contains("ErrorBuiltin"));
    assert!(!standard.contains("NativeErrorKind"));
    assert!(!standard.contains("emit_error_builtin("));

    let fixed_entries = rust_code(
        bounded(
            ERROR_SOURCE,
            "pub(super) fn emit_error_constructor_builtin(",
            "fn emit_install_error_cause_from_arg(",
        ),
        true,
    );
    assert_eq!(
        fixed_entries.matches("self.emit_error_builtin(").count(),
        11
    );
    assert_eq!(
        fixed_entries.matches("ErrorBuiltin::Constructor(").count(),
        9
    );
    assert_eq!(fixed_entries.matches("ErrorBuiltin::IsError").count(), 1);
    assert_eq!(
        fixed_entries
            .matches("ErrorBuiltin::PrototypeToString")
            .count(),
        1
    );
}
