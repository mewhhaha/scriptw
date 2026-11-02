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
        .expect("private numeric fact workload executes through Wasm AOT");
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
fn bounded_remainders_preserve_callbacks_writes_and_non_power_of_two_results() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
function remainderBySubtraction(value, divisor) {
  while (value >= divisor) value -= divisor;
  return value;
}
var calls = [], writes = {};
function record(index, four, eight, three, five) {
  calls.push(index);
  writes[index] = [four, eight, three, five];
}
function workload() {
  var total = 0;
  for (let i = 0; i < 24; i++) {
    record(i, i % 4, i % 8, i % 3, i % 5);
    total += i % 8;
  }
  return total;
}
assert(workload() === 84 && calls.length === 24, 'bounded loop keeps calls and reduction');
for (var index = 0; index < 24; index++) {
  assert(calls[index] === index, 'callback order');
  var row = writes[index];
  var divisors = [4, 8, 3, 5];
  for (var column = 0; column < divisors.length; column++) {
    assert(row[column] === remainderBySubtraction(index, divisors[column]),
      'power and non-power divisors retain Number remainder');
  }
}
print('ok');
"#,
    );
}

#[test]
fn remainder_keeps_operand_coercion_order_and_number_special_values() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var trace = '', results = [];
function workload() {
  for (let i = 0; i < 4; i++) {
    var left = { valueOf: function() { trace += 'L'; return i + 4; } };
    var right = { valueOf: function() { trace += 'R'; return 3; } };
    results.push(left % right);
  }
}
workload();
assert(trace === 'LRLRLRLR' && results.join(',') === '1,2,0,1',
  'numeric coercions run left then right on each evaluation');
var zero = -0;
assert(Object.is(zero % 8, -0) && Object.is(-8 % 4, -0) && -9 % 4 === -1,
  'remainder preserves signed dividend and negative zero');
assert(Number.isNaN(3 % 0) && Number.isNaN(Infinity % 4) && Number.isNaN(3 % NaN),
  'zero divisor and nonfinite Numbers preserve NaN');
assert(3 % Infinity === 3 && 5.5 % 4 === 1.5 && 5 % -4 === 1,
  'infinite negative and fractional divisors keep Number semantics');
var marker = {}, caught;
var throwing = { valueOf: function() { trace += 'X'; throw marker; } };
var rightAfterThrow = { valueOf: function() { trace += 'R'; return 3; } };
try { throwing % rightAfterThrow; } catch (error) { caught = error; }
assert(caught === marker && trace === 'LRLRLRLRX',
  'abrupt left conversion prevents right conversion');
print('ok');
"#,
    );
}

#[test]
fn push_lookup_override_getters_and_eval_arguments_keep_evaluation_order() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var nativePush = Array.prototype.push;
function overrideDuringLoop() {
  var output = [];
  for (let i = 0; i < 5; i++) {
    output.push(i % 4);
    if (i === 1) {
      output.push = function(value) { return nativePush.call(this, value + 10); };
    }
  }
  return output;
}
assert(overrideDuringLoop().join(',') === '0,1,12,13,10', 'push override remains live');

var trace = '', output = [];
Object.defineProperty(output, 'push', {
  configurable: true,
  get: function() {
    trace += 'get;';
    return function(value) { trace += 'call;'; return nativePush.call(this, value); };
  }
});
function argument(value) { trace += 'arg;'; return value; }
function getterLoop() {
  for (let i = 0; i < 3; i++) output.push(argument(i % 4));
}
getterLoop();
assert(trace === 'get;arg;call;get;arg;call;get;arg;call;' && output.join(',') === '0,1,2',
  'callee getter precedes argument effects and invocation');

trace = '';
var oldCalls = 0, newCalls = 0, holder = {};
holder.push = function(value) { oldCalls++; return value; };
function replacePush(value) {
  trace += 'replace;';
  holder.push = function(next) { newCalls++; return next; };
  return value;
}
function selectedCallee() {
  for (let i = 0; i < 2; i++) holder.push(replacePush(i % 4));
}
selectedCallee();
assert(oldCalls === 1 && newCalls === 1 && trace === 'replace;replace;',
  'argument replacement cannot change the already selected callee');

trace = '';
output.length = 0;
function evalArguments() {
  for (let i = 0; i < 4; i++) {
    output.push(eval("trace += 'eval;'; i = i + 1; i % 4"));
  }
}
evalArguments();
assert(trace === 'get;eval;call;get;eval;call;' && output.join(',') === '1,3',
  'direct eval argument observes and changes the live counter after method lookup');
print('ok');
"#,
    );
}

#[test]
fn abrupt_completions_and_shadowed_bindings_preserve_loop_control() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var marker = {}, trace = '', output = [], shadowed = [];
var i = 100;
function workload() {
  for (let i = 0; i < 9; i++) {
    try {
      trace += 'b' + i + ';';
      if (i === 1) continue;
      if (i === 3) throw marker;
      if (i === 6) break;
      output.push(i % 4);
      {
        let i = 20;
        shadowed.push(i % 3);
        i++;
        shadowed.push(i % 3);
      }
    } catch (error) {
      assert(error === marker, 'catch receives exact thrown value');
      trace += 'c' + i + ';';
      continue;
    } finally {
      trace += 'f' + i + ';';
    }
  }
}
workload();
assert(trace === 'b0;f0;b1;f1;b2;f2;b3;c3;f3;b4;f4;b5;f5;b6;f6;',
  'finally executes once for normal continue throw and break completions');
