//! Closed Number loops read unwritten captured cells once at entry. Every
//! case below is also correct on the original loop, which an entry miss
//! (TDZ, non-Number value) selects. Sources are sloppy Scripts so that
//! direct eval can declare a var in the calling function.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
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
        .unwrap();
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn deep_captured_chain_and_invariant_identity_tests() {
    // p1..p8 = 2..9 (44) plus six true identity tests per iteration.
    run(
        r#"
var savedMap = Map, savedSet = Set, expectedMap;
function make(input) {
  var p1 = input + 1;
  return (function () { var p2 = input + 2;
  return (function () { var p3 = input + 3;
  return (function () { var p4 = input + 4;
  return (function () { var p5 = input + 5;
  return (function () { var p6 = input + 6;
  return (function () { var p7 = input + 7;
  return (function () { var p8 = input + 8;
    var eMap = expectedMap, bMap = eMap, eSet = savedSet, bSet = eSet;
    var eNumber = Number, bNumber = eNumber, eDate = Date, bDate = eDate;
    var eArray = Array, bArray = eArray, eString = String, bString = eString;
    return function () {
      var sum = 0;
      for (var i = 0; i < 1024; i++) {
        sum += p1 + p2 + p3 + p4 + p5 + p6 + p7 + p8;
        if (bMap === eMap) sum++;
        if (bSet === eSet) sum++;
        if (bNumber === eNumber) sum++;
        if (bDate === eDate) sum++;
        if (bArray === eArray) sum++;
        if (bString === eString) sum++;
      }
      return sum;
    };
  })(); })(); })(); })(); })(); })(); })();
}
expectedMap = savedMap;
var worker = make(1);
var first = worker();
print(first + ':' + worker());
"#,
        "51200:51200",
    );
}

#[test]
fn invariant_strict_equality_follows_is_strictly_equal() {
    // 1 + 1000 + 10000 + 100000 + 10000000 per iteration: NaN is not
    // strictly equal to itself, +0 equals -0, strings compare by content,
    // undefined is not null, and distinct objects differ.
    run(
        r#"
function eqs() {
  var a = {}, b = a, c = {}, n = NaN, z = 0, nz = -0, s1 = 'ab';
  var s2 = ['a', 'b'].join(''), u;
  return function () {
    var hits = 0;
    for (var i = 0; i < 4; i++) {
      if (a === b) hits += 1;
      if (a === c) hits += 10;
      if (n === n) hits += 100;
      if (z === nz) hits += 1000;
      if (s1 === s2) hits += 10000;
      if (u === undefined) hits += 100000;
      if (u === null) hits += 1000000;
      if (a !== c) hits += 10000000;
    }
    return hits;
  };
}
print(String(eqs()()));
"#,
        "40444004",
    );
}

#[test]
fn captured_reads_observe_tdz_and_partial_progress() {
    // Zero trips read nothing; a ReferenceError comes from the first read
    // in iteration order, after the earlier statements updated `s`.
    run(
        r#"
function tdz() {
  var out = [];
  function inner(n) { var s = 0; for (var i = 0; i < n; i++) s += k; return s; }
  out.push(inner(0));
  try { inner(2); out.push('no throw'); } catch (e) { out.push(e instanceof ReferenceError); }
  let k = 3;
  out.push(inner(4));
  return out.join();
}
function partial() {
  var s = 0, i = 0;
  function hold() { return k; }
  try {
    for (i = 0; i < 3; i++) { s += 1; s += k; }
  } catch (e) {
    return s + ':' + i + ':' + (e instanceof ReferenceError);
  }
  let k = 1;
  return 'no throw';
}
print(tdz() + '|' + partial());
"#,
        "0,true,12|1:0:true",
    );
}

#[test]
fn non_number_captured_values_keep_coercions_and_order() {
    // A string concatenates; an object's valueOf runs once per iteration;
    // a string limit compares numerically against the counter.
    run(
        r#"
function strings() {
  var p = 'a';
  return function () { var s = 0; for (var i = 0; i < 3; i++) s += p; return s; };
}
function objects() {
  var count = 0;
  var o = { valueOf: function () { count++; return 2; } };
  var f = function () { var s = 0; for (var i = 0; i < 3; i++) s = s + o; return s; };
  var s = f();
  return s + ':' + count;
}
function limits() {
  var n = '3', m = 4;
  var f = function () { var s = 0; for (var i = 0; i < n; i++) s += 1; return s; };
  var g = function () { var s = 0; for (var i = 0; i < m; i++) s += 1; return s; };
  return f() + ':' + g();
}
print(strings()() + '|' + objects() + '|' + limits());
"#,
        "0aaa|6:3|3:4",
    );
}

#[test]
fn captured_snapshots_are_taken_per_loop_entry() {
    // Writes between entries, including by direct eval in the owner and
    // by a var that direct eval declares in the loop's own body, are seen.
    run(
        r#"
function counter() {
  var step = 1;
  var f = function () { var s = 0; for (var i = 0; i < 5; i++) s += step; return s; };
  var a = f();
  step = 2;
  return a + ':' + f();
}
function evalOwner() {
  var p = 1;
  function inner() { var s = 0; for (var i = 0; i < 4; i++) s += p; return s; }
  var first = inner();
  eval('p = 10');
  return first + ':' + inner();
}
function shadow() {
  var p = 1;
  return function (code) { eval(code); var s = 0; for (var i = 0; i < 3; i++) s += p; return s; };
}
function nested() {
  var limit = 3, scale = 2;
  return function () {
    var s = 0;
    for (var i = 0; i < limit; i++) {
      if (scale === 2) { if (i === 1) s += 100; else s += scale; } else s -= 1;
    }
    return s;
  };
}
print(counter() + '|' + evalOwner() + '|' + shadow()('var p = 5') + '|' + nested()());
"#,
        "5:10|4:40|15|104",
    );
}

#[test]
fn eval_loop_completion_values_track_if_without_else() {
    // ForBodyEvaluation keeps the last non-empty body value: an `if`
    // without `else` whose test is false completes with undefined.
    run(
        r#"
var c1 = eval('var t1 = 0; for (var j1 = 0; j1 < 3; j1++) { if (j1 === 1) t1 += 5; }');
var c2 = eval('var t2 = 0; for (var j2 = 0; j2 < 3; j2++) { if (j2 === 2) t2 += 5; }');
var c3 = eval('var t3 = 0; for (var j3 = 0; j3 < 3; j3++) { t3 += 1; if (j3 === 5) t3 += 5; }');
var c4 = eval('var t4 = 0; for (var j4 = 0; j4 < 3; j4++) { if (j4 === 0) t4 += 7; else t4 += 1; }');
print(String(c1) + ',' + String(c2) + ',' + String(c3) + ',' + String(c4));
"#,
        "undefined,5,undefined,9",
    );
}
