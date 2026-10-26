//! Benchmark-only WASI adapter, with the same native tier as Scriptw P and R.
use super::*;
use std::time::Instant;
use wasmtime::{Engine, Module};

mod envelope;
mod execution;

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

pub(super) fn execute(
    path: &Path,
    expected: &str,
    samples: &str,
    warmups: &str,
) -> Result<Value, String> {
    let samples: usize = samples.parse().map_err(|_| "invalid sample count")?;
    let warmups: usize = warmups.parse().map_err(|_| "invalid warmup count")?;
    if !(3..=100).contains(&samples) || !(1..=100).contains(&warmups) {
        return Err("samples must be 3..100 and warmups 1..100".into());
    }
    let startup = residency::snapshot()?;
    let total = Instant::now();
    let mut config = wasmtime::Config::new();
    configure_consumer(&mut config)?;
    let started = Instant::now();
    let engine = Engine::new(&config).map_err(|e| e.to_string())?;
    let engine_ns = started.elapsed().as_nanos();
    if engine.get_cranelift_opt_level().is_some() {
        return Err("WASI execution unexpectedly enabled a native compiler".into());
    }
    let after_engine = residency::snapshot()?;
    let started = Instant::now();
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let frame = envelope::decode(&bytes, &envelope::current_fingerprint()?)?;
    let image_read_ns = started.elapsed().as_nanos();
    let after_image_read = residency::snapshot()?;
    let ((module, admission_ns), mut load_memory) = residency::measure_loading(|| {
        let started = Instant::now();
        let raw = &bytes[frame.program.clone()];
        Module::validate(&engine, raw).map_err(|e| e.to_string())?;
        envelope::reject_implicit_start(raw)?;
        // SAFETY: this opt-in command accepts only the unchanged Module::serialize
        // output of the separately recorded prepare-wasi producer, for this exact
        // raw Wasm. The outer driver binds provenance; framing/digests grant none.
        // This is the same external-native trust contract as Wasmtime itself.
        let module = unsafe { Module::deserialize(&engine, &bytes[frame.native.clone()]) }
            .map_err(|e| e.to_string())?;
        Ok((module, started.elapsed().as_nanos()))
    })?;
    load_memory["scope"] =
        json!("native-WASI-P-admission-process-RSS-sampled-maximum-not-peak-or-live-js");
    let after_admission = residency::snapshot()?;
    let image_sha256 = hash(&bytes);
    let image_bytes = bytes.len();
    let program_sha256 = hash(&bytes[frame.program.clone()]);
    let native_bytes = frame.native.len();
    // Module owns its executable mapping; transport is not part of execution.
    drop(frame);
    drop(bytes);
    let after_transport_drop = residency::snapshot()?;
    let load_total_ns = total.elapsed().as_nanos();
    let observations = execution::observe(&engine, &module, expected, samples, warmups)?;
    let mut report = identity()?;
    report["schema"] = json!("scriptw-precompiled-wasi-execution-v1");
    report["excluded_from_scored_cohorts"] = json!(true);
    report["compiler_enabled"] = json!(false);
    report["native_load_process_memory"] = load_memory;
    report["image_sha256"] = json!(image_sha256);
    report["image_bytes"] = json!(image_bytes);
    report["program_wasm_sha256"] = json!(program_sha256);
    report["wasm_sha256"] = report["program_wasm_sha256"].clone();
    report["native_serialized_bytes"] = json!(native_bytes);
    report["runtime_key"] = Value::Null;
    report["samples"] = json!(samples);
    report["warmups"] = json!(warmups);
    report["load_phases_ns"] = json!({"engine":engine_ns, "image_read_and_envelope_admission":image_read_ns,
        "native_admission_and_deserialization":admission_ns, "process_total":load_total_ns});
    report["process_snapshots"] = json!({"startup":startup,"after_engine":after_engine,
        "after_image_read":after_image_read,"after_admission":after_admission,
        "after_transport_drop":after_transport_drop});
    report["observations"] = json!(observations);
    report["scope"] = json!("independent-precompiled-diagnostic;fresh-Store-WASI-per-invocation;process-endpoints;no-compiler-in-consumer;not-an-ordinary-score");
    Ok(decorate(report, "wasi"))
}
