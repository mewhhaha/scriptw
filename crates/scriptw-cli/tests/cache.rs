use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct TestTree(PathBuf);

impl TestTree {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "scriptw-cache-boundary-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("cache test root should be created");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn retired_cache_variable() -> String {
    ["POR", "FFOR_CACHE_DIR"].concat()
}

fn retired_default_cache_name() -> String {
    ["por", "ffor"].concat()
}

fn write_marker(path: &Path) {
    fs::create_dir_all(path.parent().expect("marker should have a parent"))
        .expect("marker parent should be created");
    fs::write(path, b"keep").expect("marker should be written");
}

fn run_cache_command(xdg_cache: &Path, retired_override: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_scriptw"))
        .arg("cache")
        .args(args)
        .env("XDG_CACHE_HOME", xdg_cache)
        .env(retired_cache_variable(), retired_override)
        .env_remove("SCRIPTW_CACHE_DIR")
        .output()
        .expect("cache command should run");
    assert!(
        output.status.success(),
        "cache command failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("cache output should be UTF-8")
}

fn reported_path(output: &str, label: &str) -> PathBuf {
    let prefix = format!("{label}-path: ");
    output
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("cache status did not report {label}"))
}

#[test]
fn cache_identity_boundary_prunes_only_the_selected_scriptw_roots() {
    let tree = TestTree::new();
    let xdg_cache = tree.path().join("xdg");
    let retired_override = tree.path().join("retired-override");
    let retired_default = xdg_cache
        .join(retired_default_cache_name())
        .join("old-entry");
    let overridden = retired_override.join("old-entry");
    let sibling = xdg_cache.join("sibling").join("entry");
    let global_wasmtime = xdg_cache.join("wasmtime").join("entry");
    for marker in [&retired_default, &overridden, &sibling, &global_wasmtime] {
        write_marker(marker);
    }

    let status = run_cache_command(&xdg_cache, &retired_override, &["status"]);
    let scriptw_root = xdg_cache.join("scriptw");
    let function = reported_path(&status, "function-cache");
    let module = reported_path(&status, "module-cache");
    let build_module = reported_path(&status, "build-module-cache");
    let program = reported_path(&status, "program-cache");
    let reported_wasmtime = reported_path(&status, "legacy-wasmtime-cache");
    assert_ne!(module, build_module);
    for path in [&function, &module, &build_module, &program] {
        assert_eq!(path.parent(), Some(scriptw_root.as_path()));
    }
    assert_eq!(reported_wasmtime, xdg_cache.join("wasmtime"));

    let scriptw_markers = [
        function.join("entry"),
        module.join("entry"),
        build_module.join("entry"),
        program.join("entry"),
    ];
    for marker in &scriptw_markers {
        write_marker(marker);
    }

    let prune = run_cache_command(&xdg_cache, &retired_override, &["prune"]);
    assert!(prune.contains("scriptw-files-removed: 4"));
    assert!(prune.contains("legacy-files-removed: 0"));
    assert!(prune.contains("legacy-wasmtime-cache: retained"));
    assert!(scriptw_markers.iter().all(|marker| !marker.exists()));
    assert!(retired_default.exists());
    assert!(overridden.exists());
    assert!(sibling.exists());
    assert!(global_wasmtime.exists());

    let explicit = run_cache_command(
        &xdg_cache,
        &retired_override,
        &["prune", "--legacy-wasmtime"],
    );
    assert!(explicit.contains("legacy-files-removed: 1"));
    assert!(!global_wasmtime.exists());
    assert!(retired_default.exists());
    assert!(overridden.exists());
    assert!(sibling.exists());
}
