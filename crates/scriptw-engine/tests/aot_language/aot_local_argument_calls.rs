use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, RealmBuilder, RunOptions,
};

fn assert_call_semantics(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
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
        .unwrap_or_else(|error| panic!("local argument call failed: {error}\n{source}"));
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        outcome.note.contains("boolean(true)"),
        "{}\n{source}",
        outcome.note
    );
}

#[test]
fn dynamic_closure_array_calls_share_mutable_captures_and_keep_actual_this() {
    assert_call_semantics(
        r#"
'use strict';
function prepare(seed) {
  var tick = seed;
  return [function(row) { tick++; return row.x + tick + this.length; },
          function(row) { tick += 3; return row.y + tick + this.length; }];
}
var steps = prepare(7), row = {x: 11, y: 17};
function frame(index, row) { var result = steps[index & 1](row); return result; }
var first = frame(0, row), second = frame(1, row), third = frame(0, row);
var other = prepare(100), saved = steps;
steps = other;
var fresh = frame(0, row);
steps = saved;
gc();
first === 21 && second === 30 && third === 25 && fresh === 114 && frame(0, row) === 26;
"#,
    );
}

#[test]
fn saved_callee_receiver_and_arguments_are_evaluated_once_before_dispatch() {
    assert_call_semantics(
        r#"
'use strict';
var log = [], marker = {value: 7};
var holder = {base: 13, get method() { log.push('callee'); return function(a,b) {
  log.push('body'); return this.base + a.value + b.value;
}; }};
function receiver() { log.push('receiver'); return holder; }
// Redefine through the descriptor: the callee was already read.
function second() { log.push('second'); Object.defineProperty(holder, 'method', {value: 99}); gc(); return marker; }
var result = receiver().method((log.push('first'), marker), second());
result === 27 && holder.method === 99 && log.join('|') === 'receiver|callee|first|second|body';
"#,
    );
}

#[test]
fn guard_misses_keep_bound_proxy_native_and_observed_argument_contracts() {
    assert_call_semantics(
        r#"
'use strict';
function local(a,b) { return this.offset + a + b; }
function observed(a,b) { return arguments.length + arguments[0] + b; }
var receiver = {offset: 7}, log = [];
var choices = [local.bind(receiver, 3), new Proxy(local, {apply(target, self, args) {
  log.push('proxy'); return Reflect.apply(target, receiver, args);
}}), Math.max, observed];
function frame(i,a,b) { var value = choices[i](a,b); return value; }
var bound = frame(0, 5, 99), proxy = frame(1, 3, 5), native = frame(2, 3, 5), args = frame(3, 3, 5);
var extra = local.call(receiver, 3, 5, 99), missing = local.call(receiver, 3);
bound === 15 && proxy === 15 && native === 5 && args === 10 && extra === 15 &&
Number.isNaN(missing) && log.join('|') === 'proxy';
"#,
    );
}

#[test]
fn direct_local_calls_restore_realms_and_propagate_throws_and_strict_caller_barriers() {
    assert_call_semantics(
        r#"
var marker = {}, log = [];
function observe() { return observe.caller === null; }
function invoke(callback, value) { 'use strict'; var before = callback(); return before && value.x; }
function throwing(value) { 'use strict'; throw value; }
function object(value) { 'use strict'; return {value}; }
var input = {get x() { log.push('getter'); gc(); return 7; }};
var called = invoke(observe, input), caught = false;
try { throwing(marker); } catch (error) { caught = error === marker; }
var result = object(marker);
called === 7 && caught && result.value === marker && Object.getPrototypeOf(result) === Object.prototype &&
log.join('|') === 'getter' && observe.caller === null;
"#,
    );
}

#[test]
fn cross_program_typed_entries_use_actual_code_and_defining_realm() {
    assert_call_semantics(
        r#"
function make(value) { 'use strict'; return {value}; }
var original = make, marker = {};
(0,eval)("function make(value) { 'use strict'; return [value]; }");
var replaced = make(marker), kept = original(marker);
var foreign = __scriptwCreateRealm().global;
foreign.eval("function make(value) { 'use strict'; return {value}; }");
var foreignMake = foreign.make;
var result = foreignMake(marker);
Array.isArray(replaced) && replaced[0] === marker && kept.value === marker &&
Object.getPrototypeOf(result) === foreign.Object.prototype && result.value === marker &&
Object.getPrototypeOf(original(marker)) === Object.prototype;
"#,
    );
}

