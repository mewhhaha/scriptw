use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion,
    ObservedJsValue, ObservedNumber, RealmBuilder,
};

fn assert_result(source: &str, expected: ObservedJsValue) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let engine = Engine::new(RealmBuilder::new().build());
    let compilation = engine
        .profile_script_compilation(
            source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
        )
        .unwrap_or_else(|error| {
            panic!("ordinary call sequence compilation failed: {error}\n{source}")
        });
    // This precondition applies to the whole rich program, including ordinary
    // construction, spread boundaries and result checks. Keep its largest
    // body well below the measured giant-body/JIT case (over one MiB), while
    // allowing fixed code unrelated to the repeated calls. The AOT emission
    // controls separately enforce the tighter source-body and growth bounds.
    // Inspect the same validated artifact executed below; compile only once.
    assert!(
        compilation.footprint.largest_code_body_bytes < 128 * 1024,
        "large ordinary source body: {:?}\n{source}",
        compilation.footprint
    );
    eprintln!(
        "numeric call control P: {} bytes, {} code bytes, largest {} bytes",
        compilation.footprint.module_bytes,
        compilation.footprint.code_body_bytes,
        compilation.footprint.largest_code_body_bytes
    );
    let execution = engine
        .profile_wasm_execution(&compilation.artifact, Some(30_000), true)
        .unwrap_or_else(|error| {
            panic!("ordinary call sequence execution failed: {error}\n{source}")
        });
    assert_eq!(execution.outcome.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        execution.outcome.completion,
        ObservedCompletion::Normal(expected),
        "{}\n{source}",
        execution.outcome.note
    );
}

fn calls(name: &str, first: usize, last: usize) -> String {
    (first..=last)
        .map(|value| format!("{name}({value});\n"))
        .collect()
}

