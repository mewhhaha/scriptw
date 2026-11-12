use std::fs;
use std::path::Path;

const REPLACER_SOURCE: &str = include_str!("../../src/builtins/json/stringify_replacer.rs");
const STRINGIFY_SOURCE: &str = include_str!("../../src/builtins/json/stringify.rs");
const HELPER_SOURCE: &str = include_str!("../../src/runtime_helpers.rs");
const JSON_SOURCE: &str = include_str!("../../src/builtins/json.rs");
const CLI_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/language_numerics.rs");
const FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_json_stringify_replacer_invocation_roles.js"
);
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/json-stringify-replacer-invocation-authority.md"
);

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn quoted_literal_end(source: &str, quote_start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut offset = quote_start + 1;
    let mut escaped = false;
    while offset < bytes.len() {
        let byte = bytes[offset];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
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
        b'"' => quoted_literal_end(source, start),
        b'\'' => character_literal_end(source, start),
        b'b' if bytes.get(start + 1) == Some(&b'\'') => character_literal_end(source, start + 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'"') => quoted_literal_end(source, start + 1),
        b'r' => raw_literal_end(source, start, 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'r') => raw_literal_end(source, start, 2),
        _ => None,
    }
}

struct RustCode {
    normalized: String,
    identifiers: String,
}

fn rust_code(source: &str) -> RustCode {
    let bytes = source.as_bytes();
    let mut normalized = String::new();
    let mut identifiers = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = literal_end(source, offset) {
            normalized.push_str(&source[offset..end]);
            identifiers.push(' ');
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
            assert_eq!(depth, 0, "unterminated block comment");
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
            normalized.push(character);
            identifiers.push(character);
        }
        offset += character.len_utf8();
    }
    RustCode {
        normalized,
        identifiers,
    }
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
            exact_identifier_count(&rust_code(&source).identifiers, identifier)
        })
        .sum()
}

#[test]
fn invocation_authority_is_the_exact_private_move_only_domain() {
    let lexical_probe = rust_code(
        r###"
        // JsonStringifyReplacerInvocationLocals
        JsonStringifyReplacerInvocationLocals /* nested /* ignored */ comment */;
        "JsonStringifyReplacerInvocationLocals"; b"JsonStringifyReplacerInvocationLocals";
        c"JsonStringifyReplacerInvocationLocals"; r"JsonStringifyReplacerInvocationLocals";
        br##"JsonStringifyReplacerInvocationLocals"##;
        cr#"JsonStringifyReplacerInvocationLocals"#;
        'J'; b'J'; 'lifetime; r#JsonStringifyReplacerInvocationLocals;
        "###,
    );
    assert_eq!(
        exact_identifier_count(
            &lexical_probe.identifiers,
            "JsonStringifyReplacerInvocationLocals"
        ),
        2
    );

    let module = rust_code(REPLACER_SOURCE);
    for role in ["Function", "Receiver", "PropertyKey", "Value"] {
        assert!(module.normalized.contains(&format!(
            "pub(super)structJsonStringifyReplacer{role}Locals<'v>(&'vValueLocals);"
        )));
        assert!(module.normalized.contains(&format!(
            "{0}:JsonStringifyReplacer{role}Locals<'v>",
            match role {
                "Function" => "replacer",
                "Receiver" => "receiver",
                "PropertyKey" => "property_key",
                "Value" => "value",
                _ => unreachable!(),
            }
        )));
    }
    assert!(REPLACER_SOURCE.contains("#[must_use"));
    assert!(module
        .normalized
        .contains("invocation:JsonStringifyReplacerInvocationLocals<'_>,"));
    for forbidden in [
        "derive(",
        "implClonefor",
        "implCopyfor",
        "pub(crate)",
        "TaggedLocals",
    ] {
        assert!(!module.normalized.contains(forbidden), "found {forbidden}");
    }
    assert!(JSON_SOURCE.contains("mod stringify_replacer;"));
    assert!(!JSON_SOURCE.contains("pub mod stringify_replacer"));
}

#[test]
fn role_and_authority_census_is_closed_over_product_sources() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        count_identifier_in_rust_sources(&root, "JsonStringifyReplacerInvocationLocals"),
        6
    );
    for role in ["Function", "Receiver", "PropertyKey", "Value"] {
        assert_eq!(
            count_identifier_in_rust_sources(&root, &format!("JsonStringifyReplacer{role}Locals")),
            6
        );
    }
    assert_eq!(REPLACER_SOURCE.matches("= invocation;").count(), 1);
    assert_eq!(
        STRINGIFY_SOURCE
            .matches("self.emit_json_apply_replacer_with_this(")
            .count(),
        1
    );
}

