use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion, RealmBuilder,
    RunOptions,
};

fn assert_compact(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
    let source =
        format!("function assert(value,message){{if(!value)throw new Error(message);}}\n{source}");
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
        .expect("compact ordinary control executes through compiled Wasm");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}\n{source}",
        observed.completion
    );
}

#[test]
fn dense_cache_sites_are_distinct_across_functions_and_table_growth() {
    let mut source = String::from("var records=[];\n");
    for index in 0..24 {
        source.push_str(&format!(
            "function make{index}(value){{return {{key{index}:value}};}}\nfunction read{index}(record){{return record.key{index};}}\nrecords[{index}]=make{index}({index});\n"
        ));
    }
    source.push_str("gc();\n");
    for index in (0..24).rev() {
        source.push_str(&format!(
            "assert(read{index}(records[{index}])==={index},'site {index}');records[{index}].key{index}={index}+100;\n"
        ));
    }
    source.push_str("gc();\n");
    for index in 0..24 {
        source.push_str(&format!(
            "assert(read{index}(records[{index}])==={index}+100,'updated site {index}');\n"
        ));
    }
    assert_compact(&source);
}

#[test]
fn a_late_cache_site_works_while_earlier_literal_and_read_sites_stay_dormant() {
    let mut source = String::new();
    for index in 0..130 {
        source.push_str(&format!(
            "function dormant{index}(value){{var object={{key{index}:value}};return object.key{index};}}\n"
        ));
    }
    source.push_str("var marker={};assert(dormant129(marker)===marker,'late site');gc();assert(dormant129(7)===7,'warm late site');");
    assert_compact(&source);
}

#[test]
fn compact_literal_values_and_descriptor_snapshots_survive_gc_and_value_only_set() {
    assert_compact(
        r#"
var token={},symbol=Symbol('v');
function make(value){return {z:value,'2':'two','1':'one',u:undefined,n:null,b:true,s:'str',big:11n,minus:-0,nan:NaN};}
function read(record){return record.z;}
var first=make(token),second=make(symbol),before=Object.getOwnPropertyDescriptor(first,'z');
assert(first!==second && read(first)===token && read(second)===symbol,'fresh objects and shared source shape');
gc();assert(first.u===undefined && Object.hasOwn(first,'u') && first.n===null && first.b===true && first.s==='str','all scalar tags');
assert(first.big===11n && Object.is(first.minus,-0) && Number.isNaN(first.nan),'exact number bits and BigInt');
first.z=7n;gc();assert(read(first)===7n && read(second)===symbol && before.value===token,'live values and independent descriptor snapshot');
Object.preventExtensions(first);first.z=undefined;
assert(read(first)===undefined && Object.hasOwn(first,'z'),'existing writable Set on nonextensible object');
assert(Object.keys(first).join(',')==='1,2,z,u,n,b,s,big,minus,nan','source order and integer OwnKeys order');
var descriptor=Object.getOwnPropertyDescriptor(first,'z');
assert(descriptor.writable && descriptor.enumerable && descriptor.configurable,'default data attributes');
"#,
    );
}

