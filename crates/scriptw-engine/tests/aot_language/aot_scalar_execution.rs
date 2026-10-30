//! Scalar source regions retain JavaScript values and abrupt evaluation order.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, ObservedCompletion, ObservedJsValue, RealmBuilder,
    RunOptions, WasmExecutionFailureKind,
};

fn assert_scalar_execution(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions::default(),
            RunOptions {
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("scalar execution control should complete normally");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        outcome.completion,
        ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
        "{source}"
    );
}

fn assert_scalar_execution_modes(source: &str) {
    for directive in ["", "\"use strict\";\n"] {
        assert_scalar_execution(&format!("{directive}{source}"));
    }
}

#[test]
fn scalar_chunks_keep_epoch_deadlines_between_bounded_groups() {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    for directive in ["", "\"use strict\";\n"] {
        for body in [
            "value=value*1.0000000000000002+index/7;",
            "value=(Math.imul(value,3)+1)>>>0;",
        ] {
            let source = format!("{directive}(function run(){{var value=1;for(var index=0;index<1000000000000;index++){{{body}}}return value;}})();");
            let error = Engine::new(RealmBuilder::new().build())
                .observe_script(
                    &source,
                    CompileOptions::default(),
                    RunOptions {
                        timeout_ms: Some(100),
                        ..RunOptions::default()
                    },
                )
                .expect_err("bounded groups must retain interruptible loop backedges");
            assert_eq!(
                error.wasm_execution_failure_kind(),
                Some(WasmExecutionFailureKind::Timeout),
                "{source}: {error}"
            );
        }
    }
}

#[test]
fn scalar_unrolled_iterations_preserve_rounding_tails_and_counter_boundaries() {
    assert_scalar_execution_modes(
        r#"
function up(value,start,stop) {
  var index=start;
  for(; (index+0)<stop; index++) value=(value+index*0.1)*1.0000000000000002-index/7;
  return [value,index];
}
function referenceUp(value,index,stop) {
  while((index+0)<stop) {value=(value+index*0.1)*1.0000000000000002-index/7;index++;}
  return [value,index];
}
function down(value,start,stop) {
  var index=start;
  for(; stop<index; index=index-1) value=(value+index*0.1)*1.0000000000000002-index/7;
  return [value,index];
}
function referenceDown(value,index,stop) {
  while(stop<index) {value=(value+index*0.1)*1.0000000000000002-index/7;index--;}
  return [value,index];
}
function inclusive(value,start,stop) {
  var index=start;
  for(; stop<=index; index-=1) value=(value+index*0.1)*1.0000000000000002-index/7;
  return [value,index];
}
function referenceInclusive(value,index,stop) {
  while(stop<=index) {value=(value+index*0.1)*1.0000000000000002-index/7;index--;}
  return [value,index];
}
function same(left,right) {return Object.is(left[0],right[0]) && Object.is(left[1],right[1]);}
var seeds=[0,-0,0.1,Number.MIN_VALUE,9007199254740992,1e308,NaN,Infinity,-Infinity];
var okay=true;
for(var seed=0;seed<seeds.length;seed++) {
  for(var count=0;count<19;count++) {
    if(!same(up(seeds[seed],0,count),referenceUp(seeds[seed],0,count))) okay=false;
    if(!same(down(seeds[seed],count,0),referenceDown(seeds[seed],count,0))) okay=false;
    if(!same(inclusive(seeds[seed],count,0),referenceInclusive(seeds[seed],count,0))) okay=false;
  }
}
var starts=[-0,0.5,-3.25,NaN,Infinity,9007199254740988,9007199254740991,9007199254740992];
var stops=[13.5,14.25,2,10,10,9007199254740991,9007199254740992,9007199254740992];
for(var caseIndex=0;caseIndex<starts.length;caseIndex++) {
  if(!same(up(-0,starts[caseIndex],stops[caseIndex]),referenceUp(-0,starts[caseIndex],stops[caseIndex]))) okay=false;
}
if(!same(down(1,-9007199254740991,-9007199254740992),referenceDown(1,-9007199254740991,-9007199254740992))) okay=false;
if(!same(up(-0,0,NaN),referenceUp(-0,0,NaN))) okay=false;
if(!same(up(-0,0,-Infinity),referenceUp(-0,0,-Infinity))) okay=false;
if(!same(up(-0,Infinity,Infinity),referenceUp(-0,Infinity,Infinity))) okay=false;
function zeroSign() {
  var total=0;
  for(var index=-0;index<13;index++) total+=1/index;
  return total===-Infinity && index===13;
}
okay && zeroSign();
"#,
    );
}

