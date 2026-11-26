use std::fs::{self, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use scriptw_engine::{
    wasm_aot_script_is_cached, CompileOptions, Engine, ExecutionBackend, HostClock,
    HostOutputEvent, HostSurfacePolicy, MonotonicClockInstant, ObservedCompletion, ObservedJsValue,
    ObservedNumber, ObservedRunOutcome, Realm, RealmBuilder, RunOptions, UtcEpochMilliseconds,
};
use scriptw_intl::ConfiguredSystemTimeZone;

const COLD_EXECUTION_TIMEOUT_MS: u64 = 120_000;

struct FixedClock(AtomicU64);

impl HostClock for FixedClock {
    fn utc_epoch_milliseconds(&self) -> UtcEpochMilliseconds {
        UtcEpochMilliseconds::new(1234).expect("fixed valid Date clock")
    }

    fn monotonic_instant(&self) -> MonotonicClockInstant {
        MonotonicClockInstant::new(self.0.fetch_add(1, Ordering::Relaxed))
    }
}

fn configured_realm(identifier: &str) -> Realm {
    RealmBuilder::new()
        .with_system_time_zone(
            ConfiguredSystemTimeZone::resolve(identifier).expect("control zone must be admitted"),
        )
        .with_host_clock(Box::new(FixedClock(AtomicU64::new(0))))
        .build()
}

fn options() -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    }
}

fn run_options() -> RunOptions {
    RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(COLD_EXECUTION_TIMEOUT_MS),
        ..RunOptions::default()
    }
}

fn assert_observation(observation: ObservedRunOutcome, output: &str, source: &str) {
    assert_eq!(observation.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        observation.completion,
        ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0))),
        "{source}",
    );
    assert_eq!(
        observation.output_events,
        vec![HostOutputEvent::PrintLine(output.into())],
        "{source}",
    );
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

const TEST_NAME: &str = "date_cached_source_and_artifact_use_each_executing_realms_choice";
const CHILD_CACHE_ROOT_ENV: &str = "SCRIPTW_DATE_CACHE_REALM_CHILD_ROOT";

// Each executable runs this same single semantic test. The parent measures
// emitted bytes; only its child executes the two modes with private caches.
fn compiled_cases() -> Vec<(String, Engine, Engine, u64)> {
    let source =
        include_str!("../fixtures/date_system_time_zone/cached_source_uses_executing_realm.js");
    let mut cases = Vec::new();
    for directive in ["", "\"use strict\";\n"] {
        let script = format!("{directive}{source}");
        let named = Engine::new(configured_realm("US/Eastern"));
        let fixed = Engine::new(configured_realm("+01:00"));
        let unit = named
            .compile_script(&script, options())
            .expect("source must compile");
        let named_artifact = named
            .emit_wasm(&unit)
            .expect("named Engine must emit real Wasm");
        let fixed_artifact = fixed
            .emit_wasm(&unit)
            .expect("same unit must emit for fixed Engine");
        assert!(named_artifact.bytes.starts_with(b"\0asm"));
        assert_eq!(
            named_artifact.bytes, fixed_artifact.bytes,
            "zone choice must not be compiled into the artifact"
        );
        let bytes = u64::try_from(named_artifact.bytes.len())
            .expect("artifact length must fit the program cache byte budget");
        cases.push((script, named, fixed, bytes));
    }
    cases
}

fn program_budget(cases: &[(String, Engine, Engine, u64)]) -> u64 {
    let bytes = cases
        .iter()
        .try_fold(0_u64, |total, (_, _, _, bytes)| total.checked_add(*bytes));
    let bytes = bytes.expect("two emitted program artifacts must fit a u64 cache byte budget");
    assert!(bytes > 0, "the measured program tier must be nonempty");
    bytes
}

struct OwnedCacheRoot(PathBuf);

impl OwnedCacheRoot {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("fixture clock must follow Unix epoch")
            .as_nanos();
        for attempt in 0_u32..100 {
            let path = std::env::temp_dir().join(format!(
                "scriptw-date-cache-realm-{}-{nonce}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => {
                    let owned = Self(path);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&owned.0, fs::Permissions::from_mode(0o700))
                            .expect("private fixture cache permissions must be writable");
                    }
                    return owned;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("exclusive fixture cache root must be created: {error}"),
            }
        }
        panic!("fixture cache root names exhausted");
    }

    fn remove(&self) -> io::Result<()> {
        fs::remove_dir_all(&self.0)
    }
}

