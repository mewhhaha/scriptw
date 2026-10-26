//! Shared benchmark-only host. External native images require trusted producers.
#[path = "runtime_precompiled/consume.rs"]
#[allow(dead_code)]
mod consume;
#[path = "runtime_precompiled/produce.rs"]
#[allow(dead_code)]
mod produce;
#[path = "runtime_precompiled/residency.rs"]
mod residency;
#[path = "compiler_precompiled_baseline/wasi.rs"]
mod wasi;

use scriptw_engine::{CompilerIdentity, CompilerSourceRevision, PrecompiledEmbeddedProgram};
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
    scriptw_engine::configure_wasm_runtime(config);
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

fn shared_configuration() -> Value {
    json!({"native_compilation_mode":"size-optimized", "register_allocation":"backtracking",
        "target":"generic", "target_triple":env!("SCRIPTW_PERF_TARGET"),
        "wasm_backtrace_details":"disable", "wasmtime_version":"47.0.0",
        "runtime_policy":"scriptw-required-wasmtime-experimental-v1",
        "consumer_compiler_enabled":false, "collector":"copying",
        "gc_initial_bytes":16777216u64, "worker_stack_bytes":WORKER_STACK_BYTES,
        "max_wasm_stack_bytes":33554432u64, "store_memory_limit_bytes":1073741824u64,
        "epoch_deadline":u64::MAX / 2})
}

fn decorate(mut report: Value, adapter: &str) -> Value {
    report
        .as_object_mut()
        .expect("benchmark reports are objects")
        .remove("excluded_from_scored_cohorts");
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

fn prepare_scriptw(fixtures: &Path, output: &Path) -> Result<Value, String> {
    let mut report = produce::run_with_config(fixtures, output, configure_producer)?;
    for case in report["cases"]
        .as_array_mut()
        .ok_or("missing producer cases")?
    {
        let name = case["file"].as_str().ok_or("missing producer file")?;
        let wasm_name = format!("{name}.wasm");
        let image_name = case["image"].as_str().ok_or("missing producer image")?;
        let bytes = fs::read(output.join(image_name)).map_err(|e| e.to_string())?;
        // SAFETY: these are the exact bytes just written above by this process's
        // EmbeddedRuntime::precompile, not an external or digest-based authority.
        let image =
            unsafe { PrecompiledEmbeddedProgram::from_bytes(&bytes) }.map_err(|e| e.to_string())?;
        write_new(&output.join(&wasm_name), image.program_wasm())?;
        case["wasm"] = json!(wasm_name);
    }
    report["compiler_enabled"] = json!(true); // precompile required a real compiler.
    let report = decorate(report, "scriptw");
    // Preserve the delegated producer's original receipt, then publish the
    // augmented receipt in this newly created, exclusively owned directory.
    fs::rename(
        output.join("producer.json"),
        output.join("scriptw-producer.json"),
    )
    .map_err(|e| e.to_string())?;
    write_new(&output.join("producer.json"), report.to_string().as_bytes())?;
    Ok(report)
}

fn execute_scriptw(args: &[String]) -> Result<Value, String> {
    let mut report = consume::run_with_config(
        Path::new(&args[1]),
        &args[2],
        &args[3],
        &args[4],
        configure_consumer,
    )?;
    for row in report["observations"]
        .as_array_mut()
        .ok_or("missing observations")?
    {
        // The delegated path has already required normal completion and exact
        // output. It rejects reported exceptions rather than returning rows.
        row["stderr"] = json!("");
        row["matched"] = json!(true);
    }
    Ok(decorate(report, "scriptw"))
}

fn run() -> Result<Value, String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    // Bind/read the executable identically before either adapter's measured
    // load endpoints. Subsequent identity() calls use its immutable cached owner.
    let mut identity = identity()?;
    match (args.first().map(String::as_str), args.len()) {
        (Some("identity"), 1) => {
            identity["schema"] = json!("scriptw-precompiled-shared-host-v1");
            Ok(decorate(identity, "shared"))
        }
        (Some("prepare-scriptw"), 3) => prepare_scriptw(Path::new(&args[1]), Path::new(&args[2])),
        (Some("prepare-wasi"), 3) => wasi::prepare(Path::new(&args[1]), Path::new(&args[2])),
        (Some("execute-scriptw"), 5) => execute_scriptw(&args),
        (Some("execute-wasi"), 5) => wasi::execute(Path::new(&args[1]), &args[2], &args[3], &args[4]),
        _ => Err("usage: compiler_precompiled_baseline identity | prepare-scriptw FIXTURES NEW_DIRECTORY | prepare-wasi INPUT.wasm NEW_IMAGE | execute-{scriptw|wasi} IMAGE EXPECTED_STDOUT SAMPLES WARMUPS".into()),
    }
}

fn main() {
    let result = std::thread::Builder::new()
        .stack_size(WORKER_STACK_BYTES)
        .spawn(run)
        .map_err(|e| e.to_string())
        .and_then(|thread| {
            thread
                .join()
                .map_err(|_| "shared precompiled host panicked".to_owned())
        })
        .and_then(|result| result);
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
