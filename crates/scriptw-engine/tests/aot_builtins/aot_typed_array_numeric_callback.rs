use scriptw_engine::{
    CompileOptions, EmbeddedEvaluation, EmbeddedRuntime, Engine, ExecutionBackend, ForeignValue,
    HostSurfacePolicy, JsCompletion, RealmBuilder, RunOptions,
};

fn check_callbacks(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compiler worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{source}");
        let outcome = Engine::new(RealmBuilder::new().build())
            .run_script(
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
            .unwrap_or_else(|error| panic!("numeric callback execution failed: {error}\n{source}"));
        assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
        assert!(
            outcome.note.contains("boolean(true)"),
            "{}\n{source}",
            outcome.note
        );
    }
}

#[test]
fn ordinary_and_arrow_number_callbacks_keep_typed_array_merge_semantics() {
    check_callbacks(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
function ascending(a, b) { return a - b; }
var descending = (a, b) => b - a;
for (var C of [Int8Array, Uint16Array, Int32Array, Float16Array, Float32Array, Float64Array]) {
  var source = new C([4, 1, 3, 2]);
  var copy = source.toSorted(ascending);
  check(copy.join() === '1,2,3,4' && source.join() === '4,1,3,2', 'ordinary copying comparator');
  check(source.sort(descending) === source && source.join() === '4,3,2,1', 'Arrow in-place comparator');
}
function tied(a, b) { return 0 / 0; }
var input = new Float64Array([3, -0, 2, 0, NaN, Infinity, -Infinity]);
var stable = input.toSorted(tied);
for (var index = 0; index < input.length; index++)
  check(Object.is(stable[index], input[index]), 'NaN callback normalizes before stable merge');
var zero = new Float64Array([0, -0]).toSorted(ascending);
check(Object.is(zero[0], 0) && Object.is(zero[1], -0), 'callback signed-zero ties are stable');
var zeros = new Float64Array([3, -0, 2, 0]).toSorted((a, b) => -0);
check(zeros[0] === 3 && Object.is(zeros[1], -0) && zeros[2] === 2 && Object.is(zeros[3], 0), 'negative-zero comparison');
function infiniteSign(a, b) { return (a - b) * (1 / 0); }
var extremes = new Float64Array([Infinity, 3, -Infinity, -2, 3]).toSorted(infiniteSign);
check(extremes[0] === -Infinity && extremes[1] === -2 && extremes[2] === 3 &&
  extremes[3] === 3 && extremes[4] === Infinity, 'infinite comparison results');
var defaults = new Float64Array([NaN, 0, -0, Infinity, -Infinity]).toSorted();
check(defaults[0] === -Infinity && Object.is(defaults[1], -0) && Object.is(defaults[2], 0) &&
  defaults[3] === Infinity && Number.isNaN(defaults[4]), 'default comparison stays distinct');
check(new BigInt64Array([3n, -1n, 2n]).toSorted().join() === '-1,2,3', 'default BigInt comparison');
check(ascending.length === 2 && descending.length === 2 &&
  Object.prototype.hasOwnProperty.call(ascending, 'prototype') &&
  !Object.prototype.hasOwnProperty.call(descending, 'prototype'), 'real ordinary and Arrow objects');
true;
"#,
    );
}