#[test]
fn arity_padding_source_depth_and_tail_call_retirement_keep_ordinary_behavior() {
    assert_call_semantics(
        r#"
'use strict';
function pair(a,b) { return [a,b]; }
function twice(value) { var result = pair(value); return result; }
function recur(value) { var result = value ? recur(value - 1) : 7; return result; }
function tail(value) { return value ? tail(value - 1) : 11; }
var marker = {}, missing = twice(marker), zero = pair(), extra = pair(marker, 9, 99), caught = false;
try { recur(10000); } catch (error) { caught = error instanceof RangeError; }
gc();
missing[0] === marker && missing[1] === undefined && zero[0] === undefined && zero[1] === undefined &&
extra[0] === marker && extra[1] === 9 && caught && tail(10000) === 11;
"#,
    );
}

#[test]
fn typed_tail_calls_keep_proxy_fallback_and_finally_operand_order() {
    assert_call_semantics(
        r#"
'use strict';
var log = [], marker = {};
function target(value) { log.push('target'); return value; }
var indirect = target;
function frame(value) { return indirect(value); }
var first = frame(marker);
indirect = new Proxy(target, {apply(fn, self, args) {
  log.push('proxy'); return Reflect.apply(fn, self, args);
}});
var second = frame(marker);
function guarded(value) {
  try { return frame((log.push('argument'), value)); }
  finally { log.push('finally'); }
}
var third = guarded(marker);
first === marker && second === marker && third === marker &&
log.join('|') === 'target|proxy|target|argument|proxy|target|finally';
"#,
    );
}

#[test]
fn canonical_constructor_adapter_preserves_receiver_new_target_and_actual_arguments() {
    assert_call_semantics(
        r#"
function Maker(a,b) { 'use strict'; this.a=a; this.b=b; this.target=new.target; }
function Alternate() {}
class Derived extends Maker {}
var marker = {}, log = [], zero = new Maker();
var extra = new Maker(marker, 2, (log.push('extra'), 99));
var alternate = Reflect.construct(Maker, [marker], Alternate);
var derived = new Derived(marker, 3);
var foreign = __scriptwCreateRealm().global, foreignTarget = foreign.Function();
foreignTarget.prototype = null;
var foreignInstance = Reflect.construct(Maker, [marker, 4], foreignTarget);
zero.a === undefined && zero.b === undefined && zero.target === Maker &&
extra.a === marker && extra.b === 2 && extra.target === Maker &&
Object.getPrototypeOf(extra) === Maker.prototype && alternate.a === marker &&
alternate.b === undefined && alternate.target === Alternate &&
Object.getPrototypeOf(alternate) === Alternate.prototype &&
derived.a === marker && derived.b === 3 && derived.target === Derived &&
Object.getPrototypeOf(derived) === Derived.prototype &&
foreignInstance.a === marker && foreignInstance.b === 4 && foreignInstance.target === foreignTarget &&
Object.getPrototypeOf(foreignInstance) === foreign.Object.prototype && log.join('|') === 'extra';
"#,
    );
}

#[test]
fn guarded_known_source_calls_keep_the_actual_closure_context_after_replacement() {
    assert_call_semantics(
        r#"
'use strict';
function make(seed) { return function frame(row) { seed += row.x; return seed; }; }
var frame = make(7), original = frame;
function run(row) { var result = frame(row); return result; }
var row = {x: 3}, first = run(row);
frame = make(100);
var second = run(row), saved = original(row);
frame = new Proxy(original, {apply(target, self, args) { return Reflect.apply(target, self, args) + 1000; }});
var proxied = run(row);
first === 10 && second === 103 && saved === 13 && proxied === 1016;
"#,
    );
}

