use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_get_behavior(body: &str) {
    let source = format!(
        "function assert(condition, message) {{ if (!condition) throw new Error(message); }}\n{body}"
    );
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compiler worker");
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
        .expect("ordinary property probe fixture compiles and executes through Wasm AOT");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}\n{source}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        vec![HostOutputEvent::PrintLine("ok".into())],
        "{source}"
    );
}

#[test]
fn ordinary_get_resolves_one_property_and_observes_mutation_on_each_query() {
    assert_get_behavior(
        r#"
var symbol = Symbol('value'), receiver = {tag: 'receiver'}, calls = 0;
var prototype = {data: 2, missingGetter: 'must stay shadowed'};
Object.defineProperty(prototype, 'accessor', {configurable: true, get() {
  calls++;
  assert(this === receiver, 'original accessor receiver');
  return this.tag;
}});
var object = Object.create(prototype);
object.own = 1;
object[symbol] = undefined;
Object.defineProperty(object, 'missingGetter', {set(value) { throw 'setter cannot be read'; }});
assert(Reflect.get(object, 'own', receiver) === 1 && Reflect.get(object, 'data', receiver) === 2, 'own and inherited data');
assert(Reflect.get(object, symbol, receiver) === undefined && Object.hasOwn(object, symbol), 'present undefined');
assert(Reflect.get(object, 'missingGetter', receiver) === undefined, 'absent getter is a found property');
assert(Reflect.get(object, 'missing', receiver) === undefined, 'missing through null prototype');
assert(Reflect.get(Object.prototype, '__proto__', receiver) === Object.getPrototypeOf(receiver), 'intrinsic accessor keeps explicit receiver');
assert(Reflect.get(Object.prototype, '__proto__', Object.create(null)) === null, 'intrinsic accessor preserves null prototype result');
assert(Reflect.get(object, 'accessor', receiver) === 'receiver' && calls === 1, 'getter once');
Object.defineProperty(object, 'changing', {configurable: true, get() {
  calls++;
  Object.defineProperty(object, 'changing', {value: 7, writable: true, configurable: true});
  prototype.data = 8;
  return 6;
}});
assert(object.changing === 6 && object.changing === 7 && object.data === 8 && calls === 2, 'getter changes later reads');
delete object.changing;
prototype.changing = 9;
assert(object.changing === 9, 'deletion exposes prototype');
Object.defineProperty(prototype, 'data', {get() { calls++; return 10; }, configurable: true});
assert(object.data === 10 && calls === 3, 'data to accessor transition');
Object.defineProperty(prototype, 'data', {value: undefined, configurable: true});
assert(object.data === undefined && calls === 3, 'accessor to undefined data transition');
print('ok');
"#,
    );
}

#[test]
fn ordinary_get_reaches_prototype_proxy_once_without_descriptor_or_has_queries() {
    assert_get_behavior(
        r#"
var events = [], receiver = {}, reason = {}, shouldThrow = false;
var proxy = new Proxy(Object.create(null), {
  get(target, key, actualReceiver) {
    events.push(String(key));
    assert(actualReceiver === receiver, 'proxy original receiver');
    if (shouldThrow) throw reason;
    return 13;
  },
  has() { throw 'Get must not query HasProperty'; },
  getOwnPropertyDescriptor() { throw 'Get must not query proxy descriptor'; },
  getPrototypeOf() { throw 'Get must not traverse beyond proxy trap'; }
});
var object = Object.create(proxy);
object.own = undefined;
Object.defineProperty(object, 'noGetter', {set(value) {}});
assert(Reflect.get(object, 'own', receiver) === undefined, 'own undefined stops lookup');
assert(Reflect.get(object, 'noGetter', receiver) === undefined && events.length === 0, 'missing getter stops lookup');
assert(Reflect.get(object, 'missing', receiver) === 13 && events.join() === 'missing', 'single proxy get');
Object.defineProperty(object, 'effect', {get() { events.push('own getter'); return 17; }});
assert(Reflect.get(object, 'effect', receiver) === 17 && events.join() === 'missing,own getter', 'own accessor stops lookup');
for (var abrupt of [reason, undefined]) {
  reason = abrupt;
  shouldThrow = true;
  var threw = false;
  try { Reflect.get(object, 'abrupt', receiver); }
  catch (error) { threw = true; assert(error === abrupt, 'prototype proxy abrupt identity'); }
  assert(threw, 'prototype proxy throws');
}
assert(events.join() === 'missing,own getter,abrupt,abrupt', 'one trap per abrupt query');
print('ok');
"#,
    );
}