#[test]
fn callback_fallback_preserves_coercion_boxed_values_symbols_bigints_bound_and_proxy_calls() {
    check_callbacks(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
function difference(a, b) { return a - b; }
var calls = 0, conversions = 0;
var values = new Int16Array([3, 1, 2]);
var copied = values.toSorted(function(a, b) {
  calls++;
  return { valueOf: function() { conversions++; gc(); return a - b; } };
});
check(copied.join() === '1,2,3' && calls > 0 && conversions === calls, 'observable result ToNumber');
check(values.toSorted(function(a, b) { return new Number(a - b); }).join() === '1,2,3', 'boxed Number result');
var boxedAConversions = 0, boxedBConversions = 0;
var boxedA = { valueOf: function() { boxedAConversions++; return 3; } };
var boxedB = { valueOf: function() { boxedBConversions++; return 1; } };
var ordinary = [boxedA, boxedB].toSorted(difference);
check(ordinary[0] === boxedB && ordinary[1] === boxedA, 'actual object operands retain sorted identities');
check(boxedAConversions > 0 && boxedBConversions > 0, 'actual object operands decline scalar entry with observable coercions');
var numberA = new Number(3), numberB = new Number(1);
var boxed = [numberA, numberB].toSorted(difference);
check(boxed[0] === numberB && boxed[1] === numberA, 'actual boxed Number operands');
var symbolThrew = false;
try { [Symbol('operand'), 1].toSorted(difference); } catch (error) { symbolThrew = error instanceof TypeError; }
check(symbolThrew, 'actual Symbol operand conversion');
var bad = [function(a, b) { return Symbol('comparison'); }, function(a, b) { return 1n; }];
for (var comparator of bad) {
  var threw = false;
  try { new Int16Array([2, 1]).sort(comparator); } catch (error) { threw = error instanceof TypeError; }
  check(threw, 'comparison ToNumber rejects Symbol and BigInt');
}
var bigintThrew = false;
try { new BigInt64Array([2n, 1n]).sort(difference); } catch (error) { bigintThrew = error instanceof TypeError; }
check(bigintThrew, 'BigInt operands use actual callback and reject BigInt result');
check(new BigInt64Array([2n, 1n]).sort(function(a, b) { return 0; }).join() === '2,1', 'BigInt stable callback fallback');
function first(a, b) { return a; }
var bound = first.bind(null, 0);
check(new Int16Array([3, 1]).toSorted(bound).join() === '3,1', 'bound prefix arguments');
var observed = 0;
function observeArguments(a, b) {
  check(arguments.length === 2 && arguments[0] === a && arguments[1] === b, 'actual arguments observer');
  observed++; return a - b;
}
check(values.toSorted(observeArguments).join() === '1,2,3' && observed > 0, 'observer excludes scalar entry');
function make(direction) { return function(a, b) { return (a - b) * direction; }; }
check(values.toSorted(make(-1)).join() === '3,2,1', 'captured environment excludes scalar entry');
var proxyCalls = 0;
var proxy = new Proxy(difference, { apply: function(target, receiver, argumentsList) {
  check(receiver === undefined && argumentsList.length === 2, 'original callback Call operands');
  proxyCalls++; gc(); return argumentsList[1] - argumentsList[0];
} });
check(new Int16Array([1, 3, 2]).toSorted(proxy).join() === '3,2,1' && proxyCalls > 0, 'Proxy apply owns result');
var token = {}, failed = false, unchanged = new Int16Array([3, 1, 2]);
try { unchanged.sort(function(a, b) { return { valueOf: function() { throw token; } }; }); }
catch (error) { failed = error === token; }
check(failed && unchanged.join() === '3,1,2', 'original coercion throw precedes writeback');
true;
"#,
    );
}

#[test]
fn real_comparator_identity_and_snapshots_survive_reentry_and_gc() {
    check_callbacks(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
function difference(a, b) { return a - b; }
var source = new Float64Array([4, 1, 3, 2]), entered = 0;
var selected = new Proxy(difference, { apply: function(target, receiver, values) {
  entered++;
  selected = function(a, b) { return b - a; };
  if (entered === 1) { source.fill(99); gc(); }
  return values[0] - values[1];
} });
var result = source.toSorted(selected);
check(result.join() === '1,2,3,4' && source.join() === '99,99,99,99' && entered > 1,
  'saved callback and complete source snapshot');
var retained = (a, b) => b - a;
for (var round = 0; round < 3; round++) {
  var garbage = [];
  for (var index = 0; index < 128; index++) garbage.push({ marker: index });
  garbage = null; gc();
  check(new Float64Array([1, 3, 2]).sort(retained).join() === '3,2,1', 'rooted callback after collection');
}
true;
"#,
    );
}

#[test]
fn numeric_callback_entries_follow_the_actual_foreign_realm_and_program() {
    check_callbacks(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
function local(a, b) { return a - b; }
var other = __scriptwCreateRealm();
var foreign = other.evalScript('(function compare(a,b){return b-a;})');
var arrow = other.evalScript('((a,b)=>a-b)');
var later = other.evalScript('(function compare(a,b){return a-b;})');
var held = foreign;
other.evalScript('globalThis.compare = function(a,b){return 0;};');
gc();
check(Object.getPrototypeOf(held) === other.global.Function.prototype, 'foreign callable identity');
for (var comparator of [held, arrow, later, local]) {
  var expected = comparator === held ? '3,2,1' : '1,2,3';
  check(new Float64Array([2, 1, 3]).toSorted(comparator).join() === expected,
    'TypedArray calls actual defining Program entry');
  check([2, 1, 3].toSorted(comparator).join() === expected, 'Array shares actual defining Program entry');
}
var method = other.global.Float64Array.prototype.toSorted;
var result = method.call(new Float64Array([3, 1, 2]), local);
check(Object.getPrototypeOf(result) === other.global.Float64Array.prototype &&
  result.join() === '1,2,3', 'method Realm remains independent of callback Realm');
gc();
check(new Int16Array([2, 3, 1]).sort(held).join() === '3,2,1' && held(4, 9) === 5,
  'retained foreign callback and original ordinary entry after later P and GC');
true;
"#,
    );
}

