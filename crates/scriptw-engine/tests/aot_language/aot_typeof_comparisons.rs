use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_typeof_fixture(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    for directive in ["", "\"use strict\";"] {
        let source = format!("{directive}\n{source}");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                CompileOptions {
                    host_surface_policy: HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(30_000),
                    ..RunOptions::default()
                },
            )
            .expect("typeof comparisons execute compiled Wasm");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            observed.completion
        );
        assert_eq!(
            observed.output_events,
            [HostOutputEvent::PrintLine("ok".into())]
        );
    }
}

#[test]
fn literal_typeof_comparisons_keep_all_tags_callability_effects_and_abrupts() {
    assert_typeof_fixture(include_str!("../fixtures/typeof_literal_comparisons.js"));
}

#[test]
fn literal_typeof_comparisons_keep_html_dda_local_to_the_actual_exotic() {
    assert_typeof_fixture(include_str!("../fixtures/typeof_html_dda_comparisons.js"));
}
