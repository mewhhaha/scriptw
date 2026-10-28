//! `functions` CLI integration tests.

use crate::*;

#[test]
fn inspect_reports_phase_twelve_function_form_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_function_forms.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("arrow_functions=3"));
    assert!(stdout.contains("named_function_exprs=2"));
    assert!(stdout.contains("lexical_this_captures=1"));
}

#[test]
fn inspect_reports_phase_twenty_six_function_builtin_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_function_builtins.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // `pick` references `arguments`, enabling named runtime environments for
    // this Script. Its AggregateError/call/apply References perform live lookup
    // rather than carrying resolved intrinsic identities into lowering.
    assert!(stdout.contains("aggregate_errors=0"), "{stdout}");
    assert!(stdout.contains("function_proto_calls=0"), "{stdout}");
    assert!(stdout.contains("function_proto_applies=0"), "{stdout}");
    // All source invocations remain present, including the Error constructor
    // and both AggregateError constructors in the final conjunction.
    assert!(stdout.contains("calls=13"), "{stdout}");
    assert!(stdout.contains("constructs=3"), "{stdout}");
    assert!(
        stdout.contains("environment_identifier_calls=1"),
        "{stdout}"
    );
}

#[test]
fn inspect_reports_phase_twenty_seven_boxed_builtin_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_boxed_builtins.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("boxed_builtin_globals=3"));
    // The fixture never calls `Number`, `String` or `Boolean` as a function.
    // Before c4e15caf5 each `new Number(1)`/`new String("x")`/`new Boolean(false)`
    // was recorded both as a construct and as a call, which is where the old
    // `boxed_builtin_calls=3` came from; they are now constructs only.
    assert!(stdout.contains("boxed_builtin_calls=0"), "{stdout}");
    assert!(stdout.contains("boxed_builtin_constructs=3"));
    // None of the three receiver adaptations is resolved statically. `call`,
    // `apply` and `bind` resolve to their intrinsics only under the
    // live-prototype proof in `lowering/intrinsic_method.rs`, and the earlier
    // `instanceof` operands in this `&&` chain (which may run a user
    // `@@hasInstance`) erase the recorded `%Function.prototype%` facts first,
    // so `isNumberBox.call(1)`, `isBooleanBox.call(false)` and
    // `isStringBox.apply("x", [])` stay dynamic calls.
    // `run_wasm_backend_succeeds_for_supported_boxed_builtin_fixture` checks
    // that all three still box their primitive receiver.
    assert!(stdout.contains("boxed_receiver_adaptations=0"), "{stdout}");
}