#[test]
fn own_data_cache_rechecks_live_storage_after_calls_redefinition_delete_and_symbol_insertion() {
    assert_compact(
        r#"
var token={},symbol=Symbol('key'),trace='';
function make(value){return {b:3,a:value};}
function other(value){return {a:value,b:4};}
function read(record){return record.a;}
function change(record){Object.defineProperty(record,'a',{get:function(){trace+='g';return this.b+10;},configurable:true});gc();}
var first=make(token),second=other(9);
assert(read(first)===token && read(second)===9 && read(first)===token,'shape identity preserves distinct insertion slots');
change(first);assert(read(first)===13 && trace==='g','accessor replacement invalidates live storage guard');
Object.defineProperty(first,'a',{value:6,writable:true,enumerable:true,configurable:true});
assert(read(first)===6,'accessor to data uses dictionary descriptor');
delete first.a;
var proto={get a(){trace+='p';return this.b+20;}};
Object.setPrototypeOf(first,proto);assert(read(first)===23 && trace==='gp','deleted own cache falls through to prototype receiver');
first.a=5;assert(read(first)===23,'prototype accessor is not bypassed by a stale own slot');
var third=make(7);read(third);third[symbol]=token;gc();
assert(read(third)===7 && third[symbol]===token,'Symbol publication preserves values after transition');
delete third.b;third.b=8;assert(Reflect.ownKeys(third)[0]==='a' && Reflect.ownKeys(third)[1]==='b' && Reflect.ownKeys(third)[2]===symbol,'delete and reinsert preserve specified key order');
var calls=0,proxy=new Proxy(make(8),{get:function(target,key,receiver){calls++;return Reflect.get(target,key,receiver);}});
assert(read(proxy)===8 && calls===1,'proxy does not enter concrete own-value cache');
"#,
    );
}

#[test]
fn compact_transition_preserves_freeze_attributes_rejected_definitions_and_gc_roots() {
    assert_compact(
        r#"
var value={},symbol=Symbol('v');
function make(){return {value:value,symbol:symbol,big:23n};}
function read(record){return record.value;}
var first=make();read(first);Object.freeze(first);gc();
assert(Object.isFrozen(first) && read(first)===value && first.symbol===symbol && first.big===23n,'freeze transition retains all roots');
assert(!Reflect.set(first,'value',7) && !Reflect.deleteProperty(first,'value'),'nonwritable nonconfigurable after transition');
assert(!Reflect.defineProperty(first,'other',{value:1}) && read(first)===value,'rejected structural definition keeps old value');
var second=make();Object.seal(second);second.value=9;gc();assert(read(second)===9 && Object.isSealed(second),'sealed writable update');
var descriptor=Object.getOwnPropertyDescriptor(second,'value');
assert(descriptor.writable && descriptor.enumerable && !descriptor.configurable,'attribute change materializes ordinary descriptor');
"#,
    );
}

#[test]
fn compact_and_packed_set_paths_preserve_distinct_receiver_constraints() {
    assert_compact(
        r#"
var token={},ordinary={'0':10,'1':11},packed=[-0];
assert(Reflect.set(ordinary,'0',7,packed) && packed[0]===7 && ordinary[0]===10,'compact target writes the packed receiver');
var packedTarget=[-0],compactReceiver={'0':5};
assert(Reflect.set(packedTarget,'0',9,compactReceiver) && compactReceiver[0]===9 && Object.is(packedTarget[0],-0),'packed target writes the compact receiver');
Object.preventExtensions(compactReceiver);
assert(Reflect.set(packedTarget,'0',token,compactReceiver) && compactReceiver[0]===token && Object.is(packedTarget[0],-0),'existing receiver data remains writable when nonextensible');
assert(!Reflect.set(ordinary,'1',13,compactReceiver) && !Object.hasOwn(compactReceiver,'1') && ordinary[1]===11,'missing nonextensible receiver property rejects');
var setterCalls=0,accessorReceiver={};
Object.defineProperty(accessorReceiver,'0',{set:function(){setterCalls++;},configurable:true});
assert(!Reflect.set(packedTarget,'0',17,accessorReceiver) && setterCalls===0 && Object.is(packedTarget[0],-0),'receiver accessor is not invoked for a target data descriptor');
var readonly=[19];Object.defineProperty(readonly,'length',{writable:false});
assert(!Reflect.set(ordinary,'1',23,readonly) && readonly.length===1 && !Object.hasOwn(readonly,'1') && ordinary[1]===11,'receiver Array length rejection does not update target');
var trace='',proxyTarget={'0':29},proxyReceiver=new Proxy(proxyTarget,{
  getOwnPropertyDescriptor:function(target,key){trace+='get:'+key+'|';gc();return Reflect.getOwnPropertyDescriptor(target,key);},
  defineProperty:function(target,key,descriptor){trace+='define:'+key+'|';gc();return Reflect.defineProperty(target,key,descriptor);},
  set:function(){throw new Error('receiver Set trap must not replace DefineOwnProperty');}
});
assert(Reflect.set(ordinary,'0',31,proxyReceiver) && proxyTarget[0]===31 && ordinary[0]===10,'Proxy receiver gets its actual own-definition path');
assert(trace==='get:0|define:0|','receiver descriptor lookup and publication occur once in order');
"#,
    );
}

