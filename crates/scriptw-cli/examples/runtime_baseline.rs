//! Opt-in same-source Wasmtime measurements for scripts/benchmark-v8.py.
#[path = "runtime_baseline/phase_observer.rs"]
mod phase_observer;
#[path = "runtime_baseline/policy_options.rs"]
mod policy_options;
use scriptw_engine::{
    CompileOptions, CompilerIdentity, CompilerSourceRevision, Engine, ExecutionBackend,
    HostOutputEvent, ObservedCompletion, RealmBuilder, WasmProcessMemoryObservation,
    WasmProcessMemorySample,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 5 {
        return Err(
            "usage: runtime-baseline FIXTURES SAMPLES WARMUPS CASE ARTIFACT_DIR [--row-phases] [--native-mode fast|size-optimized|speed] [--gc-initial-mib 8|16|32|64|128] [--regalloc single-pass|backtracking]"
                .into(),
        );
    }
    let options = policy_options::parse(&args[5..])?;
    let observe_phases = options.row_phases;
    let root = PathBuf::from(&args[0]);
    let samples: usize = args[1].parse().map_err(|_| "invalid sample count")?;
    let warmups: usize = args[2].parse().map_err(|_| "invalid warmup count")?;
    if !(3..=100).contains(&samples) || !(1..=100).contains(&warmups) {
        return Err("samples must be 3..100 and warmups 1..100".into());
    }
    scriptw_engine::configure_compilation_jobs(1)?;
    let plan: Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let cases = plan["execution"]
        .as_array()
        .ok_or("missing execution inventory")?;
    let cases: Vec<_> = cases
        .iter()
        .filter(|case| case["file"] == args[3])
        .collect();
    if cases.len() != 1 {
        return Err("CASE must select exactly one manifest entry".into());
    }
    let observer = observe_phases
        .then(phase_observer::Observer::new)
        .transpose()?;
    let realm = match &observer {
        Some(observer) => RealmBuilder::new().with_host_hooks(Box::new(observer.clone())),
        None => RealmBuilder::new(),
    };
    let engine = Engine::new(realm.build());
    let mut results = Vec::new();
    for case in cases {
        let name = case["file"].as_str().ok_or("missing filename")?;
        if PathBuf::from(name).components().count() != 1 || !name.ends_with(".js") {
            return Err("fixture must be a basename".into());
        }
        let expected = case["stdout"].as_str().ok_or("missing expected output")?;
        let source = fs::read_to_string(root.join(name)).map_err(|e| e.to_string())?;
        let compilation = engine
            .profile_script_compilation(
                &source,
                CompileOptions {
                    filename: Some(name.into()),
                    ..CompileOptions::default()
                },
            )
            .map_err(|e| e.to_string())?;
        let artifact_path = PathBuf::from(&args[4]).join(format!("{name}.wasm"));
        let mut artifact_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(artifact_path)
            .map_err(|e| e.to_string())?;
        std::io::Write::write_all(&mut artifact_file, &compilation.artifact.bytes)
            .map_err(|e| e.to_string())?;
        let mut observations = Vec::new();
        for index in 0..warmups + samples {
            if let Some(observer) = &observer {
                observer.begin(index >= warmups)?;
            }
            let p = match options.policy {
                Some(policy) => engine.profile_wasm_execution_with_options(
                    &compilation.artifact,
                    Some(60_000),
                    false,
                    policy,
                ),
                None => engine.profile_wasm_execution(&compilation.artifact, Some(60_000), false),
            }
            .map_err(|e| format!("{name}: {e}"))?;
            if options
                .policy
                .is_some_and(|requested| p.options != requested)
            {
                return Err(format!(
                    "{name}: executing Engine differs from requested native/GC policy"
                ));
            }
            let mut output = String::new();
            for event in &p.outcome.output_events {
                match event {
                    HostOutputEvent::PrintLine(line) => {
                        output.push_str(line);
                        output.push('\n');
                    }
                    HostOutputEvent::ReportedException(line) => {
                        return Err(format!("{name}: {line}"))
                    }
                }
            }
            if p.outcome.backend_used != ExecutionBackend::WasmAot
                || !matches!(p.outcome.completion, ObservedCompletion::Normal(_))
                || output != expected
            {
                return Err(format!(
                    "{name}: unexpected product execution: {:?}; output {output:?}",
                    p.outcome
                ));
            }
            let process_rss = match &p.process_memory {
                WasmProcessMemoryObservation::Sampled(sample) => json!({
                    "status": "sampled", "scope": "runtime-invocation-process-rss",
                    "interval_ns": WasmProcessMemorySample::SAMPLE_INTERVAL.as_nanos(),
                    "elapsed_ns": sample.elapsed().as_nanos(), "sample_count": sample.sample_count(),
                    "initial_rss_bytes": sample.initial_rss_bytes(),
                    "final_rss_bytes": sample.final_rss_bytes(),
                    "maximum_rss_bytes": sample.maximum_rss_bytes()}),
                WasmProcessMemoryObservation::Unavailable(reason) => json!({
                    "status": "unavailable", "reason": format!("{reason:?}")}),
            };
            let mut observation = json!({"warmup": index < warmups,
                "execution_ns": p.timings.execution_and_completion.as_nanos(),
                "phases_ns": {"admission": p.timings.admission.as_nanos(),
                    "engine": p.timings.engine.as_nanos(),
                    "module": p.timings.module.as_nanos(),
                    "store_and_linker": p.timings.store_and_linker.as_nanos(),
                    "host_import_registration": p.timings.host_import_registration.as_nanos(),
                    "instantiate": p.timings.instantiate.as_nanos(),
                    "export_lookup": p.timings.export_lookup.as_nanos(),
                    "total": p.timings.total.as_nanos()},
                "memory": {"process_rss": process_rss,
                    "gc_capacity_before_bytes": p.heap.capacity_before_execution_bytes,
                    "gc_capacity_after_bytes": p.heap.capacity_after_execution_bytes,
                    "unavailable": {"live_gc_bytes": format!("{:?}", p.live_gc_bytes),
                        "allocation_count": format!("{:?}", p.allocation_count),
                        "allocation_bytes": format!("{:?}", p.allocation_bytes),
                        "collection_count": format!("{:?}", p.collection_count),
                        "gc_pause_time": format!("{:?}", p.gc_pause_time),
                        "peak_gc_heap_bytes": format!("{:?}", p.peak_gc_heap_bytes)}},
                "module_cache": format!("{:?}", p.module_cache),
                "host_import_cache": format!("{:?}", p.host_import_cache), "stdout": output,
                "matched_runtime_image_cache": format!("{:?}", p.matched_runtime_image_cache),
                "runtime_policy": p.runtime_policy});
            if options.policy.is_some() {
                observation["actual_native_policy"] = policy_options::policy_json(p.options);
            }
            if let Some(observer) = &observer {
                observation["row_phases"] = observer.finish()?;
            }
            observations.push(observation);
        }
        let runtime = compilation.artifact.runtime.as_ref().map(|runtime| {
            json!({"key": runtime.key().as_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
                "wasm_sha256": format!("{:x}", Sha256::digest(runtime.bytes())),
                "wasm_bytes": runtime.bytes().len()})
        });
        results.push(json!({"file": name, "source_sha256": format!("{:x}", Sha256::digest(source.as_bytes())),
            "source_compilation_profile": "portable", "linked_runtime": runtime,
            "wasm_emission_diagnostics": compilation.artifact.debug_dump,
            "wasm_sha256": format!("{:x}", Sha256::digest(&compilation.artifact.bytes)),
            "wasm_bytes": compilation.artifact.bytes.len(),
            "compilation_ns": {"preparation": compilation.preparation.as_nanos(),
                "lowering": compilation.lowering.as_nanos(), "emission": compilation.emission.as_nanos(),
                "validation": compilation.validation.as_nanos()},
            "footprint": {"defined_functions": compilation.footprint.defined_functions,
                "code_body_bytes": compilation.footprint.code_body_bytes,
                "largest_code_body_bytes": compilation.footprint.largest_code_body_bytes,
                "data_payload_bytes": compilation.footprint.data_payload_bytes},
            "observations": observations}));
    }
    let identity = CompilerIdentity::current()?;
    let revision = match identity.source_revision() {
        CompilerSourceRevision::GitCommit(commit) => Some(commit.as_str()),
        CompilerSourceRevision::UnversionedArchive => None,
    };
    println!(
        "{}",
        serde_json::to_string(
            &json!({"schema": match (observe_phases, options.policy.is_some()) {
            (false, false) => "scriptw-wasmtime-baseline-v2",
            (true, false) => "scriptw-wasmtime-row-phases-v1",
            (false, true) => "scriptw-wasmtime-policy-baseline-v1",
            (true, true) => "scriptw-wasmtime-policy-row-phases-v1",
        },
        "requested_native_policy": options.policy.map(policy_options::policy_json),
        "source_commit": revision, "source_fingerprint": identity.source_fingerprint().to_string(),
        "executable_sha256": identity.executable_sha256().to_string(),
        "samples": samples, "warmups": warmups, "cases": results})
        )
        .map_err(|e| e.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
