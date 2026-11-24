use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, ObservedCompletion, RealmBuilder,
    RunOptions,
};

#[test]
fn legacy_caller_is_null_only_on_ordinary_sloppy_functions() {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let source = r#"
function inner() { return inner.caller; }
function outer() { return inner(); }
function strictOuter() { "use strict"; var result = inner(); return result; }
var descriptor = Object.getOwnPropertyDescriptor(inner, 'caller');
print(descriptor.value === null && !descriptor.writable && !descriptor.enumerable && descriptor.configurable);
print(inner.caller === null && outer() === outer && strictOuter() === null);
print(outer.call(null) === outer && outer.apply(null) === outer && outer.bind(null)() === outer);
print(Function().caller === null);
function strictFunction() { "use strict"; }
var object = { method() {}, get accessor() {} };
var restricted = [
  strictFunction, () => {}, function* () {}, async function () {}, async () => {}, async function* () {},
  object.method, Object.getOwnPropertyDescriptor(object, 'accessor').get, class C {}, inner.bind(null),
  Function, Array, print
];
var absent = 0;
var throws = 0;
for (var index = 0; index < restricted.length; index++) {
  if (!Object.prototype.hasOwnProperty.call(restricted[index], 'caller')) absent++;
  try { restricted[index].caller; } catch (error) { if (error instanceof TypeError) throws++; }
}
print(absent === restricted.length && throws === restricted.length);
var poison = Object.getOwnPropertyDescriptor(Function.prototype, 'caller');
print(typeof poison.get === 'function' && poison.get === poison.set
  && !poison.enumerable && poison.configurable);
try { Function.prototype.caller; } catch (error) { print(error instanceof TypeError); }
Object.freeze(inner);
try { Object.defineProperty(inner, 'caller', { value: strictFunction }); }
catch (error) { print(error instanceof TypeError && inner.caller === null); }
void 0;
"#;
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("caller property policy executes through Wasm AOT");
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        vec![HostOutputEvent::PrintLine("true".to_string()); 8]
    );
}

fn assert_reflection_script(source: &str, label: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(60_000),
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|error| panic!("{label}: {error}"));
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{label}: {:?}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        vec![HostOutputEvent::PrintLine(
            "legacy-reflection-ok".to_string()
        )],
        "{label}"
    );
}

const REFLECTION_HARNESS: &str =
    include_str!("../fixtures/legacy_callable_reflection/canonical_harness.js");

#[test]
fn original_legacy_reflection_staging_bodies_keep_all_eleven_executions() {
    let cases = [
        (
            "arguments-property-access-in-function",
            include_str!(
                "../fixtures/legacy_callable_reflection/arguments-property-access-in-function.js"
            ),
        ),
        (
            "function-caller-skips-eval-frames",
            include_str!(
                "../fixtures/legacy_callable_reflection/function-caller-skips-eval-frames.js"
            ),
        ),
        (
            "function-properties",
            include_str!("../fixtures/legacy_callable_reflection/function-properties.js"),
        ),
        (
            "regress-577648-1",
            include_str!("../fixtures/legacy_callable_reflection/regress-577648-1.js"),
        ),
        (
            "regress-577648-2",
            include_str!("../fixtures/legacy_callable_reflection/regress-577648-2.js"),
        ),
        (
            "regress-584355",
            include_str!("../fixtures/legacy_callable_reflection/regress-584355.js"),
        ),
        (
            "regress-586482-1",
            include_str!("../fixtures/legacy_callable_reflection/regress-586482-1.js"),
        ),
        (
            "regress-586482-2",
            include_str!("../fixtures/legacy_callable_reflection/regress-586482-2.js"),
        ),
        (
            "regress-586482-3",
            include_str!("../fixtures/legacy_callable_reflection/regress-586482-3.js"),
        ),
        (
            "regress-586482-4",
            include_str!("../fixtures/legacy_callable_reflection/regress-586482-4.js"),
        ),
    ];
    for (name, body) in cases {
        let source = format!("{REFLECTION_HARNESS}\n{body}\nprint('legacy-reflection-ok');\n");
        assert_reflection_script(&source, name);
        if name == "regress-584355" {
            let strict = format!("'use strict';\n{source}");
            assert_reflection_script(&strict, "strict regress-584355");
        }
    }
}

