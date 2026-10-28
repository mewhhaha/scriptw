//! `typed_array` CLI integration tests.

use crate::*;

#[test]
fn run_wasm_backend_validates_typedarray_length_before_prototype() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typed_array_length_before_prototype.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(4)"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_arraylike_length_abrupt_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_from_arraylike_length_abrupt.js",
        ))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_assert_throws_abrupt_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_assert_throws_abrupt.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_early_validation_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_early_validation.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_empty_construction_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_empty_construction.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_empty_harness_assert_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_empty_harness_assert.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_dynamic_constructor_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_dynamic_constructor.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_constructor_arrays_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_constructor_arrays.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_custom_constructor_validation_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_from_custom_constructor_validation.js",
        ))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_captured_custom_ctor_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_captured_custom_ctor.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_custom_ctor_existing_instance_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_from_custom_ctor_existing_instance.js",
        ))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_callback_parameter_constructors_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_from_callback_parameter_constructors.js",
        ))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_callback_prototype_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_callback_prototype.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_inherited_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_inherited.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_nonempty_numeric_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_nonempty_numeric.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_object_missing_zero_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_object_missing_zero.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_zero_sign_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_zero_sign.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_abrupt_values_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_abrupt_values.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_callback_sources_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_callback_sources.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_mapper_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_mapper.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_mapper_arguments_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_mapper_arguments.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_mapper_this_global_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_mapper_this_global.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_nan_conversion_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_nan_conversion.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_real_nan_harness_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_real_nan_harness.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_nested_constructor_context_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_from_nested_constructor_context.js",
        ))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_shared_callback_contexts_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_from_shared_callback_contexts.js",
        ))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_iterator_abrupt_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_from_iterator_abrupt.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_mapfn_not_callable_global_getter_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_from_mapfn_not_callable_global_getter.js",
        ))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_from_custom_ctor_global_called_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_from_custom_ctor_global_called.js",
        ))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(262"));
}

#[test]
fn run_wasm_backend_clones_typedarrays_without_arraybuffer_species_lookup() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_constructor_no_species.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(123"));
}

#[test]
fn run_wasm_backend_validates_typedarray_constructor_source_buffer_witness() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_constructor_source_buffer_witness.js",
        ))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_accessors_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_accessors.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(123"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_iterators_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_iterators.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(123"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_find_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_find.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(124"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_every_some_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_every_some.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(125"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_search_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_search.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(132"), "{stdout}");
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_map_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_map.js"))
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
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_filter_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_filter.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(127"));
}

#[test]
fn run_wasm_backend_preserves_bigint_typedarray_reduce_default_accumulator() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_bigint_reduce_default_accumulator.js",
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
    assert!(stdout.contains("number(131"), "{stdout}");
}

#[test]
fn run_wasm_backend_enumerates_typedarray_own_keys_in_spec_order() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_own_property_keys.js"))
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
fn run_wasm_backend_get_own_property_names_uses_typedarray_buffer_witness() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_object_get_own_property_names_typed_array_witness.js",
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
    assert!(stdout.contains("number(951"), "{stdout}");
}

#[test]
fn run_wasm_backend_reads_typedarray_own_property_descriptors() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_get_own_property.js"))
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
fn run_wasm_backend_reads_typedarray_canonical_numeric_indices_without_prototype_lookup() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_get.js"))
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
fn run_wasm_backend_uses_live_typedarray_property_and_index_buffer_witnesses() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_property_index_buffer_witness.js",
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
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_sets_typedarray_from_array_like_and_overlapping_typedarray_sources() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_prototype_set.js"))
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
fn run_wasm_backend_revalidates_typedarray_set_buffer_witnesses() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_set_buffer_witness.js"))
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
fn run_wasm_backend_subarray_uses_non_throwing_typed_array_buffer_witness() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_subarray_buffer_witness.js"))
        .output()
        .expect("run command should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(967"), "{stdout}");
}

#[test]
fn run_wasm_backend_slices_typedarrays_with_species_and_resizable_buffer_semantics() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_prototype_slice.js"))
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
fn run_wasm_backend_validates_constructed_typedarray_targets_with_live_witness() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_constructed_target_witness.js",
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
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}

#[test]
fn run_wasm_backend_reverses_typedarray_into_same_type_without_mutating_source() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_prototype_to_reversed.js"))
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
fn run_wasm_backend_stably_sorts_typedarray_copy_with_numeric_and_bigint_ordering() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_prototype_to_sorted.js"))
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
fn run_wasm_backend_reverses_typedarray_in_place_and_returns_receiver() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_prototype_reverse.js"))
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
fn run_wasm_backend_copies_typedarray_bytes_with_spec_ordering() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_prototype_copy_within.js"))
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
fn run_wasm_backend_stably_sorts_typedarray_in_place_across_buffer_states() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_prototype_sort.js"))
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
fn run_wasm_backend_copies_typedarray_with_one_replacement() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_prototype_with.js"))
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
fn run_wasm_backend_validates_typedarray_with_entry_buffer_witness() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_with_buffer_witness.js"))
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
fn run_wasm_backend_checks_typedarray_integer_indices_through_buffer_witness() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_has_property.js"))
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
fn run_wasm_backend_deletes_typedarray_integer_indexed_properties() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_delete.js"))
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
fn run_wasm_backend_defines_typedarray_integer_indexed_properties() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_define_own_property.js"))
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
fn run_wasm_backend_sets_typedarray_integer_indexed_properties() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_set.js"))
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
fn run_wasm_backend_sets_array_receiver_through_typedarray_prototype() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_set_array_receiver.js"))
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
fn run_wasm_backend_succeeds_for_typedarray_clone_length_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_clone_length.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(123"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_constructor_shadowed_parameter_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typedarray_constructor_shadowed_parameter.js",
        ))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(123"));
}

#[test]
fn run_wasm_backend_succeeds_for_supported_typedarray_indexed_write_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_indexed_writes.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("number(1022"));
}

#[test]
fn run_wasm_backend_succeeds_for_typed_array_prevent_extensions_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_prevent_extensions.js"))
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
fn run_wasm_backend_succeeds_for_supported_typedarray_to_string_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_to_string_core.js"))
        .output()
        .expect("run command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"));
    assert!(stdout.contains("boolean(true)"));
}

#[test]
fn run_wasm_backend_succeeds_for_typedarray_join_buffer_witness_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_join_buffer_witness.js"))
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
    assert!(stdout.contains("number(123"), "{stdout}");
}

#[test]
fn run_wasm_backend_typedarray_from_array_skips_length_prevalidation() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path(
            "wasm_typed_array_from_array_skips_length_prevalidation.js",
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
    assert!(stdout.contains("number(7)"), "{stdout}");
}

#[test]
fn run_wasm_backend_preserves_hidden_typedarray_constructor_identity() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_intrinsic_identity.js"))
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
fn run_wasm_backend_keeps_typedarray_sort_snapshots_across_arguments_mutation_and_gc() {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("run")
        .arg("--execution-backend")
        .arg("wasm")
        .arg(fixture_path("wasm_typedarray_sort_snapshot_arguments.js"))
        .output()
        .expect("run command should run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend_used: WasmAot"), "{stdout}");
    assert!(stdout.contains("boolean(true)"), "{stdout}");
}
