//! The packed Get boundary preserves all values before restoring full Completion.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn cached_read_outcomes_preserve_normal_values_whole_throws_and_realms() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for directive in ["", "\"use strict\";\n"] {
        let fixture = include_str!("../fixtures/cached_read_outcome.js");
        let outcome = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &format!("{directive}{fixture}"),
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
            .expect("cached read outcomes execute compiled Wasm");
        assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(outcome.completion, ObservedCompletion::Normal(_)),
            "{directive:?}: {:?}",
            outcome.completion
        );
        assert_eq!(
            outcome.output_events,
            [HostOutputEvent::PrintLine("cached-read-outcome:ok".into())]
        );
    }
}
