//! Unscored derived-module diagnostic: initialize once, retain and call `run`.
use super::*;
use scriptw_engine::{
    CompileOptions, EmbeddedEvaluation, EmbeddedRuntime, Engine, HostOutputEvent, JsCompletion,
    RealmBuilder,
};
use std::time::Instant;

const PRODUCER_SCHEMA: &str = "scriptw-retained-call-producer-v1";
const EXECUTION_SCHEMA: &str = "scriptw-retained-call-execution-v1";

fn reject_observers() -> Result<(), String> {
    if std::env::vars_os().any(|(key, _)| {
        let key = key.to_string_lossy();
        key.starts_with("SCRIPTW_PRECOMPILED_PHASE_")
            || key == "SCRIPTW_PRECOMPILED_EXECUTION_WINDOWS"
    }) {
        return Err(
            "retained-call diagnostics reject ambient precompiled phase/window observers".into(),
        );
    }
    Ok(())
}

fn counts(samples: &str, warmups: &str) -> Result<(usize, usize), String> {
    let samples = samples.parse().map_err(|_| "invalid sample count")?;
    let warmups = warmups.parse().map_err(|_| "invalid warmup count")?;
    if !(3..=100).contains(&samples) || !(1..=100).contains(&warmups) {
        return Err("samples must be 3..100 and warmups 1..100".into());
    }
    Ok((samples, warmups))
}

fn wrapper(original: &str) -> String {
    format!("export function run() {{\n{original}\n}}\n")
}

fn basename(name: &str) -> Result<(), String> {
    if !matches!(
        Path::new(name).components().next(),
        Some(std::path::Component::Normal(_))
    ) || Path::new(name).components().count() != 1
        || name.contains('\\')
    {
        return Err("diagnostic files must be basenames".into());
    }
    Ok(())
}

fn declaration(mut report: Value, schema: &str, compiler_enabled: bool) -> Value {
    report["schema"] = json!(schema);
    report["excluded_from_scored_cohorts"] = json!(true);
    report["excluded_from_ordinary_cohorts"] = json!(true);
    report["compiler_enabled"] = json!(compiler_enabled);
    report["shared_configuration"] = shared_configuration();
    report["scope"] = json!("diagnostic-derived-strict-module;retained-Store-Realm-and-export;not-a-fresh-entry-or-scored-cohort");
    report
}

pub(super) fn prepare(fixtures: &Path, output: &Path) -> Result<Value, String> {
    reject_observers()?;
    scriptw_engine::configure_compilation_jobs(1)?;
    let manifest = fs::read(fixtures.join("manifest.json")).map_err(|e| e.to_string())?;
    let plan: Value = serde_json::from_slice(&manifest).map_err(|e| e.to_string())?;
    let cases = plan["execution"]
        .as_array()
        .ok_or("missing execution manifest")?;
    if cases.len() != 17 {
        return Err("retained diagnostic requires the original 17-case manifest".into());
    }
    fs::create_dir(output).map_err(|e| e.to_string())?;
    let compiler = Engine::new(RealmBuilder::new().build());
    let mut config = wasmtime::Config::new();
    configure_producer(&mut config)?;
    let native = wasmtime::Engine::new(&config).map_err(|e| e.to_string())?;
    let mut report = declaration(identity()?, PRODUCER_SCHEMA, true);
    report["manifest_sha256"] = json!(hash(&manifest));
    let mut results = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for case in cases {
        let name = case["file"].as_str().ok_or("missing fixture name")?;
        basename(name)?;
        if !name.ends_with(".js") || !seen.insert(name) {
            return Err("fixture names must be distinct JavaScript basenames".into());
        }
        let expected = case["stdout"].as_str().ok_or("missing expected stdout")?;
        let original = fs::read_to_string(fixtures.join(name)).map_err(|e| e.to_string())?;
        let derived = wrapper(&original);
        let source_name = format!("{name}.retained.mjs");
        let original_name = format!("{name}.original.js");
        let image_name = format!("{name}.retained.swpc");
        let started = Instant::now();
        let unit = compiler
            .compile_module(
                &derived,
                CompileOptions {
                    filename: Some(source_name.clone()),
                    ..CompileOptions::default()
                },
            )
            .map_err(|e| e.to_string())?;
        let artifact = compiler.emit_wasm(&unit).map_err(|e| e.to_string())?;
        let source_compilation_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let image = EmbeddedRuntime::precompile(&native, &artifact).map_err(|e| e.to_string())?;
        let native_preparation_ns = started.elapsed().as_nanos();
        write_new(&output.join(&original_name), original.as_bytes())?;
        write_new(&output.join(&source_name), derived.as_bytes())?;
        write_new(&output.join(&image_name), image.as_bytes())?;
        write_new(
            &output.join(format!("{name}.retained.wasm")),
            image.program_wasm(),
        )?;
        let mut receipt = report.clone();
        receipt["file"] = json!(name);
        receipt["stdout"] = json!(expected);
        receipt["original_source_file"] = json!(original_name);
        receipt["derived_source_file"] = json!(source_name);
        receipt["original_source_sha256"] = json!(hash(original.as_bytes()));
        receipt["derived_source_sha256"] = json!(hash(derived.as_bytes()));
        receipt["image_sha256"] = json!(hash(image.as_bytes()));
        receipt["image_bytes"] = json!(image.as_bytes().len());
        receipt["program_wasm_sha256"] = json!(hash(image.program_wasm()));
        receipt["runtime_key"] = json!(image.runtime_key().map(hash_key));
        receipt["image"] = json!(image_name);
        receipt["phases_ns"] = json!({"source_compilation":source_compilation_ns,
            "native_preparation":native_preparation_ns});
        write_new(
            &output.join(&image_name).with_extension("json"),
            receipt.to_string().as_bytes(),
        )?;
        results.push(receipt);
    }
    report["cases"] = json!(results);
    write_new(&output.join("producer.json"), report.to_string().as_bytes())?;
    Ok(report)
}

