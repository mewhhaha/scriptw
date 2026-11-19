//! The consumer executes an image from the documented producer: the shared
//! host's `prepare-scriptw` code, built from the same compiler source.
//! Images bind that source fingerprint, so a stale producer's image is
//! refused at admission with both fingerprints named.
#![allow(dead_code)]

#[path = "../../scriptw-cli/examples/runtime_precompiled/produce.rs"]
mod produce;

include!("../../scriptw-cli/examples/runtime_precompiled/shared.rs");

use std::process::Command;

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../scriptw-cli/tests/fixtures/v8_execution_hotpaths"
);
const FIXTURE: &str = "integer.js";
/// Envelope offsets: an 8-byte magic, then the producer's source fingerprint.
const FINGERPRINT: std::ops::Range<usize> = 8..40;

struct Workspace(std::path::PathBuf);

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Produce the one fixture's image exactly as `compiler_precompiled_baseline
/// prepare-scriptw` does, in this test's own build of the same source.
fn produce_image(workspace: &Path) -> (Value, std::path::PathBuf) {
    let manifest: Value =
        serde_json::from_slice(&fs::read(Path::new(FIXTURES).join("manifest.json")).unwrap())
            .unwrap();
    let case = manifest["execution"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["file"] == FIXTURE)
        .unwrap()
        .clone();
    let fixtures = workspace.join("fixtures");
    fs::create_dir(&fixtures).unwrap();
    fs::copy(Path::new(FIXTURES).join(FIXTURE), fixtures.join(FIXTURE)).unwrap();
    fs::write(
        fixtures.join("manifest.json"),
        json!({"execution": [case]}).to_string(),
    )
    .unwrap();
    let output = workspace.join("images");
    // The shared host runs its producer on a worker of this stack size.
    let report = std::thread::Builder::new()
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || produce::run_with_config(&fixtures, &output, configure_producer))
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
    let output = workspace.join("images");
    let image = output.join(report["cases"][0]["image"].as_str().unwrap());
    (report, image)
}

fn consume(image: &Path, expected: &str, allocation: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_scriptw-precompiled-consumer"));
    command
        .args([
            "execute-scriptw",
            image.to_str().unwrap(),
            expected,
            "3",
            "1",
        ])
        .env_remove(STORE_ALLOCATION_VARIABLE);
    for (name, _) in std::env::vars() {
        if name.starts_with("SCRIPTW_PRECOMPILED_PHASE_") {
            command.env_remove(name);
        }
    }
    if let Some(allocation) = allocation {
        command.env(STORE_ALLOCATION_VARIABLE, allocation);
    }
    command.output().unwrap()
}

#[test]
fn the_consumer_executes_a_same_source_producer_image_and_refuses_a_stale_one() {
    let workspace = Workspace(std::env::temp_dir().join(format!(
        "scriptw-precompiled-consumer-e2e-{}",
        std::process::id()
    )));
    let _ = fs::remove_dir_all(&workspace.0);
    fs::create_dir_all(&workspace.0).unwrap();
    let (producer, image) = produce_image(&workspace.0);
    let expected = producer["cases"][0]["stdout"].as_str().unwrap().to_owned();
    let identity = identity().unwrap();
    assert_eq!(
        producer["source_fingerprint"],
        identity["source_fingerprint"]
    );

    for (allocation, strategy) in [(None, "pooled"), (Some("on-demand"), "on-demand")] {
        let output = consume(&image, &expected, allocation);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "consumer failed: {stderr}");
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["schema"], "scriptw-precompiled-execution-v1");
        assert_eq!(report["adapter"], "scriptw");
        assert_eq!(report["compiler_enabled"], false);
        assert_eq!(report["native_source_compiler_enabled"], false);
        assert_eq!(report["source_fingerprint"], producer["source_fingerprint"]);
        assert_eq!(report["image_sha256"], producer["cases"][0]["image_sha256"]);
        assert_eq!(report["runtime_key"], producer["cases"][0]["runtime_key"]);
        assert_eq!(report["store_allocation"]["strategy"], strategy);
        let observations = report["observations"].as_array().unwrap();
        assert_eq!(observations.len(), 4);
        for observation in observations {
            assert_eq!(observation["completion"], "normal");
            assert_eq!(observation["stdout"], expected.as_str());
        }
    }

    // A producer built from other source wrote another fingerprint. Re-seal
    // the checksum so admission reaches the fingerprint comparison.
    let mut stale = fs::read(&image).unwrap();
    stale[FINGERPRINT].fill(0x5a);
    let body = stale.len() - 32;
    let digest = Sha256::digest(&stale[..body]);
    stale[body..].copy_from_slice(&digest);
    let stale_path = workspace.0.join("stale.swpc");
    write_new(&stale_path, &stale).unwrap();
    let output = consume(&stale_path, &expected, None);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let current = identity["source_fingerprint"].as_str().unwrap();
    assert!(
        stderr.contains(&"5a".repeat(32)) && stderr.contains(current),
        "admission must name both fingerprints: {stderr}"
    );
}
