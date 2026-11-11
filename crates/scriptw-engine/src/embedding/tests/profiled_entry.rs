//! Profile work on the caller thread and include synchronous host callback work.
use super::*;

fn script_runtime(source: &str) -> EmbeddedRuntime {
    configure_compilation_jobs(1).unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_script(source, CompileOptions::default())
        .unwrap();
    let artifact = compiler.emit_wasm(&unit).unwrap();
    EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap()
}

fn fresh_store(runtime: &EmbeddedRuntime, realm: Realm) -> WasmtimeStore<WasmStoreState> {
    let state = runtime.new_state(realm, ()).unwrap();
    let mut store = WasmtimeStore::new(&runtime.engine, state);
    store.limiter(|state| state.limits_mut());
    store
}

fn entry(runtime: &EmbeddedRuntime, store: &mut WasmtimeStore<WasmStoreState>) -> EmbeddedInstance {
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime.register(store, &mut linker).unwrap();
    runtime.instantiate(store, &mut linker).unwrap()
}

#[test]
#[cfg(target_os = "linux")]
fn profiled_evaluation_samples_caller_execution_and_host_subspan() {
    use std::{thread::ThreadId, time::Duration};

    struct CpuHook {
        caller: ThreadId,
        caller_clock: WasmThreadCpuInstant,
        spans: Arc<Mutex<Vec<(ThreadId, Duration)>>>,
    }
    impl HostHooks for CpuHook {
        fn print_line(&self, _: &str) {
            let thread = std::thread::current().id();
            assert_eq!(thread, self.caller);
            let start = WasmThreadCpuInstant::now().unwrap();
            assert!(matches!(
                start.duration_since(&self.caller_clock),
                WasmThreadCpuObservation::Sampled(_)
            ));
            let mut value = 1_u64;
            for _ in 0..2_000_000 {
                value = std::hint::black_box(value.wrapping_mul(17).wrapping_add(3));
            }
            let WasmThreadCpuObservation::Sampled(cpu) = start.elapsed() else {
                panic!("host callback moved away from the caller's execution thread");
            };
            self.spans.lock().unwrap().push((thread, cpu));
        }
    }

    let runtime =
        script_runtime("function nested(){print('nested');return 42;} print('outer');nested();");
    let spans = Arc::new(Mutex::new(Vec::new()));
    let caller = std::thread::current().id();
    let realm = RealmBuilder::new()
        .with_host_hooks(Box::new(CpuHook {
            caller,
            caller_clock: WasmThreadCpuInstant::now().unwrap(),
            spans: Arc::clone(&spans),
        }))
        .build();
    let mut store = fresh_store(&runtime, realm);
    let (outcome, cpu) = entry(&runtime, &mut store)
        .evaluate_profiled(&mut store)
        .unwrap();
    assert_eq!(
        outcome.output_events,
        [
            HostOutputEvent::PrintLine("outer".into()),
            HostOutputEvent::PrintLine("nested".into()),
        ]
    );
    let spans = spans.lock().unwrap();
    assert_eq!(spans.len(), 2);
    assert!(spans.iter().all(|(thread, _)| *thread == spans[0].0));
    let hook_cpu: Duration = spans.iter().map(|(_, cpu)| *cpu).sum();
    let WasmThreadCpuObservation::Sampled(worker_cpu) = cpu else {
        panic!("Linux caller-thread CPU is available");
    };
    assert!(
        worker_cpu >= hook_cpu,
        "call span encloses both host subspans"
    );
}

#[test]
fn profiled_evaluation_preserves_nested_normal_and_throw_outcomes() {
    for (source, throws) in [
        (
            "function nested(){print('normal');return -0;} nested();",
            false,
        ),
        (
            "function nested(){print('throw');throw 'marker';} nested();",
            true,
        ),
    ] {
        let runtime = script_runtime(source);
        let mut ordinary_store = fresh_store(&runtime, RealmBuilder::new().build());
        let ordinary = entry(&runtime, &mut ordinary_store)
            .evaluate(&mut ordinary_store)
            .unwrap();
        let mut profiled_store = fresh_store(&runtime, RealmBuilder::new().build());
        let (profiled, _) = entry(&runtime, &mut profiled_store)
            .evaluate_profiled(&mut profiled_store)
            .unwrap();
        assert_eq!(profiled, ordinary);
        assert_eq!(
            matches!(profiled.completion, ObservedCompletion::Throw(_)),
            throws
        );
    }
}

#[test]
fn profiled_initialization_preserves_pending_and_reinitialization_errors() {
    let artifact = artifact(
        "await new Promise(() => {});",
        ForeignModuleRegistry::default(),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let mut ordinary_store = fresh_store(&runtime, RealmBuilder::new().build());
    let mut ordinary = entry(&runtime, &mut ordinary_store);
    assert!(matches!(
        ordinary.initialize(&mut ordinary_store).unwrap(),
        EmbeddedEvaluation::Pending
    ));
    let ordinary_error = ordinary.initialize(&mut ordinary_store).unwrap_err();
    let mut profiled_store = fresh_store(&runtime, RealmBuilder::new().build());
    let mut profiled = entry(&runtime, &mut profiled_store);
    assert!(matches!(
        profiled.initialize_profiled(&mut profiled_store).unwrap().0,
        EmbeddedEvaluation::Pending
    ));
    let profiled_error = profiled
        .initialize_profiled(&mut profiled_store)
        .unwrap_err();
    assert_eq!(profiled_error.to_string(), ordinary_error.to_string());

    let mut ordinary_store = fresh_store(&runtime, RealmBuilder::new().build());
    let ordinary_error = entry(&runtime, &mut ordinary_store)
        .evaluate(&mut ordinary_store)
        .unwrap_err();
    let mut profiled_store = fresh_store(&runtime, RealmBuilder::new().build());
    let profiled_error = entry(&runtime, &mut profiled_store)
        .evaluate_profiled(&mut profiled_store)
        .unwrap_err();
    assert_eq!(profiled_error.to_string(), ordinary_error.to_string());
    assert_eq!(
        profiled_error.wasm_execution_failure_kind(),
        ordinary_error.wasm_execution_failure_kind()
    );
}

#[test]
fn profiled_initialization_preserves_fatal_host_error_and_failed_state() {
    let artifact = artifact("import {add} from 'dd:host'; add(1,2);", registry(false));
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let mut errors = Vec::new();
    for profiled in [false, true] {
        let mut store = fresh_store(&runtime, RealmBuilder::new().build());
        let mut linker = WasmtimeLinker::new(&runtime.engine);
        linker
            .func_wrap("dd", "add", |_: f64, _: f64| -> wasmtime::Result<f64> {
                Err(wasmtime::Error::msg("profiled-fatal-host"))
            })
            .unwrap();
        linker.func_wrap("dd", "record", |_: f64| {}).unwrap();
        runtime.register(&mut store, &mut linker).unwrap();
        let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
        let error;
        if profiled {
            error = instance.initialize_profiled(&mut store).unwrap_err();
        } else {
            error = instance.initialize(&mut store).unwrap_err();
        }
        assert!(error.to_string().contains("profiled-fatal-host"));
        assert!(store.data().embedding.as_ref().unwrap().phase == StorePhase::Failed);
        errors.push((
            error.wasm_execution_failure_kind(),
            instance.initialize(&mut store).unwrap_err().to_string(),
        ));
    }
    assert_eq!(errors[0], errors[1]);
}
