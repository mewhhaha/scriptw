use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};
use scriptw_runtime::rooted_snapshot::{
    SnapshotDescriptor, SnapshotKey, SnapshotLimits, SnapshotObjectKind, SnapshotOutcome,
};

#[path = "aot_intrinsic_property_layouts/no_index.rs"]
mod no_index;

fn check(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
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
        .expect("intrinsic property storage executes through Wasm AOT");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "completion: {:?}; source:\n{source}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine("ok".into())]
    );
}

#[test]
fn closed_intrinsic_layouts_preserve_exact_keys_attributes_aliases_and_observation() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
function checkMethod(object, key, name, length) {
  var descriptor = Object.getOwnPropertyDescriptor(object, key);
  assert(descriptor && descriptor.value === object[key], 'own data property ' + String(key));
  assert(descriptor.writable && !descriptor.enumerable && descriptor.configurable,
    'method attributes ' + String(key));
  assert(descriptor.get === undefined && descriptor.set === undefined,
    'method is a data descriptor ' + String(key));
  assert(descriptor.value.name === name && descriptor.value.length === length,
    'native metadata ' + name);
}

var reflectKeys = Reflect.ownKeys(Reflect);
assert(reflectKeys[reflectKeys.length - 1] === Symbol.toStringTag &&
  reflectKeys.slice(0, -1).join(',') ===
  'construct,apply,get,getPrototypeOf,getOwnPropertyDescriptor,set,has,defineProperty,deleteProperty,isExtensible,preventExtensions,setPrototypeOf,ownKeys',
  'Reflect exact key order');
var jsonKeys = Reflect.ownKeys(JSON);
assert(jsonKeys[jsonKeys.length - 1] === Symbol.toStringTag &&
  jsonKeys.slice(0, -1).join(',') === 'parse,stringify,rawJSON,isRawJSON',
  'JSON exact key order');
var atomicsKeys = Reflect.ownKeys(Atomics);
assert(atomicsKeys[atomicsKeys.length - 1] === Symbol.toStringTag &&
  atomicsKeys.slice(0, -1).join(',') ===
  'add,and,compareExchange,exchange,load,notify,or,pause,store,sub,wait,waitAsync,xor,isLockFree',
  'Atomics exact key order');

checkMethod(Math, 'abs', 'abs', 1);
checkMethod(Reflect, 'ownKeys', 'ownKeys', 1);
checkMethod(JSON, 'parse', 'parse', 2);
checkMethod(Atomics, 'isLockFree', 'isLockFree', 1);
checkMethod(Object.prototype, 'hasOwnProperty', 'hasOwnProperty', 1);
checkMethod(Array.prototype, 'push', 'push', 1);
checkMethod(String.prototype, 'slice', 'slice', 2);

var arrayKeys = Reflect.ownKeys(Array.prototype);
var arrayStringKeys = arrayKeys.filter(function(key) { return typeof key === 'string'; });
assert(arrayStringKeys.join(',') ===
  'length,constructor,toString,concat,join,slice,splice,fill,sort,toLocaleString,flat,flatMap,at,toReversed,toSpliced,toSorted,with,reverse,copyWithin,includes,indexOf,lastIndexOf,find,findIndex,findLast,findLastIndex,every,some,forEach,filter,map,reduce,reduceRight,pop,push,shift,unshift,keys,entries,values',
  'Array exact string-key order');
var arraySymbols = arrayKeys.filter(function(key) { return typeof key === 'symbol'; });
assert(arraySymbols.length === 2 && arraySymbols[0] === Symbol.iterator &&
  arraySymbols[1] === Symbol.unscopables, 'Array exact symbol-key order');
var arrayLength = Object.getOwnPropertyDescriptor(Array.prototype, 'length');
assert(arrayLength.value === 0 && arrayLength.writable && !arrayLength.enumerable &&
  !arrayLength.configurable, 'Array prototype length descriptor');