#[test]
fn scalar_affine_chunks_preserve_unsigned_signed_and_observable_intermediates() {
    assert_scalar_execution_modes(
        r#"
function unsigned(value,count) {
  for(var index=0;index<count;index++) value=(Math.imul(value,3)+1)>>>0;
  return [value,index];
}
function referenceUnsigned(value,count) {
  var index=0;
  while(index<count) {value=(Math.imul(value,3)+1)>>>0;index++;}
  return [value,index];
}
function signed(value,count) {
  for(var index=count;index>0;index--) value=(Math.imul(value,5)-7)|0;
  return [value,index];
}
function referenceSigned(value,count) {
  var index=count;
  while(index>0) {value=(Math.imul(value,5)-7)|0;index--;}
  return [value,index];
}
function additive(value,count) {
  for(var index=0;index<count;index+=1) value=((value>>>0)+4294967295)>>>0;
  return [value,index];
}
function referenceAdditive(value,count) {
  var index=0;
  while(index<count) {value=((value>>>0)+4294967295)>>>0;index++;}
  return [value,index];
}
function same(left,right) {return Object.is(left[0],right[0]) && Object.is(left[1],right[1]);}
var seeds=[-0,0,1,-1,2147483647,2147483648,4294967295,4294967297,7.75,NaN,Infinity,-Infinity,9007199254740991,9007199254740992];
var okay=true;
for(var seed=0;seed<seeds.length;seed++) {
  for(var count=0;count<27;count++) {
    if(!same(unsigned(seeds[seed],count),referenceUnsigned(seeds[seed],count))) okay=false;
    if(!same(signed(seeds[seed],count),referenceSigned(seeds[seed],count))) okay=false;
    if(!same(additive(seeds[seed],count),referenceAdditive(seeds[seed],count))) okay=false;
  }
}
function nested(value,count) {
  var full=0;
  for(var index=0;index<count;index++) value=(full=Math.imul(value,3)+4294967295)>>>0;
  return [value,full];
}
function referenceNested(value,count) {
  var full=0,index=0;
  while(index<count) {value=(full=Math.imul(value,3)+4294967295)>>>0;index++;}
  return [value,full];
}
function nonlinear(value,count) {
  for(var index=0;index<count;index++) value=(Math.imul(value,value)+index)>>>0;
  return value;
}
function referenceNonlinear(value,count) {
  var index=0;
  while(index<count) {value=(Math.imul(value,value)+index)>>>0;index++;}
  return value;
}
var nestedActual=nested(2147483647,19),nestedExpected=referenceNested(2147483647,19);
okay && same(nestedActual,nestedExpected) && nonlinear(123456789,19)===referenceNonlinear(123456789,19);
"#,
    );
}

