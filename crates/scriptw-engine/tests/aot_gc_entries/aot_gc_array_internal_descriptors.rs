//! Internal Array descriptor acquisition preserves the same observable object
//! operations while bypassing public descriptor-object transport.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion, RealmBuilder,
    RunOptions,
};

fn check(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
    for directive in ["", "'use strict';\n"] {
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &format!("{directive}function assert(v,m){{if(!v)throw m;}}\n{source}"),
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
            .expect("Array descriptor control executes through Wasm AOT");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            observed.completion
        );
    }
}

#[test]
fn own_undefined_attributes_and_accessor_edges_remain_distinct_from_absence() {
    check(
        r#"
var a=[undefined],events=[],proto=Object.create(Array.prototype),token={};
Object.defineProperty(proto,'0',{set:function(v){events.push('wrong');},configurable:true});
Object.setPrototypeOf(a,proto);
assert(Reflect.set(a,'0',token) && a[0]===token && events.length===0,'own undefined shadows inherited setter');
Object.defineProperty(a,'0',{value:undefined,writable:false,enumerable:false,configurable:false});
gc();assert(!Reflect.set(a,'0',token) && !Reflect.deleteProperty(a,'0'),'fixed undefined data flags');
var d=Object.getOwnPropertyDescriptor(a,'0');
assert(d.value===undefined && !d.writable && !d.enumerable && !d.configurable,'fresh reflection of complete flags');
var getter=function(){gc();assert(this===a,'getter receiver');return token;};
var setter=function(v){gc();assert(this===a && v===token,'setter edges');events.push('set');};
Object.defineProperty(a,'1',{get:getter,set:setter,enumerable:true,configurable:true});
assert(a[1]===token && Reflect.set(a,'1',token) && events.join(',')==='set','accessor identity and effects');
var before=Object.getOwnPropertyDescriptor(a,'1');
Object.defineProperty(a,'1',{value:7,writable:true});gc();
assert(before.get===getter && before.set===setter && before.enumerable && before.configurable && a[1]===7,
 'descriptor snapshots remain independent of live data/accessor redefinition');
var symbol=Symbol('named');a[symbol]=token;a.named=undefined;
assert(Reflect.set(a,symbol,7) && a[symbol]===7 && Object.hasOwn(a,'named'),'symbol and named storage on Array');
"#,
    );
}

#[test]
fn push_rechecks_inherited_descriptors_after_each_setter_and_keeps_error_realm() {
    check(
        r#"
var events=[],a=[],proto=Object.create(Array.prototype);
Object.defineProperty(proto,'0',{set:function(v){
 events.push('setter:'+v);assert(this===a,'original receiver');gc();
 Object.defineProperty(proto,'1',{value:'blocked',writable:false,configurable:true});
},configurable:true});
Object.setPrototypeOf(a,proto);
var foreign=__scriptwCreateRealm().global,caught;
try{foreign.Array.prototype.push.call(a,'first','second');}catch(e){caught=e;}
assert(caught instanceof foreign.TypeError && events.join(',')==='setter:first','later argument sees mutated inherited flags and called Realm');
assert(a.length===0 && !Object.hasOwn(a,'0') && !Object.hasOwn(a,'1'),'failed push retains only preceding setter effects');
var marker={},b=[],bp=Object.create(Array.prototype);
Object.defineProperty(bp,'0',{set:function(){gc();throw marker;},configurable:true});Object.setPrototypeOf(b,bp);
try{Array.prototype.push.call(b,1);throw 'missing setter throw';}catch(e){assert(e===marker,'whole thrown identity');}
"#,
    );
}

#[test]
fn virtual_length_nonextensibility_and_non_index_boundary_preserve_failures() {
    check(
        r#"
var a=[1];Object.defineProperty(a,'length',{writable:false});
for(var count=0;count<2;count++){
 var caught=false;try{if(count)Array.prototype.push.call(a,2);else Array.prototype.push.call(a);}catch(e){caught=e instanceof TypeError;}
 assert(caught && a.length===1 && !(1 in a),'readonly length rejects zero and nonzero arguments');
}
var sealed=[];Object.preventExtensions(sealed);
var rejected=false;try{sealed.push('value');}catch(e){rejected=e instanceof TypeError;}
assert(rejected && sealed.length===0 && !Object.hasOwn(sealed,'0'),'extensibility checked on actual receiver');
var wide=[];wide.length=4294967295;
var range=false;try{wide.push('named-tail');}catch(e){range=e instanceof RangeError;}
gc();assert(range && wide.length===4294967295 && wide['4294967295']==='named-tail',
 'non-index own property is published before final invalid Array length');
var named=Object.getOwnPropertyDescriptor(wide,'4294967295');
assert(named.writable && named.enumerable && named.configurable,'boundary named descriptor flags');
"#,
    );
}

