use super::*;
use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

fn runtime() -> Arc<RuntimeArtifact> {
    static RUNTIME: OnceLock<Arc<RuntimeArtifact>> = OnceLock::new();
    Arc::clone(RUNTIME.get_or_init(|| {
        std::thread::Builder::new()
            .name("native-context-profile-fixture".into())
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                scriptw_aot_wasm::runtime_artifact_with_inputs(
                    &scriptw_intl::IntlDataSelection::new(IntlCompilationProfile::Minimal),
                    scriptw_aot_wasm::RuntimeArtifactInputs::new(None)
                        .with_source_compilation_profile(
                            scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,
                        ),
                )
                .unwrap()
            })
            .unwrap()
            .join()
            .unwrap()
    }))
}

/// The constructor must receive a real NativeHost R from this same Store.
fn owner(
    engine: &WasmtimeEngine,
    runtime: &Arc<RuntimeArtifact>,
) -> (
    WasmtimeStore<WasmHostState>,
    WasmtimeLinker<WasmHostState>,
    wasmtime::Instance,
) {
    let mut timings = WasmRuntimeEmbeddedImageTimings::default();
    let module = embedded_runtime::load_native(
        engine,
        runtime,
        WasmNativeCompilationMode::Fast,
        &mut timings,
    )
    .unwrap_or_else(|| compile_wasm_module(engine, runtime.bytes()).unwrap());
    let realm = RealmBuilder::new().build();
    let mut store = WasmtimeStore::new(
        engine,
        WasmHostState {
            user_data: None,
            embedding: None,
            weak: Default::default(),
            native_source_compiler: None,
            native_template_instances: Default::default(),
            monotonic_clock_origin: realm.host_clock().monotonic_instant(),
            realm,
            output_events: WasmOutputEvents::DelegateOnly,
            intl_kernel: intl_data_images::WasmIntlKernel::Absent,
            can_block: false,
            async_waiters: WasmStoreAsyncWaiters::new(None),
            shared_memory_backing: None,
            agent_group: None,
            agent_commands: None,
            agent_leaving: None,
            limits: WasmtimeStoreLimitsBuilder::new().build(),
        },
    );
    store.set_epoch_deadline(u64::MAX / 2);
    let mut linker = WasmtimeLinker::new(engine);
    host_imports::register(&mut linker, &module).unwrap();
    for import in module.imports() {
        if let WasmtimeExternType::Memory(ty) = import.ty() {
            if ty.is_shared() {
                let memory = WasmtimeSharedMemory::new(engine, ty).unwrap();
                store.data_mut().shared_memory_backing =
                    Some(WasmSharedMemoryBacking::new(memory.clone()));
                linker
                    .define(&store, import.module(), import.name(), memory)
                    .unwrap();
            } else {
                let memory = WasmtimeMemory::new(&mut store, ty).unwrap();
                linker
                    .define(&store, import.module(), import.name(), memory)
                    .unwrap();
            }
        }
    }
    let instance = linker.instantiate(&mut store, &module).unwrap();
    (store, linker, instance)
}

#[test]
fn native_context_retains_original_runtime_execution_and_armed_deadline() {
    let engine = shared_wasm_engine().unwrap();
    let runtime = runtime();
    let (mut store, mut linker, instance) = owner(&engine, &runtime);
    let execution = Arc::new(WasmAgentExecutionControl::new());
    execution.arm(Instant::now(), None);
    linker
        .func_wrap("context-control", "value", || 29i32)
        .unwrap();
    let context = NativeSourceCompilerContext::new(
        &linker,
        instance,
        Arc::clone(&runtime),
        WasmCompilationPolicy::from_options(&CompileOptions::default()),
        Arc::clone(&execution),
        WasmNativeCompilationMode::Fast,
    )
    .unwrap();
    assert!(Arc::ptr_eq(context.runtime(), &runtime));
    assert!(Arc::ptr_eq(&context.execution, &execution));
    assert_eq!(context.execution().remaining().unwrap(), None);
    assert!(context.check_budget().is_ok());
    let ns = scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE;
    let factory = scriptw_aot_wasm::NATIVE_REGEXP_LITERAL_FACTORY_EXPORT;
    assert!(
        context.linker.get(&mut store, ns, factory).is_err(),
        "the context must not eagerly publish R"
    );
    assert!(context
        .resolve_import(&mut store, ns, factory)
        .unwrap()
        .into_func()
        .is_some());
    assert!(context
        .resolve_import(&mut store, ns, "not-an-R-export")
        .is_err());
    let host = context
        .resolve_import(&mut store, "context-control", "value")
        .unwrap()
        .into_func()
        .unwrap();
    assert_eq!(
        host.typed::<(), i32>(&store)
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        29
    );
    assert!(
        context.linker.get(&mut store, ns, factory).is_err(),
        "lazy resolution must not publish the export"
    );
}

