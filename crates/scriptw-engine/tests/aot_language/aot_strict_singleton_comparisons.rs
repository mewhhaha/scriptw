use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_singleton_fixture(source: &str) {
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
            .expect("singleton comparisons execute compiled Wasm");
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
fn strict_singletons_preserve_tags_operand_order_and_whole_abrupts() {
    assert_singleton_fixture(include_str!("../fixtures/strict_singleton_comparisons.js"));
}

#[test]
fn strict_singletons_do_not_admit_loose_html_dda_equality() {
    assert_singleton_fixture(include_str!("../fixtures/strict_singleton_html_dda.js"));
}
