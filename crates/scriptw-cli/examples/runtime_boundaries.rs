//! Separate portable-bundle and existing-Realm call measurement cohort.
#[path = "runtime_boundaries/call_observer.rs"]
mod call_observer;
#[path = "runtime_boundaries/result_ready.rs"]
mod result_ready;
use scriptw_engine::{
    Artifact, CompileOptions, CompilerIdentity, CompilerSourceRevision, Engine, ExecutionBackend,
    HostOutputEvent, IntlCompilationProfile, ObservedCompletion, RealmBuilder, WasmRuntimeProfile,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path, time::Instant};

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn compile_options(path: &str) -> CompileOptions {
    CompileOptions {
        filename: Some(
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path)
                .into(),
        ),
        ..CompileOptions::default()
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|error| error.to_string())
}

fn artifact_json(artifact: &Artifact) -> Value {
    json!({"program_sha256": hash(&artifact.bytes), "program_bytes": artifact.bytes.len(),
        "source_compilation_profile": "portable", "linked_runtime": artifact.runtime.as_ref().map(|runtime|
            json!({"key": runtime.key().as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>(),
                "sha256": hash(runtime.bytes()), "bytes": runtime.bytes().len()}))})
}

fn execution_json(profile: &WasmRuntimeProfile, expected: &str) -> Result<Value, String> {
    let mut stdout = String::new();
    for event in &profile.outcome.output_events {
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
    if profile.outcome.backend_used != ExecutionBackend::WasmAot
        || !matches!(profile.outcome.completion, ObservedCompletion::Normal(_))
        || stdout != expected
    {
        return Err(format!(
            "unexpected product completion/output: {:?}; {stdout:?}",
            profile.outcome
        ));
    }
    Ok(
        json!({"backend": "wasm-aot", "completion": "normal", "stdout": stdout,
        "runtime_policy": profile.runtime_policy,
        "native_mode": profile.native_mode.as_str(),
        "actual_native_policy": {"native_mode": profile.options.native_mode().as_str(),
            "gc_initial_mib": profile.options.initial_gc_capacity().mebibytes(),
            "register_allocation": profile.options.register_allocation().as_str()},
        "module_cache": format!("{:?}", profile.module_cache),
        "runtime_module_cache": profile.module_loading.runtime_cache.map(|outcome| format!("{outcome:?}")),
        "module_loading_ns": {"runtime": profile.module_loading.runtime.as_nanos(),
            "program": profile.module_loading.program.as_nanos(),
            "embedded_image": profile.module_loading.embedded_image.as_ref().map(|image| json!({
                "admission": image.admission.as_nanos(),
                "native_backing_preparation": image.native_backing_preparation.as_nanos(),
                "native_lease_decode_and_digest": image.native_lease_decode_and_digest.as_nanos(),
                "deserialize": image.deserialize.as_nanos()}))},
        "host_import_cache": format!("{:?}", profile.host_import_cache),
        "matched_runtime_image_cache": format!("{:?}", profile.matched_runtime_image_cache),
        "phases_ns": {"admission": profile.timings.admission.as_nanos(),
            "engine": profile.timings.engine.as_nanos(), "module": profile.timings.module.as_nanos(),
            "store_and_linker": profile.timings.store_and_linker.as_nanos(),
            "host_import_registration": profile.timings.host_import_registration.as_nanos(),
            "instantiate": profile.timings.instantiate.as_nanos(),
            "export_lookup": profile.timings.export_lookup.as_nanos(),
            "execution_and_completion": profile.timings.execution_and_completion.as_nanos(),
            "total": profile.timings.total.as_nanos()},
        "gc_capacity_before_bytes": profile.heap.capacity_before_execution_bytes,
        "gc_capacity_after_bytes": profile.heap.capacity_after_execution_bytes,
        "allocation_count": "unavailable: pinned Wasmtime has no public per-Store counter",
        "peak_gc_heap_bytes": "unavailable: boundary capacity snapshots are not a peak"}),
    )
}

fn run() -> Result<Value, String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    if !matches!(
        (mode, args.len()),
        ("prepare", 3) | ("source", 3) | ("fresh", 3) | ("calls", 4)
    ) {
        return Err("usage: runtime_boundaries prepare SOURCE NEW_BUNDLE | source SOURCE EXPECTED_STDOUT | fresh BUNDLE EXPECTED_STDOUT | calls BUNDLE EXPECTED_STDOUT SAMPLES(7..100)".into());
    }
    scriptw_engine::configure_compilation_jobs(1)?;
    let observer = if mode == "calls" {
        Some(call_observer::Observer::new(
            args[3].parse().map_err(|_| "invalid samples")?,
        )?)
    } else {
        None
    };
    let realm = match &observer {
        Some(observer) => RealmBuilder::new().with_host_hooks(Box::new(observer.clone())),
        None => RealmBuilder::new(),
    };
    let engine = Engine::new(realm.build());
    let mut result = if mode == "prepare" {
        let source = fs::read_to_string(&args[1]).map_err(|error| error.to_string())?;
        let compilation = engine
            .profile_script_compilation(&source, compile_options(&args[1]))
            .map_err(|error| error.to_string())?;
        let bundle = compilation
            .artifact
            .encode_portable_bundle(&IntlCompilationProfile::Minimal)
            .map_err(|error| error.to_string())?;
        write_new(Path::new(&args[2]), &bundle)?;
        json!({"mode": "prepare", "source_sha256": hash(source.as_bytes()),
            "source_bytes": source.len(),
            "bundle_sha256": hash(&bundle), "bundle_bytes": bundle.len(),
            "artifact": artifact_json(&compilation.artifact),
            "compilation_ns": {"preparation": compilation.preparation.as_nanos(),
                "lowering": compilation.lowering.as_nanos(), "emission": compilation.emission.as_nanos(),
                "validation": compilation.validation.as_nanos()}})
    } else {
        let expected = fs::read_to_string(&args[2]).map_err(|error| error.to_string())?;
        let started = Instant::now();
        let (artifact, bytes, compilation_ns) = if mode == "source" {
            let bytes = fs::read(&args[1]).map_err(|error| error.to_string())?;
            let source = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
            let compilation = engine
                .profile_script_compilation(source, compile_options(&args[1]))
                .map_err(|error| error.to_string())?;
            let spans = json!({"preparation": compilation.preparation.as_nanos(),
                "lowering": compilation.lowering.as_nanos(), "emission": compilation.emission.as_nanos(),
                "validation": compilation.validation.as_nanos()});
            (compilation.artifact, bytes, Some(spans))
        } else {
            // Fresh/calls accept no JS source input. Their sole executable
            // input is the admitted raw P/R bundle, loaded in this child.
            let bytes = fs::read(&args[1]).map_err(|error| error.to_string())?;
            let artifact = engine
                .load_portable_bundle(&bytes)
                .map_err(|error| error.to_string())?;
            (artifact, bytes, None)
        };
        let load_ns = started.elapsed().as_nanos();
        let profile = engine
            .profile_wasm_execution(&artifact, Some(60_000), false)
            .map_err(|error| error.to_string())?;
        let load_and_execute_ns = started.elapsed().as_nanos();
        let execution = execution_json(&profile, &expected)?;
        let calls = observer
            .as_ref()
            .map(call_observer::Observer::finish)
            .transpose()?;
        // Only a checked real Wasm result may reach this marker. Expensive
        // artifact/executable provenance follows it and remains in the final
        // JSON; the parent independently binds that JSON to this nonce/output.
        let ready = result_ready::emit(&expected)?;
        let mut result = json!({"mode": mode, "artifact": artifact_json(&artifact),
            "execution": execution, "result_ready": ready});
        if let Some(compilation_ns) = compilation_ns {
            result["source_sha256"] = json!(hash(&bytes));
            result["source_bytes"] = json!(bytes.len());
            result["source_read_and_compilation_ns"] = json!(load_ns);
            result["source_to_result_ns"] = json!(load_and_execute_ns);
            result["compilation_ns"] = compilation_ns;
            result["boundary"] = json!("Fresh child reads JavaScript and runs the same real Portable preparation/lowering/emission as prepare, then product Wasm execution. Source compilation is inside these spans and the parent checked-result-ready interval; final artifact/executable provenance is outside READY.");
        } else {
            result["bundle_sha256"] = json!(hash(&bytes));
            result["bundle_bytes"] = json!(bytes.len());
            result["bundle_read_and_admission_ns"] = json!(load_ns);
            result["bundle_load_and_execute_ns"] = json!(load_and_execute_ns);
            result["boundary"] = json!("Fresh child loads raw precompiled JS-to-Wasm P+R without JS parsing/lowering/emission; includes safe bundle admission and Wasm native compilation/cache lookup. The parent records checked-result-ready arrival separately from instrumented whole-child accounting.");
        }
        if let Some(calls) = calls {
            result["calls"] = calls;
            result["guest_warmups"] = json!(10);
        }
        result
    };
    let cache_identity = scriptw_engine::compiler_cache_identity()?;
    result["requested_compiler_cache_image_hash"] = json!(std::env::var(
        "SCRIPTW_COMPILER_CACHE_IMAGE_HASH"
    )
    .unwrap_or_else(|_| "sha256".into()));
    result["compiler_cache_identity"] = json!({
        "image_hash_algorithm": cache_identity.algorithm().as_str(),
        "image_digest": cache_identity.image_digest_hex(),
        "fingerprint_domain": cache_identity.algorithm().fingerprint_domain(),
    });
    let cache = scriptw_engine::wasm_runtime_module_cache_observation();
    result["module_cache_accounting"] = json!({
        "entry_limit": cache.entry_limit, "image_byte_limit": cache.image_byte_limit,
        "retained_entries": cache.retained_entries, "retained_image_bytes": cache.retained_image_bytes,
    });
    let identity = CompilerIdentity::current()?;
    result["schema"] = json!("scriptw-portable-boundaries-child-v1");
    result["source_fingerprint"] = json!(identity.source_fingerprint().to_string());
    result["source_commit"] = json!(match identity.source_revision() {
        CompilerSourceRevision::GitCommit(commit) => Some(commit.as_str()),
        CompilerSourceRevision::UnversionedArchive => None,
    });
    result["executable_sha256"] = json!(identity.executable_sha256().to_string());
    Ok(result)
}

fn main() {
    match run() {
        Ok(result) => println!("{result}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
