use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, module: bool, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let compile = CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    };
    let options = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    };
    let outcome = if module {
        engine.observe_module(source, compile, options)
    } else {
        engine.observe_script(source, compile, options)
    }
    .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "module={module}: {:?}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn strict_arguments_snapshots_preserve_absent_getters_reentry_and_whole_values() {
    let source = include_str!("../fixtures/strict_arguments_projection.js");
    for module in [false, true] {
        run(source, module, "strict arguments projection ok");
    }
}

#[test]
fn sloppy_mapped_arguments_and_strict_unmapped_arguments_remain_distinct() {
    run(
        r#"
function sloppy(a){ a=7; return arguments[0]; }
function strict(a){ 'use strict'; a=7; return arguments[0]; }
function duplicate(a,a){ a=9; return [arguments[0],arguments[1]]; }
var duplicateResult=duplicate(1,2);
if (sloppy(1)!==7 || strict(1)!==1 || duplicateResult[0]!==1 || duplicateResult[1]!==9) throw new Error('mapping');
print('mapped control ok');
"#,
        false,
        "mapped control ok",
    );
}

#[test]
fn absent_index_uses_the_defining_realm_object_and_getter_error() {
    run(
        r#"
var other=__scriptwCreateRealm();
var projected=other.evalScript('(function project(value) { "use strict"; return [arguments[0],arguments.length]; })');
var ForeignTypeError=other.evalScript('TypeError');
var marker={}, result=projected(marker,2,3);
if(result[0]!==marker || result[1]!==3) throw new Error('foreign snapshot');
other.evalScript('Object.defineProperty(Object.prototype,"0",{configurable:true,get:function(){throw new TypeError("absent");}});');
var caught=false;
try { projected(); } catch(error) {
  caught=true;
  if(!(error instanceof ForeignTypeError) || error instanceof TypeError) throw new Error('getter Realm');
}
if(!caught) throw new Error('missing getter Throw');
print('arguments Realm ok');
"#,
        false,
        "arguments Realm ok",
    );
}
