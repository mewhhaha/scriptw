use super::wasi::envelope;
use super::*;
use std::time::Instant;
use wasmtime::{Engine, Module};

pub(super) fn prepare(input: &Path, output: &Path) -> Result<Value, String> {
    let total = Instant::now();
    let bytes = fs::read(input).map_err(|e| e.to_string())?;
    envelope::reject_implicit_start(&bytes)?;
    let mut config = wasmtime::Config::new();
    configure_producer(&mut config)?;
    let started = Instant::now();
    let engine = Engine::new(&config).map_err(|e| e.to_string())?;
    let engine_ns = started.elapsed().as_nanos();
    if engine.get_cranelift_opt_level() != Some(wasmtime::OptLevel::SpeedAndSize) {
        return Err("WASI producer has an unexpected native compiler policy".into());
    }
    let started = Instant::now();
    let module = Module::new(&engine, &bytes).map_err(|e| e.to_string())?;
    let compilation_ns = started.elapsed().as_nanos();
    let started = Instant::now();
    let native = module.serialize().map_err(|e| e.to_string())?;
    let serialization_ns = started.elapsed().as_nanos();
    let image = envelope::encode(&bytes, &native, &envelope::current_fingerprint()?)?;
    write_new(output, &image)?;
    let mut report = identity()?;
    report["schema"] = json!("scriptw-precompiled-wasi-producer-v1");
    report["excluded_from_scored_cohorts"] = json!(true);
    report["compiler_enabled"] = json!(true);
    report["file"] = json!(input
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("non-UTF8 input basename")?);
    report["wasm"] = report["file"].clone();
    report["wasm_sha256"] = json!(hash(&bytes));
    report["program_wasm_sha256"] = json!(hash(&bytes));
    report["program_wasm_bytes"] = json!(bytes.len());
    report["image"] = json!(output
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("non-UTF8 image basename")?);
    report["image_sha256"] = json!(hash(&image));
    report["image_bytes"] = json!(image.len());
    report["native_serialized_bytes"] = json!(native.len());
    report["phases_ns"] = json!({"engine":engine_ns, "native_compilation":compilation_ns,
        "native_serialization":serialization_ns, "total":total.elapsed().as_nanos()});
    Ok(decorate(report, "wasi"))
}
