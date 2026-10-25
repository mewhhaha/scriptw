//! Inputs that can change the emitted program-Wasm cache entry.
//!
//! A Cargo.lock checksum does not cover an edited path dependency or a
//! [patch.crates-io] source. Keep those bytes in the fingerprint too. Include
//! non-Rust resources: include_str!/include_bytes! are compiler inputs, not
//! merely documentation. Cache invalidation is deliberately conservative.

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) const FINGERPRINT_SCHEME: &str = "scriptw-program-cache-compiler-v3";
pub(crate) const NATIVE_FINGERPRINT_SCHEME: &str = "scriptw-native-compiler-v1";

// Native codegen dependencies have a separate identity from Scriptw's complete
// program compiler. Cargo.lock binds registry dependencies; all vendored source
// binds patched dependencies. Feature/profile manifests also invalidate it.
// This does not replace or narrow COMPILER_INPUTS or raw R admission.
pub(crate) const NATIVE_COMPILER_INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "vendor",
    "crates/scriptw-engine/Cargo.toml",
    "crates/scriptw-runtime-image/Cargo.toml",
];

pub(crate) const COMPILER_INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "crates/scriptw-front",
    "crates/scriptw-ir",
    "crates/scriptw-aot-wasm",
    "crates/scriptw-runtime",
    "crates/scriptw-runtime-image", // Cargo producer and immutable image accessors.
    "crates/scriptw-intl",
    "crates/scriptw-intl-build-data", // Opaque immutable Cargo Intl trust token.
    "crates/scriptw-engine/Cargo.toml",
    "crates/scriptw-engine/src",
    "crates/scriptw-engine/build.rs",
    "crates/scriptw-engine/compiler_fingerprint.rs",
    "vendor",
];

// These exact project paths are outside the product dependency closure: Cargo
// integration tests, the IR's cfg(test) module tree, and explicit cfg(test)-only
// AOT/Engine modules. Do not generalize this to every path named "tests": vendored
// sources and production resources retain conservative coverage.
const TEST_ONLY_INPUTS: &[&str] = &[
    "crates/scriptw-front/src/tests",
    "crates/scriptw-front/src/json/tests.rs",
    "crates/scriptw-ir/src/ir/tests.rs",
    "crates/scriptw-ir/src/analysis/private_environment_tests.rs",
    "crates/scriptw-ir/src/runtime_global_script/tests.rs",
    "crates/scriptw-aot-wasm/src/tests.rs",
    "crates/scriptw-aot-wasm/src/tests",
    "crates/scriptw-aot-wasm/src/planning/tests.rs",
    "crates/scriptw-aot-wasm/src/control_flow/tests.rs",
    "crates/scriptw-runtime/src/rooted_snapshot/tests.rs",
    "crates/scriptw-engine/src/tests",
    "crates/scriptw-front/tests",
    "crates/scriptw-ir/tests",
    "crates/scriptw-ir/src/tests",
    "crates/scriptw-aot-wasm/tests",
    "crates/scriptw-aot-wasm/src/runtime_artifact/build_package/tests.rs",
    "crates/scriptw-aot-wasm/src/runtime_artifact/cache/tests.rs",
    "crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile/tests.rs",
    "crates/scriptw-runtime/tests",
    "crates/scriptw-intl/tests",
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
    "crates/scriptw-engine/src/native_function_compiler_tests",
    "crates/scriptw-engine/src/wasm_native_function_compiler/observer.rs",
    "crates/scriptw-engine/src/wasm_native_function_compiler/tests.rs",
    "crates/scriptw-engine/src/wasm_native_function_compiler/tests",
    "crates/scriptw-engine/src/native_global_script_compiler_tests.rs",
    "crates/scriptw-engine/src/native_global_script_compiler_tests",
    "crates/scriptw-engine/src/wasm_native_global_script_compiler/observer.rs",
    "crates/scriptw-engine/src/wasm_native_global_script_compiler/tests.rs",
    "crates/scriptw-engine/src/wasm_native_global_script_compiler/tests",
    "crates/scriptw-engine/src/native_direct_eval_compiler_tests.rs",
    "crates/scriptw-engine/src/native_direct_eval_compiler_tests",
    "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/observer.rs",
    "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests.rs",
    "crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests",
    "crates/scriptw-engine/src/native_language_feature_policy_tests.rs",
];

