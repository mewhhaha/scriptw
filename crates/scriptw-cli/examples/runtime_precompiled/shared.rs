use scriptw_engine::{CompilerIdentity, CompilerSourceRevision};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn identity() -> Result<Value, String> {
    let identity = CompilerIdentity::current()?;
    let revision = match identity.source_revision() {
        CompilerSourceRevision::GitCommit(commit) => Some(commit.as_str()),
        CompilerSourceRevision::UnversionedArchive => None,
    };
    Ok(json!({"source_commit": revision,
        "source_fingerprint": identity.source_fingerprint().to_string(),
        "executable_sha256": identity.executable_sha256().to_string(),
        "compiler_identity_scheme": CompilerIdentity::source_fingerprint_scheme()}))
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