Array.prototype[3] = 'three';
Array.prototype[1] = 'one';
var indexedArrayKeys = Reflect.ownKeys(Array.prototype);
assert(indexedArrayKeys[0] === '1' && indexedArrayKeys[1] === '3' &&
  indexedArrayKeys[2] === 'length' && indexedArrayKeys[3] === 'constructor' &&
  indexedArrayKeys[indexedArrayKeys.length - 2] === Symbol.iterator &&
  indexedArrayKeys[indexedArrayKeys.length - 1] === Symbol.unscopables,
  'Array indexed keys merge with intrinsic named and symbol keys');
delete Array.prototype[1];
delete Array.prototype[3];
Array.prototype.length = 0;

var stringKeys = Reflect.ownKeys(String.prototype);
var stringStringKeys = stringKeys.filter(function(key) { return typeof key === 'string'; });
assert(stringStringKeys.join(',') ===
  'length,constructor,toString,valueOf,charAt,concat,charCodeAt,codePointAt,at,anchor,big,blink,bold,fixed,fontcolor,fontsize,italics,link,small,strike,sub,substr,substring,sup,match,matchAll,replace,replaceAll,search,indexOf,lastIndexOf,slice,split,padStart,padEnd,repeat,endsWith,includes,startsWith,normalize,localeCompare,toLocaleLowerCase,toLocaleUpperCase,toLowerCase,toUpperCase,trim,trimStart,trimLeft,trimEnd,trimRight,isWellFormed,toWellFormed',
  'String exact string-key order');
var stringSymbols = stringKeys.filter(function(key) { return typeof key === 'symbol'; });
assert(stringSymbols.length === 1 && stringSymbols[0] === Symbol.iterator,
  'String exact symbol-key order');
var stringLength = Object.getOwnPropertyDescriptor(String.prototype, 'length');
assert(stringLength.value === 0 && !stringLength.writable && !stringLength.enumerable &&
  !stringLength.configurable, 'String prototype length descriptor');
var boxedString = new String('ab');
boxedString.named = 11;
boxedString[2] = 'c';
assert(boxedString[0] === 'a' && boxedString[1] === 'b' && boxedString[2] === 'c' &&
  boxedString.named === 11 && boxedString.length === 2,
  'boxed String ordinary data and virtual character indices coexist');
assert(!Reflect.defineProperty(boxedString, '0', { value: 'z' }) &&
  !Reflect.defineProperty(boxedString, 'length', { value: 9 }) &&
  boxedString[0] === 'a' && boxedString.length === 2,
  'boxed String virtual index and own length retain nonconfigurable attributes');
var getterReceiver = {};
var getterCount = 0;
Object.defineProperty(boxedString, 'slice', {
  get: function() { getterCount++; return this; }, configurable: true
});
assert(boxedString.slice === boxedString &&
  Reflect.get(boxedString, 'slice', getterReceiver) === getterReceiver && getterCount === 2,
  'boxed String own named accessor precedes inherited method and preserves receiver');
delete boxedString.slice;
assert(boxedString.slice(1) === 'b' && 'abcd'.slice(1, 3) === 'bc',
  'boxed and primitive String misses read the live prototype method');

assert(Array.prototype.values === Array.prototype[Symbol.iterator], 'Array iterator alias');
assert(Array.prototype.toString === Uint8Array.prototype.toString &&
  (function() { return arguments[Symbol.iterator] === Array.prototype.values; })(1),
  'canonical Array iterator and cross-family method identities');
assert(String.prototype.trimStart === String.prototype.trimLeft &&
  String.prototype.trimEnd === String.prototype.trimRight, 'String trim aliases');
var protoDescriptor = Object.getOwnPropertyDescriptor(Object.prototype, '__proto__');
assert(typeof protoDescriptor.get === 'function' && typeof protoDescriptor.set === 'function' &&
  !protoDescriptor.enumerable && protoDescriptor.configurable,
  'Object prototype accessor descriptor');
