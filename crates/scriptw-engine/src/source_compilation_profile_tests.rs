use super::*;
use scriptw_aot_wasm::RuntimeSourceCompilationProfile;

fn portable(engine: &Engine, source: &str) -> Artifact {
    let unit = engine
        .compile_script(source, CompileOptions::default())
        .expect("source lowers independently of native host compilation");
    let artifact = engine.emit_wasm(&unit).expect("portable product emission");
    assert_eq!(
        RuntimeSourceCompilationProfile::from_module(&artifact.bytes).unwrap(),
        RuntimeSourceCompilationProfile::Portable
    );
    if let Some(runtime) = &artifact.runtime {
        assert_eq!(
            RuntimeSourceCompilationProfile::from_module(runtime.bytes()).unwrap(),
            RuntimeSourceCompilationProfile::Portable
        );
    }
    artifact
}

fn run_portable(engine: &Engine, artifact: &Artifact) -> Result<RunOutcome, EngineError> {
    run_on_sized_stack(|| {
        engine.run_with_wasm_bytes_inner(
            WasmProgramRef::new(
                &artifact.bytes,
                artifact.runtime.as_ref().map(|runtime| &runtime.0),
            )
            .with_compilation_policy(artifact.compilation_policy.as_ref().unwrap()),
            Some(30_000),
            true,
            WasmModuleMemoryCachePolicy::BypassRetention,
        )
    })
}

#[test]
#[cfg(feature = "native-source-compiler")]
fn public_exports_remain_portable_after_native_source_execution_warms_the_runtime() {
    let engine = Engine::new(RealmBuilder::new().build());
    let native = engine
        .run_script(
            r#"
var suffix = '/*' + Math.random() + '*/';
var make = Function('return 10;' + suffix);
var indirect = (0, eval)('20' + suffix);
function direct() { let local = 12; return eval('local' + suffix); }
make() + indirect + direct();
"#,
            CompileOptions::default(),
            RunOptions {
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("native execution explicitly retains all three compiler callbacks");
    assert_eq!(native.backend_used, ExecutionBackend::WasmAot);
    assert!(native.note.contains("number(42)"), "{}", native.note);
    let artifact = portable(&engine, "function answer() { return 42; } answer();");
    assert!(artifact.runtime.is_some());
    let result = run_portable(&engine, &artifact).expect("portable compiled semantics execute");
    assert!(result.note.contains("number(42)"), "{}", result.note);
}

#[test]
fn portable_prepared_code_runs_and_unprepared_source_rejects_outside_javascript() {
    let engine = Engine::new(RealmBuilder::new().build());
    let prepared = portable(&engine, "eval('40 + 2');");
    let result = run_portable(&engine, &prepared).expect("prepared source is compiled Wasm");
    assert!(result.note.contains("number(42)"), "{}", result.note);
    for source in [
        "try { eval('40 + 2 /*' + Math.random() + '*/'); } catch (e) { 42; }",
        "try { (0, eval)('40 + 2 /*' + Math.random() + '*/'); } catch (e) { 42; }",
        "try { Function('return 42; /*' + Math.random() + '*/')(); } catch (e) { 42; }",
    ] {
        let artifact = portable(&engine, source);
        let error = run_portable(&engine, &artifact)
            .err()
            .expect("portable unknown source cannot cross a compiler boundary or be caught");
        assert_eq!(
            error.wasm_execution_failure_kind(),
            Some(WasmExecutionFailureKind::DynamicSource)
        );
        assert!(error.parse_diagnostic().is_none());
    }
}

#[test]
fn linked_program_source_profile_cannot_be_substituted_before_instantiation() {
    let engine = Engine::new(RealmBuilder::new().build());
    let artifact = portable(&engine, "function answer() { return 42; } answer();");
    let runtime = artifact.runtime.as_ref().unwrap();
    let selected = IntlDataSelection::new(IntlCompilationProfile::Minimal);
    let native_runtime = run_on_sized_stack(|| {
        scriptw_aot_wasm::runtime_artifact_with_inputs(
            &selected,
            scriptw_aot_wasm::RuntimeArtifactInputs::new(None)
                .with_source_compilation_profile(RuntimeSourceCompilationProfile::NativeHost),
        )
        .unwrap()
    });
    assert_ne!(runtime.key(), native_runtime.key());
    let error = run_on_sized_stack(|| {
        engine.run_with_wasm_bytes_inner(
            WasmProgramRef::new(&artifact.bytes, Some(&native_runtime)),
            Some(30_000),
            true,
            WasmModuleMemoryCachePolicy::BypassRetention,
        )
    })
    .err()
    .expect("a portable P cannot link against a native R");
    assert!(error
        .message()
        .contains("source-compilation profiles differ"));
}
