//! Scalar source regions retain JavaScript values and abrupt evaluation order.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, ObservedCompletion, ObservedJsValue, RealmBuilder,
    RunOptions,
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
