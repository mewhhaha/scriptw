use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, ObservedCompletion, ObservedJsValue,
    ObservedNumber, PromiseRejectionPolicy, RealmBuilder, RunOptions,
};

fn execution() -> RunOptions {
    RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    }
}

fn ignore_rejections() -> CompileOptions {
    CompileOptions {
        promise_rejection_policy: PromiseRejectionPolicy::Ignore,
        ..CompileOptions::default()
    }
}

#[test]
fn default_host_policy_reports_unhandled_rejections_as_failure() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let error = Engine::new(RealmBuilder::new().build())
        .run_script(
            "Promise.reject(new RangeError('unhandled marker')); 17;",
            CompileOptions::default(),
            execution(),
        )
        .expect_err("the default CLI/library policy reports a failed run");
    assert_eq!(
        error.wasm_javascript_exception_constructor_name(),
        Some("RangeError")
    );
    assert!(error.message().contains("unhandled marker"), "{error}");
}

#[test]
fn default_ecmascript_tracker_preserves_completion_and_runs_all_jobs() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
            r#"
var reason = { toString() { print('unexpected conversion'); throw 'conversion'; } };
Promise.reject(reason);
Promise.reject(reason);
Promise.resolve().then(() => { print('first job'); Promise.resolve().then(() => print('nested job')); });
print('entry');
17;
"#,
            ignore_rejections(),
            execution(),
        )
        .unwrap();
    assert_eq!(
        outcome.completion,
        ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(17.0)))
    );
    assert_eq!(
        outcome.output_events,
        ["entry", "first job", "nested job"]
            .into_iter()
            .map(|line| HostOutputEvent::PrintLine(line.into()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn ignored_host_reports_do_not_change_rejection_delivery() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
            r#"
Promise.reject('handled').catch(value => print(value));
Promise.resolve().then(() => { throw 'reaction'; }).catch(value => print(value));
Promise.any([]).catch(error => print(error instanceof AggregateError));
"#,
            ignore_rejections(),
            execution(),
        )
        .unwrap();
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{outcome:?}"
    );
    assert_eq!(
        outcome.output_events,
        ["handled", "true", "reaction"]
            .into_iter()
            .map(|line| HostOutputEvent::PrintLine(line.into()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn ignoring_unhandled_rejections_preserves_a_primary_script_throw() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let error = Engine::new(RealmBuilder::new().build())
        .run_script(
            "Promise.resolve().then(() => { throw new RangeError('job'); }); throw new TypeError('script marker');",
            ignore_rejections(),
            execution(),
        )
        .expect_err("the actual Script throw remains a failure");
    assert_eq!(
        error.wasm_javascript_exception_constructor_name(),
        Some("TypeError")
    );
    assert!(error.message().contains("script marker"), "{error}");
}

#[test]
fn cached_programs_keep_their_selected_host_policy() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let source = "Promise.reject(new RangeError('cache marker')); 31;";
    for policy in [
        PromiseRejectionPolicy::FailRun,
        PromiseRejectionPolicy::Ignore,
        PromiseRejectionPolicy::FailRun,
        PromiseRejectionPolicy::Ignore,
    ] {
        let outcome = engine.run_script(
            source,
            CompileOptions {
                promise_rejection_policy: policy,
                ..CompileOptions::default()
            },
            execution(),
        );
        match policy {
            PromiseRejectionPolicy::FailRun => {
                assert_eq!(
                    outcome
                        .unwrap_err()
                        .wasm_javascript_exception_constructor_name(),
                    Some("RangeError")
                );
            }
            PromiseRejectionPolicy::Ignore => {
                let outcome = outcome.unwrap();
                assert!(outcome.note.contains("number(31"), "{}", outcome.note);
            }
        }
    }
}

#[test]
fn compiled_units_retain_the_explicit_host_policy() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let source = "Promise.reject('retained'); 23;";
    let unit = engine.compile_script(source, ignore_rejections()).unwrap();
    let artifact = engine.emit_wasm(&unit).unwrap();
    assert!(!artifact.bytes.is_empty());
    let outcome = engine
        .run_compiled_unit(&unit, source, execution())
        .unwrap();
    assert!(outcome.note.contains("number(23"), "{}", outcome.note);
}

#[test]
fn retained_async_module_completion_cannot_be_discarded_by_host_policy() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let source = "await 0; throw new TypeError('module evaluation marker');";
    for options in [ignore_rejections(), CompileOptions::default()] {
        let unit = engine.compile_module(source, options).unwrap();
        assert!(!engine.emit_wasm(&unit).unwrap().bytes.is_empty());
        let error = engine
            .run_compiled_unit(&unit, source, execution())
            .unwrap_err();
        assert_eq!(
            error.wasm_javascript_exception_constructor_name(),
            Some("TypeError")
        );
        assert!(
            error.message().contains("module evaluation marker"),
            "{error}"
        );
    }
}

#[test]
fn synchronous_modules_keep_the_selected_rejection_policy() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for source in [
        "Promise.reject('background'); print('finished');",
        "async function background() { await 0; throw 'background'; } background(); print('finished');",
    ] {
        let outcome = Engine::new(RealmBuilder::new().build())
            .observe_module(source, ignore_rejections(), execution())
            .unwrap();
        assert!(
            matches!(outcome.completion, ObservedCompletion::Normal(_)),
            "{outcome:?}"
        );
        assert_eq!(
            outcome.output_events,
            vec![HostOutputEvent::PrintLine("finished".into())]
        );
    }
}
