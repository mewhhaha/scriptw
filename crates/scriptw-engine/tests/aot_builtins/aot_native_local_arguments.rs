//! Native methods with a local-arguments entry and `F.call(...)` forwarding
//! must observe exactly what the argument-vector dispatch observes.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_script(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compiler worker");
    let observation = Engine::new(RealmBuilder::new().build())
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
        .expect("native local-argument behavior executes through Wasm AOT");
    assert_eq!(observation.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observation.completion, ObservedCompletion::Normal(_)),
        "{:?}\n{source}",
        observation.completion,
    );
    assert_eq!(
        observation.output_events,
        vec![HostOutputEvent::PrintLine("ok".into())],
        "{source}",
    );
}

#[test]
fn local_entry_methods_keep_missing_extra_receiver_and_realm_rules() {
    assert_script(
        r#"
'use strict';
function assert(value, message) { if (!value) throw new Error(message); }
var map = new Map(), weak = new WeakMap(), key = {};
assert(map.set(key) === map && map.has(key) && map.get(key) === undefined, 'missing value is undefined');
assert(map.set(1, 2, 3).get(1, 'extra') === 2, 'extra arguments are ignored');
assert(map.has() === false && map.get() === undefined, 'missing key is undefined');
assert(weak.set(key, 7) === weak && weak.get(key) === 7 && weak.has(key), 'weak entries');
assert(weak.get(1) === undefined && weak.has() === false, 'non-object weak keys');
var weakError = false;
try { weak.set(1, 2); } catch (error) { weakError = error instanceof TypeError; }
assert(weakError, 'invalid weak key throws');
assert('a:b'.indexOf(':') === 1 && 'a:b'.indexOf() === -1 && 'undefined'.indexOf() === 0, 'indexOf defaults');
assert('abc'.indexOf('c', 1, 'extra') === 2 && 'abc'.indexOf('a', 1) === -1, 'indexOf position');
assert('abcdef'.slice(2) === 'cdef' && 'abcdef'.slice() === 'abcdef' && 'abcdef'.slice(1, 3) === 'bc', 'slice defaults');
assert('ab'.charCodeAt() === 97 && 'ab'.charCodeAt(1) === 98 && isNaN('ab'.charCodeAt(5)), 'charCodeAt');
assert((255).toString(16) === 'ff' && (255).toString() === '255' && (255).toString(undefined) === '255', 'radix');
var radixError = false;
try { (1).toString(1); } catch (error) { radixError = error instanceof RangeError; }
assert(radixError, 'radix range');
assert(Array.isArray([]) && !Array.isArray() && Array.isArray(new Proxy([], {})), 'isArray');
assert(Object.getPrototypeOf([]) === Array.prototype, 'getPrototypeOf');
var protoError = false;
try { Object.getPrototypeOf(); } catch (error) { protoError = error instanceof TypeError; }
assert(protoError, 'missing getPrototypeOf argument is undefined');
assert({a: 1}.hasOwnProperty('a') && !{}.hasOwnProperty() && {undefined: 1}.hasOwnProperty(), 'hasOwnProperty');
var receiverError = false;
try { Map.prototype.get.call({}, 1); } catch (error) { receiverError = error instanceof TypeError; }
assert(receiverError, 'incompatible receiver throws');
var thisError = false;
try { String.prototype.indexOf.call(undefined, 'a'); } catch (error) { thisError = error instanceof TypeError; }
assert(thisError, 'nullish string receiver throws');
var stringObject = new String('xyz');
assert(String.prototype.slice.call(stringObject, 1) === 'yz' && String.prototype.charCodeAt.call(5) === 53, 'coerced receivers');
var constructError = false;
try { new map.get(1); } catch (error) { constructError = error instanceof TypeError; }
assert(constructError, 'methods are not constructors');
assert(map.get.length === 1 && map.set.length === 2 && 'a'.slice.length === 2, 'lengths');
print('ok');
"#,
    );
}

