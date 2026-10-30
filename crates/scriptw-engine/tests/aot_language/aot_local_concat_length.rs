//! Whole-function local consumers preserve ordinary binding/coercion semantics.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, ObservedCompletion, ObservedJsValue, RealmBuilder,
    RunOptions,
};

fn assert_modes(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    for directive in ["", "\"use strict\";\n"] {
        let source = format!("{directive}{source}");
        let outcome = Engine::new(RealmBuilder::new().build())
            .observe_script(&source, CompileOptions::default(), RunOptions::default())
            .expect("local consumer control completes normally");
        assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            outcome.completion,
            ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
            "{source}"
        );
    }
}

#[test]
fn local_concat_length_preserves_full_fallback_values_and_ordered_coercion() {
    assert_modes(
        r#"
function measure(left,right){const joined=left+right;return joined.length;}
function mutable(left,right){let joined=left+right;return joined['length'];}
var strings=measure('a\0','\ud800b')===4 && mutable('ab','c')===3;
var events='';
var left={[Symbol.toPrimitive](hint){events+='L'+hint;return 'ab';}};
var right={[Symbol.toPrimitive](hint){events+='R'+hint;return '\ud800';}};
var coerced=measure(left,right)===3 && events==='LdefaultRdefault';
events='';
var marker={},threw=false;
var abrupt={[Symbol.toPrimitive](){events+='A';throw marker;}};
try{measure(abrupt,right);}catch(error){threw=error===marker;}
var abruptOrder=threw && events==='A';
var prior=Object.getOwnPropertyDescriptor(Number.prototype,'length');
var gets=0,receiver=false;
Object.defineProperty(Number.prototype,'length',{configurable:true,get:function(){'use strict';gets++;receiver=this===3;return marker;}});
var numeric=measure(1,2)===marker && gets===1 && receiver;
var constructed=new measure(1,2)===marker && gets===2 && receiver;
class Derived extends Object{constructor(left,right){const joined=left+right;return joined.length;}}
var derivedObject=new Derived(1,2)===marker && gets===3 && receiver;
var mixed=false,symbol=false;
try{measure(1n,2);}catch(error){mixed=error instanceof TypeError;}
try{measure(Symbol('key'),'x');}catch(error){symbol=error instanceof TypeError;}
if(prior)Object.defineProperty(Number.prototype,'length',prior);else delete Number.prototype.length;
var ordinaryConstruction=new measure('ab','c') instanceof measure;
var derivedPrimitive=false;
try{new Derived('ab','c');}catch(error){derivedPrimitive=error instanceof TypeError;}
strings && coerced && abruptOrder && numeric && constructed && derivedObject && gets===3 &&
mixed && symbol && ordinaryConstruction && derivedPrimitive;
"#,
    );
}

#[test]
fn local_concat_length_keeps_other_uses_captures_tdz_and_operand_effects() {
    assert_modes(
        r#"
function both(left,right){const joined=left+right;return [joined,joined.length];}
function capture(left,right){const joined=left+right;return ()=>joined;}
function replace(left,right){let joined=left+right;joined='replacement';return joined.length;}
function computed(left,right,key){const joined=left+right;return joined[key];}
function tdz(right){let joined=joined+right;return joined.length;}
var pair=both('ab','cd'),reader=capture('ab','cd');
var beforeInit=false;
try{tdz('x');}catch(error){beforeInit=error instanceof ReferenceError;}
var effects='';
function left(){effects+='L';return {[Symbol.toPrimitive](){effects+='l';return 'ab';}};}
function right(){effects+='R';return {[Symbol.toPrimitive](){effects+='r';return 'c';}};}
function effectful(){const joined=left()+right();return joined.length;}
var ordered=effectful()===3 && effects==='LRlr';
var conversions=0;
var key={[Symbol.toPrimitive](){conversions++;return 'length';}};
var dynamic=computed('a','bc',key)===3 && conversions===1;
function* generator(left,right){const joined=left+right;return joined.length;}
var generated=generator('ab','c').next();
pair[0]==='abcd' && pair[1]===4 && reader()==='abcd' && replace('a','b')===11 &&
beforeInit && ordered && dynamic && generated.value===3 && generated.done===true;
"#,
    );
}
