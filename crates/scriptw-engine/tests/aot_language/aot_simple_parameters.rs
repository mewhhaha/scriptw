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
function localDuplicate(a,a) { return a; }
if (localDuplicate(1,2)!==2 || localDuplicate(1)!==undefined) throw 'duplicate local parameter';
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

#[test]
fn parameter_sealing_preserves_initializer_tdz_and_escaped_parameter_cells() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for module in [false, true] {
        let source = r#"
var marker = {}, escaped, trace = [];
function same(actual, expected) {
    if (!Object.is(actual, expected)) throw 'parameter-seal value';
}
function referenceError(action) {
    var caught = false;
    try { action(); } catch (error) {
        if (!(error instanceof ReferenceError)) throw error;
        caught = true;
    }
    if (!caught) throw 'parameter-seal missing TDZ';
}
function later(first = second, second = 2) { return first + second; }
function self(value = value) { return value; }
function pattern([first = second], second = 2) { return first + second; }
referenceError(function () { later(); });
referenceError(function () { self(); });
referenceError(function () { pattern([]); });
same(later(3), 5);
same(pattern([3]), 5);
function ordered(first = (trace.push('first'), 3), second = (trace.push('second'), first)) {
    first = second + 1;
    return first + second;
}
same(ordered(), 7);
same(trace.join(','), 'first,second');
function abort(first = (escaped = function () { return second; }, 1),
               second = (function () { throw marker; })()) { return first; }
try { abort(); throw 'parameter-seal missing throw'; } catch (error) { same(error, marker); }
gc();
referenceError(escaped);
function split(value = 3, read = function () { return value; }) {
    var value;
    value = 9;
    return [read(), value];
}
var result = split();
same(result[0], 3); same(result[1], 9);
function evalParameter(first, second) { return eval('first = second; first'); }
same(evalParameter(1, marker), marker);
function* resumed(value) { yield value; value = marker; return value; }
var iterator = resumed(-0);
same(iterator.next().value, -0); gc(); same(iterator.next().value, marker);
print('parameter-seal:ok');
"#;
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
            [HostOutputEvent::PrintLine("parameter-seal:ok".into())]
        );
    }
}