#[test]
fn recursive_arguments_and_abrupt_exits_restore_each_actual_activation() {
    let body = r#"
var saved = [], marker = {}, trace = '';
function recursive(depth, token) {
  var active = recursive.arguments;
  saved.push(active);
  assert.sameValue(active, arguments);
  assert.sameValue(active[0], depth);
  assert.sameValue(active[1], token);
  assert.sameValue(active.callee, recursive);
  assert.sameValue(recursive.caller, depth === 2 ? outer : recursive);
  // Unsealed implementation-owned properties follow every actual activation.
  // Frozen snapshots and user-owned redefinitions have separate controls.
  if (depth) {
    var nested = recursive(depth - 1, token + 1);
    assert.sameValue(nested, token + 1);
    assert.sameValue(recursive.arguments, active);
    assert.sameValue(recursive.arguments[1], token);
  }
  assert.sameValue(recursive.arguments, active);
  return token;
}
function failure() {
  assert.sameValue(failure.caller, outer);
  assert.sameValue(failure.arguments, arguments);
  throw marker;
}
function probe() { return probe.caller; }
function outer() {
  var active = outer.arguments;
  assert.sameValue(recursive(2, 30), 30);
  assert.sameValue(recursive.arguments, null);
  assert.sameValue(recursive.caller, null);
  try { failure(9); } catch (error) {
    assert.sameValue(error, marker);
    assert.sameValue(failure.arguments, null);
    assert.sameValue(failure.caller, null);
    assert.sameValue(probe(), outer);
    trace += 'c';
  } finally {
    assert.sameValue(outer.arguments, active);
    assert.sameValue(probe(), outer);
    trace += 'f';
  }
  return active;
}
var first = outer(17);
assert.sameValue(first[0], 17);
assert.sameValue(first.callee, outer);
assert.sameValue(trace, 'cf');
assert.sameValue(saved.length, 3);
assert.sameValue(saved[0] === saved[1], false);
assert.sameValue(saved[1] === saved[2], false);
assert.sameValue(outer.arguments, null);
assert.sameValue(outer.caller, null);
assert.sameValue(probe(), null);
print('legacy-reflection-ok');
"#;
    assert_reflection_script(
        &format!("{REFLECTION_HARNESS}\n{body}"),
        "recursive and abrupt activation restoration",
    );
}

#[test]
fn private_arguments_survive_source_binding_replacements_and_mapped_writes() {
    let body = r#"
function overwritten(value) {
  var actual = overwritten.arguments;
  assert.sameValue(actual, arguments);
  value = 12;
  assert.sameValue(actual[0], 12);
  actual[0] = 13;
  assert.sameValue(value, 13);
  arguments = 'replacement';
  assert.sameValue(overwritten.arguments, actual);
  assert.sameValue(overwritten.arguments[0], 13);
  return actual;
}
function named(arguments, tail) {
  var actual = named.arguments;
  assert.sameValue(arguments, 'source-parameter');
  assert.sameValue(actual[0], 'source-parameter');
  assert.sameValue(actual[1], tail);
  assert.sameValue(actual.length, 2);
  arguments = 'changed';
  assert.sameValue(named.arguments, actual);
  assert.sameValue(actual[0], 'changed');
  return actual;
}
function nonSimple(value = 4) {
  assert.sameValue(nonSimple.arguments, arguments);
  assert.sameValue(arguments[0], 8);
  value = 99;
  assert.sameValue(nonSimple.arguments[0], 8);
  assert.throws(TypeError, function() { nonSimple.arguments.callee; });
  return nonSimple.arguments;
}
var actual = overwritten(1);
assert.sameValue(actual[0], 13);
assert.sameValue(overwritten.arguments, null);
assert.sameValue(named('source-parameter', 6)[0], 'changed');
assert.sameValue(named.arguments, null);
assert.sameValue(nonSimple(8)[0], 8);
assert.sameValue(nonSimple.arguments, null);
var descriptor = Object.getOwnPropertyDescriptor(overwritten, 'arguments');
assert.sameValue(descriptor.value, null);
assert.sameValue(descriptor.writable, false);
assert.sameValue(descriptor.enumerable, false);
assert.sameValue(descriptor.configurable, true);
print('legacy-reflection-ok');
"#;
    assert_reflection_script(
        &format!("{REFLECTION_HARNESS}\n{body}"),
        "private arguments identity and mapping",
    );
}