#[test]
fn scalar_chunks_keep_live_imul_fallback_and_written_loop_control() {
    assert_scalar_execution_modes(
        r#"
function run(value,count) {
  for(var index=0;index<count;index++) value=(Math.imul(value,3)+1)>>>0;
  return value;
}
var original=Math.imul,calls=0;
Math.imul=function(left,right){calls++;return left+right;};
var replaced=run(1,19)===77 && calls===19;
var descriptor=Object.getOwnPropertyDescriptor(Math,'imul'),gets=0;
Object.defineProperty(Math,'imul',{configurable:true,get(){gets++;return original;}});
var expected=1,index=0;
while(index<19){expected=(original(expected,3)+1)>>>0;index++;}
var accessor=run(1,19)===expected && gets===19;
Object.defineProperty(Math,'imul',descriptor);
Math.imul=original;
function changingLimit(value,count) {
  for(var index=0;index<count;index++){count--;value=(Math.imul(value,3)+1)>>>0;}
  return [value,index,count];
}
function changingCounter(value,count) {
  for(var index=0;index<count;index++){index++;value=(Math.imul(value,3)+1)>>>0;}
  return [value,index];
}
var limit=changingLimit(1,19),counter=changingCounter(1,19),reference=1,step=0;
while(step<10){reference=(Math.imul(reference,3)+1)>>>0;step++;}
replaced && accessor && limit[0]===reference && limit[1]===10 && limit[2]===9 &&
counter[0]===reference && counter[1]===20;
"#,
    );
}

#[test]
fn scalar_number_locals_and_predicates_preserve_ieee_results() {
    assert_scalar_execution(
        r#"
function check(input) {
  var value=+input;
  var doubled=value+value;
  var negated=-value;
  var below=value<0;
  if (value<0) { if (!below) throw 'boolean'; }
  if (value<=0) { if (value>0) throw 'ordering'; }
  if (value>=0) { if (value<0) throw 'ordering'; }
  return [doubled, negated, below];
}
var negativeZero=check(-0);
var notNumber=check(NaN);
var positive=check(Infinity);
var negative=check(-Infinity);
Object.is(negativeZero[0],-0) && Object.is(negativeZero[1],0) &&
negativeZero[2]===false && Number.isNaN(notNumber[0]) &&
Number.isNaN(notNumber[1]) && notNumber[2]===false &&
positive[0]===Infinity && positive[1]===-Infinity &&
negative[0]===-Infinity && negative[2]===true;
"#,
    );
}

#[test]
fn scalar_comparison_snapshots_left_before_right_mutation_and_coercion() {
    assert_scalar_execution(
        r#"
function mutateRight() {
  var value=2;
  var selected=false;
  if (value<(value=100,3)) selected=true;
  return selected && value===100;
}
var order='';
var marker={};
function left() { order+='L'; return {valueOf(){order+='l';return 2;}}; }
function right() { order+='R'; return {valueOf(){order+='r';return 3;}}; }
var selected=false;
if (+left()<+right()) selected=true;
var converted=order==='LlRr';
order='';
function abrupt() { order+='A'; throw marker; }
var exact=false;
try { if (+abrupt()<+right()) throw 'unreachable'; } catch(error) { exact=error===marker; }
mutateRight() && selected && converted && exact && order==='A';
"#,
    );
}

#[test]
fn scalar_local_initialization_proof_preserves_lexical_and_capture_lifetimes() {
    assert_scalar_execution(
        r#"
function hoisted() { var result=value===undefined; var value=42; return result && value===42; }
function defaultTdz(left=right,right=1) { return left+right; }
var defaultError=false;
try { defaultTdz(); } catch(error) { defaultError=error instanceof ReferenceError; }
var lexicalError=false;
try { let value=value+1; } catch(error) { lexicalError=error instanceof ReferenceError; }
var captureError=false;
try { const read=()=>value; read(); let value=3; } catch(error) { captureError=error instanceof ReferenceError; }
function argumentsCapture(value=()=>arguments[1]) { return value(); }
var readers=[];
for (let index=0;index<3;index++) readers.push(()=>index);
function changingCapture() {
  var value=1;
  function replace() { value='changed'; }
  replace();
  return value+1==='changed1';
}
hoisted() && defaultError && lexicalError && captureError &&
argumentsCapture(undefined,42)===42 && readers[0]()===0 &&
readers[1]()===1 && readers[2]()===2 && changingCapture();
"#,
    );
}