var parent = {}, child = {};
protoDescriptor.set.call(child, parent);
assert(protoDescriptor.get.call(child) === parent,
  'Object prototype accessor getters and setters preserve the receiver');
assert(Object.getOwnPropertyDescriptor(Math, Symbol.toStringTag).value === 'Math' &&
  !Object.getOwnPropertyDescriptor(Math, Symbol.toStringTag).writable &&
  !Object.getOwnPropertyDescriptor(Math, Symbol.toStringTag).enumerable &&
  Object.getOwnPropertyDescriptor(Math, Symbol.toStringTag).configurable,
  'Math tag descriptor');

var oldAbs = Math.abs;
Math.abs = function replacement() { return 17; };
assert(Math.abs() === 17 && oldAbs(-3) === 3, 'overwrite observed property retains old value');
delete Math.max;
Math.max = oldAbs;
var mathKeys = Reflect.ownKeys(Math);
assert(mathKeys[mathKeys.length - 2] === 'max' && mathKeys[mathKeys.length - 1] === Symbol.toStringTag,
  'delete and reinsert appends before trailing symbol');
assert(Object.getOwnPropertyDescriptor(Math, 'max').enumerable, 'reinsert uses assignment attributes');

var late = {};
Object.defineProperty(JSON, 'stringify', { value: late, writable: false, configurable: true });
assert(JSON.stringify === late && Object.getPrototypeOf(JSON.parse('{}')) === Object.prototype,
  'replace unobserved property without materializing sibling methods');
Object.defineProperty(JSON, 'rawJSON', {
  get: function() { if (this === JSON) return late; return null; }, configurable: true
});
assert(JSON.rawJSON === late, 'accessor recipe observes the live receiver');
Object.defineProperty(Reflect, 'get', { get: function() { return late; }, configurable: true });
assert(Reflect.get === late && Reflect.ownKeys({ x: 1 })[0] === 'x',
  'accessor promotion retains sibling behavior');
var trimLeft = String.prototype.trimLeft;
Object.defineProperty(String.prototype, 'trimStart', { value: late, writable: false });
assert(String.prototype.trimStart === late && String.prototype.trimLeft === trimLeft &&
  trimLeft.call(' x ') === 'x ' &&
  Object.getOwnPropertyDescriptor(String.prototype, 'trimLeft').writable,
  'promotion and alias overwrite keep independent descriptors and shared original identity');

var arrayPush = Array.prototype.push;
Object.freeze(Array.prototype);
assert(Object.isFrozen(Array.prototype) && arrayPush.call([], 1) === 1,
  'freeze after method observation');
var stringSlice = String.prototype.slice;
Object.seal(String.prototype);
assert(Object.isSealed(String.prototype) && stringSlice.call('abcd', 1, 3) === 'bc',
  'seal after method observation');
Object.freeze(JSON);
assert(Object.isFrozen(JSON) && JSON.parse('1') === 1,
  'freeze namespace after descriptor observation');

var foreign = __scriptwCreateRealm().global;
var foreignAbs = foreign.Math.abs;
var foreignPush = foreign.Array.prototype.push;
gc();
assert(foreignAbs !== oldAbs && foreignAbs(-4) === 4 &&
  Object.getPrototypeOf(foreignAbs) === foreign.Function.prototype,
  'foreign Realm materializes an independent native');
assert(foreignPush !== arrayPush && foreignPush.call([], 9) === 1 &&
  foreign.Array.prototype.values === foreign.Array.prototype[foreign.Symbol.iterator],
  'foreign Realm preserves Array alias identity');
assert(foreign.JSON.parse('true') === true && foreign.Reflect.ownKeys({ a: 1 })[0] === 'a' &&
  foreign.Atomics.isLockFree(1) === Atomics.isLockFree(1), 'foreign namespaces remain usable');