#[test]
fn compact_array_function_and_arguments_values_remain_rooted_across_both_storage_transitions() {
    assert_compact(
        r#"
var token={},other={};
function make(value){return {list:[1,-0,NaN],call:function(){gc();return value;},args:arguments};}
function readList(record){return record.list;}
var first=make(token),second=make(other),saved=Object.getOwnPropertyDescriptor(first,'list');
assert(readList(first)===saved.value && first.call()===token && second.call()===other,'Array and closure values survive compact own reads');
first.list[3]=first;gc();
assert(first.list[3]===first && Object.is(first.list[1],-0) && Number.isNaN(first.list[2]),'packed to descriptor transition keeps the compact receiver cycle');
Object.defineProperty(first,'args',{value:first.args,writable:false});gc();
assert(first.args[0]===token && second.args[0]===other && first.call()===token,'Arguments and captured function survive ordinary dictionary transition');
first.list=[-0,NaN];gc();
assert(readList(first)!==saved.value && saved.value[3]===first && Object.is(saved.value[1],-0),'saved descriptor retains the old Array independently of live property replacement');
first.list[1000000]=first.call;gc();
assert(first.list[1000000]()===token && Object.is(first.list[0],-0) && Number.isNaN(first.list[1]),'Function value survives packed to sparse indexed transition');
assert(second.call()===other && second.list.length===3 && second.args[0]===other,'shared source shape does not share mutable values or Arguments identity');
"#,
    );
}

#[test]
fn unresolved_static_getv_cache_keeps_primitive_receivers_and_nullish_abrupt_behavior() {
    assert_compact(
        r#"
var token={},seen,reads=0;
function read(value){return value.cacheReadValue;}
function computed(value,key){return value[key];}
function primitiveGetter(){'use strict';seen=this;reads++;gc();return token;}
Object.defineProperty(Number.prototype,'cacheReadValue',{get:primitiveGetter,configurable:true});
Object.defineProperty(String.prototype,'cacheReadValue',{get:primitiveGetter,configurable:true});
try {
  var ordinary={cacheReadValue:token};
  assert(read(ordinary)===token,'unresolved GetV warms a compact own-data cache');
  assert(read(3)===token && seen===3 && reads===1,'boxed Number lookup retains original strict getter receiver');
  assert(read('text')===token && seen==='text' && reads===2,'boxed String lookup retains original strict getter receiver');
  assert(read(ordinary)===token && reads===2,'primitive misses do not replace own-data semantics');
  var proxyReads=0,proxy=new Proxy(ordinary,{get:function(target,key,receiver){
    proxyReads++;assert(target===ordinary && key==='cacheReadValue' && receiver===proxy,'Proxy Get arguments');
    gc();return Reflect.get(target,key,receiver);
  }});
  assert(read(proxy)===token && proxyReads===1 && reads===2,'the same unresolved read retains the actual Proxy Get trap');
  var nullCaught=false,undefinedCaught=false;
  try{read(null);}catch(error){nullCaught=error instanceof TypeError;}
  try{read(undefined);}catch(error){undefinedCaught=error instanceof TypeError;}
  assert(nullCaught && undefinedCaught && reads===2,'GetV ToObject rejects nullish targets before lookup');
  var conversions=0,next={},key={};
  key[Symbol.toPrimitive]=function(hint){assert(hint==='string','computed key hint');conversions++;ordinary.cacheReadValue=next;gc();return 'cacheReadValue';};
  assert(computed(ordinary,key)===next && conversions===1 && read(ordinary)===next,'computed key effects run once before the current own value is read');
  var computedNull=false;try{computed(null,key);}catch(error){computedNull=error instanceof TypeError;}
  assert(computedNull && conversions===1,'computed GetV nullish rejection precedes key coercion');
  var keyError={};key[Symbol.toPrimitive]=function(){conversions++;gc();throw keyError;};
  var keyCaught=false;try{computed(ordinary,key);}catch(error){keyCaught=error===keyError;}
  assert(keyCaught && conversions===2 && read(ordinary)===next,'computed key abrupt completion is preserved before lookup');
  var thrown={};
  Object.defineProperty(ordinary,'cacheReadValue',{get:function(){gc();throw thrown;},configurable:true});
  var caught=false;try{read(ordinary);}catch(error){caught=error===thrown;}
  assert(caught && reads===2,'cache miss preserves an accessor abrupt completion after storage transition');
} finally {
  delete Number.prototype.cacheReadValue;delete String.prototype.cacheReadValue;
}
"#,
    );
}