#[test]
fn scalar_number_optimization_retains_bigint_and_dynamic_comparison_fallbacks() {
    assert_scalar_execution(
        r#"
function compare(left,right) { if (left<right) return 1; if (left>=right) return 2; return 3; }
function add(left,right) { return left+right; }
var order='';
var left={valueOf(){order+='L';return 2n;}};
var right={valueOf(){order+='R';return 3n;}};
var coercive=compare(left,right);
var mismatch=false;
try { add(1n,2); } catch(error) { mismatch=error instanceof TypeError; }
compare(1n,2n)===1 && compare(2n,2)===2 && compare(NaN,2)===3 &&
coercive===1 && order==='LR' && add(1n,2n)===3n && mismatch;
"#,
    );
}

#[test]
fn scalar_predicates_keep_derived_this_initialization_errors() {
    assert_scalar_execution(
        r#"
class Derived extends Object {
  constructor() {
    var seen=false;
    try { if (+this<0) throw 'unreachable'; } catch(error) { seen=error instanceof ReferenceError; }
    super();
    if (!seen) throw 'missing this initialization check';
  }
}
new Derived() instanceof Derived;
"#,
    );
}

#[test]
fn scalar_comparison_retains_live_property_and_call_result_tags() {
    assert_scalar_execution(
        r#"
var order='';
const marker={};
const primitive={valueOf(){order+='v';return 2;}};
const holder={value:1};
Object.defineProperty(holder,'value',{get(){order+='g';return primitive;}});
function right(){order+='R';return 3;}
var selected=false;
if(holder.value<right()) selected=true;
var direct=selected && order==='gRv';
order='';
const alias=holder.value;
var aliased=false;
if(alias<right()) aliased=true;
var retained=aliased && order==='gRv';
order='';
const callable={read(){return 1;}};
function invoke(){order+='c';return callable.read();}
callable.read=()=>primitive;
var called=false;
if(invoke()<right()) called=true;
var returned=called && order==='cRv';
order='';
const abrupt={valueOf(){order+='a';throw marker;}};
Object.defineProperty(holder,'value',{get(){order+='g';return abrupt;}});
var exact=false;
try { if(holder.value<right()) throw 'unreachable'; } catch(error) { exact=error===marker; }
direct && retained && returned && exact && order==='gRa';
"#,
    );
}

#[test]
fn scalar_number_loops_preserve_zero_iterations_ieee_values_and_operand_snapshots() {
    assert_scalar_execution(
        r#"
function recurrence(value, count) {
  "use strict";
  for (var index=0;(index+0)<count;index++) value=value*1+index/2;
  return [value,index];
}
function ordered() {
  var value=2,total=0;
  for(var index=0;index<2;index++) total+=value*(value=value+1);
  return total===18 && value===4 && index===2;
}
function predicateWrites() {
  var index=0,total=0;
  for(;(index=index+1)<3;total=total+10) total+=index*2;
  return index===3 && total===26;
}
var zero=recurrence(-0,0);
var one=recurrence(-0,1);
var several=recurrence(0.25,5);
var nan=recurrence(NaN,2);
var infinite=recurrence(Infinity,2);
var negative=recurrence(-Infinity,2);
var falseNan=recurrence(7,NaN);
Object.is(zero[0],-0) && zero[1]===0 && Object.is(one[0],0) &&
several[0]===5.25 && several[1]===5 && Number.isNaN(nan[0]) &&
infinite[0]===Infinity && negative[0]===-Infinity && falseNan[0]===7 &&
falseNan[1]===0 && ordered() && predicateWrites();
"#,
    );
}

