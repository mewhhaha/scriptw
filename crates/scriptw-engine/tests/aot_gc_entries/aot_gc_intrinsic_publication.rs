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
fn intrinsic_data_publication_preserves_flags_order_symbols_and_realm_identity() {
    check(
        r#"
var arrayKeys=Reflect.ownKeys(Array);
assert(arrayKeys[0]==='length' && arrayKeys[1]==='name' && arrayKeys[2]==='prototype','constructor metadata order');
var prototype=Object.getOwnPropertyDescriptor(Array,'prototype');
assert(prototype.value===Array.prototype && !prototype.writable && !prototype.enumerable && !prototype.configurable,'constructor prototype descriptor');
var constructor=Object.getOwnPropertyDescriptor(Array.prototype,'constructor');
assert(constructor.value===Array && constructor.writable && !constructor.enumerable && constructor.configurable,'prototype constructor back-reference');
var pi=Object.getOwnPropertyDescriptor(Math,'PI');
assert(pi.value===Math.PI && !pi.writable && !pi.enumerable && !pi.configurable,'constant attributes');
var tag=Object.getOwnPropertyDescriptor(Math,Symbol.toStringTag);
assert(tag.value==='Math' && !tag.writable && !tag.enumerable && tag.configurable,'Symbol data key attributes');
var foreign=__scriptwCreateRealm().global;gc();
assert(foreign.Array.prototype.constructor===foreign.Array && foreign.Array!==Array && foreign.Array.prototype!==Array.prototype,'constructor cycle belongs to defining Realm');
var other=Object.getOwnPropertyDescriptor(foreign.Math,foreign.Symbol.toStringTag);
assert(other.value==='Math' && !other.writable && !other.enumerable && other.configurable,'foreign Symbol installation');
tag.value='changed';constructor.value=null;gc();
assert(Math[Symbol.toStringTag]==='Math' && Array.prototype.constructor===Array && foreign.Array.prototype.constructor===foreign.Array,'reflected snapshots remain independent');
assert(!Reflect.set(Math,'PI',0) && !Reflect.deleteProperty(Math,'PI'),'constant publication keeps rejection behavior');
assert(Reflect.set(Array.prototype,'constructor',foreign.Array) && Array.prototype.constructor===foreign.Array && foreign.Array.prototype.constructor===foreign.Array,'back-reference descriptors do not alias across Realms');
"#,
    );
}

#[test]
fn intrinsic_accessor_publication_keeps_undefined_slots_throwers_and_aliases() {
    check(
        r#"
var last=Object.getOwnPropertyDescriptor(RegExp,'lastMatch'),alias=Object.getOwnPropertyDescriptor(RegExp,'$&');
assert(typeof last.get==='function' && last.set===undefined && !last.enumerable && last.configurable,'present undefined setter remains legal');
assert(alias.get===last.get && alias.set===undefined && alias!==last,'legacy aliases share getter but independent descriptors');
var input=Object.getOwnPropertyDescriptor(RegExp,'input');
assert(typeof input.get==='function' && typeof input.set==='function' && !input.enumerable && input.configurable,'getter and setter roles retain their functions');
RegExp.input='retained';gc();assert(RegExp.input==='retained' && RegExp.$_==='retained','setter survives GC and aliases retain one slot');
var caller=Object.getOwnPropertyDescriptor(Function.prototype,'caller'),args=Object.getOwnPropertyDescriptor(Function.prototype,'arguments');
assert(caller.get===caller.set && caller.get===args.get && args.get===args.set,'poison accessors share one Realm thrower');
assert(!caller.enumerable && caller.configurable && !args.enumerable && args.configurable,'poison accessor attributes');
var caught=false;try{caller.get();}catch(e){caught=e instanceof TypeError;}assert(caught,'thrower behavior is preserved');
var foreign=__scriptwCreateRealm().global;gc();
var other=Object.getOwnPropertyDescriptor(foreign.Function.prototype,'caller');
assert(other.get===other.set && other.get!==caller.get && Object.getPrototypeOf(other.get)===foreign.Function.prototype,'foreign thrower belongs to defining Realm');
caught=false;try{other.get();}catch(e){caught=e instanceof foreign.TypeError;}assert(caught,'foreign throw uses original Realm error');
var getter=last.get;last.get=null;alias.configurable=false;gc();
assert(Object.getOwnPropertyDescriptor(RegExp,'lastMatch').get===getter && Object.getOwnPropertyDescriptor(RegExp,'$&').configurable,'reflected accessor records stay independent');
Object.defineProperty(RegExp,'lastMatch',{get:function(){gc();return 'replacement';},configurable:true});
assert(RegExp.lastMatch==='replacement' && Object.getOwnPropertyDescriptor(RegExp,'$&').get===getter,'replacing one alias leaves sibling descriptor intact');
assert(delete RegExp.lastMatch && Object.getOwnPropertyDescriptor(RegExp,'$&').get===getter,'deleting one alias retains sibling function root');
"#,
    );
}

#[test]
fn deferred_methods_keep_selected_code_metadata_and_defining_realm_after_gc() {
    check(
        r#"
var functions=[Math.abs,Math.max,Array.prototype.push,Array.prototype.values,Set.prototype.add,String.prototype.slice,Object.prototype.hasOwnProperty];
var names=['abs','max','push','values','add','slice','hasOwnProperty'],lengths=[1,2,1,0,1,2,1];
gc();
for(var i=0;i<functions.length;i++){
  var f=functions[i];
  assert(f.name===names[i] && f.length===lengths[i],'each selected native keeps its own name and length');
  assert(Function.prototype.toString.call(f)==='function '+names[i]+'() { [native code] }','each selected native keeps its exact source');
  assert(Object.getPrototypeOf(f)===Function.prototype && !Object.hasOwn(f,'prototype'),'ordinary nonconstructor native prototype');
  var caught=false;try{Reflect.construct(f,[]);}catch(e){caught=e instanceof TypeError;}assert(caught,'native method is not constructable');
}
var token={},array=[],set=new Set();
assert(functions[0](-7)===7 && functions[1](2,8,3)===8,'different scalar entries retain actual code');
assert(functions[2].call(array,token)===1 && array[0]===token,'selected push keeps receiver and arguments');
assert(functions[3].call(array).next().value===token && functions[3]===Array.prototype[Symbol.iterator],'iterator entry and alias recipe identity');
assert(functions[4].call(set,token)===set && set.has(token),'selected collection entry keeps branding');
assert(functions[5].call('abcd',1,3)==='bc' && functions[6].call({x:1},'x'),'string and object entries stay distinct');
var saved=functions[0];Math.abs=function(){return 99;};gc();
assert(saved(-9)===9 && Math.abs()===99 && functions[1](4,5)===5,'previous selected records survive later publications and replacement');
var foreign=__scriptwCreateRealm().global,other=foreign.Math.max,otherAdd=foreign.Set.prototype.add;gc();
assert(other!==functions[1] && other.name==='max' && other.length===2 && other(3,9)===9,'foreign native retains its own selected metadata and code');
assert(Object.getPrototypeOf(other)===foreign.Function.prototype && Object.getPrototypeOf(otherAdd)===foreign.Function.prototype,'function prototype belongs to defining Realm');
var caught=false;try{otherAdd.call(null,token);}catch(e){caught=e instanceof foreign.TypeError;}assert(caught,'native error allocation uses defining Realm');
gc();assert(saved(-11)===11 && functions[3]===Array.prototype[Symbol.iterator] && other(1,7)===7,'rooted function/context cycles remain independent');
"#,
    );
}