#[test]
fn proxy_array_traps_invariants_and_revocation_keep_their_existing_order() {
    check(
        r#"
var events=[],target=[],proxy=new Proxy(target,{
 get:function(t,k,r){events.push('get:'+k);return Reflect.get(t,k,r);},
 set:function(t,k,v,r){events.push('set:'+k);return Reflect.set(t,k,v,r);},
 getOwnPropertyDescriptor:function(t,k){events.push('gopd:'+k);return Reflect.getOwnPropertyDescriptor(t,k);},
 defineProperty:function(t,k,d){events.push('define:'+k);gc();return Reflect.defineProperty(t,k,d);}
});
assert(Array.prototype.push.call(proxy,'x')===1 && target[0]==='x','proxy mutation');
assert(events.join(',')==='get:length,set:0,gopd:0,define:0,set:length,gopd:length,define:length','all receiver traps remain ordered');
var fixed=[undefined];Object.defineProperty(fixed,'0',{writable:false,configurable:false});
var bad=new Proxy(fixed,{get:function(){gc();return 7;},set:function(){gc();return true;}});
var getFailed=false,setFailed=false;
try{bad[0];}catch(e){getFailed=e instanceof TypeError;}
try{Reflect.set(bad,'0',7);}catch(e){setFailed=e instanceof TypeError;}
assert(getFailed && setFailed && fixed[0]===undefined,'fixed data invariant snapshots');
var noGetter=[];Object.defineProperty(noGetter,'0',{get:undefined,configurable:false});
var accessorFailed=false;try{new Proxy(noGetter,{get:function(){return 1;}})[0];}catch(e){accessorFailed=e instanceof TypeError;}
assert(accessorFailed,'missing accessor getter remains undefined');
var pair=Proxy.revocable([],{});pair.revoke();var revoked=false;
try{Array.prototype.push.call(pair.proxy,1);}catch(e){revoked=e instanceof TypeError;}
assert(revoked,'revoked Array Proxy cannot enter concrete shortcut');
"#,
    );
}

#[test]
fn proxy_descriptor_conversion_reentry_keeps_the_acquired_internal_target_snapshot() {
    check(
        r#"
for(var storage=0;storage<2;storage++){
 var target=[1],events=[],reads=0;
 if(storage){
  // Leave the same observable data attributes, but force full mutable indexed
  // descriptor storage too: packed-number reads already synthesize a record.
  Object.defineProperty(target,'0',{enumerable:false});
  Object.defineProperty(target,'0',{enumerable:true});
 }
 var proxy=new Proxy(target,{getOwnPropertyDescriptor:function(actual,key){
  assert(actual===target && key==='0','trap target and key');events.push('trap');
  return {configurable:true,writable:true,enumerable:true,get value(){
   events.push('value');reads++;
   Object.defineProperty(target,'0',{value:2,writable:false,configurable:false});
   gc();return 1;
  }};
 }});
 var reported=Object.getOwnPropertyDescriptor(proxy,'0');
 assert(reported.value===1 && reported.writable && reported.enumerable && reported.configurable,
  'invariant checks retain the target descriptor acquired before trap-result conversion');
 var current=Object.getOwnPropertyDescriptor(target,'0');
 assert(current.value===2 && !current.writable && current.enumerable && !current.configurable,
  'conversion reentry changes the live Array descriptor independently');
 assert(events.join(',')==='trap,value' && reads===1,'trap precedes one value conversion');
 var rejected=false;
 try{Object.getOwnPropertyDescriptor(proxy,'0');}catch(e){rejected=e instanceof TypeError;}
 assert(rejected && reads===2,'a later acquisition sees the now-fixed descriptor and rejects the same report');
}
"#,
    );
}

#[test]
fn public_builtin_and_descriptor_prototype_poisoning_cannot_intercept_internal_queries() {
    check(
        r#"
var push=Array.prototype.push,gopd=Object.getOwnPropertyDescriptor,rgopd=Reflect.getOwnPropertyDescriptor,
 define=Reflect.defineProperty,objectDefine=Object.defineProperty;
var keys=['value','writable','get','set','enumerable','configurable'],bags=[];
for(var i=0;i<keys.length;i++){
 var bag=Object.create(null);bag.get=function(){throw 'descriptor prototype observed';};bag.configurable=true;bags.push(bag);
}
var a=[undefined],ok=false;
try{
 for(var j=0;j<keys.length;j++)objectDefine(Object.prototype,keys[j],bags[j]);
 Object.getOwnPropertyDescriptor=function(){throw 'public Object GPD observed';};
 Reflect.getOwnPropertyDescriptor=function(){throw 'public Reflect GPD observed';};
 Reflect.defineProperty=function(){throw 'public Reflect define observed';};
 assert(push.call(a,'tail')===2 && a[0]===undefined && a[1]==='tail','internal Array descriptors ignore public replacements');
 var length=gopd(a,'length');assert(length.value===2 && length.writable && !length.enumerable && !length.configurable,'virtual length flags under poisoning');
 ok=true;
}finally{
 Object.getOwnPropertyDescriptor=gopd;Reflect.getOwnPropertyDescriptor=rgopd;Reflect.defineProperty=define;
 for(var k=0;k<keys.length;k++)delete Object.prototype[keys[k]];
}
assert(ok,'poisoning control completed');
"#,
    );
}
