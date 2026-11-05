use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn cyclic_keys_keep_alignment_current_storage_and_original_failure_order() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for (module, directive) in [(true, ""), (false, ""), (false, "'use strict';\n")] {
        let source = format!(
            "{directive}{}",
            include_str!("../fixtures/computed_key_projection.js")
        );
        let engine = Engine::new(RealmBuilder::new().build());
        let compile = CompileOptions {
            host_surface_policy: HostSurfacePolicy::Test262,
            ..CompileOptions::default()
        };
        let run = RunOptions {
            backend: ExecutionBackend::WasmAot,
            timeout_ms: Some(30_000),
            ..RunOptions::default()
        };
        let result = if module {
            engine.observe_module(&source, compile, run)
        } else {
            engine.observe_script(&source, compile, run)
        }
        .unwrap();
        assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(result.completion, ObservedCompletion::Normal(_)),
            "module={module} directive={directive:?}: {:?}",
            result.completion
        );
        assert_eq!(
            result.output_events,
            [HostOutputEvent::PrintLine(
                "computed-key-projection:ok".into()
            )]
        );
    }
}