#[test]
fn dictionary_history_compacts_without_changing_descriptors_or_key_order() {
    assert_compact(
        r#"
var firstSymbol=Symbol('first'),secondSymbol=Symbol('second'),token={mark:7},object=Object.create(null);
Object.defineProperty(object,'stable',{value:token,writable:false,enumerable:false,configurable:true});
Object.defineProperty(object,'accessor',{get:function(){gc();return token;},enumerable:true,configurable:true});
object[firstSymbol]='symbol';object['12']='twelve';object['2']='two';
var snapshot=Object.getOwnPropertyDescriptor(object,'stable');
for(var i=0;i<512;i++){object['temporary'+i]=i;delete object['temporary'+i];}
object.tail='last';object[secondSymbol]='second';gc();
var keys=Reflect.ownKeys(object);
assert(keys.length===7 && keys[0]==='2' && keys[1]==='12' && keys[2]==='stable' && keys[3]==='accessor' && keys[4]==='tail' && keys[5]===firstSymbol && keys[6]===secondSymbol,'compaction keeps specified key order');
assert(object.stable===token && object.accessor===token && snapshot.value===token && snapshot.writable===false,'descriptor identity and snapshot survive rebuild');
assert(!Reflect.set(object,'stable',5) && Object.getOwnPropertyDescriptor(object,'stable').enumerable===false,'attributes preserved');
delete object.stable;object.stable='reinserted';delete object[firstSymbol];object[firstSymbol]='reinserted symbol';
for(var i=0;i<128;i++){Object.defineProperty(object,'temporary',{value:i,configurable:true});delete object.temporary;}
keys=Reflect.ownKeys(object);
assert(keys[2]==='accessor' && keys[3]==='tail' && keys[4]==='stable' && keys[5]===secondSymbol && keys[6]===firstSymbol,'delete and reinsert move to end within key class');
var empty=Object.create(null);
for(var i=0;i<512;i++){empty.x=i;delete empty.x;}
assert(Reflect.ownKeys(empty).length===0 && Object.keys(empty).length===0,'empty dictionary after repeated slot churn');
empty.x=NaN;empty.y=-0;gc();assert(Number.isNaN(empty.x) && Object.is(empty.y,-0),'retired positions are reusable');
var array=[];array.extra=token;
for(var i=0;i<128;i++){array.temporary=i;delete array.temporary;}
assert(array.extra===token && Reflect.ownKeys(array).join(',')==='length,extra','array named dictionary uses the same compaction');
function mapped(value){for(var i=0;i<128;i++){arguments.temporary=i;delete arguments.temporary;}arguments[0]=9;return value;}
assert(mapped(3)===9,'mapped Arguments preserve alias after named-table churn');
"#,
    );
}