#[test]
fn native_context_does_not_arm_unarmed_or_refresh_expired_execution() {
    let engine = shared_wasm_engine().unwrap();
    let runtime = runtime();
    let (_store, linker, instance) = owner(&engine, &runtime);
    for expired in [false, true] {
        let execution = Arc::new(WasmAgentExecutionControl::new());
        if expired {
            execution.arm(Instant::now() - Duration::from_secs(1), Some(1));
        }
        let result = NativeSourceCompilerContext::new(
            &linker,
            instance,
            Arc::clone(&runtime),
            WasmCompilationPolicy::from_options(&CompileOptions::default()),
            Arc::clone(&execution),
            WasmNativeCompilationMode::Fast,
        );
        let error = result
            .err()
            .expect("the original invalid deadline is retained");
        assert_eq!(
            error.wasm_execution_failure_kind(),
            Some(if expired {
                WasmExecutionFailureKind::Timeout
            } else {
                WasmExecutionFailureKind::Trap
            })
        );
        assert!(execution.remaining().is_err());
    }
}

#[test]
fn native_context_profile_mismatch_is_an_admission_error_without_syntax_reclassification() {
    let engine = shared_wasm_engine().unwrap();
    let execution = Arc::new(WasmAgentExecutionControl::new());
    execution.arm(Instant::now(), None);
    let runtime = runtime();
    let (_store, linker, instance) = owner(&engine, &runtime);
    let error = NativeSourceCompilerContext::new(
        &linker,
        instance,
        runtime,
        WasmCompilationPolicy::from_options(&CompileOptions {
            intl_profile: IntlCompilationProfile::Conformance,
            ..CompileOptions::default()
        }),
        execution,
        WasmNativeCompilationMode::Fast,
    )
    .err()
    .unwrap();
    assert!(error
        .to_string()
        .contains("native source runtime admission failed"));
    assert!(error.parse_diagnostic().is_none());
    assert!(error.ir_diagnostic().is_none());
}

#[test]
fn native_function_and_regexp_eval_resolve_new_exports_from_the_original_r() {
    configure_compilation_jobs(1).unwrap();
    fn converted(text: &str) -> String {
        format!(
            "String.fromCharCode({})",
            text.encode_utf16()
                .map(|unit| unit.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
    }
    let function = "var a=[{n:17},undefined]; return a.map(function(x){return x===undefined ? 2 : x.n;}).join(':');";
    let regexp = "/ab+/g";
    let source = format!(
        "var f=Function({});print(f());var re=eval({});gc();print(re.test('abbb'));",
        converted(function),
        converted(regexp)
    );
    let function_observer =
        Arc::new(wasm_native_function_compiler::NativeFunctionCompilationObserver::default());
    let eval_observer =
        Arc::new(wasm_native_direct_eval_compiler::NativeDirectEvalCompilationObserver::default());
    let mut compiler = Engine::new(RealmBuilder::new().build());
    compiler.native_function_compilation_observer = Some(Arc::clone(&function_observer));
    compiler.native_direct_eval_compilation_observer = Some(Arc::clone(&eval_observer));
    let unit = compiler
        .compile_script(
            &source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
        )
        .unwrap();
    let artifact = run_on_sized_stack(|| {
        compiler.emit_wasm_with_cache_on_current_thread(
            &unit,
            None,
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,
        )
    })
    .unwrap();
    // The reserved RegExp factory is intentionally not an ordinary P import.
    for payload in wasmparser::Parser::new(0).parse_all(&artifact.bytes) {
        if let wasmparser::Payload::ImportSection(imports) = payload.unwrap() {
            for import in imports.into_imports() {
                let import = import.unwrap();
                assert!(
                    !(import.module == scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE
                        && import.name == scriptw_aot_wasm::NATIVE_REGEXP_LITERAL_FACTORY_EXPORT)
                );
            }
        }
    }
    let outcome = run_on_sized_stack(|| {
        compiler
            .execute_with_wasm_bytes_inner(
                artifact.program(),
                Some(30_000),
                false,
                WasmModuleMemoryCachePolicy::Retain,
                &WasmExecutionMode::Structured,
            )
            .and_then(WasmExecutionOutcome::into_structured)
    })
    .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        vec![
            HostOutputEvent::PrintLine("17:2".into()),
            HostOutputEvent::PrintLine("true".into())
        ]
    );
    let function = function_observer.snapshot();
    assert_eq!(
        (
            function.requests,
            function.completed_factories,
            function.syntax_rejections
        ),
        (1, 1, 0)
    );
    let eval = eval_observer.snapshot();
    assert_eq!(
        (
            eval.requests,
            eval.completed_factories,
            eval.syntax_rejections
        ),
        (1, 1, 0)
    );
}