assert(output.join(',') === '0,2,0,1' && shadowed.join(',') === '2,0,2,0,2,0,2,0' && i === 100,
  'shadowed writes cannot replace the induction binding');
function finallyOverridesContinue() {
  var count = 0;
  for (let index = 0; index < 5; index++) {
    try { continue; }
    finally { count++; if (index === 2) break; }
  }
  return count;
}
assert(finallyOverridesContinue() === 3, 'finally break overrides pending continue');
print('ok');
"#,
    );
}

#[test]
fn zero_signed_fractional_and_large_counter_domains_keep_number_results() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
function workloads() {
  var zero = [], signed = [], fractional = [], crossing = [], huge = [];
  for (let i = -0; i < 2; i++) zero.push(i % 4);
  for (let i = -4; i < 2; i++) signed.push(i % 4);
  for (let i = 0.5; i < 3; i++) fractional.push(i % 4);
  for (let i = 4294967294; i < 4294967298; i++) crossing.push(i % 8);
  for (let i = 9007199254740992; i < 9007199254740996; i += 2) huge.push(i % 8);
  assert(Object.is(zero[0], -0) && zero[1] === 1, 'negative-zero initial counter is observable');
  assert(Object.is(signed[0], -0) && signed.slice(1).join(',') === '-3,-2,-1,0,1',
    'negative counters retain signed remainder');
  assert(fractional.join(',') === '0.5,1.5,2.5', 'fractional counters are not truncated');
  assert(crossing.join(',') === '6,7,0,1', 'counter crosses the u32 boundary without wrapping');
  assert(huge.join(',') === '0,2', 'large Number counters retain representable increments');
  var stopped = [];
  for (let i = 0; i < Infinity; i++) {
    stopped.push(i % 3);
    if (i === 3) break;
  }
  assert(stopped.join(',') === '0,1,2,0', 'nonfinite bound keeps ordinary break control');
}
workloads();
print('ok');
"#,
    );
}

#[test]
fn captured_and_global_counters_observe_reentrant_writes() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
function captured() {
  var output = [], readers = [];
  for (let i = 0; i < 5; i++) {
    output.push(i % 4);
    readers.push(function() { return i; });
    (function() { i++; })();
  }
  return [output, readers];
}
var result = captured();
assert(result[0].join(',') === '0,2,0' && result[1][0]() === 1 &&
  result[1][1]() === 3 && result[1][2]() === 5,
  'captured counter updates and per-iteration closures stay live');
var globalCounter = 0, output = [];
function advanceGlobal() { globalCounter++; }
for (globalCounter = 0; globalCounter < 5; globalCounter++) {
  output.push(globalCounter % 4);
  advanceGlobal();
}
assert(output.join(',') === '0,2,0' && globalCounter === 6,
  'global counter calls observe current mutable binding');
function callbackChangesCounter() {
  var output = [];
  for (let i = 0; i < 5; i++) {
    var receiver = { get next() { i++; return i; } };
    output.push(receiver.next % 4);
  }
  return output;
}
assert(callbackChangesCounter().join(',') === '1,3,1', 'getter may mutate a captured counter');
var mappedTrace = '';
function changeMappedCounter(exposed) {
  mappedTrace += 'before:' + exposed[0] + ';';
  exposed[0] = -2;
  mappedTrace += 'after:' + exposed[0] + ';';
}
function mappedParameter(i) {
  var output = [], changed = false;
  for (var i = 0; i < 3; i++) {
    if (!changed) {
      changed = true;
      changeMappedCounter(arguments);
    }
    mappedTrace += 'body:' + i + ';';
    output.push(i % 4);
  }
  return [output, arguments[0]];
}
var mapped = mappedParameter(99);
assert(mapped[0].join(',') === '-2,-1,0,1,2' && mapped[1] === 3 &&
  mappedTrace === 'before:0;after:-2;body:-2;body:-1;body:0;body:1;body:2;',
  'callback writes through mapped arguments precede signed remainder and loop update');
print('ok');
"#,
    );
}

#[test]
fn generator_suspension_keeps_counter_updates_and_abrupt_resume_state() {
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var trace = '';
function* counter() {
  for (let i = 0; i < 5; i++) {
    try {
      var change = yield i % 4;
      if (change !== undefined) i += change;
    } finally {
      trace += 'f' + i + ';';
    }
  }
  return 99;
}
var iterator = counter();
assert(iterator.next().value === 0 && iterator.next(1).value === 2 &&
  iterator.next(1).value === 0, 'resumed values change the live induction counter');
var end = iterator.next();
assert(end.done && end.value === 99 && trace === 'f1;f3;f4;',
  'normal generator completion keeps increment and finally order');
trace = '';
iterator = counter();
iterator.next();
var marker = {}, caught;
try { iterator.throw(marker); } catch (error) { caught = error; }
assert(caught === marker && trace === 'f0;' && iterator.next().done,
  'abrupt resume runs finally without executing the ordinary loop update');
print('ok');
"#,
    );
}
