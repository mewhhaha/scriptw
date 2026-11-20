//! Implementation limits end in the spec's answer or a catchable RangeError,
//! never in a Wasm trap that ends the program. Expected values were checked
//! against ECMA-262 and, where V8 follows it, against Node 24.

use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, RealmBuilder, RunOptions,
};

/// Tests that drive a Store's heap toward its 1 GiB cap hold this, so at most
/// one of them runs at a time beside the target's other tests.
static HEAP_FILLING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn heap_filling() -> std::sync::MutexGuard<'static, ()> {
    HEAP_FILLING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn completion(source: &str) -> String {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    Engine::new(RealmBuilder::new().build())
        .run_script(
            source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(120_000),
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|error| panic!("{error:?}\n{source}"))
        .note
}

fn assert_completion(source: &str, expected: &str) {
    let note = completion(source);
    assert!(
        note.contains(&format!("string({expected})")),
        "expected string({expected}), got {note}\n{source}"
    );
}

// OrdinaryHasProperty and OrdinarySet tail-call the prototype's method, so a
// chain longer than any stack answers as the spec does.
#[test]
fn in_and_set_answer_on_a_prototype_chain_longer_than_the_stack() {
    assert_completion(
        r#"
var root = {}, setterReceiver;
Object.defineProperty(root, 'viaSetter', { set: function (v) { setterReceiver = this; } });
var p = root;
for (var i = 0; i < 200000; i++) p = Object.create(p);
var results = ['x' in p, 'viaSetter' in p];
p.x = 1;
results.push(p.x, Object.getOwnPropertyNames(p).join());
p.viaSetter = 2;
results.push(setterReceiver === p, Object.prototype.hasOwnProperty.call(p, 'viaSetter'));
Object.defineProperty(root, 'ro', { value: 0, writable: false });
results.push(Reflect.set(p, 'ro', 3), p.ro);
results.join();
"#,
        "false,true,1,x,true,false,false,0",
    );
}

// Recursion through helpers that the program's data drives is bounded: the
// RangeError is catchable, and the depth is restored afterwards so a later
// nested operation below the bound still succeeds.
#[test]
fn deep_json_proxy_and_join_recursion_throw_catchable_range_errors() {
    assert_completion(
        r#"
function nested(depth) { var a = [], b = a; for (var i = 0; i < depth; i++) { b[0] = []; b = b[0]; } return a; }
function attempt(f) { try { f(); return 'ok'; } catch (e) { return e instanceof RangeError ? 'RangeError' : 'other:' + e; } }
var deep = nested(100000);
var results = [];
for (var round = 0; round < 3; round++) {
  results.push(attempt(function () { JSON.stringify(deep); }));
  results.push(attempt(function () { String(deep); }));
}
var proxy = {};
for (var i = 0; i < 100000; i++) proxy = new Proxy(proxy, {});
results.push(attempt(function () { return proxy.x; }));
results.push(attempt(function () { return 'x' in proxy; }));
results.push(attempt(function () { proxy.x = 1; }));
results.push(attempt(function () { return Object.keys(proxy); }));
var shallow = {};
for (var i = 0; i < 1000; i++) shallow = new Proxy(shallow, {});
shallow.y = 7;
results.push(shallow.y, 'y' in shallow);
results.push(JSON.stringify(nested(2000)).length, String(nested(1000)));
results.join();
"#,
        "RangeError,RangeError,RangeError,RangeError,RangeError,RangeError,RangeError,RangeError,RangeError,RangeError,7,true,4002,",
    );
}

// The implementation's maximum String length, `MAX_STRING_LENGTH` in
// scriptw-aot-wasm: 2^27 - 6 * 2^20 code units, derived from the 1 GiB Store
// heap.
const MAX_STRING_LENGTH: u64 = (1 << 27) - 6 * (1 << 20);

// ECMA-262 allows Strings of 2^53 - 1 code units; a result longer than the
// implementation's maximum throws a RangeError before it is allocated. The
// spec's own RangeErrors and empty results are unchanged.
#[test]
fn strings_longer_than_the_maximum_throw_range_errors() {
    let _heap = heap_filling();
    let source = r#"
function attempt(f) { try { var r = f(); return 'ok:' + (typeof r === 'string' ? r.length : r); } catch (e) { return e instanceof RangeError ? 'RangeError' : 'other:' + e; } }
var results = [
  attempt(function () { return 'x'.repeat(2 ** 30); }),
  attempt(function () { return 'ab'.repeat(MAX / 2 + 1); }),
  attempt(function () { return ''.repeat(2 ** 40); }),
  attempt(function () { return 'x'.repeat(-1); }),
  attempt(function () { return 'abc'.padEnd(2 ** 31); }),
  attempt(function () { return 'abc'.padEnd(MAX + 1, 'xy'); }),
  attempt(function () { return 'abc'.padStart(2 ** 53 - 1, ''); }),
  attempt(function () { return 'abc'.padStart(10, 'xy'); }),
  attempt(function () { return new Array(2 ** 32 - 1).join(); }),
  attempt(function () { return [1, 2, 3].join('-'); }),
];
var big = 'y'.repeat(2 ** 26);
results.push(attempt(function () { return big + big + 'z'; }));
results.push(attempt(function () { return `${big}${big}z`; }));
results.push(attempt(function () { return big.concat(big, 'z'); }));
results.push(attempt(function () { return [big, big].join(''); }));
var observed = [];
results.push(attempt(function () {
  return 'a'.concat(big, big, { toString: function () { observed.push('late'); return ''; } });
}), observed.length);
big = null;
results.push(attempt(function () {
  var s = '';
  for (var i = 0; i < 64; i++) s += s + 'ab';
  return s;
}));
results.join();
"#
    .replace("MAX", &MAX_STRING_LENGTH.to_string());
    assert_completion(
        &source,
        "RangeError,RangeError,ok:0,RangeError,RangeError,RangeError,ok:3,ok:10,RangeError,ok:5,RangeError,RangeError,RangeError,RangeError,RangeError,0,RangeError",
    );
}

