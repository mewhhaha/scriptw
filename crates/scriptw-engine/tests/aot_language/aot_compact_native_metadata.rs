use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion,
    ObservedJsValue, RealmBuilder, RunOptions, SnapshotLimits, SnapshotOutcome,
};
use scriptw_runtime::rooted_snapshot::{SnapshotDescriptor, SnapshotKey};

fn options() -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    }
}

fn run_boolean(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
            source,
            options(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(60_000),
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn compact_native_metadata_preserves_reads_enumeration_and_profile_realm_isolation() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let artifact = engine
        .profile_script_compilation(
            r#"
function check(ok) { if (!ok) throw 'native metadata'; }
var fn = Math.max;
// Read through the same static sites before descriptor reflection or mutation.
function read(value) { return value.name + ':' + value.length; }
check(read(fn) === 'max:2' && read(Math.min) === 'min:2' && read(fn) === 'max:2');
var bound = fn.bind(null, 1);
check(read(bound) === 'bound max:1');
var gets = 0;
var proxy = new Proxy(fn, {get(target, key, receiver) { gets++; return Reflect.get(target, key, receiver); }});
check(read(proxy) === 'max:2' && gets === 2 && read(fn) === 'max:2');
var revocable = Proxy.revocable(fn, {});
check(read(revocable.proxy) === 'max:2');
revocable.revoke();
var revoked = false; try { read(revocable.proxy); } catch (e) { revoked = e instanceof TypeError; }
check(revoked);
function source(a, b) {}
var sourceGets = 0;
Object.defineProperty(source, 'name', {get() { sourceGets++; return 'source-accessor'; }});
check(read(source) === 'source-accessor:2' && sourceGets === 1 && read(fn) === 'max:2');
check(Array.prototype.values === Array.prototype[Symbol.iterator]);
check(Reflect.ownKeys(fn).join(',') === 'length,name');
check(Object.keys(fn).length === 0 && Object.values(fn).length === 0 && Object.entries(fn).length === 0);
check(Object.keys(Object.assign({}, fn)).length === 0 && Object.keys({...fn}).length === 0);
check(JSON.stringify({value:fn}) === '{}');
var seen = ''; for (var key in fn) seen += key;
check(seen === '' && Object.hasOwn(fn, 'name') && !fn.propertyIsEnumerable('name'));
var descriptors = Object.getOwnPropertyDescriptors(fn);
for (var key of ['name', 'length']) {
  var d = descriptors[key];
  check(!d.writable && !d.enumerable && d.configurable && !('get' in d));
}
check(!Reflect.set(fn, 'name', 'bad') && !Reflect.set(fn, 'length', 99));
var threw = false; try { (function () { 'use strict'; fn.length = 99; })(); } catch (e) { threw = e instanceof TypeError; }
check(threw && read(fn) === 'max:2');
var foreign = __scriptwCreateRealm().evalScript('Math.max');
check(foreign !== fn && read(foreign) === 'max:2');
Object.defineProperty(fn, 'name', {value:'local'});
check(read(fn) === 'local:2' && read(foreign) === 'max:2' && read(Math.min) === 'min:2');
gc();
check(read(fn) === 'local:2' && foreign.name === 'max');
true;
"#,
            options(),
        )
        .unwrap()
        .artifact;
    // A repeated profile invocation must create independent Realm values even
    // when the immutable native metadata shapes and compiled R are reused.
    for _ in 0..2 {
        let profile = engine
            .profile_wasm_execution(&artifact, Some(60_000), false)
            .unwrap();
        assert_eq!(
            profile.outcome.completion,
            ObservedCompletion::Normal(ObservedJsValue::Boolean(true))
        );
    }
}

#[test]
fn compact_native_metadata_promotes_on_redefinition_deletion_and_freeze() {
    run_boolean(
        r#"
function check(ok) { if (!ok) throw 'native metadata transition'; }
function read(fn) { return fn.name + ':' + fn.length; }
var fn = Math.max, alias = fn;
check(read(fn) === 'max:2');
Object.defineProperty(fn, 'name', {get() { return 'accessor'; }, enumerable:true});
check(read(alias) === 'accessor:2' && Object.keys(fn).join(',') === 'name');
check(Object.assign({}, fn).name === 'accessor' && ({...fn}).name === 'accessor');
var d = Object.getOwnPropertyDescriptor(fn, 'length');
check(d.value === 2 && !d.writable && !d.enumerable && d.configurable);
check(delete fn.length && !Object.hasOwn(fn, 'length') && fn.length === 0);
Object.defineProperty(fn, 'length', {value:7, writable:true, enumerable:true, configurable:true});
check(Reflect.ownKeys(fn).join(',') === 'name,length' && read(alias) === 'accessor:7');
check(Reflect.set(fn, 'length', 8) && fn.length === 8);
Object.freeze(fn);
check(!Reflect.set(fn, 'length', 9) && !Reflect.deleteProperty(fn, 'length'));
check(!Reflect.defineProperty(fn, 'name', {value:'bad'}) && Object.isFrozen(fn));
var compact = Math.min;
check(read(compact) === 'min:2');
Object.freeze(compact);
check(!Reflect.set(compact, 'length', 1) && !Reflect.deleteProperty(compact, 'name'));
var frozen = Object.getOwnPropertyDescriptor(compact, 'name');
check(frozen.value === 'min' && !frozen.writable && !frozen.enumerable && !frozen.configurable);
var invariant = false;
try { new Proxy(compact, {get() { return 'wrong'; }}).name; } catch (e) { invariant = e instanceof TypeError; }
check(invariant);
var restricted = Object.getOwnPropertyDescriptor(Function.prototype, 'caller').get;
var length = Object.getOwnPropertyDescriptor(restricted, 'length');
var name = Object.getOwnPropertyDescriptor(restricted, 'name');
check(length.value === 0 && !length.writable && !length.enumerable && !length.configurable);
check(name.value === '' && !name.writable && !name.enumerable && !name.configurable);
check(!Reflect.deleteProperty(restricted, 'name') && !Reflect.defineProperty(restricted, 'length', {value:1}));
gc();
read(alias) === 'accessor:8' && compact.name === 'min';
"#,
    );
}

#[test]
fn eager_constructor_metadata_preserves_order_attributes_and_fresh_realms() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let artifact = engine
        .profile_script_compilation(
            r#"
function check(ok) { if (!ok) throw 'constructor metadata'; }
var generator = Object.getPrototypeOf(function* () {}).constructor;
var asynchronous = Object.getPrototypeOf(async function () {}).constructor;
var asyncGenerator = Object.getPrototypeOf(async function* () {}).constructor;
var constructors = [
  [Object, 'Object', 1], [Function, 'Function', 1], [Array, 'Array', 1],
  [Proxy, 'Proxy', 2], [Number, 'Number', 1], [Symbol, 'Symbol', 0],
  [BigInt, 'BigInt', 1], [Int8Array, 'Int8Array', 3], [TypeError, 'TypeError', 1],
  [generator, 'GeneratorFunction', 1], [asynchronous, 'AsyncFunction', 1],
  [asyncGenerator, 'AsyncGeneratorFunction', 1]
];
for (var item of constructors) {
  var fn = item[0];
  check(fn.name === item[1] && fn.length === item[2]);
  check(Object.getOwnPropertyNames(fn).slice(0, 2).join(',') === 'length,name');
  var length = Object.getOwnPropertyDescriptor(fn, 'length');
  var name = Object.getOwnPropertyDescriptor(fn, 'name');
  check(length.value === item[2] && !length.writable && !length.enumerable && length.configurable);
  check(name.value === item[1] && !name.writable && !name.enumerable && name.configurable);
  check(!('get' in length) && !('get' in name));
  if (fn === Proxy) {
    check(!Object.hasOwn(fn, 'prototype'));
    check(Object.getOwnPropertyNames(fn).slice(0, 3).join(',') === 'length,name,revocable');
  } else {
    var prototype = Object.getOwnPropertyDescriptor(fn, 'prototype');
    check(prototype.value === fn.prototype && !prototype.writable && !prototype.enumerable && !prototype.configurable);
    check(Object.getOwnPropertyNames(fn).slice(0, 3).join(',') === 'length,name,prototype');
  }
}
var left = __scriptwCreateRealm().global, right = __scriptwCreateRealm().global;
var foreign = left.Array, other = right.Array;
check(foreign !== Array && foreign !== other && foreign.name === 'Array' && other.name === 'Array');
check(Object.getPrototypeOf(foreign) === left.Function.prototype);
check(Object.getPrototypeOf(left.Int8Array) === Object.getPrototypeOf(left.Uint8Array));
check(Object.getPrototypeOf(left.TypeError) === left.Error);
Object.defineProperty(foreign, 'name', {value:'changed', writable:true, enumerable:true});
check(foreign.name === 'changed' && other.name === 'Array' && Array.name === 'Array');
check(delete foreign.length);
Object.defineProperty(foreign, 'length', {value:7, writable:true, configurable:true});
var names = Object.getOwnPropertyNames(foreign);
check(names[0] === 'name' && names[1] === 'prototype' && names[names.length - 1] === 'length');
check(Reflect.set(foreign, 'length', 8) && foreign.length === 8);
check(!Reflect.deleteProperty(foreign, 'prototype'));
var instance = new foreign(2);
check(instance.length === 2 && Object.getPrototypeOf(instance) === foreign.prototype);
Object.freeze(foreign);
check(Object.isFrozen(foreign) && !Reflect.set(foreign, 'length', 9));
check(!Reflect.deleteProperty(foreign, 'name') && other.length === 1 && Array.length === 1);
gc();
foreign.name === 'changed' && foreign.length === 8 && other.name === 'Array' && other.length === 1;
"#,
            options(),
        )
        .unwrap()
        .artifact;
    for _ in 0..2 {
        let profile = engine
            .profile_wasm_execution(&artifact, Some(60_000), false)
            .unwrap();
        assert_eq!(
            profile.outcome.completion,
            ObservedCompletion::Normal(ObservedJsValue::Boolean(true))
        );
    }
}

#[test]
fn rooted_native_metadata_snapshots_preserve_compact_and_promoted_attributes() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for (setup, writable, enumerable, configurable) in [
        ("", false, false, true),
        (
            "Object.defineProperty(Math.max,'name',{writable:true,enumerable:true});",
            true,
            true,
            true,
        ),
        ("Object.freeze(Math.max);", false, false, false),
    ] {
        let outcome = Engine::new(RealmBuilder::new().build())
            .observe_script_graph(
                &format!("{setup} gc(); Math.max;"),
                options(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(60_000),
                    ..RunOptions::default()
                },
                SnapshotLimits::default(),
            )
            .unwrap();
        let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
            panic!("native snapshot rejected: {:?}", outcome.completion.outcome);
        };
        let name = graph.nodes()[0]
            .properties
            .iter()
            .find(|property| matches!(&property.key, SnapshotKey::String { units } if units.units() == [110,97,109,101]))
            .unwrap();
        assert!(
            matches!(name.descriptor, SnapshotDescriptor::Data { writable:w, enumerable:e, configurable:c, .. } if (w,e,c) == (writable,enumerable,configurable))
        );
    }
}