#[test]
fn strict_nonordinary_and_native_callers_are_opaque_and_forwarders_are_transparent() {
    let body = r#"
function inspect() { return inspect.caller; }
function forwarding() {
  assert.sameValue(inspect.call(null), forwarding);
  assert.sameValue(inspect.apply(null, []), forwarding);
  assert.sameValue(Reflect.apply(inspect, null, []), forwarding);
  assert.sameValue(inspect.bind(null)(), forwarding);
  assert.sameValue(eval('inspect();'), forwarding);
  assert.sameValue(eval("eval('inspect();');"), forwarding);
  var fn = inspect.call;
  assert.sameValue(fn.call(inspect, null), forwarding);
  var old = inspect.call;
  inspect.call = function replacement() { return inspect(); };
  assert.sameValue(inspect.call(), inspect.call);
  inspect.call = old;
  assert.sameValue([1].map(inspect)[0], null);
  function* generator() { yield inspect(); }
  assert.sameValue(generator().next().value, null);
  var arrow = () => inspect();
  assert.sameValue(arrow(), null);
  var object = { method() { return inspect(); } };
  assert.sameValue(object.method(), null);
  return inspect();
}
assert.sameValue(forwarding(), forwarding);
function strictCaller() {
  'use strict';
  var result = inspect();
  assert.sameValue(result, null);
  assert.throws(TypeError, function() { strictCaller.caller; });
  assert.throws(TypeError, function() { strictCaller.arguments; });
  return result;
}
function top() {
  assert.sameValue(strictCaller(), null);
  assert.sameValue(inspect(), top);
}
top();
assert.sameValue(inspect.caller, null);
assert.sameValue(inspect.arguments, null);
print('legacy-reflection-ok');
"#;
    assert_reflection_script(
        &format!("{REFLECTION_HARNESS}\n{body}"),
        "opaque and transparent actual callers",
    );
}

#[test]
fn activation_restoration_covers_parameter_throws_construction_and_suspension() {
    let body = r#"
var marker = {}, pending;
function probe() { return probe.caller; }
function defaultFailure() {
  assert.sameValue(defaultFailure.caller, defaulted);
  assert.sameValue(defaulted.arguments.length, 0);
  throw marker;
}
function defaulted(value = defaultFailure()) { return value; }
function Constructed(value) {
  assert.sameValue(Constructed.caller, outer);
  assert.sameValue(Constructed.arguments, arguments);
  this.value = value;
}
function nativeCallback() {
  assert.sameValue(nativeCallback.caller, null);
  throw marker;
}
function* suspended() {
  assert.sameValue(probe(), null);
  yield 3;
  assert.sameValue(probe(), null);
  return 4;
}
async function asynchronous() {
  assert.sameValue(probe(), null);
  await Promise.resolve(5);
  assert.sameValue(probe(), null);
  return 5;
}
function outer() {
  try { defaulted(); } catch (error) { assert.sameValue(error, marker); }
  assert.sameValue(defaulted.caller, null);
  assert.sameValue(defaulted.arguments, null);
  assert.sameValue(probe(), outer);
  assert.sameValue(new Constructed(7).value, 7);
  assert.sameValue(Constructed.caller, null);
  assert.sameValue(Constructed.arguments, null);
  try { [1].map(nativeCallback); } catch (error) { assert.sameValue(error, marker); }
  assert.sameValue(nativeCallback.caller, null);
  assert.sameValue(nativeCallback.arguments, null);
  assert.sameValue(probe(), outer);
  var generator = suspended();
  assert.sameValue(probe(), outer);
  assert.sameValue(generator.next().value, 3);
  assert.sameValue(probe(), outer);
  assert.sameValue(generator.next().value, 4);
  assert.sameValue(probe(), outer);
  pending = asynchronous();
  assert.sameValue(probe(), outer);
}
outer();
assert.sameValue(probe(), null);
pending.then(function finished(value) {
  assert.sameValue(value, 5);
  assert.sameValue(probe(), finished);
  print('legacy-reflection-ok');
});
"#;
    assert_reflection_script(
        &format!("{REFLECTION_HARNESS}\n{body}"),
        "parameter, constructor, native throw and resumable activation exits",
    );
}

#[test]
fn original_legacy_arguments_property_bodies_keep_both_execution_ids() {
    for (label, body) in [
        (
            "10.6-13-a-2",
            include_str!("../fixtures/legacy_callable_reflection/10.6-13-a-2.js"),
        ),
        (
            "10.6-13-a-3",
            include_str!("../fixtures/legacy_callable_reflection/10.6-13-a-3.js"),
        ),
    ] {
        let source = format!(
            "{REFLECTION_HARNESS}\n{body}\nassert(test1.caller === null && test2.caller === null);\nprint('legacy-reflection-ok');\n"
        );
        assert_reflection_script(&source, label);
    }
}