#[test]
fn scalar_number_loops_keep_exact_number_bitwise_domains() {
    assert_scalar_execution(
        r#"
function advance(value) {
  "use strict";
  for(var index=0;index<2;index++) value=((value>>>0)+4294967295)>>>0;
  return value;
}
function bitwise(value) {
  "use strict";
  var left=0,right=0,bits=0;
  for(var index=0;index<1;index++) {
    left=value<<33;
    right=value>>33;
    bits=~value;
  }
  return [left,right,bits];
}
function precise(value) {
  "use strict";
  for(var index=0;index<2;index++) value=value+1;
  return value;
}
function mixed(value) {
  "use strict";
  for(var index=0;index<2;index++){value=value>>>0;value=value+0.5;}
  return value;
}
function boundaries() {
  var top=0,negative=0,rounded=0,wrap=0,fraction=0;
  for(var index=0;index<1;index++) {
    top=(9007199254740991+0)>>>0;
    negative=(0-9007199254740991)>>>0;
    rounded=(9007199254740991+2)>>>0;
    wrap=(4294967295+4294967295)>>>0;
    fraction=(4294967295+0.75)>>>0;
  }
  return top===4294967295 && negative===1 && rounded===0 && wrap===4294967294 && fraction===4294967295;
}
var negative=bitwise(-2147483649);
var fraction=bitwise(7.9);
var huge=bitwise(9223372036854777856);
advance(4294967297)===4294967295 && advance(-1)===4294967293 &&
advance(Infinity)===4294967294 && advance(NaN)===4294967294 &&
negative[0]===-2 && negative[1]===1073741823 && negative[2]===-2147483648 &&
fraction[0]===14 && fraction[1]===3 && fraction[2]===-8 &&
huge[0]===4096 && huge[1]===1024 && huge[2]===-2049 &&
precise(9007199254740992)===9007199254740992 && mixed(1.25)===1.5 && boundaries();
"#,
    );
}

#[test]
fn scalar_number_loop_guards_preserve_dynamic_coercion_and_abrupt_fallbacks() {
    assert_scalar_execution(
        r#"
var events='';
var marker={};
function run(value,count) {
  "use strict";
  for(var index=0;(index+0)<count;index++) value+=index*2+0.5;
  return [value,index];
}
var text=run('x',2);
var box=run({valueOf(){events+='v';return 4;}},2);
var count={valueOf(){events+='c';return 2;}};
var dynamic=run(1,count);
var threw=false;
try {run({valueOf(){events+='a';throw marker;}},2);} catch(error){threw=error===marker;}
var bigint=false;
try {run(1n,1);} catch(error){bigint=error instanceof TypeError;}
function tdz() {for(var index=0;index<1;index++) value=value+1;let value=0;}
var beforeInit=false;
try {tdz();} catch(error){beforeInit=error instanceof ReferenceError;}
function immutable() {const value=0;for(var index=0;index<1;index++) value=value*2+1;}
var constant=false;
try {immutable();} catch(error){constant=error instanceof TypeError;}
text[0]==='x0.52.5' && box[0]===7 && dynamic[0]===4 &&
events==='vccca' && threw && bigint && beforeInit && constant;
"#,
    );
}

#[test]
fn scalar_number_loop_fallback_keeps_captures_lifetimes_reentry_and_completion() {
    assert_scalar_execution(
        r#"
function captured() {
  var value=1;
  function read(){return value;}
  for(var index=0;index<3;index++) value+=index*2;
  return read()===7;
}

function callbacks() {
  var value=0,observed='';
  function read(){observed+=value;}
  for(var index=0;index<3;index++){value+=index*2;read();}
  return value===6 && observed==='026';
}
function lexical() {
  var readers=[];
  for(let index=0;index<3;index++) readers.push(()=>index);
  return readers[0]()===0 && readers[1]()===1 && readers[2]()===2;
}
function mapped(value) {
  for(var index=0;index<2;index++) value+=index*2;
  return value===3 && arguments[0]===3;
}
var completion=eval('var last=0; for(var n=0;n<3;n++) {last=n*2; last;}');
var empty=eval('42; for(var n=0;n<0;n++) n;');
captured() && callbacks() && lexical() && mapped(1) && completion===4 && empty===undefined;
"#,
    );
}

