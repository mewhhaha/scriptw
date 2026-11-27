const PROMISE_SOURCE: &str = include_str!("../../src/builtins/promise.rs");
const CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/functions.rs");
const CLI_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_async_execution_realm.js");

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

struct NormalizedRust {
    code: String,
}

fn normalize_rust(source: &str) -> NormalizedRust {
    let bytes = source.as_bytes();
    let mut code = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = literal_end(source, offset) {
            code.push_str(&source[offset..end]);
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
        }
        offset += character.len_utf8();
    }
    NormalizedRust { code }
}

fn normalized(source: &str) -> String {
    normalize_rust(source).code
}

#[test]
fn existing_fixture_covers_default_and_async_execution_reactions() {
    assert!(CLI_TESTS
        .contains("fn run_wasm_backend_uses_async_function_realms_for_promises_and_reactions()"));
    for marker in [
        "other.Promise.resolve(0).then(",
        "await value;",
        "async captured reaction Realm",
        "async-generator captured reaction Realm",
        "async-execution-realm:ok",
    ] {
        assert!(
            CLI_FIXTURE.contains(marker),
            "missing fixture marker `{marker}`"
        );
    }
}

#[test]
fn module_reactions_accept_owned_intrinsic_records_and_keep_raw_rejection_and_realm() {
    let raw = bounded(
        PROMISE_SOURCE,
        "    fn emit_owned_promise_record_reactions(",
        "    pub(crate) fn emit_module_promise_reactions(",
    );
    for forbidden in [
        "emit_intrinsic_promise_resolve",
        "emit_get_property",
        "emit_function_handle_call",
        "PromiseConstructor",
        "Species",
    ] {
        assert!(
            !raw.contains(forbidden),
            "owned module reaction observed {forbidden}"
        );
    }
    assert_eq!(raw.matches("&initialization").count(), 1);
    assert!(raw.contains("PromiseState::ALL"));
    assert!(raw.contains("PromiseSettlement::Fulfill"));
    assert!(raw.contains("PromiseSettlement::Reject"));
    assert!(raw.contains("emit_initialize_promise_reaction"));
    assert!(raw.contains("emit_append_pending_promise_reaction"));
    assert!(raw.contains("emit_append_promise_job"));
    let module = bounded(
        PROMISE_SOURCE,
        "    pub(crate) fn emit_module_promise_reactions(",
        "    pub(crate) fn emit_module_import_await_reactions(",
    );
    assert!(module.contains("PromiseReactionInitialization::Module"));
    let import = bounded(
        PROMISE_SOURCE,
        "    pub(crate) fn emit_module_import_await_reactions(",
        "    pub(crate) fn emit_module_execution_realm_context(",
    );
    assert!(import.contains("emit_async_function_execution_realm_context_from_activation"));
    assert!(import.contains("emit_owned_promise_record_reactions"));
    assert!(!import.contains("emit_intrinsic_await_reactions"));
}

#[test]
fn canonical_evaluate_capabilities_require_intrinsic_promise_bootstrap_even_without_source_await() {
    let planning = include_str!("../../src/planning.rs");
    let entry = normalized(bounded(
        planning,
        "    pub(crate) fn full() -> Self {",
        "    pub(crate) fn should_initialize_standard_builtin(",
    ));
    assert!(entry.contains("plan.full_standard_globals=true;"));
    assert!(entry.contains("forbuiltininStandardBuiltinId::all_functions()"));
    assert!(!entry.contains("script."));
    let assembly = normalized(include_str!("../../src/emit/module_assembly.rs"));
    assert!(assembly.contains(
        "letruntime_bootstrap_plan=ifuses_heap{RuntimeBootstrapPlan::full()}else{RuntimeBootstrapPlan::default()};"
    ));
    assert!(entry.contains("plan.require_standard_builtin(StandardBuiltinId::PromiseConstructor);"));
}
