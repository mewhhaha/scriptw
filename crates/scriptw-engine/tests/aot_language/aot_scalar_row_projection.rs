use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn projected_rows_preserve_aliases_numeric_bits_and_whole_loop_fallback_effects() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for (module, directive) in [(true, ""), (false, ""), (false, "\"use strict\";\n")] {
        let source = format!(
            "{directive}{}",
            include_str!("../fixtures/scalar_row_projection.js")
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
            "module={module}, directive={directive:?}: {:?}",
            result.completion
        );
        assert_eq!(
            result.output_events,
            [HostOutputEvent::PrintLine(
                "scalar-row-projection:ok".into()
            )]
        );
    }
}

#[test]
fn projected_own_data_preserves_recipe_and_getter_realms() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let result = engine
        .observe_script(
            include_str!("../fixtures/scalar_row_projection_realms.js"),
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
    assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(result.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        result.completion
    );
    assert_eq!(
        result.output_events,
        [HostOutputEvent::PrintLine(
            "scalar-row-projection-realms:ok".into()
        )]
    );
}