fn checked_output(events: Vec<HostOutputEvent>, expected: &str) -> Result<String, String> {
    let mut stdout = String::new();
    for event in events {
        match event {
            HostOutputEvent::PrintLine(line) => {
                if stdout.len().saturating_add(line.len()).saturating_add(1) > 1024 * 1024 {
                    return Err("retained call output exceeds 1 MiB".into());
                }
                stdout.push_str(&line);
                stdout.push('\n');
            }
            HostOutputEvent::ReportedException(line) => {
                return Err(format!("reported exception: {line}"))
            }
        }
    }
    if stdout != expected {
        return Err(format!("retained call checksum mismatch: {stdout:?}"));
    }
    Ok(stdout)
}

pub(super) fn execute(
    path: &Path,
    expected: &str,
    samples: &str,
    warmups: &str,
) -> Result<Value, String> {
    reject_observers()?;
    let (samples, warmups) = counts(samples, warmups)?;
    let producer: Value =
        serde_json::from_slice(&fs::read(path.with_extension("json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut report = declaration(identity()?, EXECUTION_SCHEMA, false);
    if producer["schema"] != PRODUCER_SCHEMA
        || producer["excluded_from_scored_cohorts"] != true
        || producer["compiler_enabled"] != true
        || producer["source_commit"] != report["source_commit"]
        || producer["source_fingerprint"] != report["source_fingerprint"]
        || producer["compiler_identity_scheme"] != report["compiler_identity_scheme"]
        || producer["executable_sha256"] != report["executable_sha256"]
        || producer["shared_configuration"] != report["shared_configuration"]
        || producer["stdout"] != expected
        || producer["image"].as_str() != path.file_name().and_then(|p| p.to_str())
    {
        return Err("retained producer identity/configuration/output differs".into());
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    let read_source = |field: &str| -> Result<String, String> {
        let name = producer[field].as_str().ok_or("missing source file")?;
        basename(name)?;
        fs::read_to_string(parent.join(name)).map_err(|e| e.to_string())
    };
    let original = read_source("original_source_file")?;
    let derived = read_source("derived_source_file")?;
    if derived != wrapper(&original)
        || producer["original_source_sha256"] != hash(original.as_bytes())
        || producer["derived_source_sha256"] != hash(derived.as_bytes())
    {
        return Err("retained derived/original source provenance differs".into());
    }
    let mut config = wasmtime::Config::new();
    configure_consumer(&mut config)?;
    let engine = wasmtime::Engine::new(&config).map_err(|e| e.to_string())?;
    if engine.get_cranelift_opt_level().is_some() {
        return Err("retained consumer unexpectedly enabled a compiler".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if producer["image_sha256"] != hash(&bytes) || producer["image_bytes"] != bytes.len() {
        return Err("retained image differs from producer".into());
    }
    // SAFETY: this opt-in host accepts only its separately recorded trusted
    // prepare-retained producer's native image, like execute-scriptw. Hashes
    // bind evidence, and do not authorize external native code.
    let image =
        unsafe { PrecompiledEmbeddedProgram::from_bytes(&bytes) }.map_err(|e| e.to_string())?;
    if producer["program_wasm_sha256"] != hash(image.program_wasm())
        || producer["runtime_key"] != json!(image.runtime_key().map(hash_key))
    {
        return Err("retained program/runtime identity differs".into());
    }
    let started = Instant::now();
    let runtime = EmbeddedRuntime::from_precompiled(&engine, &image).map_err(|e| e.to_string())?;
    let native_admission_ns = started.elapsed().as_nanos();
    drop(image);
    drop(bytes);
    let started = Instant::now();
    let state = runtime
        .new_state(RealmBuilder::new().build(), ())
        .map_err(|e| e.to_string())?;
    let mut store = wasmtime::Store::new(&engine, state);
    store.limiter(|state| state.limits_mut());
    // The outer bounded driver owns the wall deadline, as for canonical runs.
    store.set_epoch_deadline(u64::MAX / 2);
    let mut linker = wasmtime::Linker::new(&engine);
    runtime
        .register(&mut store, &mut linker)
        .map_err(|e| e.to_string())?;
    let mut instance = runtime
        .instantiate(&mut store, &mut linker)
        .map_err(|e| e.to_string())?;
    let setup_ns = started.elapsed().as_nanos();
    let started = Instant::now();
    let initialized = instance.initialize(&mut store).map_err(|e| e.to_string())?;
    let initialize_ns = started.elapsed().as_nanos();
    if !matches!(
        &initialized,
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ) {
        return Err(format!(
            "retained module did not initialize normally: {initialized:?}"
        ));
    }
    checked_output(
        instance
            .take_output(&mut store)
            .map_err(|e| e.to_string())?,
        "",
    )?;
    let function = match instance
        .get_export(&mut store, "run")
        .map_err(|e| e.to_string())?
    {
        JsCompletion::Normal(value) => value,
        other => return Err(format!("retained export lookup failed: {other:?}")),
    };
    let mut observations = Vec::with_capacity(samples + warmups);
    for index in 0..samples + warmups {
        let started = Instant::now();
        let completion = instance
            .call(&mut store, &function, None, &[])
            .map_err(|e| e.to_string())?;
        let call_ns = started.elapsed().as_nanos();
        if !matches!(&completion, JsCompletion::Normal(value) if value.is_undefined()) {
            return Err(format!(
                "retained call did not return undefined normally: {completion:?}"
            ));
        }
        // Drain on every call: warmups and samples cannot accumulate output.
        let stdout = checked_output(
            instance
                .take_output(&mut store)
                .map_err(|e| e.to_string())?,
            expected,
        )?;
        observations.push(json!({"index":index,"warmup":index < warmups,"call_ns":call_ns,
            "completion":"normal","matched":true,"stdout_sha256":hash(stdout.as_bytes()),"stdout":stdout}));
    }
    report["producer"] = producer;
    report["samples"] = json!(samples);
    report["warmups"] = json!(warmups);
    report["phases_ns"] = json!({"native_admission":native_admission_ns,"setup":setup_ns,"module_initialization":initialize_ns});
    report["initialization_count"] = json!(1);
    report["call_boundary"] = json!("public EmbeddedInstance::call including worker dispatch, guest execution, output capture and completion; excludes per-call output draining/checksum validation; natural GC; same retained function/Store/Realm");
    report["observations"] = json!(observations);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_call_protocol_rejects_unbounded_counts_and_unsafe_names() {
        assert_eq!(counts("7", "10").unwrap(), (7, 10));
        for (samples, warmups) in [
            ("2", "10"),
            ("101", "10"),
            ("7", "0"),
            ("7", "101"),
            ("overflow", "1"),
        ] {
            assert!(counts(samples, warmups).is_err());
        }
        for name in [
            "",
            "..",
            "/tmp/source.js",
            "../source.js",
            "nested/source.js",
            "nested\\source.js",
        ] {
            assert!(basename(name).is_err());
        }
    }

    #[test]
    fn every_call_checks_actual_output_and_reported_exceptions() {
        assert_eq!(
            checked_output(vec![HostOutputEvent::PrintLine("42".into())], "42\n").unwrap(),
            "42\n"
        );
        assert!(checked_output(Vec::new(), "42\n").is_err());
        assert!(
            checked_output(vec![HostOutputEvent::ReportedException("bad".into())], "").is_err()
        );
        assert!(checked_output(
            vec![HostOutputEvent::PrintLine("x".repeat(1024 * 1024))],
            ""
        )
        .is_err());
    }
}
