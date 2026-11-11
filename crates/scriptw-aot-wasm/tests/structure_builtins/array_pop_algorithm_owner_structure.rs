const ARRAY_CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/array.rs");
const ARRAY_POP_OVERRIDE_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_array_pop_own_method_dispatch.js");

#[test]
fn pop_override_runtime_control_requires_generic_get_and_call_fallthrough() {
    for marker in [
        "target.pop = function (first, second, third, fourth) {",
        "receiver = this;",
        "let result = target.pop(record(1), ...[record(2), record(3)], record(4));",
        "result === 10",
        "callCount === 1",
        "target.length === 3",
        "target[0] === 1",
        "target[2] === 3",
    ] {
        assert!(
            ARRAY_POP_OVERRIDE_FIXTURE.contains(marker),
            "missing Array Pop override witness `{marker}`"
        );
    }
    assert!(ARRAY_CLI_TESTS.contains("fn run_wasm_backend_calls_an_arrays_own_pop_method()"));
    assert!(ARRAY_CLI_TESTS.contains("fixture_path(\"wasm_array_pop_own_method_dispatch.js\")"));
}
