use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn checked_throw_continuations_keep_later_abrupt_values_and_control_joins() {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    for directive in ["", "\"use strict\";"] {
        let source = format!(
            "{directive}\n{}",
            include_str!("../fixtures/completion_guard_continuations.js")
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
            .expect("checked throw continuations execute compiled Wasm");
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
fn expression_results_preserve_statement_values_across_declarations_loops_and_finally() {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    for directive in ["", "'use strict';\n"] {
        for source in [
            "37; { let object={x:1}; let result=object['x'] == 1; }",
            "var source={more:true}, key='more'; while(source[key]) { var ignored=source.more=false; 37; }",
            "try { 37; } finally { var object={x:1}; var result=object['x'] == 1; }",
            "var marker={}; try { throw marker; } catch(error) { error === marker; 37; } finally { var result='1' == 1; }",
            "function read(object,key) { return object[key]; } var object={x:37}; read(object,'x'); var ignored=object['x'] == 37;",
        ] {
            let source = format!("{directive}{source}");
            let observed = Engine::new(RealmBuilder::new().build()).observe_script(
                &source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(30_000),
                    ..RunOptions::default()
                },
            ).expect("catalog expression transport executes compiled Wasm");
            assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
            assert_eq!(observed.completion,
                ObservedCompletion::Normal(scriptw_engine::ObservedJsValue::Number(
                    scriptw_engine::ObservedNumber::from_f64(37.0))), "{source}");
        }
    }
}
