use scriptw_engine::{CompilerBuildIdentity, CompilerIdentity, CompilerSourceRevision};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

/// Whether `command` runs precompiled images without compiling any.
#[allow(dead_code, reason = "only hosts that spawn a sized worker call it")]
fn executes_only(command: Option<&str>) -> bool {
    command.is_some_and(|command| command.starts_with("execute-"))
}

/// Puts every thread of this process on glibc's main malloc arena. Call it
/// before the first other thread exists: glibc picks a thread's arena at its
/// first allocation.
///
/// An execute command allocates from one thread at a time: `main` waits for
/// the sized worker that loads the images, and that worker waits for the
/// engine's execution-stack thread. glibc still gave each its own arena (up to
/// eight per CPU), and each arena kept its own free space and partly used
/// pages: with one arena the shared host's `startup.js` process held 88 KiB
/// less private dirty memory at Store 1's `after_execution` barrier
/// (2026-11-24). Commands that compile run Cranelift on a pool of threads,
/// which would contend for one arena, so they keep glibc's default, as does
/// a host whose Rust allocations bypass glibc (`--features jemalloc`).
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[allow(dead_code, reason = "only hosts that spawn a sized worker call it")]
fn use_one_malloc_arena() -> Result<(), String> {
    use std::ffi::c_int;
    // glibc's <malloc.h>.
    const M_ARENA_MAX: c_int = -8;
    unsafe extern "C" {
        fn mallopt(param: c_int, value: c_int) -> c_int;
    }
    // SAFETY: mallopt only records an allocator parameter, under glibc's own
    // lock; it touches no memory of the caller.
    if unsafe { mallopt(M_ARENA_MAX, 1) } == 1 {
        Ok(())
    } else {
        Err("glibc refused mallopt(M_ARENA_MAX, 1)".into())
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
#[allow(dead_code, reason = "only hosts that spawn a sized worker call it")]
fn use_one_malloc_arena() -> Result<(), String> {
    Ok(())
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// What a report binds its process to: `executable` (the default) or
/// `build-source`. Any other value is refused, never ignored.
///
/// `executable` is evidence: the process hashes its whole loaded image
/// (about 0.2 s for 200 MiB) and reports `executable_sha256`. `build-source`
/// reports only the embedded source identity and a null `executable_sha256`,
/// for a driver that hashes the host file itself before and after the run and
/// measures process wall time. A null digest never equals a recorded one, so
/// such a report fails every driver check that compares the reported digest;
/// its schema is unchanged, so only `report_identity` marks it, and a reader
/// that ignores the digest (per-entry times) must not treat it as evidence.
const REPORT_IDENTITY_VARIABLE: &str = "SCRIPTW_BENCHMARK_REPORT_IDENTITY";

fn identity() -> Result<Value, String> {
    let build = CompilerBuildIdentity::current()?;
    let executable_sha256 = match std::env::var(REPORT_IDENTITY_VARIABLE).as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("executable") => {
            Some(CompilerIdentity::current()?.executable_sha256().to_string())
        }
        Ok("build-source") => None,
        Ok(other) => {
            return Err(format!(
                "{REPORT_IDENTITY_VARIABLE} must be executable or build-source, not {other:?}"
            ))
        }
        Err(error) => return Err(format!("{REPORT_IDENTITY_VARIABLE}: {error}")),
    };
    let revision = match build.source_revision() {
        CompilerSourceRevision::GitCommit(commit) => Some(commit.as_str()),
        CompilerSourceRevision::UnversionedArchive => None,
    };
    let source_only = executable_sha256.is_none();
    let mut report = json!({"source_commit": revision,
        "source_fingerprint": build.source_fingerprint().to_string(),
        "executable_sha256": executable_sha256,
        "compiler_identity_scheme": CompilerIdentity::source_fingerprint_scheme()});
    if source_only {
        report["report_identity"] = json!("build-source");
    }
    Ok(report)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|e| e.to_string())
}

fn configure_producer(config: &mut wasmtime::Config) -> Result<(), String> {
    // The producer's own SpeedAndSize/backtracking policy replaces either
    // public policy's optimization level and allocator.
    scriptw_engine::configure_wasm_runtime(config, scriptw_engine::NativeCompilationPolicy::Fast, scriptw_engine::CompilationJobs::current());
    config.cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);
    config.cranelift_regalloc_algorithm(wasmtime::RegallocAlgorithm::Backtracking);
    config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Disable);
    // Explicit Cargo TARGET also disables inference of host CPU extensions,
    // exactly as in the immutable Cargo runtime-image producer.
    config
        .target(env!("SCRIPTW_PERF_TARGET"))
        .map_err(|e| e.to_string())?;
    config.parallel_compilation(false);
    Ok(())
}

