const PARENT_ERRORS_SOURCE: &str = include_str!("../../src/builtins/errors.rs");
const ERRORS_SOURCE: &str = include_str!("../../src/builtins/errors/runtime_error.rs");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/thrown-error-diagnostic-kind-authority.md"
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

fn normalized(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn assert_before(source: &str, earlier: &str, later: &str) {
    let earlier_offset = source
        .find(earlier)
        .unwrap_or_else(|| panic!("missing earlier operation `{earlier}`"));
    let later_offset = source
        .find(later)
        .unwrap_or_else(|| panic!("missing later operation `{later}`"));
    assert!(
        earlier_offset < later_offset,
        "`{earlier}` must precede `{later}`"
    );
}

#[test]
fn diagnostic_publisher_accepts_only_native_error_kind() {
    let publisher = bounded(
        ERRORS_SOURCE,
        "    fn emit_set_thrown_error_text(",
        "    pub(crate) fn emit_throw_runtime_error(",
    );
    assert!(publisher.contains("kind: NativeErrorKind,"));
    assert!(publisher.contains("self.emit_interned_string_reference(kind.as_str(), function)?"));
    assert!(publisher.contains("message: Option<RuntimeErrorMessage>,"));
    assert!(!publisher.contains("name: &str"));
    assert!(!publisher.contains("native_error_kind("));
    for role in ["Name", "Message"] {
        assert_eq!(
            publisher
                .matches(&format!(
                    "self.emit_clear_throw_diagnostic(ThrowDiagnosticRole::{role}, function);"
                ))
                .count(),
            1
        );
        assert_eq!(
            publisher
                .matches(&format!(
                    "self.emit_store_throw_diagnostic(ThrowDiagnosticRole::{role},"
                ))
                .count(),
            1
        );
    }
    assert_before(
        publisher,
        "self.emit_interned_string_reference(kind.as_str(), function)?",
        "self.emit_store_throw_diagnostic(ThrowDiagnosticRole::Name, &name, function);",
    );
    assert_before(
        publisher,
        "self.emit_runtime_error_message_reference(message, function)?",
        "self.emit_store_throw_diagnostic(ThrowDiagnosticRole::Message, &text, function);",
    );
}

#[test]
fn all_three_producers_forward_the_error_kind_they_already_own() {
    assert!(
        !PARENT_ERRORS_SOURCE.contains("emit_set_thrown_error_text("),
        "the private runtime-error child retains diagnostic ownership"
    );
    let source = normalized(ERRORS_SOURCE);
    let throwing = bounded(
        &source,
        "pub(crate)fnemit_throw_runtime_error_with_prototype(",
        "pub(crate)fnemit_throw_current_function_realm_type_error(",
    );
    assert!(throwing.contains("RuntimeErrorForThrowArguments::new(kind,prototype,&message)"));
    assert!(!throwing.contains("emit_set_thrown_error_text("));
    let shared = bounded(
        &source,
        "pub(crate)fncompile_runtime_error_for_throw_helper(",
        "fnemit_set_thrown_error_text(",
    );
    assert!(shared.contains("forkindinNativeErrorKind::ALL"));
    assert!(shared.contains("parameters.kind.emit_is(kind,&mutfunction)"));
    assert!(shared.contains("self.emit_interned_string_reference(kind.as_str(),&mutfunction)?"));
    assert!(!shared.contains("ThrowDiagnosticRole::ConstructorName"));
    assert!(
        source.contains(
            "self.emit_set_thrown_error_text(NativeErrorKind::TypeError,None,function)?;"
        ),
        "the message-less producer still owns its closed TypeError kind"
    );
    for forbidden in [
        "emit_set_thrown_error_text(name",
        "emit_set_thrown_error_text(TYPE_ERROR_NAME",
        "emit_set_thrown_error_text(RANGE_ERROR_NAME",
        "emit_set_thrown_error_text(URI_ERROR_NAME",
    ] {
        assert!(!ERRORS_SOURCE.contains(forbidden), "found `{forbidden}`");
    }
}

#[test]
fn diagnostic_publication_follows_object_creation_and_precedes_throw_completion() {
    let source = normalized(ERRORS_SOURCE);
    let global_prototype_path = bounded(
        &source,
        "pub(crate)fnemit_throw_runtime_error(",
        "pub(crate)fnemit_throw_current_function_realm_error(",
    );
    assert_before(
        global_prototype_path,
        "self.emit_execution_realm(function)",
        "prototype_slot(kind)",
    );
    assert_before(
        global_prototype_path,
        "prototype_slot(kind)",
        "self.emit_throw_runtime_error_with_prototype(kind,message,&prototype,result,function)?",
    );
    let resolved_prototype_path = bounded(
        &source,
        "pub(crate)fnemit_throw_runtime_error_with_prototype(",
        "pub(crate)fnemit_throw_current_function_realm_type_error(",
    );
    assert_before(
        resolved_prototype_path,
        "self.emit_runtime_error_message_reference(message,function)?",
        "RuntimeErrorForThrowArguments::new(kind,prototype,&message)",
    );
    assert_before(
        resolved_prototype_path,
        "RuntimeErrorForThrowArguments::new(kind,prototype,&message)",
        "result.set_throw(&value,function)",
    );
    assert!(!resolved_prototype_path.contains("emit_execution_realm"));
    let shared = bounded(
        &source,
        "pub(crate)fncompile_runtime_error_for_throw_helper(",
        "fnemit_set_thrown_error_text(",
    );
    assert_before(
        shared,
        "RuntimeErrorObjectArguments::new(&parameters.prototype,&parameters.message)",
        "self.emit_clear_throw_diagnostic(ThrowDiagnosticRole::Name,&mutfunction)",
    );
    assert_before(
        shared,
        "self.emit_clear_throw_diagnostic(ThrowDiagnosticRole::Name,&mutfunction)",
        "self.emit_interned_string_reference(kind.as_str(),&mutfunction)?",
    );
    assert_before(
        shared,
        "self.emit_interned_string_reference(kind.as_str(),&mutfunction)?",
        "self.emit_store_throw_diagnostic(ThrowDiagnosticRole::Name,&name,&mutfunction)",
    );
    assert_before(shared,"self.emit_store_throw_diagnostic(ThrowDiagnosticRole::Name,&name,&mutfunction)","self.emit_store_throw_diagnostic(ThrowDiagnosticRole::Message,&parameters.message,&mutfunction,)");
    assert!(
        !shared.contains("result.set_throw("),
        "Completion belongs to the actual caller, outside the R publication tail"
    );
}

#[test]
fn contract_records_the_authority_and_non_claim() {
    let normalized_contract = normalized(CONTRACT);
    let normalized_task = normalized(CONTRACT);
    for evidence in [
        "NativeErrorKind",
        "published diagnostic name",
        "interchangeable raw string",
        "does not change emitted Wasm",
        "user-thrown values",
    ] {
        let normalized_evidence = normalized(evidence);
        assert!(
            normalized_contract.contains(&normalized_evidence),
            "contract evidence `{evidence}`"
        );
        assert!(
            normalized_task.contains(&normalized_evidence),
            "task evidence `{evidence}`"
        );
    }
}
