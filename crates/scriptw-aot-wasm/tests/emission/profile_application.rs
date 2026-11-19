//! Profile application over emitted P.
//!
//! `capture_profile_digests` is inert unless `SCRIPTW_PGO_CAPTURE_OUT` names a
//! directory. It records the SHA-256 of R and of every CLI fixture's P, so a
//! change to R's proof emitters can be shown to leave R and unprofiled P
//! byte-identical: run it before and after and `diff -r` the two directories.
use scriptw_aot_wasm::emit;
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::lower;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const CAPTURE_ENV: &str = "SCRIPTW_PGO_CAPTURE_OUT";
const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn fixture_paths() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../scriptw-cli/tests/fixtures")
        .canonicalize()
        .expect("CLI fixture corpus resolves");
    let mut paths = fs::read_dir(&root)
        .expect("CLI fixture corpus reads")
        .map(|entry| entry.expect("fixture entry reads").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "js"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

/// Runs `each` over every fixture on a few large-stack workers, in fixture
/// order.
fn over_fixtures<T: Send>(each: impl Fn(&str, &str) -> T + Sync) -> Vec<T> {
    let paths = fixture_paths();
    let workers = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(2)
        .min(paths.len());
    let each = &each;
    let mut results = std::thread::scope(|scope| {
        (0..workers)
            .map(|worker| {
                let paths = &paths;
                std::thread::Builder::new()
                    .stack_size(WORKER_STACK_BYTES)
                    .spawn_scoped(scope, move || {
                        paths
                            .iter()
                            .enumerate()
                            .filter(|(index, _)| index % workers == worker)
                            .map(|(index, path)| {
                                let name = path.file_name().unwrap().to_string_lossy();
                                let source = fs::read_to_string(path).unwrap();
                                (index, each(&name, &source))
                            })
                            .collect::<Vec<_>>()
                    })
                    .expect("fixture worker spawns")
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|worker| worker.join().expect("fixture worker completes"))
            .collect::<Vec<_>>()
    });
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

#[test]
fn capture_profile_digests() {
    let Some(output) = std::env::var_os(CAPTURE_ENV) else {
        return;
    };
    let output = PathBuf::from(output);
    fs::create_dir_all(&output).unwrap();
    let lines = over_fixtures(|name, source| {
        let Ok(unit) = parse(source, ParseOptions::script()) else {
            return (format!("{name} parse-error\n"), None);
        };
        match emit(&lower(&unit)) {
            Ok(artifact) => (
                format!(
                    "{name} bytes={} sha256={}\n",
                    artifact.bytes.len(),
                    hex(&artifact.bytes)
                ),
                artifact.runtime().map(|runtime| hex(runtime.bytes())),
            ),
            Err(error) => (format!("{name} emit-error {error}\n"), None),
        }
    });
    let mut runtimes = lines
        .iter()
        .filter_map(|(_, runtime)| runtime.clone())
        .collect::<Vec<_>>();
    runtimes.sort();
    runtimes.dedup();
    fs::write(output.join("runtime.sha256"), runtimes.join("\n") + "\n").unwrap();
    fs::write(
        output.join("programs.txt"),
        lines.into_iter().map(|(line, _)| line).collect::<String>(),
    )
    .unwrap();
}
