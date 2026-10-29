use scriptw_engine::{CompileOptions, Engine, ExecutionBackend, RealmBuilder, RunOptions};

fn assert_relational(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
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
        .unwrap_or_else(|error| panic!("guarded relational execution failed: {error}\n{source}"));
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        outcome.note.contains("boolean(true)"),
        "{}\n{source}",
        outcome.note
    );
}

#[test]
fn actual_number_pairs_preserve_ordered_nan_zero_and_infinity_results() {
    assert_relational(
        r#"
function mask(a,b) {
  return (a<b?1:0) | (a<=b?2:0) | (a>b?4:0) | (a>=b?8:0);
}
const rows=[[1,2,3],[2,1,12],[1,1,10],[0,-0,10],[-0,0,10],
  [NaN,0,0],[0,NaN,0],[NaN,NaN,0],[-Infinity,Infinity,3],
  [Infinity,-Infinity,12],[Infinity,Infinity,10],[-Infinity,-Infinity,10],
  [5e-324,0,12],[-5e-324,-0,3],[9007199254740991,9007199254740992,3]];
for(const row of rows) if(mask(row[0],row[1])!==row[2]) throw 'Number ordering';
function hot(){let i=0;while(i<4096)i++;return i;}
if(hot()!==4096) throw 'identifier loop';
let saved=1, reads=0;
function right(){reads++;saved='changed';return 2;}
if(!(saved<right()) || saved!=='changed' || reads!==1) throw 'saved left operand';
let mutable=1;
function replace(value){mutable=value;}
replace('20');if(mask(mutable,'3')!==3)throw 'String tag after Number';
replace(20n);if(mask(mutable,3)!==12)throw 'BigInt tag after Number';
replace(undefined);if(mask(mutable,0)!==0)throw 'undefined after Number';
true;
"#,
    );
}

#[test]
fn guarded_operands_are_read_once_before_ordered_runtime_coercions() {
    assert_relational(
        r#"
const compare=[(a,b)=>a<b,(a,b)=>a<=b,(a,b)=>a>b,(a,b)=>a>=b];
let trace=[];
const left={ [Symbol.toPrimitive](hint){trace.push('left-'+hint);return 1;} };
const right={ [Symbol.toPrimitive](hint){trace.push('right-'+hint);return 2;} };
function readLeft(value){trace.push('read-left');return value;}
function readRight(value){trace.push('read-right');return value;}
for(let i=0;i<4;i++){
  trace=[];
  if(compare[i](readLeft(1),readRight(2))!==(i<2) || trace.join(',')!=='read-left,read-right')throw 'Number reads';
  trace=[];
  if(compare[i](readLeft(left),readRight(right))!==(i<2))throw 'object result';
  if(trace.join(',')!=='read-left,read-right,left-number,right-number')throw 'coercion order';
  if(compare[i]('20','3')!==(i<2))throw 'lexicographic fallback';
  if(compare[i](9007199254740993n,9007199254740992)!==(i>=2))throw 'exact BigInt fallback';
  if(compare[i](0n,'invalid')!==false || compare[i](undefined,0)!==false)throw 'unordered fallback';
  const marker={};
  trace=[];
  const abrupt={ [Symbol.toPrimitive](){trace.push('right');throw marker;} };
  try{compare[i](Symbol(),abrupt);throw 'returned';}
  catch(error){if(error!==marker || trace.join(',')!=='right')throw 'right abrupt before ToNumber';}
}
true;
"#,
    );
}

#[test]
fn normal_false_and_abrupt_false_keep_distinct_active_handler_routes() {
    assert_relational(
        r#"
let caught=0, finalized=0;
const marker=false;
const compare=[
  function(a,b){try{return a<b;}catch(e){if(e!==marker)throw e;caught++;return 'abrupt';}finally{finalized++;}},
  function(a,b){try{return a<=b;}catch(e){if(e!==marker)throw e;caught++;return 'abrupt';}finally{finalized++;}},
  function(a,b){try{return a>b;}catch(e){if(e!==marker)throw e;caught++;return 'abrupt';}finally{finalized++;}},
  function(a,b){try{return a>=b;}catch(e){if(e!==marker)throw e;caught++;return 'abrupt';}finally{finalized++;}}
];
let leftCalls=0,rightCalls=0;
const left={ [Symbol.toPrimitive](){leftCalls++;throw marker;} };
const right={ [Symbol.toPrimitive](){rightCalls++;return 0;} };
for(let i=0;i<4;i++){
  if(compare[i](NaN,0)!==false)throw 'normal false';
  if(compare[i](left,right)!=='abrupt')throw 'abrupt false';
}
if(caught!==4 || finalized!==8 || leftCalls!==4 || rightCalls!==0)throw 'active handler order';
let symbols=0;
for(const fn of [(a,b)=>a<b,(a,b)=>a<=b,(a,b)=>a>b,(a,b)=>a>=b]){
  try{fn(Symbol(),1);throw 'returned';}catch(e){if(!(e instanceof TypeError))throw e;symbols++;}
}
symbols===4;
"#,
    );
}