assert(Math.abs() === 17 && JSON.stringify === late && Reflect.get === late,
  'foreign reads and GC do not restore local mutations');
print('ok');
"#,
    );
}

#[test]
fn rooted_snapshot_reads_fresh_and_promoted_intrinsic_layouts_without_semantic_drift() {
    fn snapshot(source: &str) -> Vec<(String, bool, bool, bool)> {
        scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script_graph(
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
                SnapshotLimits::new(512, 256, 16, 4096, 32768, 4096, 262144, 64).unwrap(),
            )
            .expect("rooted snapshot compiles and executes");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        let SnapshotOutcome::Captured { graph } = observed.completion.outcome else {
            panic!("intrinsic namespace snapshot is captured: {source}");
        };
        let node = &graph.nodes()[0];
        assert_eq!(node.kind, SnapshotObjectKind::Ordinary {});
        node.properties
            .iter()
            .map(|property| {
                let key = match &property.key {
                    SnapshotKey::String { units } => String::from_utf16(units.units()).unwrap(),
                    SnapshotKey::Symbol { .. } => "<symbol>".to_owned(),
                };
                let (writable, enumerable, configurable) = match &property.descriptor {
                    SnapshotDescriptor::Data {
                        writable,
                        enumerable,
                        configurable,
                        ..
                    } => (*writable, *enumerable, *configurable),
                    SnapshotDescriptor::Accessor {
                        enumerable,
                        configurable,
                        ..
                    } => (false, *enumerable, *configurable),
                };
                (key, writable, enumerable, configurable)
            })
            .collect()
    }

    let fresh = snapshot("gc(); Math;");
    assert_eq!(fresh[0].0, "E");
    assert_eq!(fresh[5], ("PI".into(), false, false, false));
    assert_eq!(fresh.last().unwrap().0, "<symbol>");

    let promoted = snapshot("Math.abs; Math.extra=1; delete Math.max; Math.max=2; gc(); Math;");
    assert_eq!(promoted.first().unwrap().0, "E");
    assert_eq!(promoted.last().unwrap().0, "<symbol>");
    assert_eq!(
        promoted[promoted.len() - 2],
        ("max".into(), true, true, true)
    );
    assert!(promoted
        .iter()
        .any(|row| row.0 == "extra" && row.1 && row.2 && row.3));
}

#[test]
fn unscopables_preserves_plain_mutable_data_and_identity() {
    check(
        r#"
function assert(value) { if (!value) throw new Error('unscopables storage'); }
var object = Array.prototype[Symbol.unscopables];
assert(Object.getPrototypeOf(object) === null);
var keys = Reflect.ownKeys(object);
assert(keys.length === 16);
for (var i=0;i<keys.length;i++) {
  var descriptor=Object.getOwnPropertyDescriptor(object,keys[i]);
  assert(descriptor.value===true && descriptor.writable && descriptor.enumerable && descriptor.configurable);
}
object.at=false;
object.extra=9;
delete object.find;
assert(Array.prototype[Symbol.unscopables]===object && object.at===false && object.extra===9 && !('find' in object));
Object.freeze(object);
gc();
assert(Object.isFrozen(object) && object.at===false && object.extra===9);
var foreign=__scriptwCreateRealm().global;
var other=foreign.Array.prototype[foreign.Symbol.unscopables];
assert(other!==object && other.at===true && Object.getPrototypeOf(other)===null);
print('ok');
"#,
    );
}

