use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_thrower_behavior(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let source = format!(
        "function assert(condition, message) {{ if (!condition) throw new Error(message); }}\n{source}"
    );
    let observation = Engine::new(RealmBuilder::new().build())
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
        .expect("ThrowTypeError behavior executes through Wasm AOT");
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
fn each_realm_publishes_one_thrower_for_distinct_arguments_and_function_accessors() {
    assert_thrower_behavior(
        r#"
var localArgs = (function() { 'use strict'; return arguments; })();
var first = __scriptwCreateRealm().global;
var firstArgs = (new first.Function('"use strict"; return arguments;'))();
var firstArgs2 = (new first.Function('"use strict"; return arguments;'))();
var second = __scriptwCreateRealm().global;
var secondArgs = (new second.Function('"use strict"; return arguments;'))();
var local = Object.getOwnPropertyDescriptor(localArgs, 'callee').get;
var foreign = Object.getOwnPropertyDescriptor(firstArgs, 'callee').get;
var foreign2 = Object.getOwnPropertyDescriptor(secondArgs, 'callee').get;
assert(local !== foreign && local !== foreign2 && foreign !== foreign2, 'distinct Realm identities');
assert(firstArgs !== firstArgs2, 'genuinely separate Arguments objects');
assert(foreign === Object.getOwnPropertyDescriptor(firstArgs2, 'callee').get, 'one foreign intrinsic');
var realms = [globalThis, first, second];
var objects = [localArgs, firstArgs, secondArgs];
var throwers = [local, foreign, foreign2];
for (var index = 0; index < realms.length; index++) {
  var realm = realms[index], thrower = throwers[index];
  var callee = Object.getOwnPropertyDescriptor(objects[index], 'callee');
  assert(callee.get === thrower && callee.set === thrower, 'Arguments getter/setter identity');
  assert(!callee.enumerable && !callee.configurable, 'Arguments poison attributes');
  for (var key of ['caller', 'arguments']) {
    var descriptor = Object.getOwnPropertyDescriptor(realm.Function.prototype, key);
    assert(descriptor !== undefined, 'restricted prototype descriptor exists');
    assert(descriptor.get === thrower && descriptor.set === thrower, 'shared prototype poison');
    assert(!descriptor.enumerable && descriptor.configurable, 'prototype poison attributes');
  }
  assert(Object.getPrototypeOf(thrower) === realm.Function.prototype, 'thrower function prototype');
}
var localAfter = (function(){ 'use strict'; return arguments; })();
assert(Object.getOwnPropertyDescriptor(localAfter, 'callee').get === local, 'foreign creation preserves entry singleton');
print('ok');
"#,
    );
}

#[test]
fn extracted_forwarded_and_property_calls_throw_from_the_intrinsic_realm() {
    assert_thrower_behavior(
        r#"
var foreign = __scriptwCreateRealm().global;
var args = (new foreign.Function('"use strict"; return arguments;'))();
var descriptor = Object.getOwnPropertyDescriptor(args, 'callee');
var thrower = descriptor.get;
var local = Object.getOwnPropertyDescriptor(Function.prototype, 'caller').get;
var bound = thrower.bind({});
var proxy = new Proxy(thrower, {});
var calls = [
  function(){ thrower(); },
  function(){ thrower.call(undefined); },
  function(){ thrower.apply(null, []); },
  function(){ Reflect.apply(thrower, 0, []); },
  function(){ bound(); },
  function(){ proxy(); },
  function(){ descriptor.set(1); },
  function(){ return args.callee; },
  function(){ args.callee = 1; },
  function(){ Reflect.get(args, 'callee', {}); },
  function(){ Reflect.set(args, 'callee', 1, {}); },
  function(){ return foreign.Function.prototype.caller; },
  function(){ foreign.Function.prototype.arguments = 1; }
];
var previous;
for (var call of calls) {
  var caught;
  try { call(); } catch (error) { caught = error; }
  assert(caught !== undefined, 'call throws');
  assert(Object.getPrototypeOf(caught) === foreign.TypeError.prototype, 'foreign error prototype');
  assert(!(caught instanceof TypeError), 'entry caller does not own the error');
  assert(caught !== previous, 'fresh TypeError per invocation');
  previous = caught;
}
var localError;
try { foreign.Function.prototype.call.call(local); }
catch (error) { localError = error; }
assert(localError !== undefined && Object.getPrototypeOf(localError) === TypeError.prototype, 'forwarder does not replace target Realm');
print('ok');
"#,
    );
}

#[test]
fn every_thrower_is_an_anonymous_frozen_nonconstructor() {
    assert_thrower_behavior(
        r#"
var foreign = __scriptwCreateRealm().global;
var local = Object.getOwnPropertyDescriptor(Function.prototype, 'caller').get;
var other = Object.getOwnPropertyDescriptor(foreign.Function.prototype, 'arguments').get;
for (var thrower of [local, other]) {
  assert(typeof thrower === 'function', 'callable');
  assert(thrower.name === '' && thrower.length === 0, 'anonymous and zero length');
  assert(!Object.isExtensible(thrower) && Object.isFrozen(thrower), 'non-extensible and frozen');
  assert(!Object.hasOwn(thrower, 'prototype'), 'no constructor prototype');
  var names = Object.getOwnPropertyNames(thrower);
  assert(names.length === 2 && names[0] === 'length' && names[1] === 'name', 'own property order');
  for (var key of names) {
    var descriptor = Object.getOwnPropertyDescriptor(thrower, key);
    assert(!descriptor.writable && !descriptor.enumerable && !descriptor.configurable, 'frozen metadata');
  }
  var caught;
  try { Reflect.construct(thrower, []); } catch (error) { caught = error; }
  assert(caught instanceof TypeError, 'not a constructor');
  assert(!Reflect.defineProperty(thrower, 'extra', {value: 1}), 'cannot add properties');
}
print('ok');
"#,
    );
}

#[test]
fn mutable_realm_globals_and_argument_hooks_cannot_redirect_the_thrower() {
    assert_thrower_behavior(
        r#"
var foreign = __scriptwCreateRealm().global;
var factory = new foreign.Function('"use strict"; return arguments;');
var prototype = foreign.TypeError.prototype;
var functionPrototype = foreign.Function.prototype;
var args = factory();
var thrower = Object.getOwnPropertyDescriptor(args, 'callee').get;
var calls = 0;
var poison = {toString(){calls++;throw 1;}, valueOf(){calls++;throw 2;},
  [Symbol.toPrimitive](){calls++;throw 3;}};
foreign.TypeError = function(){ throw 'mutable TypeError'; };
foreign.Function = null;
delete functionPrototype.caller;
delete functionPrototype.arguments;
var args2 = factory();
assert(Object.getOwnPropertyDescriptor(args2, 'callee').get === thrower, 'internal slot survives prototype mutation');
var caught;
try { Reflect.apply(thrower, poison, [poison]); } catch (error) { caught = error; }
assert(Object.getPrototypeOf(caught) === prototype, 'intrinsic error prototype survives global replacement');
assert(calls === 0, 'receiver and argument are ignored');
var marker = {}, evaluated = 0, abrupt;
try { thrower((function(){ evaluated++; throw marker; })()); }
catch (error) { abrupt = error; }
assert(abrupt === marker, 'argument evaluation precedes invocation');
assert(evaluated === 1, 'argument expression runs once');
print('ok');
"#,
    );
}

#[test]
fn nonsimple_and_lexical_arguments_preserve_poison_ownership_and_mapped_callee() {
    assert_thrower_behavior(
        r#"
var foreign = __scriptwCreateRealm().global;
var strict = new foreign.Function('"use strict"; return arguments;');
var defaults = new foreign.Function('value = 1', 'return arguments;');
var rest = new foreign.Function('...values', 'return arguments;');
var pattern = new foreign.Function('{value}', 'return arguments;');
var arrow = new foreign.Function('"use strict"; return (() => arguments)();');
var mapped = new foreign.Function('value', 'return arguments;');
var emptyMapped = new foreign.Function('return arguments;');
var thrower = Object.getOwnPropertyDescriptor(strict(), 'callee').get;
var factories = [strict, defaults, rest, pattern, arrow];
for (var factory of factories) {
  var args = factory({value: 3});
  var descriptor = Object.getOwnPropertyDescriptor(args, 'callee');
  assert(descriptor.get === thrower && descriptor.set === thrower, 'all unmapped paths use owner Realm');
}
for (var factory of [mapped, emptyMapped]) {
  var args = factory(1);
  var descriptor = Object.getOwnPropertyDescriptor(args, 'callee');
  assert(descriptor.value === factory && descriptor.writable && descriptor.configurable, 'mapped callee remains data');
  assert(!descriptor.enumerable && !Object.hasOwn(descriptor, 'get'), 'mapped descriptor shape');
  args.callee = 7;
  assert(args.callee === 7, 'mapped callee remains writable');
}
print('ok');
"#,
    );
}

#[test]
fn suspension_retains_the_arguments_owner_while_other_realms_are_created() {
    assert_thrower_behavior(
        r#"
var foreign = __scriptwCreateRealm().global;
var make = new foreign.Function('return { async run() { "use strict"; var args = arguments; await 0; return args; }, *runGenerator() { "use strict"; var args = arguments; yield 0; return args; } };');
var methods = make();
var thrower = Object.getOwnPropertyDescriptor(foreign.Function.prototype, 'caller').get;
var generator = methods.runGenerator();
assert(generator.next().value === 0, 'generator suspended');
var promise = methods.run();
var third = __scriptwCreateRealm().global;
var args = generator.next().value;
assert(Object.getOwnPropertyDescriptor(args, 'callee').get === thrower, 'generator retains owner');
promise.then(function(args) {
  var descriptor = Object.getOwnPropertyDescriptor(args, 'callee');
  assert(descriptor.get === thrower && descriptor.set === thrower, 'async retains owner');
  var caught;
  try { descriptor.get(); } catch (error) { caught = error; }
  assert(Object.getPrototypeOf(caught) === foreign.TypeError.prototype, 'resumed owner error Realm');
  print('ok');
});
"#,
    );
}

#[test]
fn has_instance_entries_use_operator_or_actual_native_method_realms() {
    assert_thrower_behavior(
        r#"
var foreign = __scriptwCreateRealm().global;
var localTypeError = TypeError, foreignTypeError = foreign.TypeError;
var localMethod = Function.prototype[Symbol.hasInstance];
var foreignMethod = foreign.Function.prototype[Symbol.hasInstance];
function localConstructor() {}
localConstructor.prototype = 1;
var foreignConstructor = new foreign.Function('');
foreignConstructor.prototype = 1;
var foreignOperator = new foreign.Function('value', 'target', '"use strict"; return value instanceof target;');
globalThis.TypeError = function() { throw 'replaced local TypeError'; };
foreign.TypeError = function() { throw 'replaced foreign TypeError'; };
function errorFrom(action, expected) {
  var caught = false;
  try { action(); } catch (error) {
    caught = true;
    assert(Object.getPrototypeOf(error) === expected.prototype, 'actual execution Realm');
  }
  assert(caught, 'operation must throw');
}
errorFrom(function() { return localMethod.call(foreignConstructor, {}); }, localTypeError);
errorFrom(function() { return foreignMethod.call(localConstructor, {}); }, foreignTypeError);
errorFrom(function() { return ({}) instanceof foreignConstructor; }, foreignTypeError);
errorFrom(function() { return ({}) instanceof localConstructor; }, localTypeError);
errorFrom(function() { return foreignOperator({}, 3); }, foreignTypeError);
var primitive = 3, topError;
try { ({}) instanceof primitive; } catch (error) { topError = error; }
assert(Object.getPrototypeOf(topError) === localTypeError.prototype, 'Script operator Realm');
print('ok');
"#,
    );
}