#[test]
fn scalar_number_regions_keep_coercion_order_and_abrupt_boundaries() {
    assert_result(
        r#"
var log = [], sentinel = {}, caught;
function value(name, number) {
  return {valueOf() { log.push(name); return number; }};
}
function calculate(a, b, c) { return ((+a + 1) * (+b - 2)) / (+c + 3); }
if (calculate(value('a', 4), value('b', 8), value('c', 2)) !== 6 ||
    log.join('|') !== 'a|b|c') throw 'scalar coercion order';
log = [];
try {
  calculate(value('left', 4), {valueOf() { log.push('throw'); throw sentinel; }}, value('skipped', 2));
} catch (error) { caught = error; }
if (caught !== sentinel || log.join('|') !== 'left|throw') throw 'scalar abrupt';
log = [];
var left = {valueOf() { log.push('old conversion'); return 1; }};
function right() {
  log.push('evaluate right');
  left.valueOf = function() { log.push('convert left'); return 8; };
  return {valueOf() { log.push('convert right'); return 3; }};
}
var answer = (left - right()) * 2;
answer === 10 && log.join('|') === 'evaluate right|convert left|convert right';
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn scalar_number_regions_keep_ieee_edges_and_bigint_dispatch() {
    assert_result(
        r#"
function arithmetic(a, b, c) { return (+a + +b) * -(+c); }
function difference(a, b, c) { return (+a + +b) - +c; }
function remainder(a, b) { return -(+a) % +b; }
function compare(a, b) { return ((+a + 1) * 2) < ((+b - 1) * 3); }
Object.is(arithmetic(0, -0, 1), -0) &&
Object.is(arithmetic(0, -0, -1), 0) &&
Number.isNaN(arithmetic(Infinity, 0, 0)) &&
difference(9007199254740992, 1, 9007199254740992) === 0 &&
Object.is(remainder(5, 5), -0) &&
compare(2, 4) && !compare(NaN, 4) && !compare(2, NaN) &&
((2n + 3n) * -(4n - 1n)) === -15n;
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn joined_length_entry_test_keeps_dirty_predicates_and_live_bounds() {
    assert_result(
        r#"
function dirty(a, steps) {
  'use strict';var total=0;
  for(var i=0;i++<steps;i++)total+=a.join(',').length;
  return [total,i];
}
function shrink(a, limit) {
  'use strict';var total=0;
  for(var i=0;i<limit;i++){total+=a.join(',').length;limit=0;}
  return [total,i,limit];
}
function joined(a, steps) {
  'use strict';var total=-0;
  for(var i=0;i<steps;i++)total+=a.join(',').length;
  return [total,i];
}
var zero=dirty(['x','y'],0), twice=dirty(['x','y'],4);
var reduced=shrink(['x','y'],4), nan=shrink(['x','y'],NaN);
var hits=0, coerced=joined(['x','y'],{valueOf(){hits++;return 2;}});
var events=0, receiver=[];
Object.defineProperty(receiver,'join',{get(){events++;throw 'unused getter';}});
var untouched=joined(receiver,-0), notANumber=joined(receiver,NaN);
zero[0]===0 && zero[1]===1 && twice[0]===6 && twice[1]===5 &&
reduced[0]===3 && reduced[1]===1 && reduced[2]===0 &&
nan[0]===0 && nan[1]===0 && Number.isNaN(nan[2]) &&
coerced[0]===6 && coerced[1]===2 && hits===3 && events===0 &&
Object.is(untouched[0],-0) && untouched[1]===0 && Object.is(notANumber[0],-0);
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn joined_length_regions_preserve_utf16_and_number_arithmetic() {
    assert_result(
        r#"
function joined(a, steps, seed) {
  'use strict';var total=seed;
  for(var i=0;i<steps;i++)total+=a.join('\ud800\u0000\udfff').length;
  return [total,i];
}
var strings=joined(['','\ud800','\udfff','😀'],5,0);
var lone=joined(['\ud800','\udfff'],3,0.25);
var empty=joined([],0,-0), zeros=joined([],3,-0);
var frozen=joined(Object.freeze(['a','bc']),4,0);
var sealed=joined(Object.seal(['a','bc']),4,0);
var derived=class extends Array {};
var subclass=joined(new derived('a','bc'),4,0);
var nan=joined(['x'],4,NaN), infinity=joined(['x'],4,Infinity);
strings[0]===65 && strings[1]===5 && lone[0]===15.25 && lone[1]===3 &&
Object.is(empty[0],-0) && empty[1]===0 && Object.is(zeros[0],0) && zeros[1]===3 &&
frozen[0]===24 && sealed[0]===24 && subclass[0]===24 &&
Number.isNaN(nan[0]) && nan[1]===4 && infinity[0]===Infinity && infinity[1]===4;
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn joined_length_region_misses_preserve_live_methods_and_read_order() {
    assert_result(
        r#"
function joined(a, steps) {
  'use strict';var total=0;
  for(var i=0;i<steps;i++)total+=a.join(',').length;
  return total;
}
var log=[], own=['a','b'];own.join=function(){log.push('own');return 'abcdef';};
if(joined(own,2)!==12 || log.join('|')!=='own|own')throw 'own method';
log=[];
var proxy=new Proxy(['a','b'],{get(target,key,receiver){log.push(key);return Reflect.get(target,key,receiver);}});
if(joined(proxy,2)!==6 || log.join('|')!=='join|length|0|1|join|length|0|1')throw 'Proxy order';
var original=Object.getOwnPropertyDescriptor(Array.prototype,'join'), events='';
Object.defineProperty(Array.prototype,'join',{configurable:true,get(){events+='get|';return function(){events+='call|';return 'abc';};}});
if(joined(['a'],0)!==0 || events!=='')throw 'zero trip getter';
var override=joined(['a'],2);
Object.defineProperty(Array.prototype,'join',original);
if(override!==6 || events!=='get|call|get|call|')throw 'prototype getter';
var caught=false;
delete Array.prototype.join;
try{joined(['a'],1);}catch(error){caught=error instanceof TypeError;}
Object.defineProperty(Array.prototype,'join',original);
if(!caught)throw 'deleted method';
var inherited=Object.create(Array.prototype), reads=0;
Object.defineProperty(inherited,'join',{get(){reads++;return function(){return 'xy';};}});
var custom=['a'];Object.setPrototypeOf(custom,inherited);
joined(custom,2)===4 && reads===2;
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn joined_length_preflight_keeps_late_coercions_holes_and_caps_observable() {
    assert_result(
        r#"
function joined(a, steps) {
  'use strict';var total=0;
  for(var i=0;i<steps;i++)total+=a.join(',').length;
  return total;
}
function separated(a, steps, separator) {
  'use strict';var total=0;
  for(var i=0;i<steps;i++)total+=a.join(separator).length;
  return total;
}
var events='', late=['a','b',{toString(){events+='convert|';late[3]='long';gc();return 'x';}},'d'];
if(joined(late,2)!==20 || events!=='convert|convert|')throw 'late conversion';
events='';var holey=['x',,'z'];
Object.defineProperty(Array.prototype,'1',{configurable:true,get(){events+='hole|';holey[2]='zz';return 'y';}});
var holes=joined(holey,2);
delete Array.prototype[1];
if(holes!==12 || events!=='hole|hole|')throw 'live hole';
events='';var separator={toString(){events+='separator|';return '|';}};
if(separated([],3,separator)!==0 || events!=='separator|separator|separator|')throw 'empty separator coercion';
if(joined([1,2],2)!==6 || joined([1n,2n],2)!==6)throw 'non String slots';
var caught=false;try{joined(['a',Symbol()],1);}catch(error){caught=error instanceof TypeError;}
if(!caught || joined(['a',Symbol()],0)!==0)throw 'Symbol conversion';
for(var size=256;size<=257;size++){
  var a=[];for(var j=0;j<size;j++)a.push('x');
  if(joined(a,0)!==0 || joined(a,1)!==size*2-1)throw 'preflight cap';
}
var large=[];for(var j=0;j<4096;j++)large.push('x');
joined(large,0)===0 && joined(large,1)===8191;
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn dense_number_chunks_preserve_sequential_writes_rounding_and_counter_sign() {
    assert_result(
        r#"
function chunked(a, start, seed) {
  'use strict';var sum=seed, last=seed;
  for(var i=start;i<a.length;i++){sum+=a[i];last=sum;sum+=i*a[i];}
  return [sum,i,last];
}
function reference(a, start, seed) {
  'use strict';var sum=seed, last=seed;
  function value(){return sum;}
  for(var i=start;i<a.length;i++){sum+=a[i];last=sum;sum+=i*a[i];}
  return [value(),i,last];
}
function reciprocal(a, start) {
  'use strict';var sum=0;
  for(var i=start;i<a.length;i++)sum+=(1/i)*a[i];
  return [sum,i];
}
var sizes=[0,1,2,3,4,5,6,7,8,9,15,16,17,65,129];
var pattern=[1e16,1,-1e16,-0,3,-2,0.5], seeds=[0,-0,NaN,Infinity,-Infinity,9007199254740991];
for(var n=0;n<sizes.length;n++){
  var a=[];for(var j=0;j<sizes[n];j++)a.push(pattern[j%pattern.length]);
  for(var start=0;start<6;start++)for(var k=0;k<seeds.length;k++){
    var initial=start;if(start===0)initial=-0;
    var actual=chunked(a,initial,seeds[k]), expected=reference(a,initial,seeds[k]);
    for(var p=0;p<3;p++)if(!Object.is(actual[p],expected[p]))throw 'sequential dense chunk';
  }
}
var signed=reciprocal([1,1,1,1,1],-0), positive=reciprocal([1,1,1,1,1],0);
var empty=[1];empty.length=0;
var untouched=chunked(empty,-0,-0);
signed[0]===-Infinity && signed[1]===5 && positive[0]===Infinity && positive[1]===5 &&
Object.is(untouched[0],-0) && Object.is(untouched[1],-0) && Object.is(untouched[2],-0);
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn scalar_regions_keep_private_shadows_separate_from_global_mirrors() {
    assert_result(
        r#"
var a = [90], i = 91, total = 92;
function weighted(a) {
  'use strict';
  var total = 0;
  for (var i = 0; i < a.length; i++) total += (i + 1) * a[i];
  return [total, i];
}
function block() {
  'use strict';
  { let a = [2, 4], total = 0;
    for (let i = 0; i < a.length; i++) total += (i + 1) * a[i];
    return total; }
}
function mapped(a) {
  var total = 0;
  for (var i = 0; i < a.length; i++) total += (i + 1) * a[i];
  return total === 14 && arguments[0] === a;
}
var result = weighted([1, 2, 3]);
if (result[0] !== 14 || result[1] !== 3 || block() !== 10 || !mapped([1, 2, 3])) throw 'private bindings';
if (a[0] !== 90 || i !== 91 || total !== 92) throw 'global mirror changed';
a = [1, 2, 3]; total = 0;
for (i = 0; i < a.length; i++) total += (i + 1) * a[i];
globalThis.total === 14 && globalThis.i === 3 && globalThis.a === a;
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn dense_number_array_regions_preserve_bits_counters_and_live_results() {
    assert_result(
        r#"
function weighted(a, start, total) {
  'use strict';
  for (var i = start; i < a.length; i++) total += (i + 1) * a[i];
  return [total, i];
}
var a = [], expected = 0;
for (var i = 0; i < 129; i++) { a.push(i + 0.5); expected += (i + 1) * (i + 0.5); }
gc();
var dense = weighted(a, 0, 0), zeros = weighted([-0, -0], -0, -0);
var empty = weighted([], -0, -0), offset = weighted([1, 2, 3], 1, 5);
var infinity = weighted([1], Infinity, 7);
var nanStart = weighted([1], NaN, 8), nanValue = weighted([NaN, 1], 0, 0);
var infinities = weighted([Infinity, -Infinity], 0, 0);
gc();
dense[0] === expected && dense[1] === 129 && a[0] === 0.5 && a[128] === 128.5 &&
Object.is(zeros[0], -0) && zeros[1] === 2 &&
Object.is(empty[0], -0) && Object.is(empty[1], -0) &&
offset[0] === 18 && offset[1] === 3 &&
infinity[0] === 7 && infinity[1] === Infinity &&
nanStart[0] === 8 && Number.isNaN(nanStart[1]) &&
Number.isNaN(nanValue[0]) && nanValue[1] === 2 &&
Number.isNaN(infinities[0]) && infinities[1] === 2;
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn dense_number_array_region_shape_and_index_misses_preserve_reads() {
    assert_result(
        r#"
function weighted(a, start, total) {
  'use strict';
  for (var i = start; i < a.length; i++) total += (i + 1) * a[i];
  return [total, i];
}
var fractional = weighted([1, 2, 3], 0.5, 0);
var negative = [1, 2, 3]; negative[-1] = 7;
var negativeStart = weighted(negative, -1, 0), strings = weighted(['2', 3], 0, 0);
var subclass = class extends Array {};
var derived = weighted(new subclass(1, 2, 3), 0, 0);
var sealed = weighted(Object.seal([1, 2, 3]), 0, 0);
var frozen = weighted(Object.freeze([1, 2, 3]), 0, 0);
var fixed = weighted(Object.preventExtensions([1, 2, 3]), 0, 0);
gc();
Number.isNaN(fractional[0]) && fractional[1] === 3.5 &&
negativeStart[0] === 14 && negativeStart[1] === 3 && strings[0] === 8 &&
derived[0] === 14 && sealed[0] === 14 && frozen[0] === 14 && fixed[0] === 14;
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn dense_number_array_region_misses_preserve_getters_proxies_and_abrupt_order() {
    assert_result(
        r#"
function weighted(a) {
  'use strict';
  var total = 0;
  for (var i = 0; i < a.length; i++) total += (i + 1) * a[i];
  return [total, i];
}
var events = [], holey = [2, , 6], proto = Object.create(Array.prototype);
Object.defineProperty(proto, '1', {get() { events.push('inherited'); holey.length = 2; gc(); return 4; }});
Object.setPrototypeOf(holey, proto);
var holes = weighted(holey);
if (holes[0] !== 10 || holes[1] !== 2 || events.join('|') !== 'inherited') throw 'live inherited hole';
events = [];
var accessor = [2, 4, 6];
Object.defineProperty(accessor, '0', {get() {
  events.push('own'); accessor[1] = {valueOf() { events.push('convert'); gc(); return 5; }};
  return 3;
}});
var own = weighted(accessor);
if (own[0] !== 31 || own[1] !== 3 || events.join('|') !== 'own|convert') throw 'live own accessor';
events = [];
var proxy = new Proxy([2, 4], {get(target, key, receiver) {
  events.push(key); gc(); return Reflect.get(target, key, receiver);
}});
var trapped = weighted(proxy);
if (trapped[0] !== 10 || trapped[1] !== 2 || events.join('|') !== 'length|0|length|1|length') throw 'Proxy get order';
events = [];
var reads = 0, object = {get length() { events.push('length'); return ++reads === 1 ? 3 : 1; },
  get 0() { events.push('zero'); return 2; }};
var ordinary = weighted(object);
if (ordinary[0] !== 2 || ordinary[1] !== 1 || events.join('|') !== 'length|zero|length') throw 'live generic length';
events = [];
var sentinel = {}, throwing = [2, 4, 6], caught;
Object.defineProperty(throwing, '1', {get() { events.push('throw'); gc(); throw sentinel; }});
try { weighted(throwing); } catch (error) { caught = error; }
caught === sentinel && events.join('|') === 'throw';
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn dense_number_array_regions_reject_effectful_bodies_and_expose_current_locals() {
    assert_result(
        r#"
function mutate(a) {
  'use strict';
  var total = 0;
  for (var i = 0; i < a.length; i++) { total += (i + 1) * a[i]; if (i === 0) a[1] = 9; }
  return total === 20 && i === 2;
}
function capture(a) {
  'use strict';
  var total = 0, seen = [];
  function observe() { seen.push(total); if (seen.length === 1) a.length = 1; }
  for (var i = 0; i < a.length; i++) { total += (i + 1) * a[i]; observe(); }
  return total === 2 && i === 1 && seen.join('|') === '2';
}
function shift(a) {
  'use strict';
  var total = 0;
  for (var i = 0; i < a.length; i++) { total += (i + 1) * a[i]; i++; }
  return total === 20 && i === 4;
}
function replace(a, b) {
  'use strict';
  var total = 0;
  for (var i = 0; i < a.length; i++) { total += (i + 1) * a[i]; a = b; }
  return total === 2 && i === 1;
}
mutate([2, 4]) && capture([2, 4, 6]) && shift([2, 4, 6]) && replace([2, 4, 6], [9]);
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn fixed_argument_snapshots_keep_gc_roots_identity_aliasing_and_abrupt_order() {
    assert_result(
        r#"
var first = {value: 1}, old = first, sentinel = {}, called = 0, caught, log = [];
function capture(a, b, c, d, e) {
  called++;
  gc();
  return [a, b, c, d, e, arguments];
}
var held = capture(first, (first = {value: 2}, gc(), first), -0, 2n, Symbol('key'));
if (held[0] !== old || held[0].value !== 1 || held[1] !== first ||
    held[1].value !== 2 || !Object.is(held[2], -0) || held[3] !== 2n ||
    typeof held[4] !== 'symbol' || held[5][0] !== old) throw 'snapshot values';
function later() { log.push('throw'); gc(); throw sentinel; }
try { capture((log.push('first'), {}), later(), (log.push('skipped'), {})); }
catch (error) { caught = error; }
if (caught !== sentinel || called !== 1 || log.join('|') !== 'first|throw') throw 'snapshot abrupt';
function mapped(a, b) { a = 9; arguments[1] = 8; return arguments[0] === 9 && b === 8; }
function fresh() { return arguments; }
function rest(a, ...values) { gc(); return a === old && values[0] === first && values[1] === 'tail'; }
mapped(1, 2) && rest(old, first, 'tail') && fresh() !== fresh() &&
fresh(old)[0] === old && fresh(first)[0] === first;
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn repeated_numeric_calls_keep_live_callee_captured_locals_and_hoisted_declarations() {
    let rows = calls("step", 1, 1024);
    assert_result(
        &format!(
            r#"
function run(seed) {{
  let total = seed, count = 0;
  const read = () => total;
  {rows}
  function step(value) {{
    count++; total += value;
    if (count === 17) step = replacement;
  }}
  function replacement(value) {{ count++; total += value * 2; }}
  return read() === 1049454 && count === 1024 && step === replacement;
}}
run(7);
"#
        ),
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn repeated_numeric_calls_observe_each_global_getter_and_stop_on_getter_throw() {
    let rows = calls("consumeNumeric", 1, 1024);
    assert_result(
        &format!(
            r#"
var marker = {{}}, lookups = 0, count = 0, total = 0, caught, finalized = 0, atFinally;
function consume(value) {{
  if (this !== globalThis) throw new Error('bare call receiver');
  count++; total += value; return value;
}}
Object.defineProperty(globalThis, 'consumeNumeric', {{
  configurable: true,
  get() {{ if (++lookups === 37) throw marker; return consume; }}
}});
try {{ try {{ {rows} }} finally {{ finalized++; atFinally = total; }} }}
catch (error) {{ caught = error; }}
caught === marker && lookups === 37 && count === 36 && total === 666
  && atFinally === 666 && finalized === 1;
"#
        ),
        ObservedJsValue::Boolean(true),
    );
    // A finite non-string eval makes this ordinary activation environment
    // visible. The with Reference must be resolved and read anew for every
    // row, supply the held object's receiver, and stop on a getter throw.
    assert_result(
        &format!(
            r#"
var holder, marker = {{}}, lookups = 0, count = 0, total = 0, caught, finalized = 0;
function consume(value) {{
  if (this !== holder) throw new Error('with call receiver');
  count++; total += value; return value;
}}
holder = {{ get consumeNumeric() {{ if (++lookups === 37) throw marker; return consume; }} }};
function drive() {{
  eval(0);
  try {{ with (holder) {{ {rows} }} }} finally {{ finalized++; }}
}}
try {{ drive(); }} catch (error) {{ caught = error; }}
caught === marker && lookups === 37 && count === 36 && total === 666 && finalized === 1;
"#
        ),
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn repeated_numeric_call_throw_preserves_catch_finally_and_captured_environment() {
    let rows = calls("step", 1, 1024);
    assert_result(
        &format!(
            r#"
var marker = {{}};
function run() {{
  let total = 0, count = 0, caught, finalized = 0, atFinally;
  const read = () => total;
  function step(value) {{
    count++;
    if (value === 53) {{ gc(); throw marker; }}
    total += value; return value;
  }}
  try {{ try {{ {rows} }} finally {{ finalized++; atFinally = read(); }} }}
  catch (error) {{ caught = error; }}
  return caught === marker && count === 53 && total === 1378
    && read() === 1378 && atFinally === 1378 && finalized === 1;
}}
run();
"#
        ),
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn repeated_numeric_calls_keep_argument_bits_freshness_gc_roots_and_source_caller() {
    let rows = (0..1024)
        .map(|index| match index % 4 {
            0 => "save();\n",
            1 => "save(0);\n",
            2 => "save(1e400, 2);\n",
            _ => "save(0.5, 9007199254740992, 7);\n",
        })
        .collect::<String>();
    assert_result(
        &format!(
            r#"
var records = [], valid = true, source;
function save() {{
  var index = records.length;
  valid = valid && save.caller === drive && drive.toString() === source;
  if (index === 1024) valid = valid && arguments.length === 1 && Object.is(arguments[0], -0);
  else {{
    if (index % 4 === 0) valid = valid && arguments.length === 0;
    if (index % 4 === 1) valid = valid && arguments.length === 1 && Object.is(arguments[0], 0);
    if (index % 4 === 2) valid = valid && arguments.length === 2 && arguments[0] === 1e400 && arguments[1] === 2;
    if (index % 4 === 3) valid = valid && arguments.length === 3 && arguments[0] === 0.5
      && arguments[1] === 9007199254740992 && arguments[2] === 7;
  }}
  records.push(arguments);
  if (index % 16 === 0) gc();
}}
function drive() {{
  {rows}
  // Unary minus remains an ordinary argument evaluation after the large run.
  save(-0);
}}
source = Function.prototype.toString.call(drive);
drive();
gc();
records[1][0] = 99;
valid && records.length === 1025 && records[1] !== records[5] && Object.is(records[5][0], 0)
  && records[3][1] === 9007199254740992 && records[1023][2] === 7 && Object.is(records[1024][0], -0)
  && save.caller === null && drive.toString() === source && source.indexOf('save(0.5') >= 0;
"#
        ),
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn numeric_call_runs_keep_proxy_bound_dispatch_and_effectful_argument_boundaries() {
    let before = calls("visit", 1, 512);
    let after = calls("visit", 513, 1024);
    assert_result(
        &format!(
            r#"
var receiver = {{}}, trace = [], proxyCalls = 0, vectors = [];
function old(value) {{
  if (this !== receiver) throw new Error('bound this');
  trace.push('o' + value); return value;
}}
function replacement(value) {{ trace.push('r' + value); return value; }}
var visit = new Proxy(old.bind(receiver), {{
  apply(target, thisArg, argumentsList) {{
    proxyCalls++; vectors.push(argumentsList);
    return Reflect.apply(target, thisArg, argumentsList);
  }}
}});
{before}
visit((visit = replacement, 99));
{after}
visit(...[1026]);
proxyCalls === 513 && trace.length === 1026 && vectors[0] !== vectors[1]
  && vectors[0][0] === 1 && vectors[512][0] === 99
  && trace[0] === 'o1' && trace[511] === 'o512' && trace[512] === 'o99'
  && trace[513] === 'r513' && trace[1024] === 'r1024' && trace[1025] === 'r1026';
"#
        ),
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn numeric_call_runs_keep_entry_and_prepared_realm_script_last_completion() {
    let rows = calls("finish", 1, 1024);
    let script = format!("function finish(value) {{ return value; }}\n{rows}");
    assert_result(
        &script,
        ObservedJsValue::Number(ObservedNumber::from_f64(1024.0)),
    );
    assert_result(
        &format!(
            r#"
var realm = __scriptwCreateRealm();
realm.evalScript({script:?}) === 1024 && realm.evalScript('finish.caller;') === null;
"#
        ),
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn exact_sum_accepts_safe_positive_and_negative_prefixes() {
    assert_result(
        r#"
function positive(seed) {
  'use strict'; var total=seed;
  for(var i=3;(i+0)<23;i++) total+=i*3+1;
  return [total,i];
}
function negative(seed) {
  'use strict'; var total=seed;
  for(var i=2;i<7;i++) total=total+(1-i*3);
  return [total,i];
}
var a=positive(0), b=positive(9007199254740221), c=positive(-770);
var d=negative(55), e=negative(0), f=negative(-9007199254740936);
a[0]===770 && a[1]===23 && b[0]===9007199254740991 && b[1]===23 &&
Object.is(c[0],0) && c[1]===23 && Object.is(d[0],0) && d[1]===7 &&
e[0]===-55 && e[1]===7 && f[0]===-9007199254740991 && f[1]===7;
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn exact_sum_preserves_seed_boundaries_and_rounding_misses() {
    assert_result(
        r#"
function positive(seed) {
  'use strict';var total=seed;
  for(var i=3;(i+0)<23;i++)total+=i*3+1;
  return [total,i];
}
function negative(seed) {
  'use strict';var total=seed;
  for(var i=2;i<7;i++)total=total+(1-i*3);
  return [total,i];
}
function crossing(seed) {
  'use strict';var total=seed;
  for(var i=0;i<5;i++)total+=i-2;
  return [total,i];
}
var low=positive(-9007199254740991), upper=negative(9007199254740991);
var outsidePositive=positive(9007199254740223), outsideNegative=negative(-9007199254740938);
var crossed=crossing(-0);
low[0]===-9007199254740221 && low[1]===23 && upper[0]===9007199254740936 && upper[1]===7 &&
outsidePositive[0]===9007199254740992 && outsidePositive[1]===23 &&
outsideNegative[0]===-9007199254740992 && outsideNegative[1]===7 &&
Object.is(crossed[0],0) && crossed[1]===5;
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn exact_sum_preserves_signed_zero_nonfinite_and_zero_trip_values() {
    assert_result(
        r#"
function three(seed) {
  'use strict'; var total=seed;
  for(var i=0;(i+0)<3;i++) total+=i;
  return [total,i];
}
function zeros(seed) {
  'use strict'; var total=seed;
  for(var i=0;i<3;i++) total+=-0;
  return total;
}
function empty(seed) {
  'use strict'; var total=seed;
  for(var i=3;i<3;i++) total+=i;
  return [total,i];
}
var rounded=three(9007199254740992), fractional=three(0.25);
var infinite=three(Infinity), nan=three(NaN), untouched=empty(-0);
rounded[0]===9007199254740994 && rounded[1]===3 &&
fractional[0]===3.25 && fractional[1]===3 &&
infinite[0]===Infinity && infinite[1]===3 && Number.isNaN(nan[0]) && nan[1]===3 &&
Object.is(zeros(-0),-0) && Object.is(zeros(0),0) &&
Object.is(untouched[0],-0) && untouched[1]===3;
"#,
        ObservedJsValue::Boolean(true),
    );
}

#[test]
fn exact_sum_retains_unsafe_intermediates_and_observable_coercions() {
    assert_result(
        r#"
function cancellation() {
  'use strict';var total=0;
  for(var i=0;(i+0)<3;i++) total+=(i+9007199254740991)-9007199254740991;
  return total;
}
function masked() {
  'use strict';var total=0;
  for(var i=0;i<3;i++) total+=((i+9007199254740991)-9007199254740991)*0;
  return total;
}
function coerced(seed) {
  'use strict';var total=seed;
  for(var i=0;(i+0)<3;i++) total+=i;
  return [total,i];
}
var hits=0, object=coerced({valueOf(){hits++;return 4;}}), string=coerced('v');
var symbolError=false,bigintError=false;
try{coerced(Symbol());}catch(e){symbolError=e instanceof TypeError;}
try{coerced(1n);}catch(e){bigintError=e instanceof TypeError;}
cancellation()===2 && Object.is(masked(),0) && object[0]===7 && object[1]===3 &&
hits===1 && string[0]==='v012' && string[1]===3 && symbolError && bigintError;
"#,
        ObservedJsValue::Boolean(true),
    );
}