#[test]
fn with_lookup_abrupt_edges_restore_each_actual_source_activation() {
    assert_reflection_script(
        r#"
function outer(failAt) {
  var reads=0, token={}, scope={}, privateArguments;
  function selectedGetter() {
    if(selectedGetter.caller!==inner || inner.caller!==outer) throw 'actual getter/source caller';
    if(++reads===failAt) throw token;
    return 17;
  }
  Object.defineProperty(scope,'selected',{get:selectedGetter});
  function inner(scope, selected) {
    if(inner.caller!==outer || inner.arguments[0]!==scope) throw 'entry activation';
    with(scope) { selected; selected; selected; selected; }
    return inner.arguments;
  }
  try {
    privateArguments=inner(scope,0);
    if(failAt!==0 || privateArguments[0]!==scope || privateArguments[1]!==0 || reads!==4) throw 'normal exit';
  } catch(error) {
    if(error!==token || failAt===0 || reads!==failAt) throw 'throw completion';
  }
  if(inner.caller!==null || inner.arguments!==null || selectedGetter.caller!==null
      || selectedGetter.arguments!==null || outer.arguments[0]!==failAt) throw 'restored inactive activations';
}
outer(0); outer(1); outer(3); outer(4);
print('legacy-reflection-ok');
"#,
        "With lookup source completion epilogue",
    );
}

#[test]
fn shared_source_epilogue_keeps_caught_throws_live_and_finally_completions_whole() {
    assert_reflection_script(
        r#"
var marker={}, returned={}, reads=0, scope={};
function selectedGetter() {
  if(selectedGetter.caller!==inner || inner.caller!==outer) throw 'live getter owner';
  reads++;
  throw marker;
}
Object.defineProperty(scope,'selected',{get:selectedGetter});
function inner() {
  try {
    with(scope) { selected; }
  } catch(error) {
    if(error!==marker || inner.caller!==outer || inner.arguments.length!==0) throw 'caught owner';
    return returned;
  } finally {
    if(inner.caller!==outer || inner.arguments.length!==0) throw 'finalizer owner';
  }
}
function outer() {
  if(inner()!==returned || reads!==1) throw 'whole Return';
  if(inner.caller!==null || inner.arguments!==null || selectedGetter.caller!==null
      || selectedGetter.arguments!==null) throw 'retired owner';
}
outer();
print('legacy-reflection-ok');
"#,
        "caught With throw and finally source completion epilogue",
    );
}

#[test]
fn shared_source_epilogue_preserves_generator_iteration_across_normal_suspension() {
    assert_reflection_script(
        r#"
var token={}, pending;
function probe() { return probe.caller; }
function* synchronous() {
  for(var key in {first:1,second:2}) {
    if(probe()!==null) throw 'generator caller';
    yield key;
  }
  return token;
}
async function* asynchronous() {
  for(var key in {first:1,second:2}) {
    await Promise.resolve(0);
    if(probe()!==null) throw 'async generator caller';
    yield key;
  }
  return token;
}
function outer() {
  var iterator=synchronous(), result=iterator.next();
  if(result.value!=='first' || result.done || probe()!==outer) throw 'first yield';
  result=iterator.next();
  if(result.value!=='second' || result.done || probe()!==outer) throw 'saved enumerator';
  result=iterator.next();
  if(result.value!==token || !result.done || probe()!==outer) throw 'whole generator Return';
  var asyncIterator=asynchronous();
  pending=asyncIterator.next().then(function first(result) {
    if(result.value!=='first' || result.done || probe()!==first) throw 'first async yield';
    return asyncIterator.next();
  }).then(function second(result) {
    if(result.value!=='second' || result.done || probe()!==second) throw 'saved async enumerator';
    return asyncIterator.next();
  }).then(function finished(result) {
    if(result.value!==token || !result.done || probe()!==finished) throw 'whole async Return';
    print('legacy-reflection-ok');
  });
  if(probe()!==outer) throw 'async initialization owner';
}
outer();
if(probe()!==null) throw 'inactive outer';
"#,
        "Normal Yield/Await retain saved iterator cells through shared epilogue",
    );
}

#[test]
fn shared_source_epilogue_restores_depth_after_rejected_owner_admission() {
    assert_reflection_script(
        r#"
var entries=0, firstCount=0;
function recursive() { entries++; recursive(); }
function outer() {
  for(var run=0;run<2;run++) {
    entries=0;
    try { recursive(); throw 'missing source depth rejection'; }
    catch(error) { if(!(error instanceof RangeError)) throw error; }
    if(entries<2 || (run!==0 && entries!==firstCount)) throw 'source depth did not restore';
    firstCount=entries;
    if(recursive.caller!==null || recursive.arguments!==null || outer.arguments.length!==0)
      throw 'rejected entry retired admitted owner';
  }
}
outer();
if(outer.caller!==null || outer.arguments!==null) throw 'inactive outer';
print('legacy-reflection-ok');
"#,
        "source depth guard precedes owner admission and shared epilogue",
    );
}

