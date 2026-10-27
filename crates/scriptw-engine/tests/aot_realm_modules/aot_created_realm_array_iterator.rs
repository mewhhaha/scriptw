use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_intrinsic_method_semantics(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let source = format!(
        "function assert(value, message) {{ if (!value) throw new Error(message); }}\n{source}\nprint('ok');"
    );
    let outcome = Engine::new(RealmBuilder::new().build())
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
        .expect("intrinsic method semantics execute through Wasm");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}\n{source}",
        outcome.completion,
    );
    assert_eq!(
        outcome.output_events,
        vec![HostOutputEvent::PrintLine("ok".to_string())],
    );
}

#[test]
fn intrinsic_method_data_descriptors_preserve_aliases_and_function_metadata() {
    assert_intrinsic_method_semantics(
        r#"
var arrayKeys=Reflect.ownKeys(Array.prototype),setKeys=Reflect.ownKeys(Set.prototype);
assert(arrayKeys.indexOf('values')>=0 && arrayKeys.indexOf(Symbol.iterator)>=0,'Array own keys');
assert(setKeys.indexOf('keys')>=0 && setKeys.indexOf('values')>=0,'Set own keys');
assert('values' in Array.prototype && 'keys' in Set.prototype,'method presence');gc();
var iterator=Object.getOwnPropertyDescriptor(Array.prototype,Symbol.iterator);
var values=Object.getOwnPropertyDescriptor(Array.prototype,'values');
assert(iterator.value===values.value && iterator.value===Array.prototype.values,'Array alias identity');
assert(iterator.writable && !iterator.enumerable && iterator.configurable && !('get' in iterator),'real data descriptor');
assert(values.value.name==='values' && values.value.length===0,'method name and length');
assert(Object.getPrototypeOf(values.value)===Function.prototype && !Object.prototype.hasOwnProperty.call(values.value,'prototype'),'method callable prototype');
var name=Object.getOwnPropertyDescriptor(values.value,'name'),length=Object.getOwnPropertyDescriptor(values.value,'length');
assert(!name.writable && !name.enumerable && name.configurable && !length.writable && !length.enumerable && length.configurable,'name and length descriptors');
var setDescriptor=Object.getOwnPropertyDescriptor(Set.prototype,'keys'),keys=setDescriptor.value;gc();
assert(setDescriptor.writable && !setDescriptor.enumerable && setDescriptor.configurable && !('get' in setDescriptor),'deferred Set alias remains a data property');
assert(keys===Set.prototype.values && keys===Set.prototype[Symbol.iterator],'Set aliases share one callable');
assert(keys.name==='values' && keys.length===0 && keys.call(new Set([17])).next().value===17,'Set canonical alias behavior');
assert(Object.getPrototypeOf(keys)===Function.prototype && !Object.prototype.hasOwnProperty.call(keys,'prototype'),'deferred method callable prototype');
var setName=Object.getOwnPropertyDescriptor(keys,'name'),setLength=Object.getOwnPropertyDescriptor(keys,'length');
assert(!setName.writable && !setName.enumerable && setName.configurable && !setLength.writable && !setLength.enumerable && setLength.configurable,'deferred method name and length descriptors');
"#,
    );
}

#[test]
fn intrinsic_method_mutation_and_frozen_proxy_invariants_keep_data_semantics() {
    assert_intrinsic_method_semantics(
        r#"
var replacement=function replacement(){return 19;};
assert(delete Math.sin && !('sin' in Math),'delete before value observation');
Math.cos=replacement;assert(Math.cos===replacement,'overwrite before value observation');
Object.defineProperty(Math,'tan',{writable:false});gc();
var tan=Math.tan;assert(tan.name==='tan' && tan(0)===0 && !Reflect.set(Math,'tan',replacement),'attribute-only definition retains value');
var reads=0;Object.defineProperty(Math,'atan',{get:function(){reads++;return replacement;},configurable:true});
assert(Math.atan===replacement && reads===1,'accessor replacement discards prior data value');
Object.freeze(Math);gc();
var abs=Math.abs,descriptor=Object.getOwnPropertyDescriptor(Math,'abs');
assert(descriptor.value===abs && !descriptor.writable && !descriptor.configurable && abs(-7)===7,'freeze retains canonical method');
assert(!Reflect.defineProperty(Math,'abs',{value:replacement}) && Reflect.defineProperty(Math,'abs',{value:abs}),'frozen SameValue definition');
var forwarded=new Proxy(Math,{});assert(forwarded.abs===abs && Object.getOwnPropertyDescriptor(forwarded,'abs').value===abs,'Proxy forwarding retains value');
var badGet=new Proxy(Math,{get:function(){gc();return replacement;}}),badDescriptor=new Proxy(Math,{
  getOwnPropertyDescriptor:function(){return {value:replacement,writable:false,enumerable:false,configurable:false};}
});
var getRejected=false,descriptorRejected=false;
try{badGet.abs;}catch(error){getRejected=error instanceof TypeError;}
try{Object.getOwnPropertyDescriptor(badDescriptor,'abs');}catch(error){descriptorRejected=error instanceof TypeError;}
assert(getRejected && descriptorRejected,'Proxy invariants compare actual frozen callable identity');
"#,
    );
}

