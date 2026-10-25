#[path = "../../compiler_fingerprint.rs"]
mod compiler_fingerprint;

use compiler_fingerprint::{
    compiler_rerun_inputs, fingerprint, native_fingerprint, verify_implementation, COMPILER_INPUTS,
    NATIVE_COMPILER_INPUTS,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "scriptw-fingerprint-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let workspace = Self(root);
        for input in COMPILER_INPUTS {
            let path = Path::new(input);
            if path.extension().is_some() {
                workspace.write(input, "fixture");
            } else {
                fs::create_dir_all(workspace.0.join(input)).unwrap();
            }
        }
        for input in NATIVE_COMPILER_INPUTS {
            if Path::new(input).extension().is_some() && !workspace.0.join(input).exists() {
                workspace.write(input, "fixture");
            }
        }
        workspace
    }

    fn write(&self, path: &str, contents: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn digest(&self) -> String {
        fingerprint(&self.0).unwrap()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn reused_build_script_must_match_its_actual_fingerprint_implementation() {
    let workspace = Workspace::new();
    assert_eq!(
        verify_implementation(&workspace.0).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    workspace.write(
        "crates/scriptw-engine/compiler_fingerprint.rs",
        include_str!("../../compiler_fingerprint.rs"),
    );
    verify_implementation(&workspace.0).unwrap();
    workspace.write(
        "crates/scriptw-engine/compiler_fingerprint.rs",
        "different inventory",
    );
    assert_eq!(
        verify_implementation(&workspace.0).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
}

#[test]
fn every_previously_omitted_compiler_input_invalidates_the_cache() {
    for input in [
        "crates/scriptw-runtime/src/lib.rs",
        "crates/scriptw-runtime-image/Cargo.toml",
        "crates/scriptw-runtime-image/build.rs",
        "crates/scriptw-runtime-image/build/objects.rs",
        "crates/scriptw-runtime-image/src/lib.rs",
        "crates/scriptw-intl/src/lib.rs",
        "crates/scriptw-intl-build-data/Cargo.toml",
        "crates/scriptw-intl-build-data/src/lib.rs",
        "crates/scriptw-engine/src/wasmtime_policy.rs",
        "crates/scriptw-engine/src/wasmtime_config.rs",
        "crates/scriptw-engine/src/wasm_native_function_compiler.rs",
        "crates/scriptw-engine/src/wasm_native_function_compiler/factory.rs",
        "crates/scriptw-engine/src/wasm_native_function_compiler/sources.rs",
        "crates/scriptw-engine/src/wasm_native_global_script_compiler.rs",
        "crates/scriptw-engine/src/wasm_native_global_script_compiler/factory.rs",
        "crates/scriptw-engine/src/wasm_native_global_script_compiler/sources.rs",
        "crates/scriptw-engine/src/wasm_native_direct_eval_compiler.rs",
        "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/factory.rs",
        "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/sources.rs",
        "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/sources/private_chain.rs",
        "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/sources/invocation.rs",
        "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/signature.rs",
        "crates/scriptw-engine/src/embedded_runtime/build.rs",
        "crates/scriptw-engine/src/embedded_runtime/compile.rs",
        "crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile.rs",
        "crates/scriptw-aot-wasm/src/runtime_artifact/cache.rs",
        "crates/scriptw-aot-wasm/src/runtime_artifact/build_package.rs",
        "crates/scriptw-engine/src/wasm_native_source_compiler.rs",
        "crates/scriptw-engine/src/cache/native.rs",
        "crates/scriptw-engine/src/embedded_runtime/manifest.rs",
        "vendor/boa_parser-0.21.1/src/lib.rs",
        "vendor/temporal_rs-0.1.0/src/lib.rs",
        "crates/scriptw-front/build.rs",
        "crates/scriptw-aot-wasm/src/embedded.json",
    ] {
        let workspace = Workspace::new();
        workspace.write(input, "before");
        let before = workspace.digest();
        workspace.write(input, "after");
        assert_ne!(before, workspace.digest(), "untracked input: {input}");
    }
}

#[test]
fn native_dependency_identity_invalidates_for_lockfile_features_profiles_and_patches() {
    for input in [
        "Cargo.lock",
        "Cargo.toml",
        "crates/scriptw-engine/Cargo.toml",
        "crates/scriptw-runtime-image/Cargo.toml",
        "vendor/wasmtime-47.0.0/src/compile/code_builder.rs",
        "vendor/wasmtime-internal-cache-47.0.0/src/lib.rs",
    ] {
        let workspace = Workspace::new();
        workspace.write(input, "before");
        let before = native_fingerprint(&workspace.0).unwrap();
        workspace.write(input, "after");
        assert_ne!(before, native_fingerprint(&workspace.0).unwrap(), "{input}");
    }
}

#[test]
fn native_dependency_identity_preserves_scriptw_source_invalidation_without_native_cache_churn() {
    let workspace = Workspace::new();
    workspace.write("crates/scriptw-ir/src/source.rs", "before");
    let native_before = native_fingerprint(&workspace.0).unwrap();
    let compiler_before = workspace.digest();
    workspace.write("crates/scriptw-ir/src/source.rs", "after");
    assert_eq!(native_before, native_fingerprint(&workspace.0).unwrap());
    assert_ne!(compiler_before, workspace.digest());
    fs::remove_file(workspace.0.join("Cargo.lock")).unwrap();
    assert!(native_fingerprint(&workspace.0).is_err());
}

#[test]
fn fingerprints_are_checkout_location_and_creation_order_independent() {
    let first = Workspace::new();
    let second = Workspace::new();
    first.write("vendor/fixture/z.rs", "z");
    first.write("vendor/fixture/a.rs", "a");
    second.write("vendor/fixture/a.rs", "a");
    second.write("vendor/fixture/z.rs", "z");
    assert_eq!(first.digest(), second.digest());
}

#[test]
fn adding_removing_or_renaming_an_input_changes_the_fingerprint() {
    let workspace = Workspace::new();
    let empty = workspace.digest();
    workspace.write("vendor/fixture/a.rs", "same bytes");
    let added = workspace.digest();
    assert_ne!(empty, added);
    fs::rename(
        workspace.0.join("vendor/fixture/a.rs"),
        workspace.0.join("vendor/fixture/b.rs"),
    )
    .unwrap();
    assert_ne!(added, workspace.digest());
    fs::remove_file(workspace.0.join("vendor/fixture/b.rs")).unwrap();
    assert_eq!(empty, workspace.digest());
}

#[test]
fn generated_build_outputs_do_not_invalidate_source_fingerprints() {
    let workspace = Workspace::new();
    let before = workspace.digest();
    workspace.write("vendor/fixture/target/debug/generated.rs", "generated");
    workspace.write("vendor/fixture/.git/index", "administrative state");
    assert_eq!(before, workspace.digest());
}

#[test]
fn missing_required_inputs_fail_closed() {
    let workspace = Workspace::new();
    fs::remove_file(workspace.0.join("Cargo.lock")).unwrap();
    assert_eq!(
        fingerprint(&workspace.0).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}

#[test]
fn pure_test_input_lifecycle_preserves_production_identity_and_rerun_inputs() {
    for input in [
        "crates/scriptw-front/src/tests/fixture.rs",
        "crates/scriptw-front/src/json/tests.rs",
        "crates/scriptw-ir/src/ir/tests.rs",
        "crates/scriptw-ir/src/analysis/private_environment_tests.rs",
        "crates/scriptw-ir/src/runtime_global_script/tests.rs",
        "crates/scriptw-aot-wasm/src/tests.rs",
        "crates/scriptw-aot-wasm/src/tests/fixture.rs",
        "crates/scriptw-aot-wasm/src/planning/tests.rs",
        "crates/scriptw-aot-wasm/src/control_flow/tests.rs",
        "crates/scriptw-runtime/src/rooted_snapshot/tests.rs",
        "crates/scriptw-engine/src/tests/fixture.rs",
        "crates/scriptw-front/tests/fixture.js",
        "crates/scriptw-ir/tests/regression.rs",
        "crates/scriptw-ir/src/tests/inspection.rs",
        "crates/scriptw-aot-wasm/tests/unit/regression.rs",
        "crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile/tests.rs",
        "crates/scriptw-aot-wasm/src/runtime_artifact/build_package/tests.rs",
        "crates/scriptw-aot-wasm/src/runtime_artifact/cache/tests.rs",
        "crates/scriptw-runtime/tests/regression.rs",
        "crates/scriptw-intl/tests/regression.rs",
        "crates/scriptw-engine/src/embedded_runtime/compile_tests.rs",
        "crates/scriptw-engine/src/embedded_runtime/tests.rs",
        "crates/scriptw-engine/src/wasm_runtime_link/tests.rs",
        "crates/scriptw-engine/src/wasm_compilation_policy/tests.rs",
        "crates/scriptw-engine/src/tests.rs",
        "crates/scriptw-engine/src/regexp_program_boundary_tests.rs",
        "crates/scriptw-engine/src/runtime_regexp_compiler_tests.rs",
        "crates/scriptw-engine/src/array_present_index_tests.rs",
        "crates/scriptw-engine/src/module_async_runtime_tests.rs",
        "crates/scriptw-engine/src/wasm_native_source_compiler/tests.rs",
        "crates/scriptw-engine/src/native_function_compiler_tests.rs",
        "crates/scriptw-engine/src/native_function_compiler_tests/program_ownership.rs",
        "crates/scriptw-engine/src/native_function_compiler_tests/realm_and_order.rs",
        "crates/scriptw-engine/src/wasm_native_function_compiler/observer.rs",
        "crates/scriptw-engine/src/wasm_native_function_compiler/tests.rs",
        "crates/scriptw-engine/src/wasm_native_function_compiler/tests/fixture.rs",
        "crates/scriptw-engine/src/native_global_script_compiler_tests.rs",
        "crates/scriptw-engine/src/native_global_script_compiler_tests/case.rs",
        "crates/scriptw-engine/src/wasm_native_global_script_compiler/observer.rs",
        "crates/scriptw-engine/src/wasm_native_global_script_compiler/tests.rs",
        "crates/scriptw-engine/src/wasm_native_global_script_compiler/tests/fixture.rs",
        "crates/scriptw-engine/src/native_direct_eval_compiler_tests.rs",
        "crates/scriptw-engine/src/native_direct_eval_compiler_tests/case.rs",
        "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/observer.rs",
        "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests.rs",
        "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests/fixture.rs",
        "crates/scriptw-engine/src/native_language_feature_policy_tests.rs",
    ] {
        let workspace = Workspace::new();
        let before = workspace.digest();
        let native_before = native_fingerprint(&workspace.0).unwrap();
        let rerun_before = compiler_rerun_inputs(&workspace.0).unwrap();
        for contents in ["created", "modified"] {
            workspace.write(input, contents);
            assert_eq!(before, workspace.digest(), "{input}");
            assert_eq!(
                native_before,
                native_fingerprint(&workspace.0).unwrap(),
                "{input}"
            );
            assert_eq!(
                rerun_before,
                compiler_rerun_inputs(&workspace.0).unwrap(),
                "{input}"
            );
        }
        fs::remove_file(workspace.0.join(input)).unwrap();
        assert_eq!(before, workspace.digest(), "{input}");
        assert_eq!(
            rerun_before,
            compiler_rerun_inputs(&workspace.0).unwrap(),
            "{input}"
        );
    }
}

#[test]
fn production_sources_and_manifests_are_watched_and_still_invalidate() {
    for input in [
        "Cargo.toml",
        "Cargo.lock",
        "crates/scriptw-ir/Cargo.toml",
        "crates/scriptw-ir/src/lib.rs",
        "crates/scriptw-engine/src/embedded_runtime/compile.rs",
        "crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile.rs",
        "crates/scriptw-aot-wasm/src/runtime_artifact/cache.rs",
        "crates/scriptw-aot-wasm/src/runtime_artifact/build_package.rs",
        "crates/scriptw-engine/src/wasm_native_source_compiler.rs",
    ] {
        let workspace = Workspace::new();
        workspace.write(input, "before");
        let path = workspace.0.join(input);
        let watched = compiler_rerun_inputs(&workspace.0).unwrap();
        assert!(
            watched.iter().any(|watch| path.starts_with(watch)),
            "{input}"
        );
        let before = workspace.digest();
        workspace.write(input, "after");
        assert_ne!(before, workspace.digest(), "{input}");
    }
}

#[test]
fn production_resource_subtrees_watch_added_removed_and_renamed_inputs() {
    let workspace = Workspace::new();
    workspace.write("crates/scriptw-intl/data/profile.json", "initial");
    let data = workspace.0.join("crates/scriptw-intl/data");
    let watched = compiler_rerun_inputs(&workspace.0).unwrap();
    assert!(watched.contains(&data));
    assert!(!watched.contains(&workspace.0.join("crates/scriptw-intl")));
    let before = workspace.digest();
    let added_path = data.join("added.json");
    assert!(watched.iter().any(|watch| added_path.starts_with(watch)));
    workspace.write("crates/scriptw-intl/data/added.json", "new resource");
    let added = workspace.digest();
    assert_ne!(before, added);
    fs::rename(&added_path, data.join("renamed.json")).unwrap();
    assert_ne!(added, workspace.digest());
    fs::remove_file(data.join("renamed.json")).unwrap();
    assert_eq!(before, workspace.digest());
    assert_eq!(watched, compiler_rerun_inputs(&workspace.0).unwrap());
}

#[test]
fn new_modules_in_mixed_source_roots_rescan_through_watched_declarations() {
    let workspace = Workspace::new();
    workspace.write("crates/scriptw-ir/src/lib.rs", "mod existing;");
    workspace.write("crates/scriptw-ir/src/existing.rs", "before");
    workspace.write("crates/scriptw-ir/src/tests/inspection.rs", "test only");
    let watched = compiler_rerun_inputs(&workspace.0).unwrap();
    let parent = workspace.0.join("crates/scriptw-ir/src/lib.rs");
    assert!(watched.contains(&parent));
    assert!(!watched.contains(&workspace.0.join("crates/scriptw-ir/src")));
    let before = workspace.digest();
    workspace.write("crates/scriptw-ir/src/lib.rs", "mod existing; mod added;");
    workspace.write("crates/scriptw-ir/src/added.rs", "new reachable source");
    assert_ne!(before, workspace.digest());
    assert!(compiler_rerun_inputs(&workspace.0)
        .unwrap()
        .contains(&workspace.0.join("crates/scriptw-ir/src/added.rs")));
}

#[test]
fn test_like_paths_outside_the_explicit_project_exclusions_stay_conservative() {
    for input in [
        "vendor/fixture/tests/embedded.js",
        "crates/scriptw-ir/src/testsupport.rs",
        "crates/scriptw-ir/src/tests-adjacent/resource.json",
        "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests-adjacent/resource.json",
        "crates/scriptw-intl/data/tests/resource.json",
        "crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile/tests-adjacent/resource.json",
        "crates/scriptw-engine/src/wasm_native_source_compiler/tests-adjacent/resource.json",
    ] {
        let workspace = Workspace::new();
        let before = workspace.digest();
        workspace.write(input, "production or conservative source input");
        assert_ne!(before, workspace.digest(), "{input}");
        let path = workspace.0.join(input);
        assert!(
            compiler_rerun_inputs(&workspace.0)
                .unwrap()
                .iter()
                .any(|watch| path.starts_with(watch)),
            "{input}"
        );
    }
}
