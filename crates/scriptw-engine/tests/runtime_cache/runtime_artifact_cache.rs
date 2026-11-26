//! Exercise the production disk-cache provider in separate Engine processes.

use scriptw_engine::{
    CompileOptions, CustomIntlProfile, CustomProfileId, Engine, ExecutionBackend,
    IntlCompilationProfile, ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder,
    RunOptions, SnapshotLimits, SnapshotOutcome,
};
use scriptw_runtime::rooted_snapshot::{SnapshotCompletionKind, SnapshotValue};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

const ROLE: &str = "SCRIPTW_RUNTIME_CACHE_PROCESS_TEST_ROLE";
const TEST: &str = "raw_runtime_artifacts_are_reused_by_fresh_engine_processes";
const NATIVE_TEST: &str = "native_runtime_is_reused_by_executables_with_different_mtimes";

// Keep these regressions on a real profile outside the embedded Minimal R.
// They must continue proving raw disk and Wasmtime native cache reuse.
fn unbundled_options() -> CompileOptions {
    let projection = CustomIntlProfile::new(
        CustomProfileId::parse("runtime-cache-regression").unwrap(),
        Some(&["en-US"]),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    CompileOptions {
        intl_profile: IntlCompilationProfile::CustomProjection(projection),
        ..CompileOptions::default()
    }
}

struct CacheDirectory(PathBuf);

impl Drop for CacheDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// libtest names a test by its path below the test crate root, and this file is
// a module of the consolidated `runtime_cache` target, so the re-executed child
// must be selected with `--exact <module>::<test>`.
fn libtest_path(test: &str) -> String {
    match module_path!().split_once("::") {
        Some((_, module)) => format!("{module}::{test}"),
        None => test.to_owned(),
    }
}

fn child(executable: &Path, test: &str, role: &str, directory: &Path) -> Output {
    let output = Command::new(executable)
        .args([
            "--exact",
            &libtest_path(test),
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ROLE, role)
        .env("SCRIPTW_CACHE_DIR", directory)
        .env("SCRIPTW_WASM_TRACE", "1")
        // This fixture exercises the default native configuration; the unit
        // compatibility control separately rejects nondefault valid images.
        .env_remove("WASMTIME_BACKTRACE_DETAILS")
        .output()
        .expect("fresh Engine test process starts");
    assert!(
        output.status.success(),
        "{role} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn result(output: &Output) -> (String, String) {
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let line = stdout
        .lines()
        .find_map(|line| {
            line.split_once("runtime-cache-fixture ")
                .map(|(_, result)| result)
        })
        .expect("child reports its actual emitted artifact digests");
    let (runtime, program) = line.split_once(' ').unwrap();
    (runtime.to_owned(), program.to_owned())
}

#[test]
fn raw_runtime_artifacts_are_reused_by_fresh_engine_processes() {
    if let Ok(role) = std::env::var(ROLE) {
        let engine = Engine::new(RealmBuilder::new().build());
        let source = match role.as_str() {
            "producer" => "const values = ['producer']; values.length;",
            "consumer" => "const values = ['consumer', 'different source']; values.length;",
            _ => panic!("unknown test role"),
        };
        let unit = engine.compile_script(source, unbundled_options()).unwrap();
        let artifact = engine.emit_wasm(&unit).expect("program and runtime emit");
        let runtime = artifact.runtime.as_ref().expect("heap program has R");
        println!(
            "runtime-cache-fixture {:x} {:x}",
            Sha256::digest(runtime.bytes()),
            Sha256::digest(&artifact.bytes)
        );
        return;
    }

    let directory = CacheDirectory(std::env::temp_dir().join(format!(
        "scriptw-runtime-process-cache-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    fs::create_dir(&directory.0).unwrap();
    let executable = std::env::current_exe().unwrap();
    let producer = child(&executable, TEST, "producer", &directory.0);
    let consumer = child(&executable, TEST, "consumer", &directory.0);
    let producer_trace = String::from_utf8_lossy(&producer.stderr);
    assert!(
        producer_trace.contains("runtime-cache miss:"),
        "{producer_trace}"
    );
    assert!(
        !producer_trace.contains("runtime-build-package hit:"),
        "disk-cache fixture must remain unbundled: {producer_trace}"
    );
    let consumer_trace = String::from_utf8_lossy(&consumer.stderr);
    assert!(
        consumer_trace.contains("runtime-cache hit:"),
        "{consumer_trace}"
    );
    assert!(
        !consumer_trace.contains("runtime-cache miss:"),
        "{consumer_trace}"
    );
    let (first_runtime, first_program) = result(&producer);
    let (second_runtime, second_program) = result(&consumer);
    assert_eq!(
        first_runtime, second_runtime,
        "fresh process reloads exact R bytes"
    );
    assert_ne!(
        first_program, second_program,
        "each process compiles its own source to P"
    );
}

fn executable_digest(path: &Path) -> [u8; 32] {
    let mut file = fs::File::open(path).expect("test executable is readable");
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).expect("test executable reads");
        if read == 0 {
            return hash.finalize().into();
        }
        hash.update(&buffer[..read]);
    }
}

fn assert_native_cache_counts(output: &Output, hits: usize, misses: usize) {
    let trace = String::from_utf8_lossy(&output.stderr);
    let counts: Vec<_> = trace
        .lines()
        .filter(|line| line.starts_with("scriptw wasm trace: module-cache "))
        .collect();
    assert_eq!(counts.len(), 1, "one native module-loading result: {trace}");
    assert!(
        counts[0].starts_with(&format!(
            "scriptw wasm trace: module-cache hits={hits} misses={misses} during "
        )),
        "expected native hits={hits} misses={misses}: {trace}"
    );
}

#[test]
fn native_runtime_is_reused_by_executables_with_different_mtimes() {
    if let Ok(role) = std::env::var(ROLE) {
        scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
        let engine = Engine::new(RealmBuilder::new().build());
        let (source, expected) = match role.as_str() {
            "producer" => ("const values = ['producer']; values.length;", 1.0),
            "consumer" => (
                "const values = ['consumer', 'different source']; values.length;",
                2.0,
            ),
            _ => panic!("unknown native test role"),
        };
        let unit = engine
            .compile_script(source, unbundled_options())
            .expect("native cache fixture compiles");
        let artifact = engine.emit_wasm(&unit).expect("program and runtime emit");
        let runtime = artifact.runtime.as_ref().expect("heap program has R");
        println!(
            "runtime-cache-fixture {:x} {:x}",
            Sha256::digest(runtime.bytes()),
            Sha256::digest(&artifact.bytes)
        );
        drop(artifact);
        drop(unit);

        let observed = engine
            .observe_script(
                source,
                unbundled_options(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("native cache fixture executes through Wasm AOT");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(expected))),
            "{role}: {observed:?}"
        );
        assert!(observed.output_events.is_empty(), "{role}: {observed:?}");
        return;
    }

    let directory = CacheDirectory(std::env::temp_dir().join(format!(
        "scriptw-native-runtime-process-cache-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    fs::create_dir(&directory.0).unwrap();
    let cache = directory.0.join("cache");
    let executable = std::env::current_exe().unwrap();
    let original_digest = executable_digest(&executable);
    let producer_executable = directory
        .0
        .join(format!("producer{}", std::env::consts::EXE_SUFFIX));
    let consumer_executable = directory
        .0
        .join(format!("consumer{}", std::env::consts::EXE_SUFFIX));

    // Different binaries built from the same Wasmtime release must share R.
    // Identical executable bytes keep Scriptw's compiler identity unchanged; only
    // the mtime used by Wasmtime's erroneous Git-build namespace is different.
    for (copy, seconds) in [(&producer_executable, 10), (&consumer_executable, 20)] {
        fs::copy(&executable, copy).expect("copy the test executable");
        fs::File::options()
            .write(true)
            .open(copy)
            .expect("copied executable permits timestamp changes")
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .expect("set deliberately different executable mtimes");
        assert_eq!(
            executable_digest(copy),
            original_digest,
            "binary contents must match the original test executable"
        );
    }
    assert_ne!(
        producer_executable.metadata().unwrap().modified().unwrap(),
        consumer_executable.metadata().unwrap().modified().unwrap(),
        "the filesystem must retain distinct executable mtimes"
    );

    let producer = child(&producer_executable, NATIVE_TEST, "producer", &cache);
    let consumer = child(&consumer_executable, NATIVE_TEST, "consumer", &cache);
    let (first_runtime, first_program) = result(&producer);
    let (second_runtime, second_program) = result(&consumer);
    assert_eq!(
        first_runtime, second_runtime,
        "both programs use identical R"
    );
    assert_ne!(first_program, second_program, "each source has its own P");
    assert_native_cache_counts(&producer, 0, 2);
    assert_native_cache_counts(&consumer, 1, 1);
    for output in [&producer, &consumer] {
        let trace = String::from_utf8_lossy(&output.stderr);
        assert!(
            !trace.contains("runtime-native-bundle hit:"),
            "native disk-cache fixture must remain unbundled: {trace}"
        );
    }
}

#[cfg(all(
    target_endian = "little",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )
))]
#[test]
fn embedded_runtime_executes_fresh_programs_without_a_native_runtime_compile() {
    const EMBEDDED_TEST: &str =
        "embedded_runtime_executes_fresh_programs_without_a_native_runtime_compile";
    if let Ok(role) = std::env::var(ROLE) {
        scriptw_engine::configure_compilation_jobs(1).unwrap();
        let engine = Engine::new(RealmBuilder::new().build());
        if role == "graph" {
            let observed = engine
                .observe_script_graph(
                    "const root = { value: 3 }; root.value;",
                    CompileOptions::default(),
                    RunOptions {
                        backend: ExecutionBackend::WasmAot,
                        ..RunOptions::default()
                    },
                    SnapshotLimits::default(),
                )
                .expect("rooted observation uses the same admitted embedded runtime");
            assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
            assert_eq!(observed.completion.kind, SnapshotCompletionKind::Normal);
            let SnapshotOutcome::Captured { graph } = observed.completion.outcome else {
                panic!("rooted completion was rejected");
            };
            assert_eq!(
                graph.root(),
                &SnapshotValue::Number {
                    bits: 3.0_f64.to_bits()
                }
            );
            assert!(observed.output_events.is_empty());
            return;
        }
        let (source, expected) = match role.as_str() {
            "producer" => ("const values = ['embedded first']; values.length;", 1.0),
            "consumer" => (
                "const values = ['embedded second', 'new P']; values.length;",
                2.0,
            ),
            _ => panic!("unknown embedded test role"),
        };
        let observed = engine
            .observe_script(
                source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("fresh linked program executes with embedded R");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(expected)))
        );
        assert!(observed.output_events.is_empty());
        return;
    }
    let directory = CacheDirectory(std::env::temp_dir().join(format!(
            "scriptw-embedded-runtime-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    fs::create_dir(&directory.0).unwrap();
    // R is mapped from this test executable's own file on Linux, whatever the
    // cache directory is, and copied from the linked bytes elsewhere.
    let backing = expected_native_backing();
    let executable = std::env::current_exe().unwrap();
    for role in ["producer", "consumer", "graph"] {
        let cache = directory.0.join(role);
        assert!(
            !cache.exists(),
            "each independent process starts with empty caches"
        );
        let output = child(&executable, EMBEDDED_TEST, role, &cache);
        let trace = String::from_utf8_lossy(&output.stderr);
        assert!(trace.contains("runtime-build-package hit:"), "{trace}");
        assert_embedded_compilation_modes(&trace);
        assert_native_backing(&trace, backing);
        assert!(
            !trace.contains("runtime-cache miss:"),
            "no raw R regeneration: {trace}"
        );
        assert_native_cache_counts(&output, 0, 1); // Only this fresh P compiles natively.
    }
    // A same-source hit skips compiler-package admission and attaches the
    // execution-only native runtime bundle directly.
    let cached = child(
        &executable,
        EMBEDDED_TEST,
        "producer",
        &directory.0.join("producer"),
    );
    let trace = String::from_utf8_lossy(&cached.stderr);
    assert!(trace.contains("runtime-native-bundle hit:"), "{trace}");
    assert!(
        !trace.contains("runtime-build-package hit:"),
        "warm program-cache hits skip compiler-package admission: {trace}"
    );
    assert_embedded_compilation_modes(&trace);
    assert_native_backing(&trace, backing);
    assert!(trace.contains("program-cache hit:"), "{trace}");
    assert_native_cache_counts(&cached, 1, 0); // Only P uses the native disk cache.
                                               // A regular file makes all cache directory creation fail. This is an
                                               // unavailable-cache control, with every configured budget left unchanged.
    let unavailable = directory.0.join("unavailable");
    fs::write(
        &unavailable,
        b"cache directories cannot be created under this file",
    )
    .unwrap();
    let output = child(&executable, EMBEDDED_TEST, "consumer", &unavailable);
    let trace = String::from_utf8_lossy(&output.stderr);
    assert!(trace.contains("runtime-build-package hit:"), "{trace}");
    assert_embedded_compilation_modes(&trace);
    // The image never touches the cache directory: an unavailable cache
    // leaves the executable mapping in place.
    assert_native_backing(&trace, backing);
    assert!(
        trace.contains("module-cache result unavailable:"),
        "{trace}"
    );
    // Nor is anything staged there: no process above left a native backing
    // directory in its fresh cache.
    for role in ["producer", "consumer", "graph"] {
        let mut pending = vec![directory.0.join(role)];
        while let Some(path) = pending.pop() {
            assert!(
                !path.ends_with("private-native-backing-v1"),
                "the embedded image must not be staged in the cache directory"
            );
            if path.is_dir() {
                pending.extend(
                    fs::read_dir(&path)
                        .unwrap()
                        .map(|entry| entry.unwrap().path()),
                );
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeBacking {
    ExecutableMapping,
    Copied,
}

/// The engine proves the image's range of the running executable through
/// procfs, which only Linux provides.
fn expected_native_backing() -> NativeBacking {
    if cfg!(target_os = "linux") && Path::new("/proc/self/maps").exists() {
        NativeBacking::ExecutableMapping
    } else {
        NativeBacking::Copied
    }
}

fn assert_native_backing(trace: &str, expected: NativeBacking) {
    let backing = match expected {
        NativeBacking::ExecutableMapping => "executable-mapping",
        NativeBacking::Copied => "copied",
    };
    assert!(
        trace.lines().any(|line| {
            line.contains("runtime-native-bundle hit:")
                && line.contains(&format!(", backing: {backing} ("))
        }),
        "R must load through the {backing} path: {trace}"
    );
}

#[cfg(all(
    target_endian = "little",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )
))]
fn assert_embedded_compilation_modes(trace: &str) {
    assert!(
        trace.lines().any(|line| {
            line.contains("runtime-native-bundle hit:")
                && line.ends_with("(runtime: size-optimized, execution: fast, source: NativeHost)")
        }),
        "the exact optimized R image must load in the Fast execution Engine: {trace}"
    );
    assert!(
        trace.contains("scriptw wasm trace: native compiler: fast ("),
        "the real program retains its original Fast compilation plan: {trace}"
    );
    assert!(
        !trace.contains("scriptw wasm trace: native compiler: size-optimized ("),
        "R image optimization must not change the program's plan: {trace}"
    );
}