pub(crate) fn fingerprint(workspace: &Path) -> io::Result<String> {
    let files = input_files(workspace, COMPILER_INPUTS, TEST_ONLY_INPUTS)?;
    fingerprint_files(workspace, FINGERPRINT_SCHEME, files)
}

/// Shared target directories can retain a build-script executable whose module
/// dependency paths refer to another checkout. Reject its stale input inventory
/// before it can stamp a compiler or runtime image with a different identity.
pub(crate) fn verify_implementation(workspace: &Path) -> io::Result<()> {
    let current = fs::read(workspace.join("crates/scriptw-engine/compiler_fingerprint.rs"))?;
    if current != include_bytes!("compiler_fingerprint.rs") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stale compiler fingerprint build script; rebuild its Cargo build-script artifact",
        ));
    }
    Ok(())
}

// Cargo recursively watches directory rerun inputs. Descend mixed source/test
// roots instead of watching their ancestor; keep wholly production subtrees
// recursive so adding or removing data files still triggers fingerprinting.
// A new module in a mixed Rust root also edits a watched module declaration.
pub(crate) fn compiler_rerun_inputs(workspace: &Path) -> io::Result<Vec<PathBuf>> {
    let mut inputs = Vec::new();
    for input in COMPILER_INPUTS {
        collect_rerun_inputs(workspace, &workspace.join(input), &mut inputs)?;
    }
    inputs.sort();
    inputs.dedup();
    Ok(inputs)
}

pub(crate) fn native_fingerprint(workspace: &Path) -> io::Result<String> {
    let files = input_files(workspace, NATIVE_COMPILER_INPUTS, &[])?;
    fingerprint_files(workspace, NATIVE_FINGERPRINT_SCHEME, files)
}

fn input_files(workspace: &Path, inputs: &[&str], excluded: &[&str]) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        collect_files(workspace, &workspace.join(input), excluded, &mut files)?;
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn fingerprint_files(workspace: &Path, scheme: &str, files: Vec<PathBuf>) -> io::Result<String> {
    let mut digest = Sha256::new();
    digest.update(scheme.as_bytes());
    for path in files {
        let relative = path.strip_prefix(workspace).map_err(io::Error::other)?;
        let label = relative
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        let bytes = fs::read(&path)?;
        digest.update((label.len() as u64).to_le_bytes());
        digest.update(label.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn excluded_input(workspace: &Path, path: &Path, excluded: &[&str]) -> bool {
    excluded
        .iter()
        .any(|input| path.starts_with(workspace.join(input)))
}

fn collect_files(
    workspace: &Path,
    path: &Path,
    excluded: &[&str],
    output: &mut Vec<PathBuf>,
) -> io::Result<()> {
    if excluded_input(workspace, path, excluded) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_file() {
        output.push(path.to_path_buf());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "compiler fingerprint input is not a regular file or directory: {}",
                path.display()
            ),
        ));
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        // A local rebuild of a vendored/path crate must not hash its own
        // output recursively. Git administrative state is not source input.
        if entry.file_type()?.is_dir()
            && (entry.file_name() == "target" || entry.file_name() == ".git")
        {
            continue;
        }
        collect_files(workspace, &entry.path(), excluded, output)?;
    }
    Ok(())
}

fn collect_rerun_inputs(
    workspace: &Path,
    path: &Path,
    output: &mut Vec<PathBuf>,
) -> io::Result<()> {
    if excluded_input(workspace, path, TEST_ONLY_INPUTS) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_file()
        || (metadata.is_dir()
            && !TEST_ONLY_INPUTS
                .iter()
                .any(|input| workspace.join(input).starts_with(path)))
    {
        output.push(path.to_path_buf());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "compiler rerun input is not a regular file or directory: {}",
                path.display()
            ),
        ));
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && (entry.file_name() == "target" || entry.file_name() == ".git")
        {
            continue;
        }
        collect_rerun_inputs(workspace, &entry.path(), output)?;
    }
    Ok(())
}