#[test]
fn frozen_legacy_descriptors_keep_inactive_values_inside_later_calls() {
    let body = r#"
var token = {};
function target(value) {
  assert.sameValue(arguments[0], token);
  assert.sameValue(arguments.callee, target);
  for (var key of ['caller', 'arguments']) {
    var descriptor = Reflect.getOwnPropertyDescriptor(target, key);
    assert.sameValue(descriptor.value, null);
    assert.sameValue(descriptor.writable, false);
    assert.sameValue(descriptor.configurable, false);
    assert.sameValue(target[key], null);
    assert.sameValue(Reflect.get(target, key), descriptor.value);
  }
  return arguments;
}
for (var key of ['caller', 'arguments']) {
  var initial = Reflect.getOwnPropertyDescriptor(target, key);
  assert.sameValue(initial.writable, false);
  assert.sameValue(initial.configurable, true);
  assert.sameValue(Reflect.defineProperty(target, key, {writable: false, configurable: false}), true);
}
function outer() { return target(token); }
var actual = outer();
assert.sameValue(actual[0], token);
assert.sameValue(target.arguments, null);
assert.sameValue(target.caller, null);
function callerFrozen(value) {
  assert.sameValue(callerFrozen.caller, null);
  assert.sameValue(callerFrozen.arguments, arguments);
  return arguments;
}
Reflect.defineProperty(callerFrozen, 'caller', {writable: false, configurable: false});
function argumentsFrozen(value) {
  assert.sameValue(argumentsFrozen.arguments, null);
  assert.sameValue(argumentsFrozen.caller, selectiveCaller);
  return arguments;
}
Reflect.defineProperty(argumentsFrozen, 'arguments', {writable: false, configurable: false});
function selectiveCaller() {
  assert.sameValue(callerFrozen(token)[0], token);
  assert.sameValue(argumentsFrozen(token)[0], token);
}
selectiveCaller();
assert.sameValue(callerFrozen.arguments, null);
assert.sameValue(argumentsFrozen.caller, null);
function frozen() {
  assert.sameValue(frozen.arguments, null);
  assert.sameValue(frozen.caller, null);
  return arguments;
}
Object.freeze(frozen);
var privateArguments = frozen(token);
assert.sameValue(privateArguments[0], token);
assert.sameValue(frozen.arguments, null);
assert.sameValue(frozen.caller, null);
print('legacy-reflection-ok');
"#;
    assert_reflection_script(
        &format!("{REFLECTION_HARNESS}\n{body}"),
        "inactive frozen data properties",
    );
}

#[test]
fn frozen_legacy_snapshots_survive_inner_and_outer_recursive_activation_exits() {
    let body = r#"
function make(freezeAt) {
  var capture = {args: null, caller: null};
  capture.fn = function reflected(depth, token) {
    var actual = arguments;
    assert.sameValue(actual[0], depth);
    assert.sameValue(actual[1], token);
    if (depth === freezeAt && capture.args === null) {
      assert.sameValue(reflected.arguments, actual);
      capture.args = actual;
      capture.caller = reflected.caller;
      Object.freeze(reflected);
    }
    if (depth) reflected(depth - 1, token + 1);
    assert.sameValue(reflected.arguments, capture.args);
    assert.sameValue(reflected.caller, capture.caller);
    assert.sameValue(Reflect.getOwnPropertyDescriptor(reflected, 'arguments').value, capture.args);
    assert.sameValue(Reflect.getOwnPropertyDescriptor(reflected, 'caller').value, capture.caller);
    return actual;
  };
  return capture;
}
var outerFreeze = make(2), innerFreeze = make(0);
function outer() { return [outerFreeze.fn(2, 30), innerFreeze.fn(2, 40)]; }
var returned = outer();
assert.sameValue(outerFreeze.args, returned[0]);
assert.sameValue(outerFreeze.caller, outer);
assert.sameValue(innerFreeze.args === returned[1], false);
assert.sameValue(innerFreeze.args[0], 0);
assert.sameValue(innerFreeze.args[1], 42);
assert.sameValue(innerFreeze.caller, innerFreeze.fn);
var later = outerFreeze.fn(0, 90);
assert.sameValue(later === outerFreeze.args, false);
assert.sameValue(later[1], 90);
assert.sameValue(outerFreeze.fn.arguments, outerFreeze.args);
assert.sameValue(outerFreeze.fn.caller, outer);
var marker = {}, abruptArguments, abruptCaller;
function abrupt() {
  abruptArguments = arguments;
  abruptCaller = abrupt.caller;
  Object.freeze(abrupt);
  throw marker;
}
function invokeAbrupt() { return abrupt(17); }
var caught = false;
try { invokeAbrupt(); } catch (error) { assert.sameValue(error, marker); caught = true; }
assert.sameValue(caught, true);
assert.sameValue(abrupt.arguments, abruptArguments);
assert.sameValue(abrupt.arguments[0], 17);
assert.sameValue(abrupt.caller, invokeAbrupt);
assert.sameValue(abruptCaller, invokeAbrupt);
print('legacy-reflection-ok');
"#;
    assert_reflection_script(
        &format!("{REFLECTION_HARNESS}\n{body}"),
        "active frozen recursive and abrupt snapshots",
    );
}