#[test]
fn module_held_reference_calls_keep_the_callee_read_before_argument_replacement() {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let source = r#"
var log = [];
function frame(row) { log.push('original'); if (this !== undefined) return -1; return row.x + 1; }
var original = frame;
function replacement(row) { log.push('replacement'); if (this !== undefined) return -1; return row.x + 100; }
function replace(row) { log.push('argument'); frame = replacement; gc(); return row; }
export function run(row) { var result = frame(replace(row)); return result; }
function tail(row) { return frame(replace(row)); }
var row = {x: 3}, first = run(row), second = run(row);
frame = original;
var tailed = tail(row);
frame = new Proxy(original, {apply(target, receiver, args) {
  log.push('proxy'); return Reflect.apply(target, receiver, args) + 1000;
}});
var proxied = run(row);
frame = 17;
var rejected = false;
try { run(row); } catch (error) { rejected = error instanceof TypeError; }
if (first !== 4 || second !== 103 || tailed !== 4 || proxied !== 1004 || !rejected || frame !== replacement ||
    log.join('|') !== 'argument|original|argument|replacement|argument|original|argument|proxy|original|argument') {
  throw new Error('held Reference changed while evaluating arguments');
}

print('held-reference:ok');
"#;
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_module(
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
        .unwrap_or_else(|error| panic!("Module held Reference failed: {error}"));
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(
            observed.completion,
            scriptw_engine::ObservedCompletion::Normal(_)
        ),
        "{:?}: {}",
        observed.completion,
        observed.note
    );
    assert_eq!(
        observed.output_events,
        vec![scriptw_engine::HostOutputEvent::PrintLine(
            "held-reference:ok".into()
        )]
    );
}

#[test]
fn five_argument_entries_preserve_mixed_values_padding_and_observer_fallbacks() {
    assert_call_semantics(
        r#"
'use strict';
function collect(a,b,c,d,e) { return [this,a,b,c,d,e]; }
var token = {}, symbol = Symbol('local'), receiver = {collect};
var full = receiver.collect(token, undefined, symbol, 19n, 'last');
var short = collect(token, 2, 3), empty = collect(), effects = 0;
var extra = collect(token, 2, 3, 4, 5, (effects++, 6));
var spread = collect(...[token, 2, 3, 4, 5]);
function observed(a,b,c,d,e) { return [arguments.length, a, e, arguments[5]]; }
var seen = [], proxy = new Proxy(observed, {apply(target, self, args) {
  seen.push(self, args.length); return Reflect.apply(target, self, args);
}});
var choices = [collect, proxy, Math.max, observed.bind(undefined, token)];
function invoke(index,a,b,c,d,e) { var value = choices[index](a,b,c,d,e); return value; }
var direct = invoke(0, token, symbol, 3, 4, 5);
var proxied = invoke(1, token, 2, 3, 4, 5);
var native = invoke(2, 1, 2, 3, 4, 5);
var bound = invoke(3, 1, 2, 3, 4, 5);
full[0] === receiver && full[1] === token && full[2] === undefined && full[3] === symbol &&
full[4] === 19n && full[5] === 'last' && short[0] === undefined && short[1] === token &&
short[3] === 3 && short[4] === undefined && short[5] === undefined && empty[5] === undefined &&
extra[5] === 5 && effects === 1 && spread[1] === token && spread[5] === 5 && direct[0] === choices && direct[1] === token && direct[2] === symbol &&
proxied[0] === 5 && proxied[1] === token && proxied[2] === 5 && seen[0] === choices && seen[1] === 5 &&
native === 5 && bound[0] === 6 && bound[1] === token && bound[2] === 4 && bound[3] === 5;
"#,
    );
}