#[test]
fn function_properties_and_boxed_string_virtual_properties_keep_their_object_model() {
    assert_get_behavior(
        r#"
function callable(first, second) {}
var originalPrototype = callable.prototype, seen, calls = 0;
var prototype = {inherited: 19};
Object.defineProperty(prototype, 'accessor', {get() { calls++; seen = this; return 23; }});
Object.setPrototypeOf(callable, prototype);
callable.own = undefined;
assert(callable.name === 'callable' && callable.length === 2 && callable.prototype === originalPrototype, 'function own properties');
assert(callable.own === undefined && callable.inherited === 19, 'function ordinary properties');
assert(callable.accessor === 23 && seen === callable && calls === 1, 'function accessor receiver');
var explicitReceiver = {};
assert(Reflect.get(callable, 'accessor', explicitReceiver) === 23 && seen === explicitReceiver && calls === 2, 'function explicit receiver');

var boxed = new String('ab'), boxedPrototype = Object.create(null);
Object.defineProperty(boxedPrototype, '0', {get() { throw 'virtual index lost'; }});
Object.defineProperty(boxedPrototype, 'length', {get() { throw 'virtual length lost'; }});
Object.defineProperty(boxedPrototype, '2', {get() { calls++; seen = this; return 'after'; }});
Object.setPrototypeOf(boxed, boxedPrototype);
boxed.extra = 29;
assert(boxed[0] === 'a' && boxed['1'] === 'b' && boxed.length === 2 && boxed.extra === 29, 'boxed virtual and ordinary own data');
assert(boxed[2] === 'after' && seen === boxed && calls === 3, 'boxed virtual miss reaches prototype');
assert(Reflect.get(boxed, '2', explicitReceiver) === 'after' && seen === explicitReceiver && calls === 4, 'boxed original receiver');
assert(boxed.missing === undefined, 'boxed missing');

Object.defineProperty(String.prototype, '0', {configurable: true, get() { throw 'primitive virtual index lost'; }});
Object.defineProperty(String.prototype, '2', {configurable: true, get() { 'use strict'; calls++; seen = this; return 'tail'; }});
try {
  assert('ab'[0] === 'a' && 'ab'[1] === 'b' && 'ab'.length === 2, 'primitive virtual properties');
  assert('ab'[2] === 'tail' && seen === 'ab' && calls === 5, 'primitive virtual miss receiver');
} finally {
  delete String.prototype[0];
  delete String.prototype[2];
}
print('ok');
"#,
    );
}

#[test]
fn compact_for_in_queries_read_live_flags_without_invoking_accessors_or_setters() {
    assert_get_behavior(
        r#"
var symbol = Symbol('ignored'), getterCalls = 0, setterCalls = 0;
var compact = {first: 1, later: 2, last: 3}, compactKeys = [];
for (var key in compact) compactKeys.push(key);
assert(compactKeys.join() === 'first,later,last' && getterCalls === 0, 'unmodified compact literal enumeration');
for (var key in compact) {
  if (key === 'first') Object.defineProperty(compact, 'later', {enumerable: false});
}
compactKeys = [];
for (var key in compact) compactKeys.push(key);
assert(compactKeys.join() === 'first,last', 'compact attribute mutation is visible to fresh enumeration');
var prototype = {shadowed: 1, duplicate: 1, deleted: 1, inherited: 1};
Object.defineProperty(prototype, 'setterOnly', {enumerable: true, set(value) { setterCalls++; }});
var object = Object.create(prototype);
Object.defineProperty(object, 'first', {enumerable: true, get() {
  getterCalls++;
  delete object.deleted;
  Object.defineProperty(object, 'later', {enumerable: false});
  return 1;
}});
Object.defineProperty(object, 'shadowed', {enumerable: false, value: 2});
object.duplicate = 2;
object.deleted = 2;
Object.defineProperty(object, 'later', {enumerable: true, configurable: true, get() { throw 'enumeration read value'; }});
object[symbol] = 1;
var keys = [];
for (var key in object) {
  keys.push(key);
  if (key === 'first') assert(object.first === 1, 'explicit getter read');
}
// Changing enumerability during a for-in permits either pending-key policy.
var laterIndex = keys.indexOf('later');
if (laterIndex !== -1) keys.splice(laterIndex, 1);
assert(keys.join() === 'first,duplicate,deleted,inherited,setterOnly', 'deletion and inherited duplicate suppression');
assert(getterCalls === 1 && setterCalls === 0, 'enumeration does not read values or invoke inherited setters');
var second = [];
for (var key in object) second.push(key);
assert(second.join() === 'first,duplicate,deleted,inherited,setterOnly' && getterCalls === 1 && setterCalls === 0, 'fresh enumeration preserves live descriptor state');
print('ok');
"#,
    );
}
