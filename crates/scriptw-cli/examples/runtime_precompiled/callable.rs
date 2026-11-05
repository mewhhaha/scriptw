//! Opt-in retained-module benchmark: initialize once, prepare inputs, call `run`.
use super::*;
use scriptw_engine::{
    CompileOptions, EmbeddedEvaluation, EmbeddedRuntime, Engine, ForeignValue, HostOutputEvent,
    JsCompletion, RealmBuilder,
};
use std::time::Instant;

const PRODUCER_SCHEMA: &str = "scriptw-retained-call-producer-v2";
const EXECUTION_SCHEMA: &str = "scriptw-retained-call-execution-v2";

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativePolicy {
    Fast,
    Speed,
    SizeOptimized,
}

impl NativePolicy {
    pub(super) fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None | Some("size-optimized") => Ok(Self::SizeOptimized),
            Some("fast") => Ok(Self::Fast),
            Some("speed") => Ok(Self::Speed),
            Some(_) => Err("native policy must be fast, speed or size-optimized".into()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Speed => "speed",
            Self::SizeOptimized => "size-optimized",
        }
    }

    fn settings(self) -> (wasmtime::OptLevel, wasmtime::RegallocAlgorithm) {
        match self {
            Self::Fast => (
                wasmtime::OptLevel::None,
                wasmtime::RegallocAlgorithm::SinglePass,
            ),
            Self::Speed => (
                wasmtime::OptLevel::Speed,
                wasmtime::RegallocAlgorithm::Backtracking,
            ),
            Self::SizeOptimized => (
                wasmtime::OptLevel::SpeedAndSize,
                wasmtime::RegallocAlgorithm::Backtracking,
            ),
        }
    }

    fn configure(self, config: &mut wasmtime::Config) {
        let (optimization, allocator) = self.settings();
        config.cranelift_opt_level(optimization);
        config.cranelift_regalloc_algorithm(allocator);
    }

    fn consumer_engine(self) -> Result<wasmtime::Engine, String> {
        let mut config = wasmtime::Config::new();
        configure_producer(&mut config)?;
        self.configure(&mut config);
        config.enable_compiler(false);
        let engine = wasmtime::Engine::new(&config).map_err(|e| e.to_string())?;
        if engine.get_cranelift_opt_level().is_some() {
            return Err("retained consumer unexpectedly enabled a compiler".into());
        }
        Ok(engine)
    }

    fn shared_configuration(self) -> Value {
        let mut configuration = shared_configuration();
        configuration["native_compilation_mode"] = json!(self.name());
        configuration["register_allocation"] = json!(match self {
            Self::Fast => "single-pass",
            Self::Speed | Self::SizeOptimized => "backtracking",
        });
        configuration
    }

    fn validate_producer_configuration(self, configuration: &Value) -> Result<(), String> {
        let (optimization, allocator) = self.settings();
        if configuration["cranelift_opt_level"] != format!("{optimization:?}")
            || configuration["register_allocation"] != format!("{allocator:?}")
            || configuration["parallel_compilation"] != false
            || configuration["target"] != env!("SCRIPTW_PERF_TARGET")
        {
            return Err("retained producer native configuration differs from policy".into());
        }
        Ok(())
    }
}