#[test]
fn eager_constructor_and_prototype_layouts_keep_metadata_cycles_and_exact_keys() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var generatorFunction = (function*() {}).constructor;
var asyncFunction = (async function() {}).constructor;
var asyncGeneratorFunction = (async function*() {}).constructor;
var constructors = [
  [Function, 3], [Object, 26], [ShadowRealm, 3], [Proxy, 3], [Iterator, 7],
  [Array, 8], [Number, 17], [String, 6], [Boolean, 3], [Promise, 14],
  [Map, 5], [WeakMap, 3], [WeakRef, 3], [FinalizationRegistry, 3], [WeakSet, 3],
  [AsyncDisposableStack, 3], [DisposableStack, 3], [Set, 4], [Symbol, 20], [BigInt, 5],
  [generatorFunction, 3], [asyncFunction, 3], [asyncGeneratorFunction, 3]
];
for (var pair of constructors) {
  var constructor = pair[0];
  var keys = Reflect.ownKeys(constructor);
  assert(keys.length === pair[1] && keys[0] === 'length' && keys[1] === 'name',
    'complete constructor key order ' + constructor.name);
  for (var name of ['length', 'name']) {
    var metadata = Object.getOwnPropertyDescriptor(constructor, name);
    assert(!metadata.writable && !metadata.enumerable && metadata.configurable &&
      metadata.value === constructor[name], 'native metadata descriptor ' + constructor.name);
    metadata.value = null;
    assert(constructor[name] !== null, 'metadata snapshot does not mutate constructor');
  }
  if (constructor === Proxy) {
    assert(!Object.hasOwn(constructor, 'prototype'), 'Proxy omits public prototype');
  } else {
    var descriptor = Object.getOwnPropertyDescriptor(constructor, 'prototype');
    assert(keys[2] === 'prototype' && descriptor.value === constructor.prototype &&
      !descriptor.writable && !descriptor.enumerable && !descriptor.configurable,
      'public prototype descriptor ' + constructor.name);
    assert(constructor.prototype.constructor === constructor,
      'canonical constructor and prototype cycle ' + constructor.name);
  }
}
assert(Object.getPrototypeOf(generatorFunction) === Function &&
  Object.getPrototypeOf(asyncFunction) === Function &&
  Object.getPrototypeOf(asyncGeneratorFunction) === Function,
  'dynamic constructor inheritance');
assert(Number.parseInt === parseInt && Number.parseFloat === parseFloat &&
  Number.parseInt('12', 10) === 12 && Number.parseFloat('1.5') === 1.5,
  'canonical host aliases retain executable entries');
assert(Symbol.iterator === Object.getOwnPropertyDescriptor(Symbol, 'iterator').value &&
  !Object.getOwnPropertyDescriptor(Symbol, 'iterator').writable,
  'well-known Symbol value recipes');

var prototypes = [
  [Function.prototype, 10], [Number.prototype, 7], [Boolean.prototype, 3],
  [BigInt.prototype, 5], [Promise.prototype, 5], [Map.prototype, 15],
  [WeakMap.prototype, 8], [WeakSet.prototype, 5], [WeakRef.prototype, 3],
  [FinalizationRegistry.prototype, 4], [AsyncDisposableStack.prototype, 9],
  [DisposableStack.prototype, 9], [Set.prototype, 19], [Symbol.prototype, 6],
  [Iterator.prototype, 19], [ShadowRealm.prototype, 4],
  [generatorFunction.prototype, 3], [asyncFunction.prototype, 2],
  [asyncGeneratorFunction.prototype, 3]
];
for (var pair of prototypes) {
  assert(Reflect.ownKeys(pair[0]).length === pair[1], 'complete prototype property count');
}
assert(Object.getOwnPropertyNames(Function.prototype).join(',') ===
  'length,name,constructor,call,apply,bind,toString,arguments,caller',
  'Function prototype string key order');
assert(Object.getOwnPropertyNames(Map.prototype).join(',') ===
  'constructor,clear,delete,forEach,get,getOrInsert,getOrInsertComputed,has,keys,set,values,entries,size',
  'Map prototype string key order');
assert(Object.getOwnPropertyNames(Iterator.prototype).join(',') ===
  'toArray,includes,join,forEach,every,some,find,reduce,map,filter,flatMap,take,drop,chunks,windows,constructor',
  'Iterator constructor accessor follows methods');
