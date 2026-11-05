use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn initialized_simple_parameters_keep_values_reentry_and_excluded_protocols() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for module in [false, true] {
        let engine = Engine::new(RealmBuilder::new().build());
        let source = include_str!("../fixtures/simple_parameters.js");
        let compile = CompileOptions {
            host_surface_policy: HostSurfacePolicy::Test262,
            ..CompileOptions::default()
        };
        let run = RunOptions {
            backend: ExecutionBackend::WasmAot,
            timeout_ms: Some(30_000),
            ..RunOptions::default()
        };
        let observed = if module {
            engine.observe_module(source, compile, run)
        } else {
            engine.observe_script(source, compile, run)
        }
        .unwrap();
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "module={module}: {:?}",
            observed.completion
        );
        assert_eq!(
            observed.output_events,
            [HostOutputEvent::PrintLine("simple-parameters:ok".into())]
        );
    }
}

#[test]
fn sloppy_duplicate_mapping_and_simple_parameter_eval_keep_their_cells() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_script(
            r#"
function duplicate(a,a) {
    var first=arguments[0]; a=9;
    if (arguments.length===1) return first===1 && arguments[0]===1 && a===9;
    return first===1 && arguments[1]===9 && a===9;
}
function evaluated(a,b,c) { 'use strict'; return eval('a = b; a')===b && a===b; }
if (!duplicate(1,2) || !duplicate(1) || !evaluated(3,7,0)) throw 'parameter cell regression';
print('parameter-cells:ok');
"#,
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
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine("parameter-cells:ok".into())]
    );
}