fn configure_consumer(config: &mut wasmtime::Config) -> Result<(), String> {
    configure_producer(config)?;
    config.enable_compiler(false);
    Ok(())
}

/// Opt-in A/B control for the fresh-Store consumers: `pooled` (the default)
/// or `on-demand`. Any other value is refused, never ignored.
const STORE_ALLOCATION_VARIABLE: &str = "SCRIPTW_PRECOMPILED_STORE_ALLOCATION";

/// Fresh-Store consumers run one Store after another, so their Engine takes
/// each Store's memory (Scriptw's GC heap, the WASI program's linear memory)
/// from a one-slot pool that keeps its dirty pages resident
/// (`PooledStores::sequential`). Both adapters of a host apply this one
/// policy.
#[derive(Clone, Copy)]
enum StoreAllocation {
    Pooled(scriptw_engine::PooledStores),
    OnDemand,
}

impl StoreAllocation {
    fn from_environment() -> Result<Self, String> {
        match std::env::var(STORE_ALLOCATION_VARIABLE).as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("pooled") => {
                Ok(Self::Pooled(scriptw_engine::PooledStores::sequential()))
            }
            Ok("on-demand") => Ok(Self::OnDemand),
            Ok(other) => Err(format!(
                "{STORE_ALLOCATION_VARIABLE} must be pooled or on-demand, not {other:?}"
            )),
            Err(error) => Err(format!("{STORE_ALLOCATION_VARIABLE}: {error}")),
        }
    }

    /// The Engine for the complete `config`, and the report's description.
    /// A pool is applied as the Engine is created, after every setting.
    fn engine(self, config: &wasmtime::Config) -> Result<(wasmtime::Engine, Value), String> {
        match self {
            Self::Pooled(pool) => {
                let (engine, reset) = pool.engine(config).map_err(|e| e.to_string())?;
                let reset = match reset {
                    scriptw_engine::PooledStoreReset::KeepDirtyPagesResident { bytes } => {
                        json!({"mode": "keep-dirty-pages-resident", "keep_resident_bytes": bytes})
                    }
                    scriptw_engine::PooledStoreReset::ReleaseAll => {
                        json!({"mode": "release-all", "reason": "PAGEMAP_SCAN unavailable"})
                    }
                };
                Ok((engine, json!({"strategy": "pooled",
                    "max_concurrent_stores": pool.max_concurrent().get(), "reset": reset})))
            }
            Self::OnDemand => Ok((
                wasmtime::Engine::new(config).map_err(|e| e.to_string())?,
                json!({"strategy": "on-demand"}),
            )),
        }
    }
}

fn configure_fresh_store_consumer(
    config: &mut wasmtime::Config,
) -> Result<StoreAllocation, String> {
    configure_consumer(config)?;
    StoreAllocation::from_environment()
}

fn shared_configuration() -> Value {
    json!({"native_compilation_mode":"size-optimized", "register_allocation":"backtracking",
        "target":"generic", "target_triple":env!("SCRIPTW_PERF_TARGET"),
        "wasm_backtrace_details":"disable", "wasmtime_version":"47.0.0",
        "runtime_policy":"scriptw-required-wasmtime-experimental-v1",
        "consumer_compiler_enabled":false, "collector":"copying",
        "gc_initial_bytes":16777216u64, "worker_stack_bytes":WORKER_STACK_BYTES,
        "max_wasm_stack_bytes":33554432u64, "store_memory_limit_bytes":1073741824u64})
}

fn decorate(mut report: Value, adapter: &str) -> Value {
    if report.get("phase_barriers").is_none() {
        report
            .as_object_mut()
            .expect("benchmark reports are objects")
            .remove("excluded_from_scored_cohorts");
    }
    report["comparison_cohort"] = json!("scriptw-precompiled-comparison-v1");
    report["excluded_from_ordinary_cohorts"] = json!(true);
    report["scope"] = json!(match report.get("compiler_enabled").and_then(Value::as_bool) {
        Some(false) => "same-executable-precompiled-execution;fresh-Store;entry-API-interval;loading-and-setup-separate",
        Some(true) => "separate-native-image-producer;not-a-consumer-observation",
        None => "same-executable-precompiled-comparison-identity",
    });
    report["adapter"] = json!(adapter);
    report["shared_configuration"] = shared_configuration();
    report
}

fn hash_key(key: &[u8; 32]) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}