#[test]
fn run_wasm_backend_publishes_created_realm_promise_foundation() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_promise_created_realm.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_created_realm_promise_internal_callbacks() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm-aot")
        .arg(fixture_path("wasm_promise_internal_callback_realm.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(
        stdout.contains("promise-internal-callback-realm:ok"),
        "{stdout}"
    );
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_uses_callback_realms_for_promise_created_allocations() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm-aot")
        .arg(fixture_path(
            "wasm_promise_callback_created_allocation_realm.js",
        ))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(
        stdout.contains("promise-callback-created-allocation-realm:ok"),
        "{stdout}"
    );
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_distinguishes_all_promise_combinator_modes() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm-aot")
        .arg(fixture_path("wasm_promise_combinator_modes.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("promise-combinator-modes:ok"), "{stdout}");
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_uses_created_realm_promise_combinator_algorithm_errors() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm-aot")
        .arg(fixture_path(
            "wasm_promise_combinator_algorithm_error_realm.js",
        ))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(
        stdout.contains("promise-combinator-algorithm-error-realm:ok"),
        "{stdout}"
    );
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_uses_async_function_realms_for_promises_and_reactions() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm-aot")
        .arg(fixture_path("wasm_async_execution_realm.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("async-execution-realm:ok"), "{stdout}");
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn inspect_reports_phase_twenty_eight_bind_builtin_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_bind_builtins.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Acquiring add.bind can observe an unknown own/inherited accessor.
    // That Get widens global function facts before subsequent property reads.
    // Its open call-target set recognizes one possible builtin, while none of
    // the calls retain the exact target needed for static bound-result facts.
    // The runtime fixture below still verifies all six native bound functions.
    assert!(stdout.contains(" function_proto_binds=1 "), "{stdout}");
    assert!(stdout.contains(" bound_functions=0 "), "{stdout}");
    assert!(stdout.contains(" bound_function_constructs=0 "), "{stdout}");
    // Neither `err.toString()` nor `TypeError("y").toString()` resolves to
    // `%Error.prototype.toString%` statically any more: since 5eca93e67 the
    // user-code calls earlier in this `&&` chain (`inc(2)`, `new G()`, ...)
    // invalidate what lowering knew, erasing `err`'s heap shape and widening
    // the global `TypeError` binding, so both calls read `toString` from their
    // receiver at run time. Without those calls both still resolve.
    // `run_wasm_backend_succeeds_for_supported_bind_builtin_fixture` checks
    // both results.
    assert!(stdout.contains("error_proto_to_strings=0"), "{stdout}");
}

#[test]
fn inspect_reports_phase_thirty_one_function_tostring_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_function_tostring.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("function_proto_to_strings="));
}

#[test]
fn build_wasm_succeeds_for_supported_function_form_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_function_forms.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_function_builtin_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_function_builtins.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_boxed_builtin_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_boxed_builtins.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_bind_builtin_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_bind_builtins.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn build_wasm_succeeds_for_supported_function_tostring_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_function_tostring.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn run_wasm_backend_preserves_function_self_binding_identity() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_function_self_identity.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_preserves_class_capture_alias_propagation() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_class_capture_alias_propagation.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_preserves_class_element_execution_contexts() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_class_element_execution_context.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_strict_class_callable_arguments() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_class_callable_strict_arguments.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_enforces_class_body_implicit_strict() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_class_body_implicit_strict.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_class_element_source_order() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_class_element_source_order.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_class_auto_accessor_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_class_auto_accessors.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_private_setter_identity_and_completion() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_private_setter_identity.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_private_callable_definitions() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_private_callable_definitions.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_rejects_duplicate_private_element_installation() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_private_double_initialization.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_private_in_rhs_boundaries() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_private_in_rhs_boundaries.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_private_assignment_references() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_private_assignment_references.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_nested_private_name_shadowing() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_private_name_shadowing.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_installs_private_elements_on_nonextensible_receivers() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_private_nonextensible_installation.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_private_destructuring_reference_order() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_private_destructuring_reference_order.js",
        ))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_nested_private_environment_chains() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_private_environment_chain.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_private_context_in_nested_functions() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_private_nested_function_context.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_initializes_class_instance_elements_at_constructor_boundaries() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_class_instance_element_boundaries.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_named_class_lexical_bindings() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_class_name_lexical_binding.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_parent_linked_lexical_environments() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_parent_linked_lexical_environments.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_async_for_of_iteration_environments() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_async_for_of_closure_capture.js"))
        .output()
        .expect("run command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("backend_used: WasmAot"), "{stdout}");
    assert!(stdout.contains("number(262"), "{stdout}");
    assert!(
        !stdout.contains("uncaught throw") && !stderr.contains("uncaught throw"),
        "stdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn run_wasm_backend_reports_every_unhandled_rejection_in_fifo_order() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_multiple_unhandled_rejections.js"))
        .output()
        .expect("run command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        [
            "checkpoint-second",
            "Symbol(checkpoint-symbol)",
            "unhandled rejection diagnostic ToString threw",
            "checkpoint-reentrant",
            "checkpoint-third"
        ],
        "the oldest rejection is the exported Throw; every rejection in the detached checkpoint snapshot must be printed once in FIFO order: {stdout}"
    );
    assert!(
        stderr.contains("uncaught throw") && stderr.contains("string(checkpoint-first)"),
        "the oldest rejection must keep the run in a failing Throw completion: {stderr}"
    );
    assert!(
        !stderr.contains("checkpoint-conversion"),
        "a diagnostic conversion failure must not replace the oldest rejection: {stderr}"
    );
    assert!(
        !stdout.contains("checkpoint-handled") && !stderr.contains("checkpoint-handled"),
        "a rejection handled before the checkpoint must remain silent: stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn run_wasm_backend_reports_all_rejections_without_replacing_a_primary_throw() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_multiple_unhandled_rejections_with_primary_throw.js",
        ))
        .output()
        .expect("run command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        [
            "primary-first",
            "primary-second",
            "unhandled rejection diagnostic ToString threw",
            "primary-third"
        ],
        "a pending Script throw makes every unhandled rejection diagnostic output, in FIFO order: {stdout}"
    );
    assert!(
        stderr.contains("uncaught throw: TypeError") && stderr.contains("primary-script-failure"),
        "the Script throw must remain the primary completion: {stderr}"
    );
    assert!(
        !stderr.contains("primary-conversion"),
        "a diagnostic conversion failure must not replace the primary Script throw: {stderr}"
    );
    assert!(
        !stdout.contains("primary-handled") && !stderr.contains("primary-handled"),
        "a handled rejection must remain silent beside a primary throw: stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn run_wasm_backend_succeeds_for_supported_function_form_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_function_forms.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(32"));
}