#[test]
fn sole_serialize_property_producer_constructs_roles_for_every_holder() {
    let source = rust_code(STRINGIFY_SOURCE).normalized;
    assert_eq!(
        source
            .matches("JsonStringifyReplacerInvocationLocals::new(")
            .count(),
        1
    );
    for (role, value) in [
        ("Function", "&method"),
        ("Receiver", "holder"),
        ("PropertyKey", "&key_value"),
        ("Value", "&value"),
    ] {
        assert_eq!(
            source
                .matches(&format!("JsonStringifyReplacer{role}Locals::new({value})"))
                .count(),
            1
        );
    }
    // Array/object share one container walk; it and the root use the same helper.
    assert!(source.contains(
        "JsonStringifyValueArguments::new(holder,key,context,indent,self.current_program())"
    ));
    let helpers = rust_code(HELPER_SOURCE).normalized;
    assert!(helpers.contains(
        "JsonStringifyValue/JsonStringifyValueArguments/JsonStringifyValueParameters/\"json_stringify_value\"{holder:Value,key:Value,context:(RefJsonStringifyContextNonNullable),indent:I64,caller_program:Program}=>Completion;"
    ));
    assert!(
        source.contains("self.emit_json_serialize_property(&parameters.holder,&parameters.key,")
    );
    assert_eq!(
        STRINGIFY_SOURCE
            .matches("fn emit_json_serialize_property(")
            .count(),
        1
    );
    assert_eq!(
        STRINGIFY_SOURCE
            .matches("self.emit_json_stringify_call(")
            .count(),
        2
    );
    assert!(source.contains("self.emit_json_index_key(index,f)?"));
    assert!(source.contains("self.emit_argument_vector_entry_to_value(&list,item,&key_value,f);"));
    assert!(source.contains(
        "self.emit_json_stringify_call(value,&key_value,context,child_depth,&result,f)?;"
    ));
    // A replacer function receives the root wrapper as its holder.
    assert!(source.contains("self.emit_json_define(&holder,&empty,&value,false,f)?;"));
}

#[test]
fn sole_consumer_preserves_argument_result_and_abrupt_roles() {
    // Rustfmt's trailing commas are not part of the invariant.
    let source = rust_code(REPLACER_SOURCE)
        .normalized
        .replace(",}", "}")
        .replace(",)", ")");
    assert!(source.contains("letJsonStringifyReplacerInvocationLocals{replacer,receiver,property_key,value}=invocation;"));
    assert!(source
        .contains("self.emit_json_call(replacer.0,receiver.0,&[property_key.0,value.0],output,f)"));
    let call = rust_code(bounded(JSON_SOURCE, "fn emit_json_call(", "\n}\n")).normalized;
    let order = [
        "self.emit_pre_evaluated_arg_vector(args,f)",
        "self.emit_function_or_proxy_call_with_argv(callee,receiver,&argv,&pending,f)?;",
        "self.completion().copy_from(&pending,f);",
        "self.emit_propagate_current_throw_if_needed(f);",
        "out.copy_from(pending.value(),f);",
    ];
    let mut cursor = 0;
    for marker in order {
        cursor += call[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing {marker}"))
            + marker.len();
    }
}

#[test]
fn contract_task_and_public_fixture_own_the_authority() {
    for marker in [
        "JSON.stringify replacer invocation authority",
        "transpose replacer, receiver, property key, and value roles",
        "json_stringify_replacer_invocation_authority_structure",
    ] {
        assert!(CONTRACT.contains(marker), "contract marker `{marker}`");
        assert!(CONTRACT.contains(marker), "task marker `{marker}`");
    }
    assert_eq!(
        CLI_SOURCE
            .matches("run_wasm_backend_preserves_json_stringify_replacer_invocation_roles")
            .count(),
        1
    );
    assert_eq!(
        CLI_SOURCE
            .matches("wasm_json_stringify_replacer_invocation_roles.js")
            .count(),
        1
    );
    for witness in [
        "this !== root && this[\"\"] === root && value === root",
        "this === root.object && value === 1",
        "this === root.array && value === 2",
        "throw thrown",
        "error === thrown",
    ] {
        assert!(FIXTURE.contains(witness), "fixture witness `{witness}`");
    }
}
