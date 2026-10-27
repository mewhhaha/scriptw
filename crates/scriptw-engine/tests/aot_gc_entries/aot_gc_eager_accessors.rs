//! Eager intrinsic accessor allocation keeps ordinary native function semantics.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion, RealmBuilder,
    RunOptions,
};

fn check(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for directive in ["", "'use strict';\n"] {
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &format!("{directive}function assert(v,m){{if(!v)throw new Error(m);}}\n{source}"),
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
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            observed.completion
        );
    }
}

#[test]
fn eager_getters_keep_metadata_native_source_nonconstruction_and_live_receivers() {
    check(
        r#"
var typed=Object.getPrototypeOf(Uint8Array.prototype);
var cases=[
 [ArrayBuffer.prototype,'byteLength',new ArrayBuffer(13),13],
 [typed,'length',new Uint8Array(7),7],
 [typed,'byteLength',new Uint8Array(9),9],
 [Map.prototype,'size',new Map([[1,2],[3,4]]),2]
];
for(var row of cases){
 var d=Object.getOwnPropertyDescriptor(row[0],row[1]),g=d.get;
 assert(typeof g==='function' && d.set===undefined && !d.enumerable && d.configurable,'complete getter descriptor');
 assert(g.name==='get '+row[1] && g.length===0,'name and length retain the selected entry metadata');
 assert(Object.getPrototypeOf(g)===Function.prototype,'getter internal prototype is defining Function.prototype');
 var n=Object.getOwnPropertyDescriptor(g,'name'),l=Object.getOwnPropertyDescriptor(g,'length');
 assert(n.value===g.name && !n.writable && !n.enumerable && n.configurable,'name descriptor');
 assert(l.value===0 && !l.writable && !l.enumerable && l.configurable,'length descriptor');
 var source=Function.prototype.toString.call(g);
 assert(source.indexOf('[native code]')>=0 && source.indexOf(row[1])>=0,'selected native source remains bound to accessor');
 assert(!Object.hasOwn(g,'prototype'),'ordinary intrinsic accessor has no constructor prototype');
 for(var round=0;round<3;round++){
  gc();assert(g.call(row[2])===row[3],'typed entry and receiver survive GC');
  assert(Object.getOwnPropertyDescriptor(row[0],row[1]).get===g,'accessor identity is already installed and stable');
 }
 var caught=false;try{Reflect.construct(g,[]);}catch(e){caught=e instanceof TypeError;}
 assert(caught,'accessor remains nonconstructable');
 d.get=null;assert(Object.getOwnPropertyDescriptor(row[0],row[1]).get===g,'public descriptor snapshot never owns intrinsic storage');
}
"#,
    );
}

#[test]
fn eager_getters_keep_defining_realm_prototypes_errors_and_distinct_entries() {
    check(
        r#"
var foreign=__scriptwCreateRealm().global;
var localTyped=Object.getPrototypeOf(Uint8Array.prototype),otherTyped=Object.getPrototypeOf(foreign.Uint8Array.prototype);
var cases=[
 [ArrayBuffer.prototype,foreign.ArrayBuffer.prototype,'byteLength',new foreign.ArrayBuffer(11),11],
 [localTyped,otherTyped,'length',new foreign.Uint8Array(5),5],
 [Map.prototype,foreign.Map.prototype,'size',new foreign.Map([[1,2]]),1]
];
for(var row of cases){
 var local=Object.getOwnPropertyDescriptor(row[0],row[2]).get;
 var other=Object.getOwnPropertyDescriptor(row[1],row[2]).get;gc();
 assert(other!==local && Object.getPrototypeOf(other)===foreign.Function.prototype,'independent accessor belongs to defining Realm');
 assert(other.name===local.name && other.length===local.length,'same selected metadata in each Realm');
 assert(other.call(row[3])===row[4] && local.call(row[3])===row[4],'same core brand accepts foreign valid receiver');
 var error;try{other.call({});}catch(e){error=e;}
 assert(error instanceof foreign.TypeError && !(error instanceof TypeError),'brand failure uses accessor defining Realm');
 gc();assert(Object.getOwnPropertyDescriptor(row[1],row[2]).get===other,'foreign descriptor identity persists');
}
"#,
    );
}

#[test]
fn captured_regexp_aliases_canonical_throwers_and_intl_bound_formats_keep_original_routes() {
    check(
        r#"
var last=Object.getOwnPropertyDescriptor(RegExp,'lastMatch'),alias=Object.getOwnPropertyDescriptor(RegExp,'$&');
assert(last.get===alias.get && last.set===undefined && alias.set===undefined,'captured legacy aliases retain function identity and undefined setter');
assert(last!==alias && !last.enumerable && last.configurable && alias.configurable,'aliases expose independent complete property descriptors');
var getter=last.get;last.get=null;alias.configurable=false;gc();
assert(Object.getOwnPropertyDescriptor(RegExp,'lastMatch').get===getter && Object.getOwnPropertyDescriptor(RegExp,'$&').configurable,'reflected record mutation stays private');
Object.defineProperty(RegExp,'lastMatch',{get:function(){return 'replacement';},configurable:true});gc();
assert(RegExp.lastMatch==='replacement' && Object.getOwnPropertyDescriptor(RegExp,'$&').get===getter,'replacing one captured alias leaves sibling descriptor');
var caller=Object.getOwnPropertyDescriptor(Function.prototype,'caller'),args=Object.getOwnPropertyDescriptor(Function.prototype,'arguments');
assert(caller.get===caller.set && caller.get===args.get && args.get===args.set,'canonical poison accessors share their sole thrower');
var caught=false;try{caller.get();}catch(e){caught=e instanceof TypeError;}assert(caught,'canonical thrower remains callable and throws');
var formatter=new Intl.NumberFormat('en-US',{useGrouping:false}),other=new Intl.NumberFormat('en-US',{useGrouping:false});
var bound=formatter.format;gc();
assert(formatter.format===bound && other.format!==bound && bound(1234)==='1234','captured Intl bound function remains per-instance and stable');
var foreign=__scriptwCreateRealm().global;
var poison=Object.getOwnPropertyDescriptor(foreign.Function.prototype,'caller');gc();
assert(poison.get===poison.set && poison.get!==caller.get && Object.getPrototypeOf(poison.get)===foreign.Function.prototype,'canonical thrower identity remains per-Realm');
caught=false;try{poison.get();}catch(e){caught=e instanceof foreign.TypeError;}assert(caught,'foreign canonical thrower errors use original Realm');
"#,
    );
}
