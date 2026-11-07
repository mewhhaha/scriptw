use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_set_behavior(body: &str) {
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
        .expect("OrdinarySet fixture compiles and executes through Wasm AOT");
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
fn ordinary_receiver_updates_preserve_attributes_and_creation_requires_extensibility() {
    assert_set_behavior(
        r#"
var source = {slot: 1};
for (var enumerable of [false, true]) {
  for (var configurable of [false, true]) {
    var receiver = {};
    Object.defineProperty(receiver, 'slot', {value: 2, writable: true, enumerable, configurable});
    Object.preventExtensions(receiver);
    assert(Reflect.set(source, 'slot', 3, receiver) === true, 'nonextensible existing update');
    var descriptor = Object.getOwnPropertyDescriptor(receiver, 'slot');
    assert(descriptor.value === 3 && descriptor.writable === true, 'value-only update');
    assert(descriptor.enumerable === enumerable && descriptor.configurable === configurable, 'existing flags');
    assert(Reflect.set(source, 'missing', 4, receiver) === false, 'nonextensible creation');
    assert(!Object.hasOwn(receiver, 'missing') && source.slot === 1, 'rejected create leaves objects intact');
  }
}
var receiver = {};
assert(Reflect.set(source, 'slot', 5, receiver) === true, 'create on distinct receiver');
var descriptor = Object.getOwnPropertyDescriptor(receiver, 'slot');
assert(descriptor.value === 5 && descriptor.writable && descriptor.enumerable && descriptor.configurable, 'created flags');

var poison = Object.create(null), inherited = 0;
poison.get = function() { inherited++; throw 'internal descriptor field was read'; };
poison.configurable = true;
try {
  Object.defineProperty(Object.prototype, 'get', poison);
  Object.defineProperty(Object.prototype, 'set', poison);
  assert(Reflect.set(source, 'slot', 6, receiver) === true, 'update with poisoned descriptor prototype');
  assert(Reflect.set(source, 'newSlot', 7, receiver) === true, 'create with poisoned descriptor prototype');
} finally {
  delete Object.prototype.get;
  delete Object.prototype.set;
}
assert(inherited === 0 && receiver.slot === 6 && receiver.newSlot === 7, 'private descriptor stays private');
print('ok');
"#,
    );
}

#[test]
fn ordinary_set_distinguishes_target_receiver_and_assignment_failure_modes() {
    assert_set_behavior(
        r#"
var target = {slot: 1}, receiver = {}, setterCalls = 0;
Object.defineProperty(receiver, 'slot', {value: 2, writable: false, configurable: true});
assert(Reflect.set(target, 'slot', 3, receiver) === false && receiver.slot === 2, 'readonly receiver');
Object.defineProperty(receiver, 'slot', {set(value) { setterCalls++; }, configurable: true});
assert(Reflect.set(target, 'slot', 4, receiver) === false && setterCalls === 0, 'receiver accessor must not run');
Object.defineProperty(receiver, 'slot', {value: 5, writable: true});
assert(Reflect.set(target, 'slot', 6, receiver) === true && receiver.slot === 6 && target.slot === 1, 'writable receiver');
Object.defineProperty(target, 'slot', {writable: false});
assert(Reflect.set(target, 'slot', 7, receiver) === false && receiver.slot === 6, 'readonly target rejects first');
assert(Reflect.set({}, 'slot', 8, 1) === false, 'primitive receiver');

var seenThis, seenValue, prototype = {set slot(value) { setterCalls++; seenThis = this; seenValue = value; }};
var child = Object.create(prototype), distinct = {};
assert(Reflect.set(child, 'slot', 9, distinct) === true, 'inherited target setter');
assert(setterCalls === 1 && seenThis === distinct && seenValue === 9, 'setter receiver and value');
assert(!Object.hasOwn(child, 'slot') && !Object.hasOwn(distinct, 'slot'), 'setter owns mutation');
var inheritedReceiver = Object.create(prototype);
assert(Reflect.set({slot: 1}, 'slot', 10, inheritedReceiver) === true, 'ignore receiver inherited setter');
assert(Object.hasOwn(inheritedReceiver, 'slot') && inheritedReceiver.slot === 10 && setterCalls === 1, 'create own receiver data');

function sloppyAssign(object) { object.slot = 11; return 'continued'; }
function strictAssign(object) { 'use strict'; object.slot = 12; }
assert(sloppyAssign(target) === 'continued' && target.slot === 1, 'sloppy readonly failure');
var threw = false;
try { strictAssign(target); } catch (error) { threw = true; assert(error instanceof TypeError, 'strict readonly error'); }
assert(threw && target.slot === 1, 'strict readonly failure');
var closed = Object.preventExtensions({});
assert(sloppyAssign(closed) === 'continued' && !Object.hasOwn(closed, 'slot'), 'sloppy creation failure');
threw = false;
try { strictAssign(closed); } catch (error) { threw = true; assert(error instanceof TypeError, 'strict creation error'); }
assert(threw && !Object.hasOwn(closed, 'slot'), 'strict creation failure');
print('ok');
"#,
    );
}

#[test]
fn ordinary_set_symbol_and_proto_keys_define_receiver_data_without_coercing_values() {
    assert_set_behavior(
        r#"
var symbol = Symbol('slot'), target = Object.create(null), receiver = {};
var value = {valueOf() { throw 'unexpected value coercion'; }};
target[symbol] = 1;
assert(Reflect.set(target, symbol, value, receiver) === true, 'symbol create');
assert(receiver[symbol] === value && target[symbol] === 1, 'symbol identity and receiver');
Object.defineProperty(receiver, symbol, {enumerable: false, configurable: false});
assert(Reflect.set(target, symbol, undefined, receiver) === true, 'symbol update to undefined');
var descriptor = Object.getOwnPropertyDescriptor(receiver, symbol);
assert(descriptor.value === undefined && descriptor.writable && !descriptor.enumerable && !descriptor.configurable, 'symbol flags');
target.__proto__ = 2;
var originalPrototype = Object.getPrototypeOf(receiver);
assert(Reflect.set(target, '__proto__', value, receiver) === true, 'proto create');
descriptor = Object.getOwnPropertyDescriptor(receiver, '__proto__');
assert(descriptor.value === value && descriptor.writable && descriptor.enumerable && descriptor.configurable, 'proto data flags');
assert(Object.getPrototypeOf(receiver) === originalPrototype && target.__proto__ === 2, 'proto key is ordinary data');
assert(Reflect.set(target, '__proto__', null, receiver) === true && receiver.__proto__ === null, 'proto data update');
assert(Object.getPrototypeOf(receiver) === originalPrototype, 'proto update preserves prototype');
print('ok');
"#,
    );
}

#[test]
fn proxy_receivers_keep_descriptor_presence_attributes_order_and_boolean_results() {
    assert_set_behavior(
        r#"
var target = {slot: 1}, backing = {}, events = [], descriptors = [];
var handler = {
  getOwnPropertyDescriptor(object, key) {
    assert(this === handler && object === backing && key === 'slot', 'getOwnPropertyDescriptor arguments');
    events.push('get');
    return Reflect.getOwnPropertyDescriptor(object, key);
  },
  defineProperty(object, key, descriptor) {
    assert(this === handler && object === backing && key === 'slot', 'defineProperty arguments');
    events.push('define');
    descriptors.push(descriptor);
    assert(Object.getPrototypeOf(descriptor) === Object.prototype, 'exposed descriptor prototype');
    for (var field of Reflect.ownKeys(descriptor)) {
      var attributes = Object.getOwnPropertyDescriptor(descriptor, field);
      assert(attributes.writable && attributes.enumerable && attributes.configurable, 'descriptor field attributes');
    }
    return Reflect.defineProperty(object, key, descriptor);
  }
};
var receiver = new Proxy(backing, handler);
assert(Reflect.set(target, 'slot', 2, receiver) === true, 'proxy create');
assert(Reflect.set(target, 'slot', 3, receiver) === true, 'proxy update');
assert(events.join() === 'get,define,get,define', 'receiver operation order');
assert(descriptors[0] !== descriptors[1], 'fresh descriptors');
assert(Reflect.ownKeys(descriptors[0]).join() === 'value,writable,enumerable,configurable', 'creation field order and presence');
assert(descriptors[0].value === 2 && descriptors[0].writable && descriptors[0].enumerable && descriptors[0].configurable, 'creation fields');
assert(Reflect.ownKeys(descriptors[1]).join() === 'value' && descriptors[1].value === 3, 'update only includes value');
assert(backing.slot === 3 && target.slot === 1, 'proxy receiver mutation');

for (var reply of [false, undefined]) {
  var calls = 0, rejected = new Proxy({}, {defineProperty(object, key, descriptor) { calls++; return reply; }});
  assert(Reflect.set(target, 'slot', 4, rejected) === false && calls === 1, 'false proxy Boolean result');
  function sloppyAssign() { rejected.slot = 5; }
  function strictAssign() { 'use strict'; rejected.slot = 6; }
  sloppyAssign();
  var threw = false;
  try { strictAssign(); } catch (error) { threw = true; assert(error instanceof TypeError, 'strict proxy rejection'); }
  assert(threw && calls === 3 && !Object.hasOwn(rejected, 'slot'), 'proxy assignment outcomes');
}
print('ok');
"#,
    );
}

#[test]
fn proxy_receiver_abrupts_and_revocation_stop_ordinary_set_at_the_failing_hook() {
    assert_set_behavior(
        r#"
for (var reason of [{}, undefined]) {
  for (var site = 0; site < 3; site++) {
    var events = [], handler = Object.create(null);
    handler.getOwnPropertyDescriptor = function() {
      events.push('get');
      if (site === 0) throw reason;
      return undefined;
    };
    Object.defineProperty(handler, 'defineProperty', {get() {
      events.push('lookup');
      if (site === 1) throw reason;
      return function() { events.push('define'); throw reason; };
    }});
    var receiver = new Proxy({}, handler), threw = false;
    try { Reflect.set({slot: 1}, 'slot', 2, receiver); }
    catch (error) { threw = true; assert(error === reason, 'receiver hook abrupt identity'); }
    var expected = 'get';
    if (site === 1) expected = 'get,lookup';
    if (site === 2) expected = 'get,lookup,define';
    assert(threw && events.join() === expected, 'stop at failing receiver hook');
  }
}
var pair = Proxy.revocable({}, {}), threw = false;
pair.revoke();
try { Reflect.set({}, 'slot', 3, pair.proxy); }
catch (error) { threw = true; assert(error instanceof TypeError, 'revoked receiver TypeError'); }
assert(threw, 'revoked receiver must throw');
var events = [];
pair = Proxy.revocable({}, {getOwnPropertyDescriptor() { events.push('get'); pair.revoke(); return undefined; }});
threw = false;
try { Reflect.set({}, 'slot', 4, pair.proxy); }
catch (error) { threw = true; assert(error instanceof TypeError, 'revoked before define TypeError'); }
assert(threw && events.join() === 'get', 'revocation between receiver lookup and definition');
print('ok');
"#,
    );
}