#[test]
fn user_legacy_property_definitions_and_deletions_survive_recursive_and_later_calls() {
    let body = r#"
var suppliedArguments = {}, suppliedCaller = {};
function replaced(first) {
  if (first) {
    Object.defineProperty(replaced, 'arguments', {value: suppliedArguments});
    Object.defineProperty(replaced, 'caller', {value: suppliedCaller});
    replaced(false);
  }
  assert.sameValue(replaced.arguments, suppliedArguments);
  assert.sameValue(replaced.caller, suppliedCaller);
  assert.sameValue(Reflect.getOwnPropertyDescriptor(replaced, 'arguments').value, suppliedArguments);
  assert.sameValue(Reflect.getOwnPropertyDescriptor(replaced, 'caller').value, suppliedCaller);
  return arguments;
}
replaced(true);
assert.sameValue(replaced(false)[0], false);
assert.sameValue(replaced.arguments, suppliedArguments);
assert.sameValue(replaced.caller, suppliedCaller);
var sameValueSnapshot;
function sameValue(first) {
  if (first) {
    sameValueSnapshot = arguments;
    Object.defineProperty(sameValue, 'arguments', {value: sameValueSnapshot});
    sameValue(false);
  } else {
    assert.sameValue(sameValue.caller, sameValue);
    assert.sameValue(arguments === sameValueSnapshot, false);
  }
  assert.sameValue(sameValue.arguments, sameValueSnapshot);
}
sameValue(true);
assert.sameValue(sameValue.arguments, sameValueSnapshot);
assert.sameValue(sameValueSnapshot[0], true);
assert.sameValue(sameValue.caller, null);
var reads = 0;
function accessor(first) {
  if (first) {
    Object.defineProperty(accessor, 'arguments', {get: function() { reads++; return suppliedArguments; }});
    Object.defineProperty(accessor, 'caller', {get: function() { reads++; return suppliedCaller; }});
    accessor(false);
  }
  return arguments;
}
accessor(true);
accessor(false);
assert.sameValue(reads, 0);
assert.sameValue(typeof Reflect.getOwnPropertyDescriptor(accessor, 'arguments').get, 'function');
assert.sameValue(typeof Reflect.getOwnPropertyDescriptor(accessor, 'caller').get, 'function');
assert.sameValue(reads, 0);
assert.sameValue(accessor.arguments, suppliedArguments);
assert.sameValue(accessor.caller, suppliedCaller);
assert.sameValue(reads, 2);
function erased(first) {
  if (first) {
    assert.sameValue(delete erased.arguments, true);
    assert.sameValue(delete erased.caller, true);
    erased(false);
  }
  assert.sameValue(Reflect.getOwnPropertyDescriptor(erased, 'arguments'), undefined);
  assert.sameValue(Reflect.getOwnPropertyDescriptor(erased, 'caller'), undefined);
  return arguments;
}
erased(true);
assert.sameValue(erased(false)[0], false);
assert.throws(TypeError, function() { erased.arguments; });
assert.throws(TypeError, function() { erased.caller; });
function recreated(first) {
  if (first) {
    delete recreated.arguments;
    delete recreated.caller;
    Object.defineProperty(recreated, 'arguments', {value: suppliedArguments, configurable: true});
    Object.defineProperty(recreated, 'caller', {value: suppliedCaller, configurable: true});
    recreated(false);
  }
  assert.sameValue(recreated.arguments, suppliedArguments);
  assert.sameValue(recreated.caller, suppliedCaller);
  return arguments;
}
recreated(true);
assert.sameValue(recreated(false)[0], false);
assert.sameValue(recreated.arguments, suppliedArguments);
assert.sameValue(recreated.caller, suppliedCaller);
print('legacy-reflection-ok');
"#;
    assert_reflection_script(
        &format!("{REFLECTION_HARNESS}\n{body}"),
        "user-owned replacement descriptors",
    );
}