#[test]
fn run_wasm_backend_succeeds_for_captured_arguments_length_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_arguments_captured_length.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_reads_arguments_with_computed_string_keys() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_arguments_computed_string_read.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_describes_arguments_with_computed_index_keys() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_arguments_computed_index_descriptor.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_preserves_mapped_arguments_descriptor_semantics() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_arguments_mapped_descriptors.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_arguments_callee_semantics() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_arguments_callee.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_invokes_arguments_special_property_accessors() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_arguments_special_accessors.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_reads_arguments_symbol_iterator() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_arguments_symbol_iterator.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_captured_global_object_method_call_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_captured_global_object_method_call.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(6"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_static_numeric_property_key_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_static_numeric_property_keys.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(12"));
}

#[test]
fn run_wasm_backend_succeeds_for_derived_activation_order_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_derived_activation_order.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_super_property_evaluation_order_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_super_property_evaluation_order.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn call_arguments_invalidate_previously_captured_heap_shapes() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm-aot")
        .arg(fixture_path("wasm_call_argument_snapshot_invalidation.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn function_apply_observes_array_like_user_code_effects() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm-aot")
        .arg(fixture_path("wasm_function_apply_array_like_effects.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_tracks_deleted_builtin_function_own_properties() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_function_hasown_after_length_delete.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_function_builtin_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_function_builtins.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_boxed_builtin_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_boxed_builtins.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_bind_builtin_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_bind_builtins.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_function_tostring_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_function_tostring.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_function_prototype_define_property_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_function_prototype_define_property_core.js",
        ))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

/// Each realm owns one callable, non-constructable `%Function.prototype%`
/// whose own descriptors and Function constructor links keep that exact
/// identity. `CreateBuiltinFunction` then defaults `[[Prototype]]` from the
/// defining realm across constructors, methods, accessors, namespace and
/// global functions, plus canonical parseInt/parseFloat helpers.
#[test]
fn run_wasm_backend_uses_created_realm_builtin_function_prototypes() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_created_realm_builtin_function_prototypes.js",
        ))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"), "{stdout}");
    assert!(stdout.contains("number(123"), "{stdout}");
}

/// Built-in constructors that allocate their own result read
/// `Get(newTarget, "prototype")` once, at their own spec step (Date and the
/// Temporal constructors after argument coercion), and BigInt, Symbol and
/// %TypedArray% never read it. A bound function forwards `Construct` to its
/// target, so only the target reads it.
#[test]
fn run_wasm_backend_reads_builtin_construct_new_target_prototype_once() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_builtin_construct_prototype_read_order.js",
        ))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"), "{stdout}");
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_supports_function_prototype_symbol_has_instance() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_function_prototype_symbol_has_instance.js",
        ))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"), "{stdout}");
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_runtime_empty_argument_semantics() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .args(["--jobs", "1", "run", "--execution-backend", "wasm"])
        .arg(fixture_path("wasm_runtime_empty_argument_storage.js"))
        .output()
        .expect("key and argument storage semantic fixture should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("backend_used: WasmAot"), "{stdout}");
    assert!(
        stdout.contains("runtime-empty-argument-storage-ok"),
        "{stdout}"
    );
}

#[test]
fn run_wasm_backend_preserves_lazy_native_function_source() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .args(["--jobs", "1", "run", "--execution-backend", "wasm"])
        .arg(fixture_path("wasm_lazy_native_function_source.js"))
        .output()
        .expect("native source semantic fixture should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("backend_used: WasmAot"), "{stdout}");
    assert!(
        stdout.contains("lazy-native-function-source-ok"),
        "{stdout}"
    );
}
