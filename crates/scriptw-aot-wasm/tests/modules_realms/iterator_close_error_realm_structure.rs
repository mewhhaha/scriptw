const FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_iterator_close_generated_error_realm.js"
);
const ITERATOR_CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/iterator.rs");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/iterator-close-error-realm.md");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker after: {start}"))
        .0
}

#[test]
fn runtime_witness_uses_a_borrowed_iterator_helper_realm_for_both_errors() {
    for marker in [
        "var other = __scriptwCreateRealm().global;",
        "caught instanceof other.TypeError",
        "!(caught instanceof TypeError)",
        "var nonCallableReturn =",
        "return: 0",
        "non-callable return",
        "var primitiveReturn =",
        "return 0;",
        "primitive return result",
        "var validReturn =",
    ] {
        assert!(FIXTURE.contains(marker), "fixture marker: {marker}");
    }
    assert_eq!(
        FIXTURE
            .matches("other.Iterator.prototype.some.call(")
            .count(),
        3
    );

    let cli_test = bounded(
        ITERATOR_CLI_TESTS,
        "fn run_wasm_backend_uses_borrowed_iterator_helper_realm_for_iterator_close_errors() {",
        "\n#[test]",
    );
    assert!(cli_test.contains("wasm_iterator_close_generated_error_realm.js"));
    assert!(cli_test.contains(".arg(\"wasm\")"));
    assert!(cli_test.contains("stdout.contains(\"backend_used: WasmAot\")"));
    assert!(cli_test.contains("stdout.contains(\"boolean(true)\")"));
    assert_eq!(
        ITERATOR_CLI_TESTS
            .matches("wasm_iterator_close_generated_error_realm.js")
            .count(),
        1
    );
    assert_eq!(
        ITERATOR_CLI_TESTS
            .matches(
                "fn run_wasm_backend_uses_borrowed_iterator_helper_realm_for_iterator_close_errors()",
            )
            .count(),
        1
    );
}

#[test]
fn published_boundary_names_the_owner_routes_witness_and_nonclaims() {
    for marker in [
        "emit_iterator_close",
        "emit_iterator_close_preserving_current_throw",
        "emit_iterator_close_preserving_saved_throw",
        "IteratorClose return method must be callable",
        "IteratorClose return result must be object",
        "`Intl.ListFormat`",
        "builtins/intl_listformat/iterable.rs",
        "wasm_iterator_close_generated_error_realm.js",
        "iterator_close_error_realm_structure",
        "run_wasm_backend_uses_borrowed_iterator_helper_realm_for_iterator_close_errors",
    ] {
        assert!(CONTRACT.contains(marker), "contract marker: {marker}");
    }
    for retired in ["LegacyMainRealm", "legacy main-Realm policy"] {
        assert!(
            !CONTRACT.contains(retired),
            "retired contract claim `{retired}`"
        );
    }
    let nonclaim = bounded(CONTRACT, "## Nonclaim", "## Focused verification");
    for marker in [
        "only the two errors created by IteratorClose",
        "direct `for-of`",
        "`GetIterator`",
        "`IteratorStep`",
        "direct-synchronous-for-of-protocol-error-realm.md",
        "sync-iterator-consumer-capability.md",
        "expands this contract's ownership beyond IteratorClose",
    ] {
        assert!(nonclaim.contains(marker), "nonclaim marker `{marker}`");
    }
}