impl Drop for OwnedCacheRoot {
    fn drop(&mut self) {
        if let Err(error) = self.remove() {
            if error.kind() != io::ErrorKind::NotFound {
                eprintln!("fixture cache cleanup failed for {:?}: {error}", self.0);
            }
        }
    }
}

// Borrowed wait keeps ownership during unwinding; no child is detached by
// consuming it through wait_with_output. Child output files avoid pipe stalls.
struct OwnedChild(Child);

impl OwnedChild {
    fn wait(&mut self) -> io::Result<ExitStatus> {
        self.0.wait()
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn date_cached_source_and_artifact_use_each_executing_realms_choice() {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let cases = compiled_cases();
    let budget = program_budget(&cases);
    if let Some(root) = std::env::var_os(CHILD_CACHE_ROOT_ENV) {
        assert_eq!(
            std::env::var_os("SCRIPTW_CACHE_DIR"),
            Some(root),
            "the selected child must own the cache root supplied before process start"
        );
        assert_eq!(
            std::env::var("SCRIPTW_PROGRAM_CACHE_LIMIT_BYTES")
                .expect("child must have its measured program budget")
                .parse::<u64>()
                .expect("child program budget must be a u64"),
            budget,
            "the program budget must equal the actual sloppy and strict artifact bytes"
        );
        for (script, named, fixed, _) in cases {
            assert!(
                !wasm_aot_script_is_cached(&script, &options()),
                "a fresh private cache must start without this source/options artifact"
            );
            let first = named
                .observe_script(&script, options(), run_options())
                .expect("named execution must complete");
            assert_observation(first, "America/New_York", &script);
            assert!(
                wasm_aot_script_is_cached(&script, &options()),
                "first execution retains the same source/options artifact"
            );
            let second = fixed
                .observe_script(&script, options(), run_options())
                .expect("fixed execution of cached source must complete");
            assert_observation(second, "+01:00", &script);
        }
        return;
    }
    drop(cases);

    let cache = OwnedCacheRoot::new();
    let stdout_path = cache.0.join("child.stdout");
    let stderr_path = cache.0.join("child.stderr");
    let stdout = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stdout_path)
        .expect("exclusive child stdout must be created");
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stderr_path)
        .expect("exclusive child stderr must be created");
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().expect("fixture executable must be known"))
            .args([
                "--exact",
                &libtest_path(TEST_NAME),
                "--test-threads=2",
                "--nocapture",
            ])
            .env(CHILD_CACHE_ROOT_ENV, &cache.0)
            .env("SCRIPTW_CACHE_DIR", &cache.0)
            .env("SCRIPTW_PROGRAM_CACHE_LIMIT_BYTES", budget.to_string())
            .env("SCRIPTW_WASM_TRACE", "1")
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .expect("the selected private-cache child must start"),
    );
    let status = child.wait().expect("the selected child must be reaped");
    drop(child);
    let stdout = fs::read_to_string(&stdout_path).expect("child stdout must be readable");
    let stderr = fs::read_to_string(&stderr_path).expect("child stderr must be readable");
    cache
        .remove()
        .expect("the exclusively owned cache must be removed");
    assert!(
        status.success(),
        "private-cache child failed: {status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.starts_with("scriptw wasm trace: program-cache hit: "))
            .count(),
        2,
        "fixed-Realm execution must actually read each cached mode\n{stderr}"
    );
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.starts_with("scriptw wasm trace: program-cache miss: "))
            .count(),
        2,
        "the two named-Realm executions must populate distinct source entries\n{stderr}"
    );
    // compiled_cases() also lowers twice to measure the exact cache budget.
    // Only phases with a cache result belong to the four actual executions.
    let execution_lowerings = stderr
        .split("scriptw wasm trace: parse: ")
        .filter_map(|phase| {
            let hit = phase.lines().find_map(|line| {
                if line.starts_with("scriptw wasm trace: program-cache hit: ") {
                    Some(true)
                } else if line.starts_with("scriptw wasm trace: program-cache miss: ") {
                    Some(false)
                } else {
                    None
                }
            })?;
            let lowerings = phase
                .lines()
                .filter(|line| line.starts_with("scriptw wasm trace: lower: "))
                .count();
            Some((hit, lowerings))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        execution_lowerings,
        [(false, 1), (true, 0), (false, 1), (true, 0)],
        "each cold execution lowers once; each cached execution uses the new Realm without lowering\n{stderr}"
    );
}