// The limit is the longest result a concatenation of distinct operands can
// build in a fresh product heap: one unit more is a RangeError, thrown before
// anything is allocated, and one of exactly that length is built. (Building
// one beside other near-limit garbage can still exhaust the heap; see the
// String limit's documentation.)
#[test]
fn a_string_of_the_maximum_length_is_built_and_one_unit_more_is_a_range_error() {
    let _heap = heap_filling();
    let source = r#"
var x = 'y'.repeat(MAX / 2), y = 'z'.repeat(MAX / 2), results = [];
function rangeError(f) { try { f(); return 'no error'; } catch (e) { return e instanceof RangeError; } }
results.push(rangeError(function () { return x + y + 'z'; }));
results.push(rangeError(function () { return `${x}${y}z`; }));
results.push(rangeError(function () { return x.padEnd(MAX + 1); }));
var whole = x.concat(y);
results.push(whole.length === MAX, whole.charCodeAt(MAX / 2 - 1), whole.charCodeAt(MAX - 1));
results.join();
"#
    .replace("MAX", &MAX_STRING_LENGTH.to_string());
    assert_completion(&source, "true,true,true,true,121,122");
}

// Every bound level keeps its own name, one "bound " longer than its
// target's; at the nesting limit the chain is still callable (with all 4,096
// bound arguments), and one more bind is a RangeError.
#[test]
fn bind_chains_are_bounded_by_a_catchable_range_error() {
    assert_completion(
        r#"
var f = function g() { return 1; }, made = 0, error;
try { for (var i = 0; i < 100000; i++) { f = f.bind(null, i); made++; } } catch (e) { error = e; }
[made, error instanceof RangeError, f(), f.name.length, f.length].join();
"#,
        "4096,true,1,24577,0",
    );
}

// A callable Proxy forwards its target's name, so a Proxy between two binds
// counts as a level: interleaving them cannot restart the count while the
// names keep growing.
#[test]
fn bind_chains_through_proxies_are_bounded_too() {
    assert_completion(
        r#"
var f = function g() { return 1; }, made = 0, error;
try { for (var i = 0; i < 20000; i++) { f = new Proxy(f, {}).bind(null); made++; } } catch (e) { error = e; }
[made, error instanceof RangeError, f(), f.name.length].join();
"#,
        "2048,true,1,12289",
    );
}

// BigInt magnitudes are limited to MAX_STRING_LENGTH - 1 bits. A power whose
// size provably exceeds it throws before any squaring, also for small bases
// whose results the bound 2^((b - 1) e) once let through to hours of
// multiplication (3n ** 100000000n has 1.58e8 bits); 0, 1 and -1 stay exact
// for every exponent, and conversion to a String is exact in every radix.
#[test]
fn bigint_size_limit_and_chunked_conversion() {
    assert_completion(
        r#"
function attempt(f) { try { return String(f()); } catch (e) { return e instanceof RangeError ? 'RangeError' : 'other:' + e; } }
var s = (2n ** 100000n).toString(), t = (3n ** 5000n).toString(36), u = (-(7n ** 3000n)).toString(2);
[
  attempt(function () { return 2n ** (2n ** 31n); }),
  attempt(function () { return 3n ** (2n ** 27n); }),
  attempt(function () { return 3n ** 100000000n; }),
  attempt(function () { return (-3n) ** 100000001n; }),
  attempt(function () { return 5n ** 60000000n; }),
  attempt(function () { return 1n ** (2n ** 100n); }),
  attempt(function () { return (-1n) ** (2n ** 64n + 1n); }),
  attempt(function () { return 0n ** (2n ** 64n); }),
  attempt(function () { return 1n << (2n ** 27n); }),
  attempt(function () { return (1n << 100000n).toString(2).length; }),
  attempt(function () { return BigInt.asUintN(2 ** 40, -1n); }),
  attempt(function () { return BigInt.asUintN(2 ** 53 - 1, 5n); }),
  s.length, s.slice(0, 12), s.slice(-12),
  t.length, t.slice(0, 12), t.slice(-12),
  u.length, u.slice(0, 12), u.slice(-12),
  (10n ** 1000n).toString().length, (10n ** 18n - 1n).toString(), (2n ** 64n).toString(16),
  (12345678901234567890n).toString(7), (0n).toString(), (-5n).toString(2),
].join();
"#,
        "RangeError,RangeError,RangeError,RangeError,RangeError,1,-1,0,RangeError,100001,RangeError,5,30103,999002093014,389883109376,1533,mfi5rk0rse9w,a8l6vjx20529,8424,-10000101111,001101000001,1001,999999999999999999,10000000000000000,31050262160105055621201,0,-101",
    );
}
