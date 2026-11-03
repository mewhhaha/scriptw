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
