//! Bitwise results consumed in the int32 domain keep exact Number semantics.
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
        .expect("int32-domain source compiles and executes through Wasm AOT");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

/// Each check names itself; a failing run reports every mismatch.
const CHECKS: &str = r#"
var failures = [];
function check(name, actual, expected) {
  if (!Object.is(actual, expected)) failures.push(name + '=' + String(actual));
}
function throwsType(name, thunk) {
  try { thunk(); failures.push(name + ' did not throw'); }
  catch (error) { if (!(error instanceof TypeError)) failures.push(name + ' threw ' + error); }
}
"#;

fn assert_checks(body: &str) {
    assert_wasm_true(&format!(
        "{CHECKS}{body}\nfailures.length === 0 ? true : failures.join('; ');"
    ));
}

#[test]
fn int32_results_materialize_exact_numbers_at_the_boundaries() {
    assert_checks(
        r#"
function or0(x) { return x | 0; }
function and(x, y) { return x & y; }
function ushr(x, y) { return x >>> y; }
function not(x) { return ~x; }
function notnot(x) { return ~~x; }
function chain(x) { return ((((x ^ 5) << 2) >> 1) ^ 9) << 3; }
function unsignedChain(x) { return ((x | 0) >>> 0) + 1; }
check('or0(-0)', or0(-0), 0);
check('notnot(-0)', notnot(-0), 0);
check('not(-0)', not(-0), -1);
check('or0(2^31)', or0(2147483648), -2147483648);
check('or0(2^31-1)', or0(2147483647), 2147483647);
check('or0(2^32)', or0(4294967296), 0);
check('or0(2^32+1)', or0(4294967297), 1);
check('or0(-2^32-1)', or0(-4294967297), -1);
check('or0(1e20)', or0(1e20), 1661992960);
check('or0(-1.9)', or0(-1.9), -1);
check('or0(NaN)', or0(NaN), 0);
check('or0(Infinity)', or0(Infinity), 0);
check('and(-Infinity,-1)', and(-Infinity, -1), 0);
check('not(NaN)', not(NaN), -1);
check('ushr(-1,0)', ushr(-1, 0), 4294967295);
check('ushr(2^31,0)', ushr(2147483648, 0), 2147483648);
check('ushr(2^32,0)', ushr(4294967296, 0), 0);
check('ushr(-Infinity,0)', ushr(-Infinity, 0), 0);
check('ushr(-8,28)', ushr(-8, 28), 15);
check('ushr(-1,32)', ushr(-1, 32), 4294967295);
check('ushr(1,-1)', ushr(1, -1), 0);
check('chain(1)', chain(1), 8);
check('chain(-1)', chain(-1), -24);
check('unsignedChain(-1)', unsignedChain(-1), 4294967296);
check('shl(1,31)', and(1 << ushr(31, 0), -1), -2147483648);
check('shl(1,32)', and(1 << ushr(32, 0), -1), 1);
check('nested ushr', ushr(and(-1, -1), 31), 1);
check('string operand', or0('-2147483649'), 2147483647);
check('boolean operand', and(true, 3), 1);
check('null operand', not(null), -1);
"#,
    );
}

#[test]
fn int32_predicates_compare_exact_integers() {
    assert_checks(
        r#"
function odd(x) { return (x & 1) ? 'odd' : 'even'; }
function notOdd(x) { return !(x & 1); }
function strictZero(x) { return (x & 1) === 0; }
function looseOne(x) { return 1 == (x & 1); }
function negativeZeroLeft(x) { return -0 === (x & 1); }
function half(x) { return (x | 0) === 0.5; }
function below(x) { return (x & 255) < 128; }
function negative(x) { return (x | 0) <= -1; }
function high(x) { return (x >>> 0) >= 2147483648; }
function mixed(x, y) { return (x >>> 0) < (y | 0); }
function unsignedMax(x) { return (x >>> 0) === 4294967295; }
function unsignedMinusOne(x) { return (x >>> 0) == -1; }
function notEqual(x) { return (x & 3) !== 3; }
function loop(n) { var count = 0; for (var i = 0; (i & 15) !== 15 && i < n; i++) count++; return count; }
check('odd(3)', odd(3), 'odd');
check('odd(2)', odd(2), 'even');
check('odd(-0)', odd(-0), 'even');
check('odd(NaN)', odd(NaN), 'even');
check('odd(-1)', odd(-1), 'odd');
check('notOdd(4)', notOdd(4), true);
check('strictZero(2)', strictZero(2), true);
check('strictZero(-3)', strictZero(-3), false);
check('looseOne("3")', looseOne('3'), true);
check('negativeZeroLeft(2)', negativeZeroLeft(2), true);
check('half(0.5)', half(0.5), false);
check('below(383)', below(383), true);
check('below(255)', below(255), false);
check('negative(-1)', negative(-1), true);
check('negative(4294967295)', negative(4294967295), true);
check('negative(2147483647)', negative(2147483647), false);
check('high(-1)', high(-1), true);
check('high(2147483647)', high(2147483647), false);
check('mixed(-1,-1)', mixed(-1, -1), false);
check('mixed(1,2)', mixed(1, 2), true);
check('unsignedMax(-1)', unsignedMax(-1), true);
check('unsignedMinusOne(-1)', unsignedMinusOne(-1), false);
check('notEqual(7)', notEqual(7), false);
check('notEqual(6)', notEqual(6), true);
check('loop(100)', loop(100), 15);
"#,
    );
}

