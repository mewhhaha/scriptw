use scriptw_engine::{CompileOptions, Engine, ExecutionBackend, RealmBuilder, RunOptions};

fn assert_wasm_true(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("dense switch compiles and executes through Wasm AOT");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn dense_switch_matches_only_exact_numbers_without_observable_conversion() {
    assert_wasm_true(
        r#"
function select(value) {
  switch (value) {
    case -1: return 'minus';
    case -0: return 'zero';
    case 0: return 'duplicate';
    default: return 'miss';
    case 1: return 'one';
    case 2: return 'two';
  }
}
function minimum(value) {
  switch (value) {
    case -2147483648: return 0;
    case -2147483647: return 1;
    case -2147483646: return 2;
    case -2147483645: return 3;
    default: return -1;
  }
}
function maximum(value) {
  switch (value) {
    case 2147483644: return 0;
    case 2147483645: return 1;
    case 2147483646: return 2;
    case 2147483647: return 3;
    default: return -1;
  }
}
var conversions = 0;
var object = {valueOf() { ++conversions; return 1; }, toString() { ++conversions; return '1'; }};
var misses = [1.5, -Number.MIN_VALUE, Number.MIN_VALUE, NaN, Infinity, -Infinity,
  4294967296, -4294967296, '1', true, false, null, undefined, 1n, Symbol('one'), object];
var correct = true;
for (var value of misses) if (select(value) !== 'miss') correct = false;
correct && conversions === 0 && select(-1) === 'minus' && select(0) === 'zero' &&
  select(-0) === 'zero' && select(1) === 'one' && select(2) === 'two' &&
  minimum(-2147483648) === 0 && minimum(-2147483645) === 3 &&
  minimum(2147483647) === -1 && minimum(-2147483648.5) === -1 &&
  maximum(2147483644) === 0 && maximum(2147483647) === 3 &&
  maximum(-2147483648) === -1 && maximum(2147483648) === -1;
"#,
    );
}

#[test]
fn dense_switch_keeps_fallthrough_default_holes_and_single_discriminant_evaluation() {
    assert_wasm_true(
        r#"
function select(value) {
  var result = '';
  selected: switch (value) {
    case 0: result += 'a';
    case 1: result += 'b';
    default: result += 'd';
    case 2: result += 'c'; break selected;
    case 4: result += 'e';
  }
  return result;
}
function withoutDefault(value) {
  var result = 9;
  switch (value) {
    case 0: result = 0; break;
    case 1: result = 1; break;
    case 2: result = 2; break;
    case 3: result = 3; break;
  }
  return result;
}
var evaluations = 0;
var chosen = select((++evaluations, 2));
evaluations === 1 && chosen === 'c' && select(0) === 'abdc' && select(1) === 'bdc' &&
  select(3) === 'dc' && select(4) === 'e' && select(99) === 'dc' &&
  withoutDefault(2) === 2 && withoutDefault(9) === 9;
"#,
    );
}

#[test]
fn dense_switch_keeps_lexical_tdz_hoisting_and_abrupt_environment_cleanup() {
    assert_wasm_true(
        r#"
function lexical(value) {
  'use strict';
  switch (value) {
    case 0: return typeof later;
    case 1: let later = 7; return later;
    case 2: function hoisted() { return 11; } return hoisted();
    case 3: return hoisted();
  }
}
var tdz = false;
try { lexical(0); } catch (error) { tdz = error instanceof ReferenceError; }
var trace = '';
var saved;
let outside = 'outer';
outer: for (var index = 0; index < 4; ++index) {
  try {
    switch (index) {
      case 0: let inside = 'inner'; saved = function() { return inside; }; trace += saved(); continue outer;
      case 1: { let block = 7; trace += block; break; }
      case 2: trace += outside; break outer;
      case 3: throw 'unreachable';
    }
    trace += ':' + outside;
  } finally { trace += '!'; }
}
tdz && lexical(1) === 7 && lexical(2) === 11 && lexical(3) === 11 &&
  trace === 'inner!7:outer!outer!' && saved() === 'inner' && outside === 'outer' && index === 2;
"#,
    );
}

#[test]
fn effectful_switch_selectors_keep_source_order_and_stop_after_first_match() {
    assert_wasm_true(
        r#"
var trace = '';
function select(value) {
  switch ((trace += 'd', value)) {
    case (trace += 'a', 0): return 'zero';
    default: return 'default';
    case (trace += 'b', 1): return 'one';
    case (trace += 'c', 2): return 'two';
    case (trace += 'e', 3): return 'three';
  }
}
var first = select(1), before = trace;
trace = '';
var second = select(9);
first === 'one' && before === 'dab' && second === 'default' && trace === 'dabce';
"#,
    );
}
