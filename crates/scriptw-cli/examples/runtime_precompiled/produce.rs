use super::*;
use scriptw_engine::{CompileOptions, EmbeddedRuntime, Engine, RealmBuilder};
use std::time::Instant;

pub(super) fn run(fixtures: &Path, output: &Path) -> Result<Value, String> {
    scriptw_engine::configure_compilation_jobs(1)?;
    let manifest_bytes = fs::read(fixtures.join("manifest.json")).map_err(|e| e.to_string())?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|e| e.to_string())?;
    let cases = manifest["execution"]
        .as_array()
        .ok_or("missing execution manifest")?;
    fs::create_dir(output).map_err(|e| e.to_string())?;
    let compiler = Engine::new(RealmBuilder::new().build());
    let mut config = wasmtime::Config::new();
    scriptw_engine::configure_wasm_runtime(&mut config);
    let native = wasmtime::Engine::new(&config).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for case in cases {
        let name = case["file"].as_str().ok_or("missing fixture name")?;
        if Path::new(name).components().count() != 1 || !name.ends_with(".js") || !seen.insert(name)
        {
            return Err("fixture names must be distinct JavaScript basenames".into());
        }
        let expected = case["stdout"].as_str().ok_or("missing expected stdout")?;
        let source = fs::read_to_string(fixtures.join(name)).map_err(|e| e.to_string())?;
        let started = Instant::now();
        let unit = compiler
            .compile_script(
                &source,
                CompileOptions {
                    filename: Some(name.into()),
                    ..CompileOptions::default()
                },
            )
            .map_err(|e| e.to_string())?;
        let artifact = compiler.emit_wasm(&unit).map_err(|e| e.to_string())?;
        let source_compilation_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let image = EmbeddedRuntime::precompile(&native, &artifact).map_err(|e| e.to_string())?;
        let native_preparation_ns = started.elapsed().as_nanos();
        let image_name = format!("{name}.swpc");
        write_new(&output.join(&image_name), image.as_bytes())?;
        results.push(json!({"file": name, "stdout": expected,
            "source_sha256": hash(source.as_bytes()),
            "program_wasm_sha256": hash(image.program_wasm()),
            "program_wasm_bytes": image.program_wasm().len(),
            "image": image_name, "image_sha256": hash(image.as_bytes()),
            "image_bytes": image.as_bytes().len(),
            "native_serialized_bytes": image.serialized_program_len(),
            "runtime_key": image.runtime_key().map(|key| hash_key(key)),
            "source_compilation_profile": "portable", "intl_profile": "Minimal",
            "phases_ns": {"source_compilation": source_compilation_ns, "native_preparation": native_preparation_ns}}));
    }
    let mut report = identity()?;
    report["schema"] = json!("scriptw-precompiled-producer-v1");
    report["excluded_from_scored_cohorts"] = json!(true);
    report["manifest_sha256"] = json!(hash(&manifest_bytes));
    report["cases"] = json!(results);
    write_new(&output.join("producer.json"), report.to_string().as_bytes())?;
    Ok(report)
}

pub(super) fn hash_key(key: &[u8; 32]) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}
