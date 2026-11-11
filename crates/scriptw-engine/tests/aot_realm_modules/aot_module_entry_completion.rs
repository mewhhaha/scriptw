use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, ObservedCompletion, ObservedJsValue,
    PromiseRejectionPolicy, RealmBuilder, RunOptions, WasmExecutionFailureKind,
};

fn engine() -> Engine {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    Engine::new(RealmBuilder::new().build())
}

fn execution() -> RunOptions {
    RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    }
}

fn options(policy: PromiseRejectionPolicy) -> CompileOptions {
    CompileOptions {
        promise_rejection_policy: policy,
        ..CompileOptions::default()
    }
}

const POLICIES: [PromiseRejectionPolicy; 2] = [
    PromiseRejectionPolicy::Ignore,
    PromiseRejectionPolicy::FailRun,
];

#[test]
fn prelude_failure_never_claims_a_rooted_entry_snapshot() {
    // Constructor metadata describes the actual thrown value. It does not
    // imply that Module entry evaluation started or produced a rooted snapshot.
    for (reason, rendered, constructor) in [
        ("7", "7", None),
        ("undefined", "undefined", None),
        ("({})", "object", Some("Object")),
        (
            "({ constructor: { name: 'PreludeReason' } })",
            "object",
            Some("PreludeReason"),
        ),
        ("Object.create(null)", "object", None),
        (
            "({ get constructor() { throw new Error('getter must not execute'); } })",
            "object",
            None,
        ),
    ] {
        let failure = engine()
            .observe_module(
                "throw new RangeError('entry must not execute');",
                CompileOptions {
                    module_prelude: Some(format!("throw {reason};")),
                    ..CompileOptions::default()
                },
                execution(),
            )
            .expect_err("prelude is setup, before entry observation");
        assert!(failure.is_global_prelude_failure());
        assert_eq!(
            failure.wasm_execution_failure_kind(),
            Some(WasmExecutionFailureKind::JavaScriptException)
        );
        assert_eq!(
            failure.wasm_javascript_exception_constructor_name(),
            constructor,
            "reason: {reason}, failure: {failure}"
        );
        assert!(failure.message().contains(rendered), "{failure}");
        assert!(!failure.message().contains("entry must not execute"));
    }
}

#[test]
fn synchronous_and_fulfilled_async_entries_return_only_undefined() {
    for policy in POLICIES {
        for source in ["42;", "await 0; 42;", "await Promise.resolve(42); 99;"] {
            let observed = engine()
                .observe_module(source, options(policy), execution())
                .expect("completed Module evaluates normally");
            assert_eq!(
                observed.completion,
                ObservedCompletion::Normal(ObservedJsValue::Undefined)
            );
        }
    }
}

#[test]
fn undefined_rejection_before_or_after_await_remains_a_throw_under_both_policies() {
    for policy in POLICIES {
        for source in [
            "throw undefined;",
            "throw undefined; await 0;",
            "await 0; throw undefined;",
            "await Promise.reject(undefined);",
        ] {
            let observed = engine()
                .observe_module(source, options(policy), execution())
                .expect("structured observation retains a JavaScript throw");
            assert_eq!(
                observed.completion,
                ObservedCompletion::Throw(ObservedJsValue::Undefined)
            );
        }
    }
}

#[test]
fn object_rejection_is_not_coerced_or_replaced_by_diagnostic_failure() {
    for policy in POLICIES {
        let observed = engine()
            .observe_module(
                "const reason = { get name() { print('name getter'); throw 1; }, toString() { print('conversion'); throw 2; } }; await 0; throw reason;",
                options(policy),
                execution(),
            )
            .expect("the root rejection is an observed value");
        assert_eq!(
            observed.completion,
            ObservedCompletion::Throw(ObservedJsValue::Object)
        );
        assert!(
            observed.output_events.is_empty(),
            "{:?}",
            observed.output_events
        );
    }
}

