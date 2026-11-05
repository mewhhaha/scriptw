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
fn numeric_body_preflight_keeps_order_partial_writes_reentry_and_real_call_boundaries() {
    let source = include_str!("../fixtures/numeric_body.js");
    for module in [false, true] {
        run(source, module, "numeric body ok");
    }
}
#[test]
fn numeric_body_miss_uses_actual_callable_realm_and_cell_identity() {
    run(
        r#"
var realm=__scriptwCreateRealm();
var make=realm.evalScript('(function make(start){ var tick=start; function step(row,i){ "use strict"; tick=tick+1; return row.x+i+tick; } return {step:step,read:function(){return tick;}}; })');
var one=make(1),two=make(10),ForeignTypeError=realm.evalScript('TypeError');
if(one.step({x:3},2)!==7 || two.step({x:3},2)!==16 || one.read()!==2) throw new Error('foreign cells');
var caught=false;
try{one.step(null,2);}catch(error){caught=true;if(!(error instanceof ForeignTypeError)||error instanceof TypeError)throw new Error('foreign Throw Realm');}
if(!caught||one.read()!==3)throw new Error('foreign completed prefix');
var holder={step:one.step},count=0,old=holder.step;
var result=holder.step((holder.step=two.step,count++,{x:1}),2);
if(result!==7||one.read()!==4||two.read()!==11||count!==1||holder.step===old)throw new Error('held callable');
print('numeric body Realm ok');
"#,
        false,
        "numeric body Realm ok",
    );
}