#[test]
fn callback_depth_decline_matches_generic_entry_and_recovers_with_correct_error_realm() {
    let foreign_entries = if scriptw_engine::NATIVE_SOURCE_COMPILER_ENABLED {
        r#"
function source(body) { return ['(function compare(a,b){', body, '})'].join(''); }
var foreignFast = other.global.eval(source('return a-b;'));
var foreignGeneric = other.global.eval(source('var unused=0;return a-b;'));
"#
    } else {
        r#"
var foreignFast = other.evalScript('(function compare(a,b){return a-b;})');
var foreignGeneric = other.evalScript('(function compare(a,b){var unused=0;return a-b;})');
"#
    };
    let source = r#"
function check(value, label) { if (!value) throw new Error(label); }
function fast(a, b) { return a - b; }
var arrow = (a, b) => a - b;
function generic(a, b) { var unused = 0; return a - b; }
var count = 0;
function dive(comparator) {
  var values = new Int16Array([2, 1]).toSorted(comparator);
  if (values[0] !== 1 || values[1] !== 2) throw 'wrong values';
  count++;
  dive(comparator);
}
function attempt(comparator, prototype) {
  count = 0;
  var threw = false;
  try { dive(comparator); }
  catch (error) { threw = Object.getPrototypeOf(error) === prototype; }
  check(threw, 'depth error defining Realm');
  return count;
}
var ordinary = attempt(generic, RangeError.prototype);
check(ordinary > 0 && attempt(fast, RangeError.prototype) === ordinary &&
  attempt(arrow, RangeError.prototype) === ordinary && attempt(fast, RangeError.prototype) === ordinary,
  'fast and generic source depth parity and recovery');
var other = __scriptwCreateRealm();
/* FOREIGN_ENTRIES */
var foreign = attempt(foreignGeneric, other.global.RangeError.prototype);
check(attempt(foreignFast, other.global.RangeError.prototype) === foreign && foreign === ordinary,
  'declined foreign thunk returns to its checked ordinary entry');
gc();
check(new Int16Array([3, 1, 2]).sort(fast).join() === '1,2,3' &&
  new Int16Array([3, 1, 2]).sort(foreignFast).join() === '1,2,3', 'normal calls recover after depth exhaustion');
true;
"#.replace("/* FOREIGN_ENTRIES */", foreign_entries);
    check_callbacks(&source);
}

#[test]
fn retained_embedding_comparator_survives_export_replacement_and_store_gc() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(
            r#"
export let comparator = function descending(a, b) { return b - a; };
export function forget() { comparator = null; return true; }
export function consume(compare) {
  const typed = new Float64Array([1, 3, 2]).toSorted(compare);
  const ordinary = [1, 3, 2].toSorted(compare);
  return typed.join() === '3,2,1' && ordinary.join() === '3,2,1';
}
"#,
            CompileOptions::default(),
        )
        .unwrap();
    let artifact = compiler.emit_wasm(&unit).unwrap();
    let mut config = wasmtime::Config::new();
    scriptw_engine::configure_wasm_runtime(&mut config);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let runtime = EmbeddedRuntime::compile(&engine, &artifact).unwrap();
    let state = runtime.new_state(RealmBuilder::new().build(), ()).unwrap();
    let mut store = wasmtime::Store::new(&engine, state);
    store.limiter(|state| state.limits_mut());
    store.set_epoch_deadline(u64::MAX / 2);
    let mut linker = wasmtime::Linker::new(&engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    let normal = |completion| match completion {
        JsCompletion::Normal(value) => value,
        JsCompletion::Throw(value) => panic!("embedding operation threw: {value:?}"),
    };
    let comparator = normal(instance.get_export(&mut store, "comparator").unwrap());
    let consume = normal(instance.get_export(&mut store, "consume").unwrap());
    let forget = normal(instance.get_export(&mut store, "forget").unwrap());
    assert_eq!(
        normal(instance.call(&mut store, &forget, None, &[]).unwrap()).as_boolean(),
        Some(true)
    );
    for _ in 0..3 {
        store
            .gc(None)
            .expect("actual collection with only the retained comparator root");
        let result = normal(
            instance
                .call(&mut store, &consume, None, &[comparator.clone()])
                .unwrap(),
        );
        assert_eq!(result.as_boolean(), Some(true));
    }
    let left = instance
        .value(&mut store, ForeignValue::Number(4.0))
        .unwrap();
    let right = instance
        .value(&mut store, ForeignValue::Number(9.0))
        .unwrap();
    let result = normal(
        instance
            .call(&mut store, &comparator, None, &[left, right])
            .unwrap(),
    );
    assert_eq!(
        result.as_number(),
        Some(5.0),
        "the real ordinary entry remains callable"
    );
}
