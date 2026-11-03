//! Run the publisher's complete fake-suite measurement in its own process.
//!
//! The shared CLI fixture target runs eight tests concurrently. Its former
//! publication test timed out after 900 seconds at 90/191 fake executions,
//! then advanced to 120 passing executions before the test process exited.
//! Individual cases took up to 125 seconds there; the same full selection
//! passed through the standalone product CLI in 540 seconds. A separate Cargo
//! target removes those competing compiles while preserving every measured
//! execution and the existing 900-second termination bound.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PUBLICATION_TIMEOUT: Duration = Duration::from_secs(900);

#[test]
fn compiler_identity_command_reports_the_loaded_image_and_rejects_extra_arguments() {
    let output = scriptw_cli::run_cli_capture(["compiler-identity"]);
    assert_eq!(output.exit_code, 0, "{:?}", output.stderr);
    let identity: scriptw_test262::CompilerProvenance =
        serde_json::from_slice(&output.stdout).expect("mandatory checked compiler identity");
    assert_eq!(
        identity,
        scriptw_test262::CompilerProvenance::current().unwrap()
    );
    let rejected = scriptw_cli::run_cli_capture(["compiler-identity", "unexpected"]);
    assert_eq!(rejected.exit_code, 1);
    assert!(rejected.stdout.is_empty());
}

fn fixture_root() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "scriptw-fake-publication-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&path).expect("fresh publication fixture directory should be created");
    path
}

fn bounded_publication(command: &mut Command, root: &Path) -> Output {
    let stdout_path = root.join("stdout.log");
    let stderr_path = root.join("stderr.log");
    let mut child = command
        .stdin(Stdio::null())
        .stdout(fs::File::create(&stdout_path).expect("publication stdout file should be created"))
        .stderr(fs::File::create(&stderr_path).expect("publication stderr file should be created"))
        .spawn()
        .expect("product publisher should start");
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < PUBLICATION_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(50));
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "publication CLI did not finish within {PUBLICATION_TIMEOUT:?}: {result:?}; \
                     logs retained in {}\nstdout: {}\nstderr: {}",
                    root.display(),
                    fs::read_to_string(&stdout_path).unwrap_or_default(),
                    fs::read_to_string(&stderr_path).unwrap_or_default(),
                );
            }
        }
    };
    Output {
        status,
        stdout: fs::read(stdout_path).expect("publication stdout should be readable"),
        stderr: fs::read(stderr_path).expect("publication stderr should be readable"),
    }
}

