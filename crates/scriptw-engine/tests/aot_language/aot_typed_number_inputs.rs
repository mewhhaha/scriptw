use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn typed_number_inputs_preserve_current_values_operand_order_and_failure_prefixes() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for (module, directive) in [(true, ""), (false, ""), (false, "'use strict';\n")] {
        let source = format!(
            "{directive}{}",
            include_str!("../fixtures/typed_number_inputs.js")
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
            [HostOutputEvent::PrintLine("typed-number-inputs:ok".into())]
        );
    }
}
