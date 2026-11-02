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
