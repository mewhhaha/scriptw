use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run_script(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let compile = CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    };
    let options = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    };
    let outcome = engine.observe_script(source, compile, options).unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn sloppy_functions_share_one_resolved_arguments_binding() {
    run_script(
        include_str!("../fixtures/arguments_source_resolution.js"),
        "arguments resolution ok",
    );
}

#[test]
fn script_global_arguments_never_precedes_an_implicit_binding() {
    run_script(
        include_str!("../fixtures/arguments_global_resolution.js"),
        "global arguments ok",
    );
}