#[test]
fn first_intrinsic_method_observation_uses_defining_realm_after_tampering() {
    assert_intrinsic_method_semantics(
        r#"
var other=__scriptwCreateRealm().global,second=__scriptwCreateRealm().global;
var arrayPrototype=other.Array.prototype,setPrototype=other.Set.prototype,foreignFunctionPrototype=other.Function.prototype;
assert(Reflect.ownKeys(arrayPrototype).indexOf(Symbol.iterator)>=0 && 'keys' in setPrototype,'unobserved foreign aliases exist');
other.Function=function ReplacedFunction(){};other.Array=function ReplacedArray(){};other.Set=function ReplacedSet(){};
Object.setPrototypeOf(arrayPrototype,null);Object.setPrototypeOf(setPrototype,null);gc();
var values=Object.getOwnPropertyDescriptor(arrayPrototype,Symbol.iterator).value;
assert(values===arrayPrototype.values && values!==Array.prototype.values && values!==second.Array.prototype.values,'foreign Array canonical identity');
assert(Object.getPrototypeOf(values)===foreignFunctionPrototype && values.name==='values' && values.length===0,'original defining Function prototype');
var iterator=values.call([23]);gc();assert(iterator.next().value===23 && iterator.next().done,'foreign method remains callable');
var keys=Object.getOwnPropertyDescriptor(setPrototype,'keys').value;
assert(keys===setPrototype.values && keys===setPrototype[Symbol.iterator] && keys!==Set.prototype.values,'foreign Set canonical aliases');
assert(Object.getPrototypeOf(keys)===foreignFunctionPrototype && keys.name==='values' && keys.length===0 && !Object.prototype.hasOwnProperty.call(keys,'prototype') && keys.call(new Set([29])).next().value===29,'foreign deferred Set method keeps defining Realm and metadata');
assert(Object.getOwnPropertyDescriptor(setPrototype,'values').value===keys,'foreign deferred alias descriptor retains identity');
assert(Object.getOwnPropertyDescriptor(arrayPrototype,'values').value===values,'descriptor reads retain materialized identity');
"#,
    );
}

#[test]
fn ordered_deferred_installation_keeps_alias_descriptors_independent_across_gc() {
    assert_intrinsic_method_semantics(
        r#"
var first=__scriptwCreateRealm().global,second=__scriptwCreateRealm().global;
var prototype=first.Set.prototype,foreignFunctionPrototype=first.Function.prototype;
var order=Reflect.ownKeys(prototype);
assert(order.indexOf('values')<order.indexOf('keys') && order.indexOf('keys')<order.indexOf(Symbol.iterator),'ordered String and Symbol alias publication');
Object.defineProperty(prototype,'keys',{writable:false,configurable:false});
assert(delete prototype[Symbol.iterator],'delete one unobserved alias');
gc();
var keys=Object.getOwnPropertyDescriptor(prototype,'keys'),values=Object.getOwnPropertyDescriptor(prototype,'values');
assert(keys.value===values.value && !keys.writable && !keys.configurable && values.writable && values.configurable,'aliases own separate mutable descriptors');
assert(!('get' in keys) && !keys.enumerable && !('get' in values) && !values.enumerable,'both aliases remain real data properties');
assert(Object.getPrototypeOf(keys.value)===foreignFunctionPrototype && keys.value!==second.Set.prototype.values,'recipe retains its defining Realm and per-Realm identity');
var replacement=function replacement(){};
assert(Reflect.defineProperty(prototype,'values',{value:replacement}),'replace the independently configurable alias');
assert(prototype.values===replacement && prototype.keys===keys.value && !Object.prototype.hasOwnProperty.call(prototype,Symbol.iterator),'replacing/deleting aliases cannot mutate the shared callable or other descriptor');
Object.freeze(prototype);gc();
assert(prototype.keys.call(new Set([31])).next().value===31,'surviving recipe remains callable after collection and freeze');
assert(!Reflect.defineProperty(prototype,'keys',{value:replacement}),'frozen alias compares actual callable identity');
var stringPrototype=first.String.prototype;
Object.defineProperty(stringPrototype,'trimLeft',{value:replacement});gc();
assert(stringPrototype.trimStart!==replacement && stringPrototype.trimStart.call('  x')==='x','String aliases also retain independent descriptors');
"#,
    );
}

