use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn infallible_boolean_queries_preserve_operand_effects_and_completion_boundaries() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for directive in ["", "\"use strict\";\n"] {
        let source = format!(
            "{directive}{}\n{}",
            include_str!("../fixtures/infallible_booleans.js"),
            // Engine-only host exotic; the ordinary fixture remains a Node control.
            r#"
var dda = __scriptwCreateHTMLDDA();
same(truth(dda), false, "HTMLDDA ToBoolean");
same(strict(dda, undefined), false, "HTMLDDA strict equality");
same(dda == null, true, "HTMLDDA loose fallback");
"#
        );
        let result = Engine::new(RealmBuilder::new().build())
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
            .expect("infallible Boolean queries execute compiled Wasm");
        assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(result.completion, ObservedCompletion::Normal(_)),
            "{:?}",
            result.completion
        );
        assert_eq!(
            result.output_events,
            [HostOutputEvent::PrintLine("infallible-booleans:ok".into())]
        );
    }
}
