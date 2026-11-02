use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

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
        .expect("intrinsic Array reads execute through Wasm AOT");
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
fn cold_and_observed_array_methods_keep_canonical_identity() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
function readPush(value) { return value.push; }
var first = [];
var cold = readPush(first);
assert(typeof cold === 'function' && cold === Array.prototype.push, 'cold read materializes canonical push');
assert(readPush(first) === cold && readPush([]) === cold, 'observed method is shared by fresh arrays');
assert(Object.getOwnPropertyDescriptor(Array.prototype, 'push').value === cold, 'descriptor observes same method');
assert(cold.call(first, 3) === 1 && first[0] === 3 && first.push(4) === 2, 'borrowed and member calls work');
var values = first.values;
assert(values === Array.prototype.values && first[Symbol.iterator] === values, 'iterator alias remains canonical');
assert(first.slice === Array.prototype.slice && first.constructor === Array, 'other named intrinsic rows');
print('ok');
"#,
    );
}

#[test]
fn own_properties_and_argument_effects_preserve_callee_selection_order() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
function readPush(value) { return value.push; }
var array = [];
var original = readPush(array);
var replacement = function(value) { assert(this === array, 'replacement receiver'); return value + 10; };
array.push = replacement;
assert(readPush(array) === replacement && array.push(2) === 12 && array.length === 0, 'own data shadows warmed method');
delete array.push;
var gets = 0;
Object.defineProperty(array, 'push', {
  configurable: true,
  get: function() { gets++; assert(this === array, 'own getter receiver'); return replacement; }
});
assert(readPush(array) === replacement && array.push(3) === 13 && gets === 2, 'own getter executes on every read');
delete array.push;
assert(array.push((array.push = replacement, 7)) === 1 && array[0] === 7,
  'inherited callee is selected before arguments install own shadow');
assert(array.push(4) === 14, 'subsequent call sees argument mutation');
delete array.push;
var replacementReads = 0;
assert(array.push((Object.defineProperty(Array.prototype, 'push', {
  configurable: true,
  get: function() { replacementReads++; return replacement; }
}), 8)) === 2 && array[1] === 8 && replacementReads === 0,
  'selected callee survives prototype accessor replacement during arguments');
assert(array.push(5) === 15 && replacementReads === 1, 'next call sees prototype accessor');
assert(original !== replacement, 'original identity remains distinct');
print('ok');
"#,
    );
}

#[test]
fn changed_array_prototype_chains_do_not_reuse_intrinsic_reads() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
function readPush(value) { return value.push; }
var array = [];
var original = readPush(array);
var marker = {};
var parent = { push: marker };
Object.setPrototypeOf(array, parent);
assert(readPush(array) === marker, 'replacement prototype data wins');
var gets = 0;
Object.defineProperty(parent, 'push', {
  configurable: true,
  get: function() { gets++; assert(this === array, 'intermediate accessor receiver'); return marker; }
});
assert(readPush(array) === marker && gets === 1, 'replacement prototype accessor runs');
delete parent.push;
Object.setPrototypeOf(parent, Array.prototype);
assert(readPush(array) === original, 'indirect Array prototype chain');
Object.setPrototypeOf(array, null);
assert(readPush(array) === undefined, 'null prototype has no inherited method');
Object.setPrototypeOf(array, Array.prototype);
assert(readPush(array) === original, 'restored direct prototype');
var oldParent = Object.getPrototypeOf(Array.prototype);
Object.setPrototypeOf(Array.prototype, { inherited: marker });
assert(readPush(array) === original && array.inherited === marker, 'Array prototype parent mutation retains own methods');
Object.setPrototypeOf(Array.prototype, oldParent);
print('ok');
"#,
    );
}

#[test]
fn proxy_receivers_and_prototypes_observe_get_traps() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
function readPush(value) { return value.push; }
var array = [];
var original = readPush(array);
var marker = {};
var calls = 0;
var parent = new Proxy(Array.prototype, {
  get: function(target, key, receiver) {
    assert(key === 'push' && receiver === array, 'prototype proxy trap arguments');
    calls++;
    return marker;
  }
});
Object.setPrototypeOf(array, parent);
assert(readPush(array) === marker && readPush(array) === marker && calls === 2, 'prototype proxy never bypasses trap');
Object.setPrototypeOf(array, Array.prototype);
var proxy;
proxy = new Proxy(array, {
  get: function(target, key, receiver) {
    assert(key === 'push' && receiver === proxy, 'array proxy trap arguments');
    calls++;
    return Reflect.get(target, key, receiver);
  }
});
assert(readPush(proxy) === original && readPush(proxy) === original && calls === 4, 'proxy-wrapped array retains get effects');
print('ok');
"#,
    );
}