#[test]
fn local_entry_callbacks_see_opaque_native_callers_and_restore_after_throws() {
    assert_script(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var seen = [], marker = {};
function stringHook() { gc(); seen.push(stringHook.caller); return 'b'; }
function integerHook() { seen.push(integerHook.caller); return 1; }
function radixHook() { seen.push(radixHook.caller); return 2; }
function prototypeHook() { seen.push(prototypeHook.caller); return null; }
function throwingHook() { seen.push(throwingHook.caller); throw marker; }
var key = { toString: stringHook }, throwing = { toString: throwingHook };
var proxy = new Proxy({}, { getPrototypeOf: prototypeHook });
// A sloppy caller would be revealed if a native entry skipped its boundary.
function run() {
  assert('abc'.indexOf(key) === 1, 'ToString argument hook');
  assert('abc'.slice({ valueOf: integerHook }) === 'bc', 'ToIntegerOrInfinity hook');
  assert((10).toString({ valueOf: radixHook }) === '1010', 'radix hook');
  assert({ b: 1 }.hasOwnProperty(key) === true, 'ToPropertyKey hook');
  assert(Object.prototype.hasOwnProperty.call({ b: 1 }, key) === true, 'forwarded call hook');
  assert(Object.getPrototypeOf(proxy) === null, 'Proxy trap hook');
  var caught = 0;
  try { 'abc'.indexOf(throwing); } catch (error) { if (error === marker) caught++; }
  try { Object.prototype.hasOwnProperty.call({}, throwing); } catch (error) { if (error === marker) caught++; }
  assert(caught === 2, 'abrupt argument conversion propagates');
  stringHook();
}
run();
assert(seen.length === 9, 'every hook ran once');
for (var index = 0; index < 8; index++) {
  assert(seen[index] === null, 'native caller is opaque at ' + index);
}
assert(seen[8] === run, 'native caller boundary was restored after each call and throw');
print('ok');
"#,
    );
}

#[test]
fn call_forwarding_keeps_the_actual_callee_and_receiver_dispatch() {
    assert_script(
        r#"
'use strict';
function assert(value, message) { if (!value) throw new Error(message); }
var hasOwn = Object.prototype.hasOwnProperty, object = { a: 1 }, log = [];
assert(hasOwn.call(object, 'a') && !hasOwn.call(object, 'b') && !hasOwn.call(object), 'forwarded native');
function pair(a, b) { return [this, a, b]; }
var result = pair.call(object, 1);
assert(result[0] === object && result[1] === 1 && result[2] === undefined, 'forwarded source missing argument');
result = pair.call(undefined, 1, 2, 3, 4, 5);
assert(result[0] === undefined && result[2] === 2, 'forwarded source extra arguments');
function count() { return arguments.length; }
assert(count.call(null) === 0 && count.call(null, undefined) === 1, 'argument count survives forwarding');
var custom = { call: function(a, b) { log.push(this === custom, a, b); return 'own'; } };
assert(custom.call(1, 2) === 'own' && log.join() === 'true,1,2', 'a user call method is an ordinary callee');
var notCallable = { call: Function.prototype.call };
var callError = false;
try { notCallable.call(object); } catch (error) { callError = error instanceof TypeError; }
assert(callError, 'non-callable receiver throws from call');
var trapped = new Proxy(pair, { apply: function(target, thisArgument, args) { return ['trap', thisArgument, args.length]; } });
result = trapped.call(object, 1, 2);
assert(result[0] === 'trap' && result[1] === object && result[2] === 2, 'Proxy receiver keeps its apply trap');
var bound = pair.bind(object, 'x');
result = bound.call(null, 'y');
assert(result[0] === object && result[1] === 'x' && result[2] === 'y', 'bound receiver');
class Klass {}
var classError = false;
try { Klass.call(object); } catch (error) { classError = error instanceof TypeError; }
assert(classError, 'class constructor call throws');
function tail(value) { return hasOwn.call(value, 'a'); }
assert(tail(object) === true && tail({}) === false, 'tail position forwarding');
var callCall = Function.prototype.call.call(pair, object, 9);
assert(callCall[0] === object && callCall[1] === 9, 'call applied to call');
print('ok');
"#,
    );
}
