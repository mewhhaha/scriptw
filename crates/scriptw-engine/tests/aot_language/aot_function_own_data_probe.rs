//! Current callable own data shares the ordinary read probe and full fallback.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn check(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let harness = include_str!("../fixtures/legacy_callable_reflection/canonical_harness.js");
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
            &format!("{harness}\n{source}"),
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(60_000),
                ..RunOptions::default()
            },
        )
        .expect("current function descriptors execute through compiled Wasm");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine("function-own-data:ok".into())]
    );
}

#[test]
fn function_own_data_reads_preserve_live_descriptors_legacy_tokens_and_exotic_misses() {
    check(include_str!("../fixtures/function_own_data_probe.js"));
}

#[test]
fn function_own_data_reads_preserve_recipe_and_abrupt_completion_realms() {
    check(include_str!("../fixtures/function_own_data_realms.js"));
}