var hasInstance = Object.getOwnPropertyDescriptor(Function.prototype, Symbol.hasInstance);
assert(!hasInstance.writable && !hasInstance.enumerable && !hasInstance.configurable,
  'Function hasInstance fixed descriptor');

var arrayIteratorPrototype = Object.getPrototypeOf([][Symbol.iterator]());
var stringIteratorPrototype = Object.getPrototypeOf(''[Symbol.iterator]());
var mapIteratorPrototype = Object.getPrototypeOf(new Map().entries());
var setIteratorPrototype = Object.getPrototypeOf(new Set().values());
assert(Reflect.ownKeys(arrayIteratorPrototype).length === 3 &&
  Reflect.ownKeys(stringIteratorPrototype).length === 2 &&
  Reflect.ownKeys(mapIteratorPrototype).length === 2 &&
  Reflect.ownKeys(setIteratorPrototype).length === 2,
  'iterator native layouts');
assert(arrayIteratorPrototype[Symbol.iterator] !== Iterator.prototype[Symbol.iterator] &&
  arrayIteratorPrototype[Symbol.iterator].call(arrayIteratorPrototype) === arrayIteratorPrototype,
  'separate installations of identity builtin remain distinct');
var generatorPrototype = Object.getPrototypeOf(generatorFunction.prototype.prototype);
assert(generatorPrototype === Iterator.prototype, 'generator instance prototype parent');
var asyncIteratorPrototype = Object.getPrototypeOf(asyncGeneratorFunction.prototype.prototype);
var asyncIdentity = asyncIteratorPrototype[Symbol.asyncIterator];
assert(asyncIdentity.name === '[Symbol.asyncIterator]' && asyncIdentity.length === 0 &&
  asyncIdentity.call(asyncIteratorPrototype) === asyncIteratorPrototype &&
  Function.prototype.toString.call(asyncIdentity) ===
    'function [Symbol.asyncIterator]() { [native code] }',
  'async iterator distinct native metadata and code');
print('ok');
"#,
    );
}

#[test]
fn eager_layout_accessors_aliases_and_mutation_retain_identity_across_realms_and_gc() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var foreign = __scriptwCreateRealm().global;
var map = new Map([[1, 2]]);
var size = Object.getOwnPropertyDescriptor(Map.prototype, 'size');
var otherSize = Object.getOwnPropertyDescriptor(foreign.Map.prototype, 'size');
assert(size.get.name === 'get size' && size.get.length === 0 && size.set === undefined &&
  !size.enumerable && size.configurable && size.get.call(map) === 1,
  'getter-only accessor has native metadata and absent setter');
assert(otherSize.get !== size.get &&
  Object.getPrototypeOf(otherSize.get) === foreign.Function.prototype &&
  otherSize.get.call(map) === 1, 'accessor native belongs to defining Realm');
var caught = false;
try { otherSize.get.call({}); } catch (error) { caught = error instanceof foreign.TypeError; }
assert(caught, 'accessor errors use defining Realm');
var species = Object.getOwnPropertyDescriptor(Set, Symbol.species);
var token = {};
assert(species.set === undefined && species.get.call(token) === token &&
  species.get.name === 'get [Symbol.species]', 'species getter preserves receiver');
var iteratorConstructor = Object.getOwnPropertyDescriptor(foreign.Iterator.prototype, 'constructor');
var iteratorTag = Object.getOwnPropertyDescriptor(foreign.Iterator.prototype, Symbol.toStringTag);
assert(iteratorConstructor.get.call(null) === foreign.Iterator &&
  typeof iteratorConstructor.set === 'function' && iteratorTag.get.call(null) === 'Iterator' &&
  typeof iteratorTag.set === 'function', 'plain accessors resolve defining Realm without captured public values');