#[test]
fn shared_constructor_allocation_keeps_eager_realm_links_and_bootstrap_exceptions() {
    assert_intrinsic_method_semantics(
        r#"
var foreign=__scriptwCreateRealm().global,second=__scriptwCreateRealm().global;
var array=foreign.Array,typed=foreign.Uint16Array,error=foreign.TypeError,object=foreign.Object,fn=foreign.Function;
gc();
assert(array!==Array && array!==second.Array && Object.getPrototypeOf(array)===fn.prototype,'fresh eager constructor and defining Function prototype');
var prototype=Object.getOwnPropertyDescriptor(array,'prototype'),backlink=Object.getOwnPropertyDescriptor(array.prototype,'constructor');
assert(prototype.value===array.prototype && !prototype.writable && !prototype.enumerable && !prototype.configurable,'canonical public prototype descriptor');
assert(backlink.value===array && backlink.writable && !backlink.enumerable && backlink.configurable,'independent constructor backlink');
assert(Object.getPrototypeOf(error)===foreign.Error && Object.getPrototypeOf(foreign.EvalError)===foreign.Error,'Error subclass constructor inheritance');
assert(Object.getPrototypeOf(typed)===Object.getPrototypeOf(foreign.Int8Array),'typed constructors inherit the canonical TypedArray constructor');
var bytes=Object.getOwnPropertyDescriptor(typed,'BYTES_PER_ELEMENT'),prototypeBytes=Object.getOwnPropertyDescriptor(typed.prototype,'BYTES_PER_ELEMENT');
assert(bytes.value===2 && !bytes.writable && !bytes.enumerable && !bytes.configurable,'typed constructor element-size descriptor');
assert(prototypeBytes.value===2 && !prototypeBytes.writable && !prototypeBytes.enumerable && !prototypeBytes.configurable,'typed prototype element-size descriptor');
assert(!Object.getOwnPropertyDescriptor(foreign.Proxy,'prototype'),'Proxy retains its missing public prototype');
assert(!Object.getOwnPropertyDescriptor(foreign.Iterator.prototype,'constructor'),'Iterator retains its missing constructor backlink');
foreign.Function=function ReplacementFunction(){};foreign.Array=function ReplacementArray(){};gc();
var instance=new array(2),buffer=new typed([257,513]),failure=new error('message');
assert(Object.getPrototypeOf(instance)===array.prototype && instance.length===2,'Array construction retains canonical prototype cache');
assert(Object.getPrototypeOf(buffer)===typed.prototype && buffer[0]===257 && buffer[1]===513,'typed constructor retains its exact element kind');
assert(Object.getPrototypeOf(failure)===error.prototype && failure.message==='message','Error construction retains its defining Realm');
assert(Object.getPrototypeOf(object)===fn.prototype && Object.getOwnPropertyDescriptor(fn.prototype,'constructor').value===fn,'canonical Function/Object graph remains eager');
assert(array.name==='Array' && array.length===1 && Function.prototype.toString.call(array)==='function Array() { [native code] }','constructor metadata survives the complete context cycle');
"#,
    );
}

#[test]
fn created_realm_arrays_publish_their_values_method_as_the_default_iterator() {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let source = r#"
var realm = __scriptwCreateRealm();
var other = realm.global;
var second = __scriptwCreateRealm().global;
var descriptor = Object.getOwnPropertyDescriptor(other.Array.prototype, Symbol.iterator);
print(descriptor.value === other.Array.prototype.values);
print(descriptor.writable && !descriptor.enumerable && descriptor.configurable);
print(descriptor.value !== Array.prototype.values
  && descriptor.value !== second.Array.prototype.values);
print(Object.getPrototypeOf(descriptor.value) === other.Function.prototype);
print(!Object.prototype.hasOwnProperty.call(other.Array.prototype, 'Symbol.iterator'));
var iterator = descriptor.value.call([3, 5]);
var first = iterator.next();
var last = iterator.next();
print(first.value === 3 && first.done === false
  && last.value === 5 && last.done === false && iterator.next().done === true);
print(realm.evalScript('var sum = 0; for (var value of [3, 5]) { sum += value; } sum;') === 8);
var foreign = realm.evalScript('[7, 11];');
var sum = 0;
for (var value of foreign) { sum += value; }
print(sum === 18);
void 0;
"#;
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("created-realm array iteration executes through Wasm");
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        outcome.completion,
    );
    assert_eq!(
        outcome.output_events,
        vec![HostOutputEvent::PrintLine("true".to_string()); 8],
    );
}