#[test]
fn scalar_imul_loops_keep_live_builtin_data_and_observable_lookup_fallbacks() {
    assert_scalar_execution(
        r#"
function run(value,count) {
  "use strict";
  for(var index=0;index<count;index++) value=(Math.imul(value,3)+1)>>>0;
  return value;
}
var original=Math.imul;
var ordinary=run(1,4)===121 && run(-1,2)===4294967291 && run(NaN,1)===1;
var zero=run(-0,0);
var calls=0;
Math.imul=function(left,right){calls++;return left+right;};
var patched=run(1,3)===13 && calls===3;
Math.imul=original;
var descriptor=Object.getOwnPropertyDescriptor(Math,'imul');
var gets=0;
Object.defineProperty(Math,'imul',{configurable:true,get(){gets++;return original;}});
var accessor=run(1,3)===40 && gets===3;
var noGet=run(9,0)===9 && gets===3;
Object.defineProperty(Math,'imul',descriptor);
var actualMath=Math;
var prototype=Object.getPrototypeOf(actualMath);
var inherited=Object.create(prototype);
var inheritedGets=0;
Object.defineProperty(inherited,'imul',{get(){inheritedGets++;return original;}});
delete actualMath.imul;
Object.setPrototypeOf(actualMath,inherited);
var inheritedMethod=run(1,3)===40 && inheritedGets===3;
Object.setPrototypeOf(actualMath,prototype);
Object.defineProperty(actualMath,'imul',descriptor);
var traps=0;
globalThis.Math=new Proxy(actualMath,{get(target,key){traps++;return target[key];}});
var proxy=run(1,3)===40 && traps===3;
globalThis.Math=actualMath;
var globalDescriptor=Object.getOwnPropertyDescriptor(globalThis,'Math');
var globalGets=0;
Object.defineProperty(globalThis,'Math',{configurable:true,get(){globalGets++;return actualMath;}});
var globalAccessor=run(1,2)===13 && globalGets===2;
var globalZero=run(9,0)===9 && globalGets===2;
Object.defineProperty(globalThis,'Math',globalDescriptor);
ordinary && Object.is(zero,-0) && patched && accessor && noGet && inheritedMethod &&
proxy && globalAccessor && globalZero;
"#,
    );
}

#[test]
fn scalar_imul_loop_fallback_preserves_shadowing_eval_and_argument_effects() {
    assert_scalar_execution(
        r#"
var original=Math.imul;
function shadowed(Math) {
  var value=1;
  for(var index=0;index<3;index++) value=(Math.imul(value,3)+1)>>>0;
  return value;
}
var calls=0;
var local=shadowed({imul(left,right){calls++;return left+right;}})===13 && calls===3;
function evalShadow() {
  eval('var Math={imul:function(left,right){return left+right;}};');
  var value=1;
  for(var index=0;index<3;index++) value=(Math.imul(value,3)+1)>>>0;
  return value===13;
}
var evaluated=evalShadow();
var events='';
function effectful() {
  var value=1;
  for(var index=0;index<2;index++) value=Math.imul((events+='L',value),(events+='R',3));
  return value===9 && events==='LRLR';
}
function boxed(value) {
  for(var index=0;index<2;index++) value=Math.imul(value,3);
  return value;
}
var coerced=boxed({valueOf(){events+='V';return 2;}})===18 && events==='V';
events='';
var marker={};
var abrupt=false;
try {boxed({valueOf(){throw marker;}});} catch(error){abrupt=error===marker;}
var immutable=false;
try {(function(){const value=1;for(var index=0;index<1;index++) value=Math.imul(value,3);})();}
catch(error){immutable=error instanceof TypeError;}
local && evaluated && coerced && abrupt && immutable && effectful() && Math.imul===original;
"#,
    );
}

