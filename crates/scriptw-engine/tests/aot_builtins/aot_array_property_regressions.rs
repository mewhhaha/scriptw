use scriptw_engine::{CompileOptions, Engine, ExecutionBackend, RealmBuilder, RunOptions};

fn assert_wasm_true(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let engine = Engine::new(RealmBuilder::new().build());
    let outcome = engine
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Array property regression must compile and execute through Wasm AOT");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn computed_length_keys_preserve_array_and_arguments_descriptors() {
    assert_wasm_true(
        r#"
var key = ['len', 'gth'].join(''), ok = true;
function check(a, b) {
  var values = [[], [7], arguments];
  for (var i = 0; i < values.length; i++) {
    var target = values[i], desc = Object.getOwnPropertyDescriptor(target, key);
    ok = ok && target.hasOwnProperty(key) && Object.hasOwn(target, key) &&
      desc.value === target.length && desc.writable && !desc.enumerable &&
      desc.configurable === (i === 2);
    ok = ok && Object.getOwnPropertyDescriptor(target, Symbol('length')) === undefined;
  }
  delete arguments.length;
  ok = ok && !Object.hasOwn(arguments, key) &&
    Object.getOwnPropertyDescriptor(arguments, key) === undefined;
}
check(7, 9);
ok;
"#,
    );
}

#[test]
fn computed_intrinsic_keys_use_string_content_without_invoking_accessors() {
    assert_wasm_true(
        r#"
var length = ['len', 'gth'].join(''), callee = ['cal', 'lee'].join('');
var prototype = ['proto', 'type'].join(''), calls = 0;
function source(a, b) {
  var getter = function() { throw 'descriptor lookup must not invoke the getter'; };
  Object.defineProperty(arguments, 'length', { get: getter });
  var key = { toString: function() { calls++; return length; } };
  var desc = Object.getOwnPropertyDescriptor(arguments, key);
  return desc.get === getter && desc.set === undefined && desc.configurable &&
    Object.getOwnPropertyDescriptor(arguments, callee).value === source;
}
var unboxed = Object.getOwnPropertyDescriptor('ab', length);
var boxed = Object.getOwnPropertyDescriptor(new String('ab'), length);
source(1, 2) && calls === 1 && unboxed.value === 2 && !unboxed.writable &&
  boxed.value === 2 && !boxed.configurable &&
  Object.getOwnPropertyDescriptor(source, prototype).value === source.prototype &&
  Object.getOwnPropertyDescriptor(source, Symbol('prototype')) === undefined;
"#,
    );
}

#[test]
fn indexed_proxy_setters_preserve_explicit_receiver_and_apply_arguments() {
    assert_wasm_true(
        r#"
var calls = 0, ok = true, expectedReceiver, expectedValue;
var setter = new Proxy(function(value) {
  calls++; ok = ok && this === expectedReceiver && value === expectedValue;
}, { apply: function(target, receiver, args) {
  calls++; ok = ok && receiver === expectedReceiver && args.length === 1 && args[0] === expectedValue;
  return Reflect.apply(target, receiver, args);
} });
function argumentsObject() { return arguments; }
var targets = [[], argumentsObject(1)];
for (var i = 0; i < targets.length; i++) {
  var target = targets[i], receiver = {};
  Object.defineProperty(target, '0', { set: setter, configurable: true });
  Object.defineProperty(target, '10000', { set: setter, configurable: true });
  expectedReceiver = target; expectedValue = 7; target[0] = 7;
  expectedValue = 9; target[10000] = 9;
  expectedReceiver = receiver; expectedValue = 11;
  ok = Reflect.set(target, '0', 11, receiver) && ok;
  expectedValue = 13; ok = Reflect.set(target, '10000', 13, receiver) && ok;
}
ok && calls === 16;
"#,
    );
}

#[test]
fn indexed_proxy_setter_abrupt_completion_and_missing_setters_are_preserved() {
    assert_wasm_true(
        r#"
var marker = {}, calls = 0, ok = true, target = [];
var setter = new Proxy(function() { throw 'apply trap must win'; }, {
  apply: function() { calls++; throw marker; }
});
Object.defineProperty(target, '0', { set: setter });
Object.defineProperty(target, '10000', { set: setter });
try { target[0] = 7; ok = false; } catch (e) { ok = ok && e === marker; }
try { Reflect.set(target, '10000', 9, {}); ok = false; } catch (e) { ok = ok && e === marker; }
var revocable = Proxy.revocable(function() {}, {});
Object.defineProperty(target, '1', { set: revocable.proxy });
revocable.revoke();
try { target[1] = 7; ok = false; } catch (e) { ok = ok && e instanceof TypeError; }
Object.defineProperty(target, '2', { get: function() { return 3; } });
ok = ok && !Reflect.set(target, '2', 7);
target[2] = 7;
try { (function() { 'use strict'; target[2] = 7; })(); ok = false; }
catch (e) { ok = ok && e instanceof TypeError; }
ok && calls === 2 && target[2] === 3;
"#,
    );
}

#[test]
fn multi_argument_push_observes_inherited_setter_at_fourth_index() {
    assert_wasm_true(
        r#"
var a = [];
var seen;
var n = a.push(1, 2, 3, (
  Object.defineProperty(Object.prototype, "3", {
    configurable: true,
    set: function(v) { seen = v; }
  }),
  4
));
delete Object.prototype[3];
n === 4 && a.length === 4 && seen === 4 && a[0] === 1 && a[2] === 3;
"#,
    );
}
