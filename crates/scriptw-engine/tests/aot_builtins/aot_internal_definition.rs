use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_definitions(body: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
    let source = format!(
        "function assert(value, message) {{ if (!value) throw new Error(message); }}\n{body}"
    );
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
        .expect("internal definitions compile and execute through Wasm AOT");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}\n{source}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        vec![HostOutputEvent::PrintLine("ok".into())]
    );
}

#[test]
fn internal_data_definitions_ignore_inherited_setters_and_preserve_value_identity() {
    assert_definitions(
        r#"
var setters = 0, prototype = {};
Object.defineProperty(prototype, '0', {set(value) { setters++; }, configurable: true});
Object.defineProperty(prototype, '1', {value: 'inherited', writable: false});
function Output() { return Object.create(prototype); }
var value = {valueOf() { throw 'value coercion'; }, toString() { throw 'string coercion'; }};
var output = Array.from.call(Output, [value, undefined]);
assert(setters === 0 && output[0] === value && output[1] === undefined, 'own data creation');
for (var key of ['0', '1']) {
  var descriptor = Object.getOwnPropertyDescriptor(output, key);
  assert(descriptor.writable && descriptor.enumerable && descriptor.configurable, 'complete data attributes');
}
var symbol = Symbol('entry'), result = Object.fromEntries([['__proto__', value], [symbol, value], ['missing', undefined]]);
assert(Object.getPrototypeOf(result) === Object.prototype, 'proto key does not set prototype');
assert(result.__proto__ === value && result[symbol] === value, 'symbol and reference identity');
assert(Object.hasOwn(result, 'missing'), 'undefined remains an own value');
print('ok');
"#,
    );
}

#[test]
fn internal_accessor_definitions_merge_halves_and_keep_definition_order() {
    assert_definitions(
        r#"
var events = [], calls = 0, symbol = Symbol('accessor');
function key() { events.push('key'); return symbol; }
var object = {
  get [key()]() { calls++; return this.saved; },
  set [key()](value) { calls++; this.saved = value; }
};
assert(events.join() === 'key,key' && calls === 0, 'computed keys evaluated without invoking accessors');
var descriptor = Object.getOwnPropertyDescriptor(object, symbol);
assert(typeof descriptor.get === 'function' && typeof descriptor.set === 'function', 'both accessor halves');
assert(descriptor.enumerable && descriptor.configurable && !('value' in descriptor), 'accessor attributes');
object[symbol] = 7;
assert(object[symbol] === 7 && calls === 2, 'accessor receiver and functions');
var replacement = {get value() { return 1; }, value: 2, set value(value) { this.saved = value; }};
descriptor = Object.getOwnPropertyDescriptor(replacement, 'value');
assert(descriptor.get === undefined && typeof descriptor.set === 'function', 'data to accessor clears old getter');
assert(!('writable' in descriptor), 'data attributes removed');
print('ok');
"#,
    );
}

#[test]
fn trap_absent_proxy_definition_preserves_partial_descriptors_and_trap_order() {
    assert_definitions(
        r#"
var target = {}, events = [], handler = {};
Object.defineProperty(handler, 'defineProperty', {get() {
  events.push('lookup');
  return function(object, key, descriptor) {
    events.push('trap:' + Reflect.ownKeys(descriptor).join());
    assert(this === handler && object === target, 'trap receiver and target');
    return Reflect.defineProperty(object, key, descriptor);
  };
}});
var proxy = new Proxy(new Proxy(target, handler), {});
assert(Reflect.defineProperty(proxy, 'value', {value: 1, writable: true, configurable: true}), 'forward create');
assert(Reflect.defineProperty(proxy, 'value', {value: undefined}), 'forward partial update');
assert(events.join('|') === 'lookup|trap:value,writable,configurable|lookup|trap:value', 'presence and dispatch order');
var descriptor = Object.getOwnPropertyDescriptor(target, 'value');
assert(descriptor.value === undefined && descriptor.writable && descriptor.configurable && !descriptor.enumerable, 'partial flags preserved');
var symbol = Symbol('empty getter');
assert(Reflect.defineProperty(new Proxy(target, {}), symbol, {get: undefined}), 'explicit missing getter');
descriptor = Object.getOwnPropertyDescriptor(target, symbol);
assert(descriptor.get === undefined && descriptor.set === undefined && !('value' in descriptor), 'accessor presence preserved');
Object.preventExtensions(target);
assert(Reflect.defineProperty(proxy, 'missing', {value: 1}) === false, 'rejected creation returns false');
var reason = {}, thrown = false;
try { Reflect.defineProperty(new Proxy({}, {defineProperty() { throw reason; }}), 'x', {value: 1}); }
catch (error) { thrown = error === reason; }
assert(thrown, 'trap abrupt identity');
print('ok');
"#,
    );
}