#[test]
fn scalar_integer_loop_entry_preserves_zero_sign_and_unbounded_number_seeds() {
    assert_scalar_execution(
        r#"
function unsigned(value,count) {
  "use strict";
  var original=0,reciprocal=0;
  for(var index=0;index<count;index++) {
    original=value;
    reciprocal=1/value;
    value=value>>>0;
  }
  return [value,original,reciprocal];
}
function signed(value,count) {
  "use strict";
  var original=0;
  for(var index=0;index<count;index++) {original=value;value=value|0;}
  return [value,original];
}
function literal() {
  var value=1;
  for(var index=0;index<1;index++) value=-0;
  return value;
}
var zero=unsigned(-0,0);
var negativeZero=unsigned(-0,1);
var negative=signed(-2147483648,1);
var positive=signed(2147483647,1);
var signWrap=signed(2147483648,1);
var unsignedTop=unsigned(4294967295,1);
var overflow=unsigned(4294967297,1);
var fraction=unsigned(7.75,1);
var nan=unsigned(NaN,1);
var infinite=unsigned(Infinity,1);
var negativeInfinite=unsigned(-Infinity,1);
var safeTop=unsigned(9007199254740991,1);
var rounded=unsigned(9007199254740992,1);
var roundedUp=unsigned(9007199254740994,1);
Object.is(zero[0],-0) && Object.is(negativeZero[0],0) &&
Object.is(negativeZero[1],-0) && negativeZero[2]===-Infinity &&
negative[0]===-2147483648 && negative[1]===-2147483648 &&
positive[0]===2147483647 && signWrap[0]===-2147483648 && signWrap[1]===2147483648 &&
unsignedTop[0]===4294967295 && unsignedTop[1]===4294967295 &&
overflow[0]===1 && overflow[1]===4294967297 && fraction[0]===7 && fraction[1]===7.75 &&
nan[0]===0 && Number.isNaN(nan[1]) && infinite[0]===0 && infinite[1]===Infinity &&
negativeInfinite[0]===0 && negativeInfinite[1]===-Infinity &&
safeTop[0]===4294967295 && safeTop[1]===9007199254740991 &&
rounded[0]===0 && rounded[1]===9007199254740992 &&
roundedUp[0]===2 && roundedUp[1]===9007199254740994 && Object.is(literal(),-0);
"#,
    );
}

#[test]
fn scalar_integer_residues_keep_nested_full_values_and_left_operand_snapshots() {
    assert_scalar_execution(
        r#"
function fullSum(value) {
  "use strict";
  var full=0;
  for(var index=0;index<1;index++) value=(full=(value>>>0)+4294967295)>>>0;
  return value===4294967294 && full===8589934590;
}
function ordered() {
  var value=3,full=0,result=0;
  for(var index=0;index<1;index++) result=(full=value+(value=4294967295))>>>0;
  return result===2 && full===4294967298 && value===4294967295;
}
function overwritten() {
  var value=1,full=0,result=0;
  for(var index=0;index<1;index++) result=((full=(value=value+1)*4294967296)+(value=value+1))>>>0;
  return full===8589934592 && value===3 && result===3;
}
function subtract(value) {
  "use strict";
  for(var index=0;index<1;index++) value=((value>>>0)-4294967295)>>>0;
  return value;
}
function rounded() {
  var full=0,bits=0;
  for(var index=0;index<1;index++) bits=(full=9007199254740991+2)>>>0;
  return full===9007199254740992 && bits===0;
}
fullSum(4294967295) && ordered() && overwritten() && subtract(0)===1 && rounded();
"#,
    );
}

#[test]
fn scalar_integer_loops_keep_update_writes_and_final_test_values_separate() {
    assert_scalar_execution(
        r#"
function increments(value) {
  "use strict";
  var before=0,after=0;
  for(var index=0;index<1;index++) {value=value>>>0;before=value++;after=++value;value=value|0;}
  return before===4294967295 && after===4294967297 && value===1;
}
function decrements(value) {
  "use strict";
  var before=0,after=0;
  for(var index=0;index<1;index++) {value=value|0;before=value--;after=--value;}
  return before===-2147483648 && after===-2147483650 && value===-2147483650;
}
function compound(value) {
  "use strict";
  for(var index=0;index<1;index++) {value=value>>>0;value+=0.5;}
  return value===4294967295.5;
}
function finalTest() {
  var state=4294967293,last=0;
  for(;(state=((state>>>0)+1)>>>0)<4294967295;) last=state|0;
  return state===4294967295 && last===-2;
}
function falseTest(value) {
  "use strict";
  for(;(value=(value>>>0))<0;) value=1;
  return value;
}
increments(4294967295) && decrements(-2147483648) && compound(4294967295) &&
finalTest() && Object.is(falseTest(-0),0);
"#,
    );
}
