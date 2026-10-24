use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion,
    ObservedJsValue, RealmBuilder, RunOptions,
};

fn assert_addition(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
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
        .unwrap_or_else(|error| panic!("string addition failed: {error}\n{source}"));
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        outcome.completion,
        ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
        "{source}"
    );
}

#[test]
fn literal_and_dynamic_string_addition_preserve_exact_utf16() {
    assert_addition(
        r#"
function add(left,right){return left+right;}
const folded=('a'+'\ud83d')+('\ude00'+'\0'+'\ud800');
const dynamic=add(add('a','\ud83d'),add('\ude00',add('\0','\ud800')));
const marker='\u{F0000}'+'D800';
const strings=[folded,dynamic,add('',dynamic),add(dynamic,''),marker];
gc();
folded===dynamic && folded.length===5 && folded.charCodeAt(1)===0xd83d &&
folded.charCodeAt(2)===0xde00 && folded.charCodeAt(3)===0 &&
folded.charCodeAt(4)===0xd800 && strings[2]===dynamic && strings[3]===dynamic &&
marker.length===6 && marker==='\u{F0000}D800' && marker!=='\ud800';
"#,
    );
}

#[test]
fn string_addition_fast_dispatch_retains_mixed_primitives_and_errors() {
    assert_addition(
        r#"
function add(left,right){return left+right;}
const pairs=[['a','b'],['', ''],['x',-0],[-0,'x'],['x',NaN],['x',Infinity],
             ['x',undefined],['x',null],['x',true],['x',12n],[2,3],[2n,3n]];
const expected=['ab','','x0','0x','xNaN','xInfinity','xundefined','xnull','xtrue','x12',5,5n];
for(let index=0;index<pairs.length;index++){
  if(Reflect.apply(add,undefined,pairs[index])!==expected[index])throw index;
}
let caught=0;
for(const pair of [['x',Symbol('right')],[Symbol('left'),'x'],[1n,1]]){
  try{Reflect.apply(add,undefined,pair);throw 'missing TypeError';}
  catch(error){if(!(error instanceof TypeError))throw error;caught++;}
}
caught===3;
"#,
    );
}

#[test]
fn string_addition_checks_actual_tags_after_both_evaluations_and_preserves_hooks() {
    assert_addition(
        r#"
let trace='',mode='string';
const marker={};
const left={ [Symbol.toPrimitive](hint){trace+='left:'+hint+';';return 'L';} };
const right={ [Symbol.toPrimitive](hint){trace+='right:'+hint+';';return 'R';} };
function readLeft(){trace+='eval-left;';return mode==='string'?'L':left;}
function readRight(){trace+='eval-right;';return mode==='throw'?{[Symbol.toPrimitive](){throw marker;}}:right;}
let result=readLeft()+readRight();
if(result!=='LR'||trace!=='eval-left;eval-right;right:default;')throw trace;
trace='';mode='object';result=readLeft()+readRight();
if(result!=='LR'||trace!=='eval-left;eval-right;left:default;right:default;')throw trace;
trace='';mode='throw';let caught;
try{readLeft()+readRight();}catch(error){caught=error;}
caught===marker && trace==='eval-left;eval-right;left:default;';
"#,
    );
}