#[test]
fn five_argument_calls_hold_callee_receiver_and_roots_across_all_operand_effects() {
    assert_call_semantics(
        r#"
'use strict';
var log = [], token = {}, calls = 0;
function original(a,b,c,d,e) { calls++; log.push('body'); return [this,a,b,c,d,e]; }
var holder = {get method() { log.push('callee'); return original; }};
function receiver() { log.push('receiver'); return holder; }
function argument(index) {
  log.push(index);
  if (index === 5) { Object.defineProperty(holder, 'method', {value: 17, configurable: true}); gc(); }
  return {index, token};
}
var result = receiver().method(argument(1), argument(2), argument(3), argument(4), argument(5));
var rooted = result[0] === holder;
for (var index = 1; index <= 5; index++) rooted = rooted && result[index].index === index && result[index].token === token;
var ordered = log.join('|') === 'receiver|callee|1|2|3|4|5|body';
log = [];
var reason, caught = false;
function fail() { log.push('throw'); throw reason; }
try { original(argument(1), argument(2), argument(3), argument(4), fail()); }
catch (error) { caught = error === undefined; }
var abruptOrder = log.join('|') === '1|2|3|4|throw';
log = [];
var rejected = false;
try { holder.method(argument(1), argument(2), argument(3), argument(4), argument(5)); }
catch (error) { rejected = error instanceof TypeError; }
rooted && ordered && caught && abruptOrder && rejected && calls === 1 && log.join('|') === '1|2|3|4|5';
"#,
    );
}

#[test]
fn five_argument_replacements_use_actual_closure_program_realm_and_tail_activation() {
    assert_call_semantics(
        r#"
function make(seed) { 'use strict'; return function frame(a,b,c,d,e) { seed += a; return {seed, b, c, d, e}; }; }
var frame = make(7), original = frame, token = {};
function run(a,b,c,d,e) { 'use strict'; var result = frame(a,b,c,d,e); return result; }
var first = run(3, token, 2, 4, 5);
frame = make(100);
var second = run(3, token, 2, 4, 5), saved = original(3, token, 2, 4, 5);
var foreign = __scriptwCreateRealm().global;
foreign.eval("function frame(a,b,c,d,e) { 'use strict'; return {a,b,c,d,e}; }");
frame = foreign.frame;
var foreignResult = run(3, token, 2, 4, 5);
function local(a,b,c,d,e) { 'use strict'; return {a,b,c,d,e}; }
var localResult = local(3, token, 2, 4, 5);
function observer() { return observer.caller === null; }
function barrier(a,b,c,d,e) { 'use strict'; return a() && b === token && c === 2 && d === 4 && e === 5; }
var strictBarrier = barrier(observer, token, 2, 4, 5);
function tail(n,a,b,c,d) { 'use strict'; if (n > 0) return tail(n-1,a,b,c,d); return [a,b,c,d]; }
function recurse(n,a,b,c,d) { 'use strict'; if (n > 0) { var value = recurse(n-1,a,b,c,d); return value; } return a; }
var depthChecked = false;
try { recurse(10000, token, 2, 4, 5); } catch (error) { depthChecked = error instanceof RangeError; }
var tailResult = tail(10000, token, 2, 4, 5);
first.seed === 10 && second.seed === 103 && saved.seed === 13 && foreignResult.b === token &&
Object.getPrototypeOf(foreignResult) === foreign.Object.prototype && Object.getPrototypeOf(localResult) === Object.prototype &&
strictBarrier && depthChecked && tailResult[0] === token && tailResult[1] === 2 && tailResult[2] === 4 && tailResult[3] === 5;
"#,
    );
}

#[test]
fn five_formal_constructor_adapters_keep_new_target_and_actual_vector_extent() {
    assert_call_semantics(
        r#"
function Maker(a,b,c,d,e) { 'use strict'; this.values=[a,b,c,d,e]; this.target=new.target; }
function Alternate() {}
class Derived extends Maker {}
var token = {}, effects = 0;
var missing = new Maker(token), extra = new Maker(token, 2, 3, 4, 5, (effects++, 6));
var alternate = Reflect.construct(Maker, [token,2,3,4,5], Alternate);
var derived = new Derived(token,2,3,4,5);
var Bound = Maker.bind(null, token, 2), bound = new Bound(3,4,5);
missing.values[0] === token && missing.values[4] === undefined && missing.target === Maker &&
extra.values[4] === 5 && effects === 1 && alternate.target === Alternate &&
Object.getPrototypeOf(alternate) === Alternate.prototype && alternate.values[4] === 5 &&
derived.target === Derived && Object.getPrototypeOf(derived) === Derived.prototype && derived.values[4] === 5 &&
bound.target === Maker && Object.getPrototypeOf(bound) === Maker.prototype && bound.values[0] === token && bound.values[4] === 5;
"#,
    );
}