#[test]
fn earlier_unrelated_rejection_never_replaces_the_entry_rejection() {
    for policy in POLICIES {
        for source in [
            "Promise.reject(new RangeError('background marker')); await 0; throw new TypeError('entry marker');",
            "Promise.reject(new RangeError('background marker')); throw new TypeError('entry marker'); await 0;",
        ] {
            let failure = engine()
                .run_module(source, options(policy), execution())
                .expect_err("entry rejection is primary");
            assert_eq!(
                failure.wasm_execution_failure_kind(),
                Some(WasmExecutionFailureKind::JavaScriptException)
            );
            assert_eq!(
                failure.wasm_javascript_exception_constructor_name(),
                Some("TypeError")
            );
            assert!(failure.message().contains("entry marker"), "{failure}");
        }
    }
}

#[test]
fn fulfilled_entry_keeps_the_selected_background_rejection_policy() {
    let source =
        "Promise.reject(new RangeError('background marker')); await 0; print('entry finished');";
    let ignored = engine()
        .observe_module(source, options(PromiseRejectionPolicy::Ignore), execution())
        .unwrap();
    assert_eq!(
        ignored.completion,
        ObservedCompletion::Normal(ObservedJsValue::Undefined)
    );
    assert_eq!(
        ignored.output_events,
        vec![HostOutputEvent::PrintLine("entry finished".into())]
    );
    let failure = engine()
        .run_module(
            source,
            options(PromiseRejectionPolicy::FailRun),
            execution(),
        )
        .unwrap_err();
    assert_eq!(
        failure.wasm_javascript_exception_constructor_name(),
        Some("RangeError")
    );
    assert!(failure.message().contains("background marker"));
}

#[test]
fn quiescent_pending_entry_is_a_host_outcome_for_run_and_observe() {
    for policy in POLICIES {
        for source in [
            "await new Promise(() => {});",
            "Promise.reject('background'); await new Promise(() => {});",
        ] {
            let engine = engine();
            let failure = engine
                .run_module(source, options(policy), execution())
                .unwrap_err();
            assert_eq!(
                failure.wasm_execution_failure_kind(),
                Some(WasmExecutionFailureKind::IncompleteModuleEvaluation)
            );
            assert_eq!(failure.wasm_javascript_exception_constructor_name(), None);
            let failure = engine
                .observe_module(source, options(policy), execution())
                .unwrap_err();
            assert_eq!(
                failure.wasm_execution_failure_kind(),
                Some(WasmExecutionFailureKind::IncompleteModuleEvaluation)
            );
            assert_eq!(failure.wasm_javascript_exception_constructor_name(), None);
        }
    }
}

#[test]
fn script_async_lookalike_does_not_adopt_a_module_entry() {
    let observed = engine()
        .observe_script(
            "void (async () => { await new Promise(() => {}); })(); 17;",
            options(PromiseRejectionPolicy::Ignore),
            execution(),
        )
        .expect("an ordinary Script has no Module entry completion owner");
    assert!(matches!(observed.completion, ObservedCompletion::Normal(_)));
}

#[test]
fn prelude_throw_survives_unstarted_entry_and_background_jobs() {
    for policy in POLICIES {
        let failure = engine()
            .run_module(
                "await new Promise(() => {});",
                CompileOptions {
                    module_prelude: Some("Promise.reject(new RangeError('background')); throw new TypeError('prelude marker');".into()),
                    ..options(policy)
                },
                execution(),
            )
            .unwrap_err();
        assert_eq!(
            failure.wasm_execution_failure_kind(),
            Some(WasmExecutionFailureKind::JavaScriptException)
        );
        assert_eq!(
            failure.wasm_javascript_exception_constructor_name(),
            Some("TypeError")
        );
        assert!(failure.message().contains("prelude marker"), "{failure}");
    }
}

