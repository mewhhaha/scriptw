use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn read_only_packed_descriptors_keep_snapshots_holes_and_later_mutation_semantics() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for directive in ["", "'use strict';\n"] {
        let source = format!(
            "{directive}{}",
            include_str!("../fixtures/packed_array_descriptor_reads.js")
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
            .expect("packed descriptor fixture executes through actual Wasm");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}",
            observed.completion
        );
        assert_eq!(
            observed.output_events,
            [HostOutputEvent::PrintLine("packed-descriptor-reads".into())]
        );
    }
}