#[test]
fn test262_publish_status_supports_wasm_backend() {
    let root = fixture_root();
    let readme_path = root.join("README.md");
    let suite_root = root.join("suite");
    let snapshot_dir = root.join("snapshots");
    fs::write(
        &readme_path,
        "# Scriptw\n\n## Current Status\n<!-- scriptw-status:start -->\nold status\n<!-- scriptw-status:end -->\n\n## Design\nstill here\n",
    )
    .expect("fixture README should write");
    let test_dir = suite_root.join("test/language/wasm/pass");
    fs::create_dir_all(&test_dir).expect("tiny real-matrix fixture should be created");
    fs::write(
        test_dir.join("publish-status-wasm.js"),
        "/*---\nflags: [raw]\n---*/\n\n1 + 2;\n",
    )
    .expect("tiny matrix case should write");
    // Only the requested real matrix is tiny. The publisher still measures its
    // entire canonical fake fixture suite before writing publication artifacts.
    let output = bounded_publication(
        Command::new(env!("CARGO_BIN_EXE_scriptw"))
            .arg("--jobs")
            .arg("1")
            .arg("test262")
            .arg("publish-status")
            .arg("--suite-root")
            .arg(&suite_root)
            .arg("--snapshot-dir")
            .arg(&snapshot_dir)
            .arg("--snapshot-name")
            .arg("cli-publish-status-wasm")
            .arg("--execution-backend")
            .arg("wasm")
            .arg("--readme-path")
            .arg(&readme_path)
            .env("SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES", "4")
            .env_remove("SCRIPTW_TEST262_FORCE_CASE_RUNNER")
            .env_remove("SCRIPTW_TEST262_DISABLE_CASE_RUNNER"),
        &root,
    );

    assert!(
        output.status.success(),
        "publication failed; logs retained in {}\nstdout: {}\nstderr: {}",
        root.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("execution_backend: wasm-aot"));
    assert!(stdout.contains("total: 1"));
    assert!(stdout.contains("passed: 1"));
    assert!(stdout.contains("outcome_Success: 1"));
    assert!(stdout.contains("status_json:"));
    assert!(stdout.contains("status_txt:"));
    assert!(stdout.contains("snapshot_json:"));
    assert!(stdout.contains("snapshot_txt:"));

    let readme = fs::read_to_string(&readme_path).expect("wasm README should read");
    assert!(readme.contains("Published Test262 (`wasm-aot`"));
    assert!(readme.contains("): `1/1` passed; `0` failed."));
    assert!(
        readme.contains("Native-host conformance includes cases unsupported by standalone Wasm")
    );
    assert!(readme.contains("docs/rust-rewrite/test262-current.md"));
    assert!(readme.contains("CONTRIBUTING.md#test262"));
    assert!(!readme.contains("Fake full Rust rewrite suite"));
    assert!(!readme.contains("Status refresh commands:"));

    let status_json_line = stdout
        .lines()
        .find(|line| line.starts_with("status_json: "))
        .expect("stdout should include status_json path");
    let status_json_path = status_json_line
        .strip_prefix("status_json: ")
        .expect("status_json line should have prefix");
    assert!(Path::new(status_json_path).exists());

    let status_txt_line = stdout
        .lines()
        .find(|line| line.starts_with("status_txt: "))
        .expect("stdout should include status_txt path");
    let status_txt_path = status_txt_line
        .strip_prefix("status_txt: ")
        .expect("status_txt line should have prefix");
    assert!(Path::new(status_txt_path).exists());

    let status_json = fs::read_to_string(status_json_path).expect("status JSON should read");
    let status: serde_json::Value =
        serde_json::from_str(&status_json).expect("status JSON should parse");
    let identity_output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("compiler-identity")
        .output()
        .expect("the same publication compiler reports its identity");
    assert!(identity_output.status.success());
    let compiler: serde_json::Value = serde_json::from_slice(&identity_output.stdout).unwrap();
    let checked: scriptw_test262::CompilerProvenance =
        serde_json::from_value(compiler.clone()).expect("complete native producer identity");
    assert_eq!(status["real_suite"]["compiler_identity"], compiler);
    let snapshot_path = stdout
        .lines()
        .find_map(|line| line.strip_prefix("snapshot_json: "))
        .unwrap();
    let snapshot: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(snapshot_path).unwrap()).unwrap();
    assert_eq!(snapshot["compiler_identity"], compiler);
    assert_eq!(snapshot["snapshot_version"], 8);
    assert!(readme.contains(&checked.identity().source_fingerprint().to_string()));
    assert!(readme.contains(&checked.identity().executable_sha256().to_string()));
    assert_eq!(status["producer"], "scriptw");
    assert_eq!(status["real_suite"]["backend"], "wasm-aot");
    assert_eq!(status["real_suite"]["total"], 1);
    assert_eq!(status["real_suite"]["passed"], 1);
    assert_eq!(status["fake_full"]["total"], 191);
    assert_eq!(status["fake_full"]["passed"], 191);
    assert_eq!(status["fake_wasm_safe"]["total"], 187);
    assert_eq!(status["fake_wasm_safe"]["passed"], 187);
    let status_text = fs::read_to_string(status_txt_path).expect("status text should read");
    let manifest_path = stdout
        .lines()
        .find_map(|line| line.strip_prefix("status_manifest: "))
        .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["backend"], "wasm-aot");
    use sha2::{Digest, Sha256};
    let mut generation = Sha256::new();
    generation.update(b"scriptw-status-generation-v1\0");
    for (field, path, bytes) in [
        ("json", status_json_path, status_json.as_bytes()),
        ("text", status_txt_path, status_text.as_bytes()),
    ] {
        assert_eq!(
            manifest[field]["file"],
            Path::new(path).file_name().unwrap().to_str().unwrap()
        );
        assert_eq!(
            manifest[field]["sha256"],
            format!("{:x}", Sha256::digest(bytes))
        );
        generation.update((bytes.len() as u64).to_le_bytes());
        generation.update(bytes);
    }
    assert_eq!(
        manifest["generation"],
        format!("{:x}", generation.finalize())
    );
    let text_identity = status_text
        .lines()
        .find_map(|line| line.strip_prefix("compiler_identity="))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(text_identity).unwrap(),
        compiler
    );
    assert!(status_text.contains("fake_full=191/191"));
    assert!(status_text.contains("fake_wasm_safe=187/187"));
    fs::remove_dir_all(root).expect("successful publication fixture should clean up");
}