#[test]
fn prototype_descriptor_changes_promotion_and_freeze_invalidate_reads() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
function readPush(value) { return value.push; }
var array = [];
var original = readPush(array);
var marker = {};
Array.prototype.extra = marker;
assert(readPush(array) === original && array.extra === marker, 'named property promotion retains method');
Object.defineProperty(Array.prototype, 'push', { value: marker, writable: false, configurable: true });
assert(readPush(array) === marker, 'redefined data descriptor replaces cached method');
assert(!Reflect.set(Array.prototype, 'push', original), 'nonwritable method rejects assignment');
delete Array.prototype.push;
assert(readPush(array) === undefined, 'deleted method is absent');
var gets = 0;
Object.defineProperty(Array.prototype, 'push', {
  configurable: true,
  get: function() { gets++; assert(this === array, 'prototype getter receiver'); return marker; }
});
assert(readPush(array) === marker && readPush(array) === marker && gets === 2, 'accessor replacement runs for every read');
Object.defineProperty(Array.prototype, 'push', { value: original, writable: true, configurable: true });
assert(readPush(array) === original, 'data descriptor restores original identity');
Object.freeze(Array.prototype);
assert(Object.isFrozen(Array.prototype) && readPush(array) === original && array.push(9) === 1,
  'frozen promoted prototype retains callable method');
assert(!Reflect.set(Array.prototype, 'push', marker) && readPush(array) === original, 'failed frozen write leaves read unchanged');
print('ok');
"#,
    );
}

#[test]
fn indices_symbols_lengths_and_other_receiver_kinds_keep_normal_lookup() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
function readPush(value) { return value.push; }
var array = [1, 2];
var original = readPush(array);
var indexGets = 0;
Object.defineProperty(array, '0', { get: function() { indexGets++; return 7; }, configurable: true });
assert(array.length === 2 && array[0] === 7 && indexGets === 1, 'array length and indexed accessor');
var symbol = Symbol('push');
var symbolGets = 0;
Object.defineProperty(array, symbol, {
  get: function() { symbolGets++; assert(this === array, 'symbol getter receiver'); return 11; }
});
assert(array[symbol] === 11 && symbolGets === 1 && readPush(array) === original, 'symbol key is distinct from method name');
var object = { push: 13 };
assert(readPush(object) === 13, 'ordinary own named property');
Object.setPrototypeOf(object, Array.prototype);
delete object.push;
assert(readPush(object) === original && object.push(4) === 1 && object[0] === 4, 'ordinary object inherits generic Array method');
var typed = new Uint8Array([5, 6]);
assert(readPush(typed) === undefined && typed.length === 2 && typed.slice(1)[0] === 6, 'typed array keeps its own prototype methods');
typed.push = 17;
assert(readPush(typed) === 17, 'typed array own named property');
var string = new String('ab');
assert(readPush(string) === undefined && string.length === 2 && string[0] === 'a' && string.slice(1) === 'b',
  'String wrapper indices length and native method');
Object.defineProperty(string, 'push', { get: function() { return this; } });
assert(readPush(string) === string, 'String wrapper own accessor');
print('ok');
"#,
    );
}

#[test]
fn foreign_array_cache_identity_and_local_mutations_survive_gc() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
function readPush(value) { return value.push; }
var local = [];
var localPush = readPush(local);
var foreign = __scriptwCreateRealm().global;
var ForeignArray = foreign.Array;
var foreignPrototype = ForeignArray.prototype;
var foreignFunctionPrototype = foreign.Function.prototype;
var array = new ForeignArray();
var foreignPush = readPush(array);
assert(foreignPush !== localPush && foreignPush === foreignPrototype.push, 'defining Realm owns method identity');
assert(Object.getPrototypeOf(foreignPush) === foreignFunctionPrototype, 'foreign method Function prototype');
var replacement = function() { return 19; };
Array.prototype.push = replacement;
delete foreign.Array;
delete foreign.Function;
gc();
assert(readPush(array) === foreignPush && foreignPush === foreignPrototype.push && array.push(3) === 1 && array[0] === 3,
  'foreign cached method stays rooted after public constructor deletion');
assert(readPush(local) === replacement && local.push() === 19 && localPush.call(local, 4) === 1,
  'GC and foreign lookup preserve local mutation and retained original');
assert(array.values === foreignPrototype[Symbol.iterator] && array.values !== local.values,
  'foreign iterator identity stays Realm specific');
gc();
assert(readPush(array) === foreignPush && readPush(local) === replacement, 'repeated collection retains live read identities');
print('ok');
"#,
    );
}