#[test]
fn entry_adoption_uses_no_mutable_then_or_species_lookup() {
    let observed = engine()
        .observe_module(
            r#"
Object.defineProperty(Promise.prototype, 'then', { get() { print('then getter'); throw 1; } });
Object.defineProperty(Promise, Symbol.species, { get() { print('species getter'); throw 2; } });
await 0;
print('settled');
"#,
            options(PromiseRejectionPolicy::Ignore),
            execution(),
        )
        .unwrap();
    assert_eq!(
        observed.completion,
        ObservedCompletion::Normal(ObservedJsValue::Undefined)
    );
    assert_eq!(
        observed.output_events,
        vec![HostOutputEvent::PrintLine("settled".into())]
    );
}

#[test]
fn supported_host_async_work_drains_before_entry_status_is_sampled() {
    let observed = engine()
        .observe_module(
            "const cells = new Int32Array(new SharedArrayBuffer(4)); await Atomics.waitAsync(cells, 0, 0, 2).value; print('timer settled');",
            options(PromiseRejectionPolicy::Ignore),
            execution(),
        )
        .unwrap();
    assert_eq!(
        observed.completion,
        ObservedCompletion::Normal(ObservedJsValue::Undefined)
    );
    assert_eq!(
        observed.output_events,
        vec![HostOutputEvent::PrintLine("timer settled".into())]
    );
}

#[test]
fn pending_entry_after_supported_host_work_is_still_incomplete() {
    for policy in POLICIES {
        let failure = engine()
            .observe_module(
                "const cells = new Int32Array(new SharedArrayBuffer(4)); await Atomics.waitAsync(cells, 0, 0, 2).value; await new Promise(() => {});",
                options(policy),
                execution(),
            )
            .unwrap_err();
        assert_eq!(
            failure.wasm_execution_failure_kind(),
            Some(WasmExecutionFailureKind::IncompleteModuleEvaluation)
        );
        assert_eq!(failure.wasm_javascript_exception_constructor_name(), None);
    }
}

struct Modules(PathBuf);

impl Drop for Modules {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn transitive_tla_rejection_and_pending_state_belong_to_the_entry_evaluation() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let modules = Modules(std::env::temp_dir().join(format!(
        "scriptw-module-entry-completion-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&modules.0).unwrap();
    let entry = "import './middle.js'; print('entry ran');";
    std::fs::write(modules.0.join("entry.js"), entry).unwrap();
    std::fs::write(
        modules.0.join("middle.js"),
        "import './leaf.js'; print('middle ran');",
    )
    .unwrap();
    for policy in POLICIES {
        let options = CompileOptions {
            filename: Some(modules.0.join("entry.js").to_str().unwrap().into()),
            module_root: Some(modules.0.to_str().unwrap().into()),
            ..options(policy)
        };
        std::fs::write(modules.0.join("leaf.js"), "await 0; throw undefined;").unwrap();
        let observed = engine()
            .observe_module(entry, options.clone(), execution())
            .unwrap();
        assert_eq!(
            observed.completion,
            ObservedCompletion::Throw(ObservedJsValue::Undefined)
        );
        assert!(observed.output_events.is_empty());
        std::fs::write(modules.0.join("leaf.js"), "await new Promise(() => {});").unwrap();
        let failure = engine()
            .observe_module(entry, options.clone(), execution())
            .unwrap_err();
        assert_eq!(
            failure.wasm_execution_failure_kind(),
            Some(WasmExecutionFailureKind::IncompleteModuleEvaluation)
        );
        std::fs::write(modules.0.join("leaf.js"), "await 0; print('leaf ran');").unwrap();
        let observed = engine()
            .observe_module(entry, options, execution())
            .unwrap();
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Undefined)
        );
        assert_eq!(
            observed.output_events,
            ["leaf ran", "middle ran", "entry ran"]
                .into_iter()
                .map(|line| HostOutputEvent::PrintLine(line.into()))
                .collect::<Vec<_>>()
        );
    }
}