var caller = Object.getOwnPropertyDescriptor(Function.prototype, 'caller');
var args = Object.getOwnPropertyDescriptor(Function.prototype, 'arguments');
var otherCaller = Object.getOwnPropertyDescriptor(foreign.Function.prototype, 'caller');
assert(caller.get === caller.set && caller.get === args.get && args.get === args.set &&
  otherCaller.get !== caller.get && !Object.isExtensible(caller.get),
  'poison accessors share exactly one canonical Realm thrower');

var entries = Map.prototype.entries;
var values = Set.prototype.values;
var dispose = DisposableStack.prototype.dispose;
var disposeAsync = AsyncDisposableStack.prototype.disposeAsync;
assert(entries === Map.prototype[Symbol.iterator] &&
  values === Set.prototype.keys && values === Set.prototype[Symbol.iterator] &&
  dispose === DisposableStack.prototype[Symbol.dispose] &&
  disposeAsync === AsyncDisposableStack.prototype[Symbol.asyncDispose], 'local alias groups');
assert(entries !== foreign.Map.prototype.entries &&
  values !== foreign.Set.prototype.values && dispose !== foreign.DisposableStack.prototype.dispose,
  'aliases are local to one defining Realm');
var methodSnapshot = Object.getOwnPropertyDescriptor(Set.prototype, 'keys');
methodSnapshot.value = null;
assert(Set.prototype.keys === values, 'data descriptor snapshots are independent');

// Attribute-only definitions promote whole layouts without forcing sibling identities to change.
Object.defineProperty(Map.prototype, 'entries', { writable: false });
Object.defineProperty(Set.prototype, 'keys', { value: token });
Object.defineProperty(DisposableStack.prototype, 'dispose', {
  get: function() { return this; }, configurable: true
});
assert(Map.prototype.entries === entries && Map.prototype[Symbol.iterator] === entries &&
  !Object.getOwnPropertyDescriptor(Map.prototype, 'entries').writable,
  'attribute mutation retains shared original alias identity');
assert(Set.prototype.keys === token && Set.prototype.values === values &&
  Set.prototype[Symbol.iterator] === values, 'alias replacement leaves sibling descriptors independent');
assert(DisposableStack.prototype.dispose === DisposableStack.prototype &&
  DisposableStack.prototype[Symbol.dispose] === dispose, 'accessor replacement preserves receiver and old alias');
assert(delete Number.isFinite, 'constructor own member deletion');
Number.isFinite = token;
var keys = Object.getOwnPropertyNames(Number);
assert(keys[keys.length - 1] === 'isFinite' &&
  Object.getOwnPropertyDescriptor(Number, 'isFinite').enumerable,
  'constructor deletion and reinsertion uses ordinary property order and attributes');
var oldName = Promise.name;
Object.defineProperty(Promise, 'name', { value: 'ChangedPromise' });
assert(oldName === 'Promise' && Promise.name === 'ChangedPromise' &&
  new Promise(function(resolve) { resolve(1); }) instanceof Promise,
  'native metadata changes do not change executable identity');
Object.freeze(WeakMap.prototype);
Object.seal(BigInt.prototype);
gc();
assert(Object.isFrozen(WeakMap.prototype) && Object.isSealed(BigInt.prototype) &&
  new WeakMap().set(token, 9).get(token) === 9 && BigInt.prototype.toString.call(12n) === '12',
  'freeze and seal preserve native behavior after promotion and GC');
assert(entries.call(map).next().value[1] === 2 &&
  values.call(new Set([3])).next().value === 3 &&
  foreign.Number.isFinite(3) && foreign.Promise.name === 'Promise' &&
  foreign.Set.prototype.keys === foreign.Set.prototype.values,
  'saved callables and foreign layouts survive local promotion and GC');
assert(Object.getOwnPropertyDescriptor(Map.prototype, 'size').get === size.get &&
  Object.getOwnPropertyDescriptor(foreign.Map.prototype, 'size').get === otherSize.get &&
  iteratorConstructor.get.call(null) === foreign.Iterator,
  'accessor identities remain stable through snapshots, mutation and GC');
print('ok');
"#,
    );
}