#[test]
fn internal_definition_failure_preserves_silent_and_throwing_consumers() {
    assert_definitions(
        r#"
var result = JSON.parse('{"first":1,"second":2}', function(key, value) {
  if (key === 'first') { Object.preventExtensions(this); delete this.second; }
  if (key === 'second') return 3;
  return value;
});
assert(result.first === 1 && !Object.hasOwn(result, 'second'), 'reviver silently rejects recreation');
var closed = 0, count = 0, output;
var iterator = {
  [Symbol.iterator]() { return this; },
  next() { count++; return {value: 7, done: false}; },
  return() { closed++; return {}; }
};
function ClosedOutput() { output = Object.preventExtensions({}); return output; }
var thrown = false;
try { Array.from.call(ClosedOutput, iterator); }
catch (error) { thrown = error instanceof TypeError; }
assert(thrown && count === 1 && closed === 1, 'throwing create closes iterator exactly once');
assert(!Object.hasOwn(output, '0'), 'failed definition leaves output intact');
print('ok');
"#,
    );
}

#[test]
fn array_creation_ignores_prototype_setters_and_rejects_readonly_length() {
    assert_definitions(
        r#"
var setters = 0, prototype = Object.create(Array.prototype);
Object.defineProperty(prototype, '0', {set(value) { setters++; }, configurable: true});
var target = [];
Object.setPrototypeOf(target, prototype);
function Output() { return target; }
var value = {valueOf() { throw 'unexpected conversion'; }};
var result = Array.from.call(Output, [value, undefined]);
assert(result === target && result.length === 2 && result[0] === value && setters === 0, 'array data creation ignores inherited setter');
for (var key of ['0', '1']) {
  var descriptor = Object.getOwnPropertyDescriptor(result, key);
  assert(descriptor.writable && descriptor.enumerable && descriptor.configurable, 'array index data attributes');
}
var closed = 0, steps = 0;
target = [];
Object.defineProperty(target, 'length', {writable: false});
var iterator = {
  [Symbol.iterator]() { return this; },
  next() { steps++; return {value: 7, done: false}; },
  return() { closed++; return {}; }
};
var thrown = false;
try { Array.from.call(Output, iterator); }
catch (error) { thrown = error instanceof TypeError; }
assert(thrown && steps === 1 && closed === 1 && target.length === 0, 'readonly length rejects creation and closes iterator');
target = Object.preventExtensions([0, 0]);
assert(Array.from.call(Output, {0: 'a', 1: 'b', length: 2}) === target, 'nonextensible existing indices remain writable');
assert(target.join() === 'a,b' && target.length === 2, 'existing index updates retain length');
print('ok');
"#,
    );
}

#[test]
fn forwarded_array_descriptors_preserve_length_coercion_partial_flags_and_rollback() {
    assert_definitions(
        r#"
var target = [1, 2, 3], proxy = new Proxy(new Proxy(target, {}), {}), events = [];
var length = {[Symbol.toPrimitive](hint) { events.push(hint); return 2; }};
assert(Reflect.defineProperty(proxy, 'length', {value: length, writable: false}), 'forwarded ArraySetLength');
assert(events.join() === 'number,number' && target.length === 2 && !Object.hasOwn(target, '2'), 'length conversions and deletion');
assert(!Reflect.defineProperty(proxy, '2', {value: 8}), 'readonly length prevents extension');
assert(Reflect.defineProperty(proxy, '1', {value: 9}), 'existing index update');
var descriptor = Object.getOwnPropertyDescriptor(target, '1');
assert(descriptor.value === 9 && descriptor.writable && descriptor.enumerable && descriptor.configurable, 'partial index flags preserved');
assert(Reflect.defineProperty(proxy, '4294967295', {value: 19}), 'non-index numeric key');
assert(target.length === 2 && target['4294967295'] === 19, 'non-index key leaves length intact');
target = [1, 2, 3];
Object.defineProperty(target, '2', {configurable: false});
proxy = new Proxy(target, {});
assert(!Reflect.defineProperty(proxy, 'length', {value: 1, writable: false}), 'nonconfigurable tail rejects shrink');
descriptor = Object.getOwnPropertyDescriptor(target, 'length');
assert(descriptor.value === 3 && !descriptor.writable && target[2] === 3, 'failed shrink restores boundary and applies readonly flag');
var reason = {}, thrown = false;
try { Reflect.defineProperty(new Proxy([], {}), 'length', {value: {valueOf() { throw reason; }}}); }
catch (error) { thrown = error === reason; }
assert(thrown, 'length conversion abrupt identity');
print('ok');
"#,
    );
}

#[test]
fn array_species_and_foreign_targets_preserve_definition_failure_and_error_realm() {
    assert_definitions(
        r#"
var target = [], source = [1, 2], calls = 0;
Object.defineProperty(target, 'length', {writable: false});
source.constructor = {[Symbol.species]: function() { return target; }};
var thrown = false;
try { source.map(value => { calls++; return value + 1; }); }
catch (error) { thrown = error instanceof TypeError; }
assert(thrown && calls === 1 && !Object.hasOwn(target, '0'), 'species target failure stops callbacks');
var foreign = __scriptwCreateRealm();
var foreignArray = foreign.evalScript('[]');
var ForeignRangeError = foreign.evalScript('RangeError');
thrown = false;
try { Reflect.defineProperty(new Proxy(foreignArray, {}), 'length', {value: -1}); }
catch (error) { thrown = error instanceof RangeError && !(error instanceof ForeignRangeError); }
assert(thrown && foreignArray.length === 0, 'length error uses caller builtin realm');
print('ok');
"#,
    );
}