#[test]
fn frozen_legacy_snapshots_keep_foreign_arguments_and_actual_cross_realm_caller_identity() {
    use scriptw_engine::HostSurfacePolicy;

    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let body = r#"
var other = __scriptwCreateRealm().global;
var capture = {}, token = {};
var target = other.Function('capture', `return function reflected(value) {
  var actual = arguments;
  if (!capture.args) {
    capture.args = actual;
    capture.caller = reflected.caller;
    Object.freeze(reflected);
  }
  if (reflected.arguments !== capture.args || reflected.caller !== capture.caller)
    throw 'foreign frozen snapshot changed';
  return actual;
};`)(capture);
function outer() { return target(token); }
var actual = outer();
assert.sameValue(actual, capture.args);
assert.sameValue(actual[0], token);
assert.sameValue(actual.callee, target);
assert.sameValue(Object.getPrototypeOf(actual), other.Object.prototype);
assert.sameValue(capture.caller, outer);
assert.sameValue(Object.getPrototypeOf(capture.caller), Function.prototype);
gc();
assert.sameValue(target.arguments, actual);
assert.sameValue(target.caller, outer);
assert.sameValue(Reflect.getOwnPropertyDescriptor(target, 'arguments').value, actual);
assert.sameValue(other.Reflect.getOwnPropertyDescriptor(target, 'arguments').value, actual);
assert.sameValue(Reflect.getOwnPropertyDescriptor(target, 'caller').value, outer);
var later = target(23);
assert.sameValue(later === actual, false);
assert.sameValue(later[0], 23);
assert.sameValue(target.arguments, actual);
assert.sameValue(target.caller, outer);
print('legacy-reflection-ok');
"#;
    let source = format!("{REFLECTION_HARNESS}\n{body}");
    let outcome = Engine::new(RealmBuilder::new().build())
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
        .expect("frozen legacy values retain actual cross-Realm identity through Wasm AOT");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        vec![HostOutputEvent::PrintLine("legacy-reflection-ok".into())]
    );
}

#[test]
fn deleting_and_recreating_active_legacy_properties_never_revives_publication() {
    let body = r#"
function original(target, key) {
  var descriptor = Reflect.getOwnPropertyDescriptor(target, key);
  return descriptor !== undefined && descriptor.value === null && !descriptor.writable
    && !descriptor.enumerable && descriptor.configurable;
}
function recreate(target, key) {
  assert.sameValue(delete target[key], true);
  assert.sameValue(Reflect.getOwnPropertyDescriptor(target, key), undefined);
  // The original attributes and value: only the entry record differs.
  Object.defineProperty(target, key,
    {value: null, writable: false, enumerable: false, configurable: true});
  assert.sameValue(original(target, key), true);
}
var outerArguments, middleArguments;
function twin(depth) {
  if (depth === 3) {
    outerArguments = arguments;
    assert.sameValue(twin.arguments, arguments);
    assert.sameValue(twin.caller, driver);
  }
  if (depth === 2) {
    middleArguments = arguments;
    assert.sameValue(twin.arguments, arguments);
    assert.sameValue(twin.caller, twin);
    recreate(twin, 'arguments');
    recreate(twin, 'caller');
  }
  if (depth < 2) {
    // A nested entry after the recreation publishes nothing.
    assert.sameValue(twin.arguments, null);
    assert.sameValue(twin.caller, null);
  }
  if (depth) twin(depth - 1);
  // No exit restores an earlier activation into the recreated properties.
  assert.sameValue(twin.arguments, null);
  assert.sameValue(twin.caller, null);
  assert.sameValue(original(twin, 'arguments') && original(twin, 'caller'), true);
  return arguments;
}
function driver() { return twin(3); }
assert.sameValue(driver(), outerArguments);
assert.sameValue(middleArguments[0], 2);
assert.sameValue(twin(0)[0], 0);
assert.sameValue(twin.arguments, null);
assert.sameValue(twin.caller, null);

// Deletion in a nested activation: the outer exit restores nothing, and a
// recreation after both exits stays user-owned in later calls.
var deepest;
function nested(depth) {
  if (depth < 0) {
    assert.sameValue(nested.arguments, null);
    assert.sameValue(nested.caller, null);
    return arguments;
  }
  if (depth === 0) {
    deepest = arguments;
    assert.sameValue(nested.arguments, arguments);
    assert.sameValue(delete nested.arguments, true);
    assert.sameValue(delete nested.caller, true);
    return arguments;
  }
  assert.sameValue(nested.arguments, arguments);
  nested(depth - 1);
  assert.sameValue(Reflect.getOwnPropertyDescriptor(nested, 'arguments'), undefined);
  assert.sameValue(Reflect.getOwnPropertyDescriptor(nested, 'caller'), undefined);
  return arguments;
}
assert.sameValue(nested(2)[0], 2);
assert.sameValue(deepest[0], 0);
assert.sameValue(Reflect.getOwnPropertyDescriptor(nested, 'arguments'), undefined);
Object.defineProperty(nested, 'arguments',
  {value: null, writable: false, enumerable: false, configurable: true});
Object.defineProperty(nested, 'caller',
  {value: null, writable: false, enumerable: false, configurable: true});
assert.sameValue(nested(-1)[0], -1);
assert.sameValue(nested.arguments, null);
assert.sameValue(nested.caller, null);

// Recreation while inactive, then an active call.
function later(value) {
  assert.sameValue(later.arguments, null);
  assert.sameValue(later.caller, null);
  return arguments;
}
recreate(later, 'arguments');
recreate(later, 'caller');
function laterCaller() { return later(5); }
assert.sameValue(laterCaller()[0], 5);
assert.sameValue(later.arguments, null);
assert.sameValue(original(later, 'arguments') && original(later, 'caller'), true);
print('legacy-reflection-ok');
"#;
    assert_reflection_script(
        &format!("{REFLECTION_HARNESS}\n{body}"),
        "deleted and recreated active legacy properties",
    );
}