fn native_configuration(engine: &wasmtime::Engine) -> Value {
    json!({"cranelift_opt_level":engine.get_cranelift_opt_level().map(|value|format!("{value:?}")),
        "register_allocation":engine.get_cranelift_regalloc_algorithm().map(|value|format!("{value:?}")),
        "parallel_compilation":engine.get_parallel_compilation(),"target":engine.get_target()})
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceFormat {
    WrappedScript,
    Module,
}

impl SourceFormat {
    fn parse(value: Option<&Value>) -> Result<Self, String> {
        match value {
            None => Ok(Self::WrappedScript),
            Some(Value::String(value)) if value == "wrapped-script" => Ok(Self::WrappedScript),
            Some(Value::String(value)) if value == "module" => Ok(Self::Module),
            Some(_) => Err("source_format must be wrapped-script or module".into()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::WrappedScript => "wrapped-script",
            Self::Module => "module",
        }
    }

    fn source(self, original: &str) -> String {
        match self {
            Self::WrappedScript => wrapper(original),
            Self::Module => original.to_owned(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Invocation {
    input: i32,
    stdout: String,
}

struct CasePlan {
    file: String,
    stdout: String,
    invocations: Vec<Invocation>,
    metadata: Value,
}

impl CasePlan {
    fn new(case: &Value, format: SourceFormat) -> Result<Self, String> {
        let file = case["file"].as_str().ok_or("missing fixture name")?;
        basename(file)?;
        if !file.ends_with(".js") && !file.ends_with(".mjs") {
            return Err("fixture names must be JavaScript basenames".into());
        }
        let stdout = case["stdout"].as_str().ok_or("missing expected stdout")?;
        if stdout.len() > 1024 * 1024 {
            return Err("expected stdout exceeds 1 MiB".into());
        }
        let mut metadata = json!({});
        let invocations = match format {
            SourceFormat::WrappedScript => {
                if case.get("invocations").is_some() {
                    return Err("wrapped scripts do not accept input plans".into());
                }
                Vec::new()
            }
            SourceFormat::Module => {
                for key in ["label", "category", "scope"] {
                    let text = case[key]
                        .as_str()
                        .filter(|text| !text.is_empty())
                        .ok_or_else(|| format!("missing nonempty situation {key}"))?;
                    metadata[key] = json!(text);
                }
                let operations = case["operations"]
                    .as_u64()
                    .filter(|value| *value > 0)
                    .ok_or("situation operations must be a positive integer")?;
                metadata["operations"] = json!(operations);
                let values = case["invocations"]
                    .as_array()
                    .ok_or("missing invocation plan")?;
                if !(1..=16).contains(&values.len()) {
                    return Err("invocation plans must contain 1..16 entries".into());
                }
                let invocations: Vec<Invocation> =
                    serde_json::from_value(case["invocations"].clone())
                        .map_err(|e| format!("invalid invocation plan: {e}"))?;
                if invocations[0].stdout != stdout
                    || invocations
                        .iter()
                        .any(|item| item.stdout.len() > 1024 * 1024)
                {
                    return Err(
                        "invocation output differs from first stdout or exceeds 1 MiB".into(),
                    );
                }
                invocations
            }
        };
        Ok(Self {
            file: file.to_owned(),
            stdout: stdout.to_owned(),
            invocations,
            metadata,
        })
    }

    fn write_to(&self, receipt: &mut Value) {
        receipt["file"] = json!(self.file);
        receipt["stdout"] = json!(self.stdout);
        if !self.invocations.is_empty() {
            receipt["invocations"] = json!(self.invocations);
        }
        for (key, value) in self
            .metadata
            .as_object()
            .expect("case metadata is an object")
        {
            receipt[key] = value.clone();
        }
    }
}

fn manifest(plan: &Value) -> Result<(SourceFormat, Vec<CasePlan>), String> {
    let format = SourceFormat::parse(plan.get("source_format"))?;
    if format == SourceFormat::Module && plan["schema"] != "scriptw-retained-situations-v1" {
        return Err("module manifests require scriptw-retained-situations-v1 schema".into());
    }
    let cases = plan["execution"]
        .as_array()
        .ok_or("missing execution manifest")?;
    if !(1..=128).contains(&cases.len()) {
        return Err("retained manifests require 1..128 execution cases".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut result = Vec::with_capacity(cases.len());
    for case in cases {
        let case = CasePlan::new(case, format)?;
        if !seen.insert(case.file.clone()) {
            return Err("fixture names must be distinct".into());
        }
        result.push(case);
    }
    Ok((format, result))
}

fn declaration(
    mut report: Value,
    schema: &str,
    compiler_enabled: bool,
    policy: NativePolicy,
    format: SourceFormat,
) -> Value {
    report["schema"] = json!(schema);
    report["excluded_from_scored_cohorts"] = json!(true);
    report["excluded_from_ordinary_cohorts"] = json!(true);
    report["compiler_enabled"] = json!(compiler_enabled);
    report["native_policy"] = json!(policy.name());
    report["source_format"] = json!(format.name());
    report["shared_configuration"] = policy.shared_configuration();
    report["scope"] = json!(match format {
        SourceFormat::WrappedScript => "derived-strict-module;retained-Store-Realm-and-export;not-a-fresh-entry-or-scored-cohort",
        SourceFormat::Module => "standalone-source-module;retained-Store-Realm-and-exports;untimed-prepare-and-checked-public-run;not-a-fresh-entry-or-scored-cohort",
    });
    report
}

pub(super) fn prepare(
    fixtures: &Path,
    output: &Path,
    policy: NativePolicy,
) -> Result<Value, String> {
    reject_observers()?;
    scriptw_engine::configure_compilation_jobs(1)?;
    let manifest_bytes = fs::read(fixtures.join("manifest.json")).map_err(|e| e.to_string())?;
    let plan: Value = serde_json::from_slice(&manifest_bytes).map_err(|e| e.to_string())?;
    let (format, cases) = manifest(&plan)?;
    fs::create_dir(output).map_err(|e| e.to_string())?;
    let compiler = Engine::new(RealmBuilder::new().build());
    let mut config = wasmtime::Config::new();
    configure_producer(&mut config)?;
    policy.configure(&mut config);
    let native = wasmtime::Engine::new(&config).map_err(|e| e.to_string())?;
    let configuration = native_configuration(&native);
    policy.validate_producer_configuration(&configuration)?;
    let mut report = declaration(identity()?, PRODUCER_SCHEMA, true, policy, format);
    report["native_configuration"] = configuration;
    report["manifest_sha256"] = json!(hash(&manifest_bytes));
    let mut results = Vec::new();
    for case in cases {
        let name = &case.file;
        let original = fs::read_to_string(fixtures.join(name)).map_err(|e| e.to_string())?;
        let derived = format.source(&original);
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
        case.write_to(&mut receipt);
        receipt["original_source_file"] = json!(original_name);
        receipt["derived_source_file"] = json!(source_name);
        receipt["original_source_sha256"] = json!(hash(original.as_bytes()));
        receipt["derived_source_sha256"] = json!(hash(derived.as_bytes()));
        receipt["image_sha256"] = json!(hash(image.as_bytes()));
        receipt["image_bytes"] = json!(image.as_bytes().len());
        receipt["program_wasm_sha256"] = json!(hash(image.program_wasm()));
        receipt["program_wasm_bytes"] = json!(image.program_wasm().len());
        receipt["native_serialized_bytes"] = json!(image.serialized_program_len());
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
    let policy = NativePolicy::parse(Some(
        producer["native_policy"]
            .as_str()
            .ok_or("missing native policy")?,
    ))?;
    let format = SourceFormat::parse(Some(
        producer
            .get("source_format")
            .ok_or("missing source format")?,
    ))?;
    let case = CasePlan::new(&producer, format)?;
    policy.validate_producer_configuration(&producer["native_configuration"])?;
    let mut report = declaration(identity()?, EXECUTION_SCHEMA, false, policy, format);
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
    if derived != format.source(&original)
        || producer["original_source_sha256"] != hash(original.as_bytes())
        || producer["derived_source_sha256"] != hash(derived.as_bytes())
    {
        return Err("retained derived/original source provenance differs".into());
    }
    let engine = policy.consumer_engine()?;
    report["native_configuration"] = native_configuration(&engine);
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
        || producer["program_wasm_bytes"] != image.program_wasm().len()
        || producer["native_serialized_bytes"] != image.serialized_program_len()
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
    let prepare = match format {
        SourceFormat::WrappedScript => None,
        SourceFormat::Module => Some(
            match instance
                .get_export(&mut store, "prepare")
                .map_err(|e| e.to_string())?
            {
                JsCompletion::Normal(value) => value,
                other => return Err(format!("retained prepare lookup failed: {other:?}")),
            },
        ),
    };
    let mut observations = Vec::with_capacity(samples + warmups);
    for index in 0..samples + warmups {
        let mut prepare_ns = 0;
        let mut invocation_index = None;
        let mut input = None;
        let mut expected = case.stdout.as_str();
        if let Some(prepare) = &prepare {
            let planned_index = index % case.invocations.len();
            let invocation = &case.invocations[planned_index];
            invocation_index = Some(planned_index);
            input = Some(invocation.input);
            expected = &invocation.stdout;
            // Value conversion and input preparation are outside the run clock.
            let argument = instance
                .value(
                    &mut store,
                    ForeignValue::Number(f64::from(invocation.input)),
                )
                .map_err(|e| e.to_string())?;
            let started = Instant::now();
            let completion = instance
                .call(&mut store, prepare, None, &[argument])
                .map_err(|e| e.to_string())?;
            prepare_ns = started.elapsed().as_nanos();
            if !matches!(&completion, JsCompletion::Normal(value) if value.is_undefined()) {
                return Err(format!(
                    "retained prepare did not return undefined normally: {completion:?}"
                ));
            }
            checked_output(
                instance
                    .take_output(&mut store)
                    .map_err(|e| e.to_string())?,
                "",
            )?;
        }
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
            "invocation_index":invocation_index,"input":input,"prepare_ns":prepare_ns,
            "completion":"normal","matched":true,"stdout_sha256":hash(stdout.as_bytes()),"stdout":stdout}));
    }
    report["producer"] = producer;
    report["samples"] = json!(samples);
    report["warmups"] = json!(warmups);
    report["phases_ns"] = json!({"native_admission":native_admission_ns,"setup":setup_ns,"module_initialization":initialize_ns});
    report["initialization_count"] = json!(1);
    report["call_boundary"] = json!("public EmbeddedInstance::call including caller-thread guarded-fiber entry, guest execution, output capture and completion; excludes input conversion, prepare and per-call output draining/checksum validation; natural GC; same retained function/Store/Realm");
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

    fn situation() -> Value {
        json!({"file":"request.mjs","label":"Request","category":"state","scope":"retained request",
            "operations":10,"stdout":"1\n","invocations":[
                {"input":1,"stdout":"1\n"},{"input":-2,"stdout":"-2\n"}]})
    }

    fn situation_manifest(case: Value) -> Value {
        json!({"schema":"scriptw-retained-situations-v1","source_format":"module","execution":[case]})
    }

    #[test]
    fn source_format_preserves_standalone_bytes_and_defaults_to_wrapping() {
        let source = "// café\nexport function prepare(input) {}\nexport function run() {}\n";
        let (format, cases) = manifest(&situation_manifest(situation())).unwrap();
        assert_eq!(format, SourceFormat::Module);
        assert_eq!(format.source(source).as_bytes(), source.as_bytes());
        assert_eq!(cases[0].invocations[1].input, -2);
        let legacy = json!({"execution":[{"file":"legacy.js","stdout":"42\n"}]});
        let (format, cases) = manifest(&legacy).unwrap();
        assert_eq!(format, SourceFormat::WrappedScript);
        assert_eq!(
            format.source("print(42);"),
            "export function run() {\nprint(42);\n}\n"
        );
        assert!(cases[0].invocations.is_empty());
        for invalid in [json!(null), json!(1), json!("script"), json!("Module")] {
            assert!(SourceFormat::parse(Some(&invalid)).is_err());
        }
        let mut invalid = situation_manifest(situation());
        invalid["schema"] = json!("unrecognized-schema");
        assert!(manifest(&invalid).is_err());
    }

    #[test]
    fn manifests_bound_case_inventory_and_reject_duplicates() {
        assert!(manifest(&json!({"execution":[]})).is_err());
        let mut cases: Vec<Value> = (0..128)
            .map(|index| json!({"file":format!("case-{index}.js"),"stdout":""}))
            .collect();
        assert_eq!(manifest(&json!({"execution":cases})).unwrap().1.len(), 128);
        cases.push(json!({"file":"case-128.js","stdout":""}));
        assert!(manifest(&json!({"execution":cases})).is_err());
        assert!(manifest(
            &json!({"execution":[{"file":"same.js","stdout":""},{"file":"same.js","stdout":""}]})
        )
        .is_err());
        let mut invalid = situation();
        invalid["file"] = json!("../request.js");
        assert!(manifest(&situation_manifest(invalid)).is_err());
    }

    #[test]
    fn invocation_plans_are_typed_bounded_and_preserved_in_receipts() {
        let case = CasePlan::new(&situation(), SourceFormat::Module).unwrap();
        let mut receipt = json!({});
        case.write_to(&mut receipt);
        let restored = CasePlan::new(&receipt, SourceFormat::Module).unwrap();
        assert_eq!(restored.invocations.len(), 2);
        assert_eq!(restored.invocations[1].input, -2);
        assert_eq!(restored.invocations[1].stdout, "-2\n");
        for invalid in [
            json!([]),
            json!([{"input":2147483648_i64,"stdout":"1\n"}]),
            json!([{"input":1.5,"stdout":"1\n"}]),
            json!([{"input":1,"stdout":"different\n"}]),
            json!([{"input":1,"stdout":"1\n","extra":true}]),
            json!(vec![json!({"input":1,"stdout":"1\n"}); 17]),
        ] {
            let mut case = situation();
            case["invocations"] = invalid;
            assert!(CasePlan::new(&case, SourceFormat::Module).is_err());
        }
        let mut case = situation();
        case["invocations"] = json!(vec![json!({"input":i32::MIN,"stdout":"1\n"}); 16]);
        assert_eq!(
            CasePlan::new(&case, SourceFormat::Module)
                .unwrap()
                .invocations
                .len(),
            16
        );
        case["operations"] = json!(0);
        assert!(CasePlan::new(&case, SourceFormat::Module).is_err());
        let mut case = situation();
        case["scope"] = json!("");
        assert!(CasePlan::new(&case, SourceFormat::Module).is_err());
        assert!(CasePlan::new(&situation(), SourceFormat::WrappedScript).is_err());
    }

    #[test]
    fn native_policies_bind_recorded_configuration() {
        assert_eq!(
            NativePolicy::parse(None).unwrap(),
            NativePolicy::SizeOptimized
        );
        for (name, optimization, allocator) in [
            ("fast", "None", "SinglePass"),
            ("speed", "Speed", "Backtracking"),
            ("size-optimized", "SpeedAndSize", "Backtracking"),
        ] {
            let policy = NativePolicy::parse(Some(name)).unwrap();
            let engine = policy.consumer_engine().unwrap();
            let consumer = native_configuration(&engine);
            assert!(consumer["cranelift_opt_level"].is_null());
            assert!(consumer["register_allocation"].is_null());
            assert_eq!(consumer["parallel_compilation"], false);
            assert!(consumer["target"].is_null());
            assert_eq!(policy.name(), name);
            assert_eq!(
                policy.shared_configuration()["native_compilation_mode"],
                name
            );
            let mut actual = json!({"cranelift_opt_level":optimization,"register_allocation":allocator,
                "parallel_compilation":false,"target":env!("SCRIPTW_PERF_TARGET")});
            policy.validate_producer_configuration(&actual).unwrap();
            actual["register_allocation"] = json!("other");
            assert!(policy.validate_producer_configuration(&actual).is_err());
        }
        for invalid in ["", "Fast", "single-pass", "speed-and-size"] {
            assert!(NativePolicy::parse(Some(invalid)).is_err());
        }
    }
}