#[test]
fn int32_switch_discriminants_select_exact_cases() {
    assert_checks(
        r#"
function masked(v) {
  switch (v & 3) { case 0: return 'a'; case 1: return 'b'; case 2: return 'c'; case 3: return 'd'; }
  return 'none';
}
function unsigned(v) {
  switch (v >>> 0) { case 0: return 'a'; case 1: return 'b'; case 2: return 'c'; case 3: return 'd'; default: return 'default'; }
}
function signed(v) {
  switch (v | 0) { case -2: return 'm2'; case -1: return 'm1'; case 0: return 'z'; case 1: return 'p1'; }
  return 'none';
}
check('masked(5)', masked(5), 'b');
check('masked(-1)', masked(-1), 'd');
check('masked(NaN)', masked(NaN), 'a');
check('masked("6")', masked('6'), 'c');
check('masked(-0)', masked(-0), 'a');
check('unsigned(-1)', unsigned(-1), 'default');
check('unsigned(-4294967295)', unsigned(-4294967295), 'b');
check('unsigned(2^31)', unsigned(2147483648), 'default');
check('signed(4294967295)', signed(4294967295), 'm1');
check('signed(4294967294)', signed(4294967294), 'm2');
check('signed(2^31)', signed(2147483648), 'none');
check('signed(1.5)', signed(1.5), 'p1');
"#,
    );
}

#[test]
fn int32_array_keys_read_elements_and_keep_every_miss_route() {
    assert_checks(
        r#"
var rows = [10, 20, 30, 40];
rows['-1'] = 'negative';
rows[4294967295] = 'not an index';
function masked(i) { return rows[i & 3]; }
function signed(i) { return rows[i | 0]; }
function unsigned(i) { return rows[i >>> 0]; }
function read(target, i) { return target[i & 1]; }
check('masked(5)', masked(5), 20);
check('masked(-1)', masked(-1), 40);
check('signed(-1)', signed(-1), 'negative');
check('signed(4294967298)', signed(4294967298), 30);
check('unsigned(-1)', unsigned(-1), 'not an index');
check('unsigned(-4294967294)', unsigned(-4294967294), 30);
check('rows.length', rows.length, 4);
var holes = [1, , 3];
check('hole', read(holes, 1), undefined);
Array.prototype[1] = 'inherited';
check('inherited hole', read(holes, 1), 'inherited');
delete Array.prototype[1];
var reads = 0;
var accessor = [0, 0];
Object.defineProperty(accessor, 1, { get() { reads++; return 'getter'; } });
check('getter', read(accessor, 3), 'getter');
check('getter reads', reads, 1);
check('string', read('ab', 1), 'b');
check('object', read({ 1: 'one' }, 1), 'one');
check('typed', read(new Int8Array([7, -8]), 1), -8);
throwsType('nullish', function () { read(null, 1); });
var steps = [function (row) { return row + 1; }, function (row) { return row * 2; }];
steps['-1'] = function (row) { return -row; };
function frame(index, row) { return steps[index & 1](row); }
function frameSigned(index, row) { return steps[index | 0](row); }
check('frame(0)', frame(0, 5), 6);
check('frame(3)', frame(3, 5), 10);
check('frameSigned(-1)', frameSigned(-1, 5), -5);
throwsType('frameSigned(7)', function () { frameSigned(7, 5); });
"#,
    );
}

