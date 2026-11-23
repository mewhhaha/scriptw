//! Guarded builtin routes in return position of strict code: the inline arm
//! returns its value normally, and every guard miss (a replaced method, a
//! non-Number operand) keeps the proper tail call with once-evaluated operands.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

const PRELUDE: &str = r#"
var log = [];
var strict = (function () { return this === undefined; })();
function check(name, actual, expected) {
  if (actual !== expected) throw new Error(name + ': ' + String(actual) + ' !== ' + String(expected));
}
"#;

/// The strict Script, the Module (strict by goal) and a sloppy Script whose
/// return-position calls continue rather than tail call.
fn assert_goals(body: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    for (goal, source) in [
        ("strict script", format!("'use strict';\n{PRELUDE}{body}")),
        ("module", format!("{PRELUDE}{body}")),
        ("sloppy script", format!("{PRELUDE}{body}")),
    ] {
        let engine = Engine::new(RealmBuilder::new().build());
        let options = CompileOptions {
            host_surface_policy: HostSurfacePolicy::Test262,
            ..CompileOptions::default()
        };
        let run = RunOptions {
            backend: ExecutionBackend::WasmAot,
            timeout_ms: Some(60_000),
            ..RunOptions::default()
        };
        let observed = if goal == "module" {
            engine.observe_module(&source, options, run)
        } else {
            engine.observe_script(&source, options, run)
        }
        .unwrap_or_else(|error| panic!("{goal}: {error}"));
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot, "{goal}");
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{goal}: {:?}",
            observed.completion
        );
        assert_eq!(
            observed.output_events,
            vec![HostOutputEvent::PrintLine("ok".into())],
            "{goal}"
        );
    }
}

#[test]
fn math_imul_in_return_position_keeps_replacement_and_to_number_order() {
    assert_goals(
        r#"
function f(a, b) { return Math.imul(a, b); }
check('small', f(3, 4), 12);
check('wrap', f(0xffffffff, 5), -5);
check('square', f(0x7fffffff, 0x7fffffff), 1);
check('modulo', f(4294967299, 2), 6);
check('truncate', f(2.9, -3.9), -6);
check('NaN', f(NaN, 3), 0);
check('Infinity', f(Infinity, 2), 0);
check('negative zero', Object.is(f(-0, 5), 0), true);
check('negative product', Object.is(f(-1, 0), 0), true);

// Non-Numbers take the native call: ToUint32(a) completes before ToUint32(b).
var a = { valueOf: function () { log.push('a'); return 6; } };
var b = { valueOf: function () { log.push('b'); return 7; } };
check('objects', f(a, b), 42);
check('order', log.join(), 'a,b');
check('strings', f('3', '5'), 15);
log = [];
try {
  f({ valueOf: function () { log.push('first'); throw new SyntaxError('first'); } },
    { valueOf: function () { log.push('second'); return 1; } });
  throw new Error('abrupt ToNumber was swallowed');
} catch (error) {
  check('abrupt kind', error instanceof SyntaxError, true);
}
check('abrupt order', log.join(), 'first');
try {
  f(1n, 2);
  throw new Error('BigInt operand accepted');
} catch (error) {
  check('BigInt', error instanceof TypeError, true);
}

// A replacement installed before the call is the callee actually called.
var original = Math.imul;
Math.imul = function (x, y) {
  log.push('patched ' + (this === Math) + ' ' + x + ' ' + y);
  return 'patched';
};
log = [];
check('patched result', f(2, 3), 'patched');
check('patched call', log.join(), 'patched true 2 3');
Math.imul = original.bind(null);
check('bound', f(6, 7), 42);
Math.imul = original;
check('restored', f(6, 7), 42);

// The miss arm is still a proper tail call in strict code.
if (strict) {
  Math.imul = function (n, total) {
    'use strict';
    return n === 0 ? total : f(n - 1, total + 1);
  };
  check('tail miss', f(100000, 0), 100000);
  Math.imul = original;
}
print('ok');
"#,
    );
}

#[test]
fn other_guarded_routes_in_return_position_keep_their_fallbacks() {
    assert_goals(
        r#"
function push(array, value) { return array.push(value); }
function join(array, separator) { return array.join(separator); }
function joinDefault(array) { return array.join(); }
function regexp() { return RegExp('a+', 'g'); }
function leaf() { return 17; }
function callLeaf() { return leaf(); }
function numeric(a, b) { return a * 2 + b; }
function callNumeric(a, b) { return numeric(a, b); }

var array = [1];
check('push', push(array, 2), 2);
check('push contents', array.join(), '1,2');
try {
  push(Object.freeze([1]), 3);
  throw new Error('frozen push accepted');
} catch (error) {
  check('frozen push', error instanceof TypeError, true);
}
check('join', join([1, 2, 3], '-'), '1-2-3');
check('join default', joinDefault([1, null, undefined, 'x']), '1,,,x');
check('join coerced separator', join([1, 2], 0), '102');
var first = regexp(), second = regexp();
check('regexp fresh', first !== second, true);
check('regexp shape', first.source + '/' + first.flags + '/' + first.lastIndex, 'a+/g/0');
check('leaf', callLeaf(), 17);
check('numeric leaf', callNumeric(3, 4), 10);
check('numeric leaf coercion', callNumeric('3', 4), 10);

var originalPush = Array.prototype.push;
Array.prototype.push = function (value) { return 'patched push ' + value; };
check('patched push', push([], 9), 'patched push 9');
Array.prototype.push = originalPush;
var originalJoin = Array.prototype.join;
Array.prototype.join = function (separator) { return 'patched join ' + separator; };
check('patched join', join([1], '+'), 'patched join +');
Array.prototype.join = originalJoin;
var OriginalRegExp = RegExp;
globalThis.RegExp = function (pattern, flags) { return 'patched ' + pattern + flags; };
check('patched regexp', regexp(), 'patched a+g');
globalThis.RegExp = OriginalRegExp;
leaf = function () { return 'replaced leaf'; };
check('replaced leaf', callLeaf(), 'replaced leaf');
numeric = function (a, b) { return 'replaced ' + a + b; };
check('replaced numeric leaf', callNumeric(1, 2), 'replaced 12');
print('ok');
"#,
    );
}
