use super::*;
use scriptw_engine::{
    EmbeddedRuntime, ExecutionBackend, HostOutputEvent, ObservedCompletion,
    PrecompiledEmbeddedProgram, RealmBuilder,
};
use std::time::Instant;

pub(super) fn run(
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
    let total_started = Instant::now();
    let mut config = wasmtime::Config::new();
    scriptw_engine::configure_precompiled_wasm_runtime(&mut config);
    let started = Instant::now();
    let engine = wasmtime::Engine::new(&config).map_err(|e| e.to_string())?;
    let engine_ns = started.elapsed().as_nanos();
    let compiler_enabled = engine.get_cranelift_opt_level().is_some();
    if compiler_enabled {
        return Err("execution diagnostic unexpectedly enabled a native compiler".into());
    }
    let after_engine = residency::snapshot()?;
    let started = Instant::now();
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    // SAFETY: this opt-in diagnostic consumes only the exact native image from
    // its separately recorded `prepare` process. The outer measurement driver
    // binds that producer/file authority. This command is not an untrusted-code
    // loader; a checksum alone cannot satisfy this native-origin requirement.
    let image =
        unsafe { PrecompiledEmbeddedProgram::from_bytes(&bytes) }.map_err(|e| e.to_string())?;
    let image_read_ns = started.elapsed().as_nanos();
    let after_image_read = residency::snapshot()?;
    let ((runtime, admission_ns), native_load_process_memory) = residency::measure_loading(|| {
        let started = Instant::now();
        let runtime =
            EmbeddedRuntime::from_precompiled(&engine, &image).map_err(|e| e.to_string())?;
        Ok((runtime, started.elapsed().as_nanos()))
    })?;
    let after_admission = residency::snapshot()?;
    let image_sha256 = hash(&bytes);
    let image_bytes = bytes.len();
    let program_wasm_sha256 = hash(image.program_wasm());
    let runtime_key = image.runtime_key().map(produce::hash_key);
    // Both native Modules own their mappings. The execution floor therefore
    // retains neither transport copy; load endpoints above include both.
    drop(image);
    drop(bytes);
    let after_transport_drop = residency::snapshot()?;
    let load_total_ns = total_started.elapsed().as_nanos();
    let mut observations = Vec::new();
    for index in 0..samples + warmups {
        let before_store = residency::snapshot()?;
        let observation_started = Instant::now();
        let started = Instant::now();
        let state = runtime
            .new_state(RealmBuilder::new().build(), ())
            .map_err(|e| e.to_string())?;
        let mut store = wasmtime::Store::new(&engine, state);
        store.limiter(|state| state.limits_mut());
        // The outer driver enforces a process deadline. This example retains
        // the required epoch checks and leaves epoch ticking to its owner.
        store.set_epoch_deadline(u64::MAX / 2);
        let mut linker = wasmtime::Linker::new(&engine);
        let state_linker_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        runtime
            .register(&mut store, &mut linker)
            .map_err(|e| e.to_string())?;
        let register_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let instance = runtime
            .instantiate(&mut store, &mut linker)
            .map_err(|e| e.to_string())?;
        let instantiate_ns = started.elapsed().as_nanos();
        let after_instantiate = residency::snapshot()?;
        let gc_before = store.gc_heap_capacity();
        let started = Instant::now();
        let outcome = instance.evaluate(&mut store).map_err(|e| e.to_string())?;
        let execution_ns = started.elapsed().as_nanos();
        let gc_after = store.gc_heap_capacity();
        let after_execution = residency::snapshot()?;
        let total_ns = observation_started.elapsed().as_nanos();
        let mut stdout = String::new();
        for event in &outcome.output_events {
            match event {
                HostOutputEvent::PrintLine(line) => {
                    stdout.push_str(line);
                    stdout.push('\n');
                }
                HostOutputEvent::ReportedException(line) => {
                    return Err(format!("reported exception: {line}"))
                }
            }
        }
        if outcome.backend_used != ExecutionBackend::WasmAot
            || !matches!(outcome.completion, ObservedCompletion::Normal(_))
            || stdout != expected
        {
            return Err(format!(
                "unexpected precompiled execution: {outcome:?}; stdout{stdout:?}"
            ));
        }
        drop(linker);
        drop(store);
        let after_store_drop = residency::snapshot()?;
        observations.push(json!({"index": index, "warmup": index < warmups,
            "completion": "normal", "stdout": stdout,
            "phases_ns": {"state_linker": state_linker_ns, "register": register_ns,
                "instantiate": instantiate_ns, "execution": execution_ns, "total": total_ns},
            "gc_capacity_before_bytes": gc_before, "gc_capacity_after_bytes": gc_after,
            "process_snapshots": {"before_store": before_store, "after_instantiate": after_instantiate,
                "after_execution": after_execution, "after_store_drop": after_store_drop}}));
    }
    let mut report = identity()?;
    report["schema"] = json!("scriptw-precompiled-execution-v1");
    report["excluded_from_scored_cohorts"] = json!(true);
    report["compiler_enabled"] = json!(compiler_enabled);
    report["native_load_process_memory"] = native_load_process_memory;
    report["image_sha256"] = json!(image_sha256);
    report["image_bytes"] = json!(image_bytes);
    report["program_wasm_sha256"] = json!(program_wasm_sha256);
    report["runtime_key"] = json!(runtime_key);
    report["samples"] = json!(samples);
    report["warmups"] = json!(warmups);
    report["load_phases_ns"] = json!({"engine": engine_ns, "image_read_and_envelope_admission": image_read_ns,
        "native_admission_and_deserialization": admission_ns, "process_total": load_total_ns});
    report["process_snapshots"] = json!({"startup": startup, "after_engine": after_engine,
        "after_image_read": after_image_read, "after_admission": after_admission,
        "after_transport_drop": after_transport_drop});
    report["observations"] = json!(observations);
    report["scope"] = json!("independent-precompiled-diagnostic;fresh-Store-Realm-per-invocation;process-endpoints;R-decode-in-load-costs;no-compiler-in-consumer;not-an-ordinary-score");
    Ok(report)
}