#[test]
fn legacy_publication_survives_dictionary_compaction_and_growth_while_active() {
    let body = r#"
var observed = [];
function grown(depth, expectedCaller) {
  assert.sameValue(grown.arguments, arguments);
  assert.sameValue(grown.caller, expectedCaller);
  if (depth === 1) {
    // Holes before caller and arguments, then enough insertions to compact
    // and grow the function's dictionary: the entry records move.
    assert.sameValue(delete grown.length, true);
    assert.sameValue(delete grown.name, true);
    for (var index = 0; index < 96; index++) grown['p' + index] = index;
    for (var index = 0; index < 96; index += 2) delete grown['p' + index];
    for (var index = 96; index < 160; index++) grown['p' + index] = index;
    assert.sameValue(grown.arguments, arguments);
    assert.sameValue(grown.caller, driver);
    observed.push(grown(0, grown));
    // The exit of the nested activation restored this one.
    assert.sameValue(grown.arguments, arguments);
    assert.sameValue(grown.caller, driver);
  }
  return arguments;
}
function driver() { return grown(1, driver); }
var outer = driver();
assert.sameValue(outer[0], 1);
assert.sameValue(observed[0][0], 0);
assert.sameValue(grown.arguments, null);
assert.sameValue(grown.caller, null);
assert.sameValue(grown.p95, 95);
assert.sameValue(grown.p94, undefined);
assert.sameValue(grown.p159, 159);
var keys = Reflect.ownKeys(grown);
assert.sameValue(keys.indexOf('length'), -1);
assert.sameValue(keys.indexOf('caller') < keys.indexOf('p1'), true);
// A later activation still owns both moved properties.
function laterDriver() { return grown(0, laterDriver); }
assert.sameValue(laterDriver()[0], 0);
assert.sameValue(grown.arguments, null);

// Growth inside a nested activation, restoration by the outer exit.
function nestedGrowth(depth) {
  if (depth) {
    var active = arguments;
    nestedGrowth(depth - 1);
    assert.sameValue(nestedGrowth.arguments, active);
    assert.sameValue(nestedGrowth.caller, nestedDriver);
    return arguments;
  }
  for (var index = 0; index < 128; index++) nestedGrowth['q' + index] = index;
  assert.sameValue(nestedGrowth.arguments, arguments);
  assert.sameValue(nestedGrowth.caller, nestedGrowth);
  return arguments;
}
function nestedDriver() { return nestedGrowth(1); }
assert.sameValue(nestedDriver()[0], 1);
assert.sameValue(nestedGrowth.arguments, null);
assert.sameValue(nestedGrowth.caller, null);

// A deleted record dropped by compaction stays dead after recreation.
function dropped(first) {
  if (first) {
    assert.sameValue(delete dropped.arguments, true);
    for (var index = 0; index < 128; index++) dropped['r' + index] = index;
    Object.defineProperty(dropped, 'arguments',
      {value: null, writable: false, enumerable: false, configurable: true});
    dropped(false);
    assert.sameValue(dropped.caller, droppedDriver);
  } else {
    assert.sameValue(dropped.caller, dropped);
  }
  assert.sameValue(dropped.arguments, null);
  return arguments;
}
function droppedDriver() { return dropped(true); }
assert.sameValue(droppedDriver()[0], true);
assert.sameValue(dropped.arguments, null);
assert.sameValue(dropped.caller, null);
print('legacy-reflection-ok');
"#;
    assert_reflection_script(
        &format!("{REFLECTION_HARNESS}\n{body}"),
        "legacy publication across dictionary compaction and growth",
    );
}