#[test]
fn int32_operands_keep_to_numeric_order_and_bigint_errors() {
    assert_checks(
        r#"
var log = [];
var left = { valueOf() { log.push('left'); return 6; } };
var right = { valueOf() { log.push('right'); return 3; } };
var big = { valueOf() { log.push('big'); return 1n; } };
function coercedLeft(x) { return x | (log.push('rhs'), 1); }
function coercedRight(y) { return (log.push('lhs'), 2) & y; }
function both(x, y) { return x >>> y; }
function nested(x, y) { return (x & y) | 0; }
function mask(x) { return x & 1; }
function complement(x) { return ~x; }
check('coercedLeft', coercedLeft(left), 7);
check('coercedLeft order', log.join(), 'rhs,left');
log = [];
check('coercedRight', coercedRight(right), 2);
check('coercedRight order', log.join(), 'lhs,right');
log = [];
check('both', both(left, right), 0);
check('both order', log.join(), 'left,right');
log = [];
check('nested', nested(left, right), 2);
check('nested order', log.join(), 'left,right');
log = [];
throwsType('mixed BigInt object', function () { mask(big); });
check('mixed BigInt order', log.join(), 'big');
log = [];
throwsType('mixed BigInt left', function () { coercedLeft(1n); });
check('mixed BigInt left order', log.join(), 'rhs');
throwsType('mixed BigInt right', function () { coercedRight(1n); });
throwsType('BigInt >>> BigInt', function () { both(1n, 1n); });
throwsType('BigInt >>> Number', function () { both(1n, 1); });
throwsType('Number >>> BigInt', function () { both(1, 1n); });
throwsType('Symbol operand', function () { mask(Symbol('s')); });
function bigAnd(x, y) { return x & y; }
check('BigInt & BigInt', bigAnd(6n, 3n), 2n);
throwsType('BigInt pair then | 0', function () { nested(6n, 3n); });
check('BigInt complement', complement(1n), -2n);
check('Number complement', complement(-1), 0);
var thrown = { valueOf() { throw new RangeError('stop'); } };
var order = [];
try { coercedLeft(thrown); } catch (error) { order.push(error instanceof RangeError); }
check('abrupt ToNumeric', order.join(), 'true');
"#,
    );
}

#[test]
fn int32_captured_slots_match_ordinary_calls_at_every_entry_value() {
    for step in [
        "tick=(tick+1)&255;return row.x+tick;",
        "tick=(tick+3)|0;if(row.x&1)tick=tick^row.y;return tick;",
        "tick=~tick;return tick+0.5;",
        "tick=tick>>>1;return tick;",
        "tick=(tick<<31)>>31;if(tick)tick=tick|row.y;return tick;",
    ] {
        assert_wasm_true(&format!(
            r#"
var rows, step, readTick;
function prepare(x, y) {{
  rows = [{{x: x, y: y}}, {{x: y, y: x}}, {{x: x, y: y}}, {{x: y, y: x}}];
  var tick = x;
  step = function (row) {{ {step} }};
  readTick = function () {{ return tick; }};
}}
function run() {{ var sum = 0; for (var i = 0; i < 32; i++) sum += step(rows[i & 3]); return sum; }}
var values = [-0, 0, 1, -1, 1.5, -1.5, 255, 2147483647, 2147483648, -2147483648, 4294967295, NaN, Infinity];
var same = true;
for (var x of values) {{
  for (var y of [3, -0, 2147483648]) {{
    prepare(x, y);
    var actual = run(), actualTick = readTick();
    prepare(x, y);
    step = new Proxy(step, {{}});
    var expected = run(), expectedTick = readTick();
    if (!Object.is(actual, expected) || !Object.is(actualTick, expectedTick)) same = false;
  }}
}}
same;
"#
        ));
    }
}

#[test]
fn int32_captured_slots_shared_by_two_closures_publish_exact_numbers() {
    assert_wasm_true(
        r#"
var rows, steps, readTick;
function prepare(input) {
  rows = [];
  for (var i = 0; i < 4; i++) rows.push({x: input + i, y: input * 3 + i * 2, mask: (input + i) & 3});
  var tick = input;
  steps = [function (row) {
    tick = (tick + 1) & 255;
    var value = row.x + tick;
    if (row.mask & 1) value = value ^ row.y;
    return value & 1023;
  }, function (row) {
    tick = (tick + 3) & 255;
    var value = row.y + tick;
    if (row.mask & 2) value = value ^ row.x;
    return value & 1023;
  }];
  readTick = function () { return tick; };
}
function frame(index, row) { return steps[index & 1](row); }
function run() { var sum = 0; for (var i = 0; i < 32; i++) sum += frame(i, rows[i & 3]); return sum; }
var same = true;
for (var input of [0, -0, 5, 1.5, 254, 2147483647, -2147483648, 4294967296, NaN]) {
  prepare(input);
  var actual = run(), actualTick = readTick();
  prepare(input);
  steps = [new Proxy(steps[0], {}), new Proxy(steps[1], {})];
  var expected = run(), expectedTick = readTick();
  if (!Object.is(actual, expected) || !Object.is(actualTick, expectedTick)) same = false;
}
same;
"#,
    );
}
