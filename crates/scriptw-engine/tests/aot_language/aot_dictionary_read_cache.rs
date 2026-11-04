use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn dictionary_hints_revalidate_live_entries_without_caching_values_or_exotic_semantics() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for directive in ["", "\"use strict\";\n"] {
        let source = format!(
            "{directive}{}",
            include_str!("../fixtures/dictionary_read_cache.js")
        );
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
            .unwrap();
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{directive:?}: {:?}",
            observed.completion
        );
        assert_eq!(
            observed.output_events,
            [HostOutputEvent::PrintLine(
                "dictionary-read-cache:ok".into()
            )]
        );
    }
}
