use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, ObservedCompletion, ObservedJsValue, RealmBuilder,
    RunOptions,
};

fn assert_addition(source: &str) {
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
        .expect("guarded addition compiles and executes through Wasm AOT");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        outcome.completion,
        ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
        "source:\n{source}"
    );
}

fn number_source(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value == f64::INFINITY {
        "Infinity".to_owned()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_owned()
    } else {
        format!("{value:?}")
    }
}

#[test]
fn runtime_number_pairs_preserve_ieee_results_and_compound_values() {
    let values = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        1.5,
        -1.5,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::MIN_POSITIVE,
        9_007_199_254_740_991.0,
        9_007_199_254_740_992.0,
        -9_007_199_254_740_992.0,
        f64::MAX,
        -f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];
    let inputs = values
        .into_iter()
        .flat_map(|left| {
            values
                .into_iter()
                .flat_map(move |right| [left, right, left + right])
        })
        .map(number_source)
        .collect::<Vec<_>>()
        .join(",");
    assert_addition(&format!(
        r#"
var inputs = new Float64Array([{inputs}]);
function add(left, right) {{ return left + right; }}
function compound(left, right) {{
  var value = left;
  var result = (value += right);
  if (!Object.is(result, value)) throw 'compound value differs from store';
  return result;
}}
for (var index = 0; index < inputs.length; index += 3) {{
  var left = inputs[index], right = inputs[index + 1], expected = inputs[index + 2];
  if (!Object.is(add(left, right), expected)) throw 'runtime number pair';
  if (!Object.is(compound(left, right), expected)) throw 'runtime compound pair';
}}
function accumulate(seed, step, count) {{
  var value = seed;
  for (var i = 0; i < count; i++) value += step;
  return value;
}}
accumulate(0, 1, 128) === 128 && accumulate('x', 1, 2) === 'x11' &&
accumulate(1n, 2n, 2) === 5n && accumulate(true, 1, 2) === 3;
"#
    ));
}

#[test]
fn guarded_compound_addition_keeps_saved_values_and_each_reference_owner() {
    assert_addition(
        r#"
function check(ok) { if (!ok) throw 'saved addition reference'; }
function local() {
  var value = 1;
  var result = (value += (value = 'replacement', 2));
  check(result === 3 && value === 3);
  value = 'x';
  result = (value += (value = 1, 2));
  check(result === 'x2' && value === 'x2');
}
local();
function captured() {
  var value = 1;
  function replace() { value = 'replacement'; return 2; }
  var result = (value += replace());
  check(result === 3 && value === 3);
}
captured();
globalThis.guardedSavedNumber = 1;
var globalResult = (guardedSavedNumber += (guardedSavedNumber = 'replacement', 2));
check(globalResult === 3 && guardedSavedNumber === 3);
var trace = [], stored = 1;
var target = {
  get value() { trace.push('get'); return stored; },
  set value(value) { trace.push('set:' + value); stored = value; }
};
function receiver() { trace.push('receiver'); return target; }
function key() { trace.push('key'); return 'value'; }
function rhs() { trace.push('rhs'); stored = 'replacement'; return 2; }
var propertyResult = (receiver()[key()] += rhs());
check(propertyResult === 3 && stored === 3 && trace.join(',') === 'receiver,key,get,rhs,set:3');
var scope = {value: 1}, withResult;
with (scope) { withResult = (value += (scope.value = 'replacement', 2)); }
check(withResult === 3 && scope.value === 3);
class PrivateOwner {
  #value = 1;
  run() {
    var result = (this.#value += (this.#value = 'replacement', 2));
    return result === 3 && this.#value === 3;
  }
}
check(new PrivateOwner().run());
class Parent {
  get value() { trace.push('super get'); return 1; }
  set value(value) { trace.push('super set:' + value); this.saved = value; }
}
class Child extends Parent {
  run() {
    var result = (super.value += (trace.push('super rhs'), this.saved = 'replacement', 2));
    return result === 3 && this.saved === 3;
  }
}
trace = [];
check(new Child().run() && trace.join(',') === 'super get,super rhs,super set:3');
var immutableTrace = [];
(function () {
  'use strict';
  const value = 1;
  var caught;
  try { value += (immutableTrace.push('rhs'), 2); } catch (error) { caught = error; }
  check(caught instanceof TypeError && value === 1 && immutableTrace.join(',') === 'rhs');
})();
true;
"#,
    );
}

#[test]
fn non_number_pairs_retain_live_coercion_order_and_abrupt_completion() {
    assert_addition(
        r#"
function check(ok) { if (!ok) throw 'addition fallback'; }
function add(left, right) { return left + right; }
for (var row of [[1,'2','12'],['1',2,'12'],[null,1,1],[true,1,2],
                 [undefined,1,NaN],[1n,2n,3n],[Object(1),2,3],[2,Object('x'),'2x']]) {
  check(Object.is(add(row[0], row[1]), row[2]));
}
var trace = [];
var left = {[Symbol.toPrimitive](hint) { trace.push('left:' + hint); return 1; }};
var right = {[Symbol.toPrimitive](hint) { trace.push('right:' + hint); return 2; }};
function readLeft() { trace.push('left expression'); return left; }
function readRight() { trace.push('right expression'); return right; }
check(readLeft() + readRight() === 3);
check(trace.join(',') === 'left expression,right expression,left:default,right:default');
var original = {valueOf() { throw 'old hook'; }}, value = original;
trace = [];
var changed = (value += (trace.push('rhs'), original[Symbol.toPrimitive] = function(hint) {
  trace.push('new:' + hint); return 'new';
}, value = 0, 2));
check(changed === 'new2' && value === 'new2' && trace.join(',') === 'rhs,new:default');
var marker = {}, caught, finalized = 0;
left[Symbol.toPrimitive] = function(hint) { trace.push('left:' + hint); throw marker; };
trace = [];
try { readLeft() + readRight(); } catch (error) { caught = error; } finally { finalized++; }
check(caught === marker && finalized === 1 && trace.join(',') === 'left expression,right expression,left:default');
function failRight() { trace.push('right throw'); throw marker; }
trace = [];
caught = undefined;
try { readLeft() + failRight(); } catch (error) { caught = error; }
check(caught === marker && trace.join(',') === 'left expression,right throw');
for (var pair of [[1n,2],[1,2n],[Symbol(),1],[1,Symbol()],['x',Symbol()]]) {
  caught = undefined;
  try { add(pair[0], pair[1]); } catch (error) { caught = error; }
  check(caught instanceof TypeError);
}
var sum = 0, throws = 0;
var abrupt = {[Symbol.toPrimitive]() { throw marker; }};
for (var index = 0; index < 4; index++) {
  try { sum += index === 1 ? abrupt : 1; }
  catch (error) { check(error === marker && sum === 1); throws++; }
}
sum === 3 && throws === 1 && add(2, 3) === 5;
"#,
    );
}
