const PROMISE_SOURCE: &str = include_str!("../../src/builtins/promise.rs");

const PROMISE_TRY_CALLBACK_TYPE_ERROR_SOURCE: &str =
    include_str!("../../src/builtins/promise/promise_try_callback_type_error.rs");

fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker: {end}"))
        .0
}

const CLI_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_promise_internal_callback_realm.js");

#[test]
fn promise_internal_callback_fixture_observes_the_borrowed_try_error_realm() {
    for marker in [
        "other.Promise.try(0)",
        "Promise.try callback TypeError realm",
        "Promise.try callback rejection checkpoint",
        "promise-internal-callback-realm:ok",
    ] {
        assert!(
            CLI_FIXTURE.contains(marker),
            "missing runtime witness: {marker}"
        );
    }
    assert!(
        CLI_FIXTURE
            .find("Promise.try callback rejection checkpoint")
            .unwrap()
            < CLI_FIXTURE
                .find("promise-internal-callback-realm:ok")
                .unwrap(),
        "the success sentinel must be gated by the Promise.try rejection reaction"
    );
}

#[test]
fn promise_try_callback_type_error_proof_is_private_and_one_shot() {
    assert_eq!(
        PROMISE_SOURCE
            .matches("\nmod promise_try_callback_type_error;\n")
            .count(),
        1
    );
    assert!(!PROMISE_SOURCE.contains("pub mod promise_try_callback_type_error;"));
    assert!(!PROMISE_SOURCE.contains("promise_try_callback_type_error::"));
    assert!(!PROMISE_SOURCE.contains("PromiseTryCallbackTypeErrorPrototypeLocal"));
    let declaration = between(
        PROMISE_TRY_CALLBACK_TYPE_ERROR_SOURCE,
        "#[must_use",
        "impl FunctionBuilder<'_>",
    );
    assert!(declaration.contains("pub(super) struct PromiseTryCallbackTypeErrorPrototypeLocal"));
    assert!(!declaration.contains("#[derive"));
    for capability in ["Clone", "Copy", "Debug", "PartialEq", "Eq"] {
        assert!(!PROMISE_TRY_CALLBACK_TYPE_ERROR_SOURCE.contains(&format!(
            "impl {capability} for PromiseTryCallbackTypeErrorPrototypeLocal"
        )));
    }
}
