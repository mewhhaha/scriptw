//! `dynamic` CLI integration tests.

use crate::*;

#[test]
fn guarded_object_paths_preserve_saved_inputs_and_live_storage_after_callbacks() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .args(["run", "--execution-backend", "wasm-aot"])
        .arg(fixture_path("wasm_guarded_object_paths.js"))
        .output()
        .expect("guarded object path controls should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("backend_used: WasmAot"), "{stdout}");
    assert!(stdout.contains("guarded-object-paths:ok"), "{stdout}");
}

#[test]
fn numeric_array_reads_preserve_own_data_and_generic_get_semantics() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .args(["run", "--execution-backend", "wasm-aot"])
        .arg(fixture_path("wasm_numeric_array_own_data_read.js"))
        .output()
        .expect("numeric array read controls should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("backend_used: WasmAot"), "{stdout}");
    assert!(stdout.contains("numeric-array-own-data:ok"), "{stdout}");
}

#[test]
fn inspect_reports_phase_fifteen_dynamic_primitive_ir_shape() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("inspect")
        .arg(fixture_path("wasm_dynamic_primitives.js"))
        .output()
        .expect("inspect command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("string_concats=1"));
    assert!(stdout.contains("typeof_uses=1"));
    assert!(stdout.contains("nullish_ops=1"));
}

#[test]
fn build_wasm_succeeds_for_supported_dynamic_primitive_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("build")
        .arg("wasm")
        .arg(fixture_path("wasm_dynamic_primitives.js"))
        .output()
        .expect("build command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("built Wasm artifact"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_dynamic_primitive_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_dynamic_primitives.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("ab"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_dynamic_builtin_getter_call_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_dynamic_builtin_getter_call.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(4"));
}

#[test]
fn run_wasm_backend_succeeds_for_dynamic_truthy_call_in_callback_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_dynamic_truthy_call_in_callback.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_reads_dynamic_array_and_arguments_indexes() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_dynamic_array_index_read.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(126"));
    assert!(stdout.contains("dynamic-awaited-object:7"), "{stdout}");
}

#[test]
fn run_wasm_backend_resolves_dynamic_global_identifiers_at_runtime() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_dynamic_global_identifier_read.js"))
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
        stdout.contains("dynamic-global-identifier:true"),
        "{stdout}"
    );
}

#[test]
fn run_wasm_backend_resolves_typeof_through_runtime_global_properties() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typeof_runtime_global_resolution.js"))
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
fn run_wasm_backend_succeeds_for_dynamic_array_set_length_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_dynamic_array_set_length.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_dynamic_array_index_write_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_dynamic_array_index_write.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_preserves_ordinary_array_constructor_property_semantics() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_dynamic_array_constructor_assignment.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_dynamic_super_proxy_new_target_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_dynamic_super_proxy_new_target.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}
