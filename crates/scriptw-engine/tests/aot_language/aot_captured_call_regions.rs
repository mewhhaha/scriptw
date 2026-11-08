use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_module(
            source,
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
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn captured_regions_preserve_shared_cells_ordered_fallback_and_callable_replacement() {
    run(
        include_str!("../fixtures/captured_call_regions.js"),
        "captured call regions ok",
    );
}

#[test]
fn captured_regions_keep_foreign_realms_zero_trip_and_depth_fallback() {
    run(
        r#"
var rows=[{x:1},{x:2},{x:3},{x:4}], step;
function make(start) { var tick=start; return function(row,index) { 'use strict'; tick=tick+1; return row.x+index+tick; }; }
step=make(0);
function run() { var sum=0; for(var i=0;i<16;i++)sum+=step(rows[i&3],i); return sum; }
var foreign=__scriptwCreateRealm();
var ForeignTypeError=foreign.evalScript('TypeError');
step=foreign.evalScript('(function(row,index){"use strict";return row.x+index;})');
if(run()!==160)throw new Error('foreign live callable');
rows[0]=null;
try{run();throw new Error('missing throw');}catch(error){if(!(error instanceof ForeignTypeError)||error instanceof TypeError)throw new Error('foreign realm');}
rows=[{x:1},{x:2},{x:3},{x:4}];
var observed=0;
step=new Proxy(function(row,index){return row.x+index;},{apply:function(target,receiver,args){observed++;return Reflect.apply(target,receiver,args);}});
function empty() {var sum=0;for(var i=16;i<16;i++)sum+=step(rows[i&3],i);return sum;}
if(empty()!==0||observed!==0)throw new Error('zero trip');
step=make(0);
function recurse(depth){if(depth===0)return run();return recurse(depth-1)+0;}
var overflow=false;
try{recurse(200);}catch(error){if(!(error instanceof RangeError))throw error;overflow=true;}
if(!overflow)throw new Error('source depth');
if(run()!==296)throw new Error('depth restored');
print('captured call boundaries ok');
"#,
        "captured call boundaries ok",
    );
}
