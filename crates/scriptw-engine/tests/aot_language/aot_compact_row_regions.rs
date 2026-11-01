//! Whole-region own-data proofs preserve the original loop on every miss.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion,
    ObservedJsValue, RealmBuilder, RunOptions,
};

fn assert_rows(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
    for directive in ["", "\"use strict\";"] {
        let source = format!(
            r#"{directive}
function row(x,y){{return {{x:x,y:y}};}}
function gridRow(values){{return {{name:'row',values:values}};}}
function sum(rows,start,total){{
  for(var i=start;i<rows.length;i++) total+=rows[i].x+rows[i].y;
  return [total,i];
}}
function grid(rows,column,start,total){{
  for(var i=start;i<rows.length;i++) total+=rows[i].values[column];
  return [total,i];
}}
{source}"#
        );
        let result = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
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
            .expect("compact row control executes compiled Wasm");
        assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            result.completion,
            ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
            "{source}"
        );
    }
}

#[test]
fn compact_rows_keep_number_bits_rounding_offsets_and_final_counter() {
    assert_rows(
        r#"
var rows=[row(90,90),row(1,2),row(3,4)];
var offset=sum(rows,1,0),zero=sum([row(-0,-0),row(-0,-0)],0,-0);
var rounding=sum([row(-1e16,1)],0,1e16),nan=sum([row(NaN,5)],0,0);
var empty=sum([],-0,-0),one=sum([row(2,3)],0,7);
offset[0]===10 && offset[1]===3 && Object.is(zero[0],-0) && zero[1]===2 &&
rounding[0]===0 && rounding[1]===1 && Number.isNaN(nan[0]) && nan[1]===1 &&
Object.is(empty[0],-0) && Object.is(empty[1],-0) && one[0]===12 && one[1]===1;
"#,
    );
}

#[test]
fn compact_nested_rows_preserve_column_identity_number_bits_and_live_reentry() {
    assert_rows(
        r#"
var rows=[gridRow([10,90]),gridRow([20,2]),gridRow([30,4])];
var offset=grid(rows,1,1,0),negativeZero=grid([gridRow([-0]),gridRow([-0])],-0,0,-0);
var nan=grid([gridRow([NaN])],0,0,0),first=grid(rows,0,0,0);
rows[1].values[0]=7;gc();var second=grid(rows,0,0,0);
offset[0]===6 && offset[1]===3 && Object.is(negativeZero[0],-0) && negativeZero[1]===2 &&
Number.isNaN(nan[0]) && nan[1]===1 && first[0]===60 && second[0]===47 && second[1]===3;
"#,
    );
}

#[test]
fn compact_row_late_accessors_and_proxies_execute_once_in_source_order() {
    assert_rows(
        r#"
var calls='',rows=[row(1,1),row(2,2),row(3,3)];
rows[2]={get x(){calls+='x';rows[0].x=100;return 3;},get y(){calls+='y';return 3;}};
var accessor=sum(rows,0,0);
var target=row(5,6),receiver;
var proxy=new Proxy(target,{get:function(t,k,r){calls+=k;receiver=r;return Reflect.get(t,k,r);}});
var proxied=sum([row(1,2),proxy],0,0);
accessor[0]===12 && accessor[1]===3 && proxied[0]===14 && proxied[1]===2 &&
calls==='xyxy' && receiver===proxy && rows[0].x===100;
"#,
    );
}

#[test]
fn compact_row_shape_and_descriptor_misses_keep_inherited_receiver_and_abrupt_order() {
    assert_rows(
        r#"
var trace='',first=row(1,2),second={y:4,x:3},third=row(5,6);
var mixed=sum([first,second,third],0,0);
delete third.x;Object.setPrototypeOf(third,{get x(){trace+='p';return this.y+1;}});
var inherited=sum([first,second,third],0,0);
Object.defineProperty(second,'y',{get:function(){trace+='a';throw 'stop';}});
var thrown=false;try{sum([first,second,third],0,0);}catch(error){thrown=error==='stop';}
var nullThrown=false;try{sum([first,null],0,0);}catch(error){nullThrown=error instanceof TypeError;}
mixed[0]===21 && mixed[1]===3 && inherited[0]===23 && inherited[1]===3 &&
trace==='pa' && thrown && nullThrown;
"#,
    );
}

#[test]
fn compact_nested_column_misses_keep_property_conversion_and_later_row_mutations() {
    assert_rows(
        r#"
var rows=[gridRow([1,2]),gridRow([3,4])],trace='';
var key={[Symbol.toPrimitive]:function(hint){trace+=hint+',';rows[1].values[1]=9;return '1';}};
var converted=grid(rows,key,0,0),fractional=grid(rows,0.5,0,0);
rows[0].values[-1]=5;rows[1].values[-1]=6;
var negative=grid(rows,-1,0,0),outOfRange=grid(rows,9,0,0);
converted[0]===11 && converted[1]===2 && trace==='string,string,' &&
Number.isNaN(fractional[0]) && fractional[1]===2 && negative[0]===11 && negative[1]===2 &&
Number.isNaN(outOfRange[0]) && outOfRange[1]===2;
"#,
    );
}

#[test]
fn compact_nested_holes_and_proxy_arrays_preserve_getters_and_receiver() {
    assert_rows(
        r#"
var calls='',first=[1,2],last=[3,,5];
Object.setPrototypeOf(last,{get 1(){calls+='h';return 7;}});
var holes=grid([gridRow(first),gridRow(last)],1,0,0);
var target=[8,9],receiver;
var proxy=new Proxy(target,{get:function(t,k,r){calls+=k;receiver=r;return Reflect.get(t,k,r);}});
var proxied=grid([gridRow(first),gridRow(proxy)],1,0,0);
holes[0]===9 && holes[1]===2 && proxied[0]===11 && proxied[1]===2 &&
calls==='h1' && receiver===proxy;
"#,
    );
}

#[test]
fn compact_zero_trip_keeps_test_effects_and_avoids_unused_key_coercion() {
    assert_rows(
        r#"
var trace='',proxy=new Proxy([],{get:function(t,k,r){trace+=k;return Reflect.get(t,k,r);}});
var key={[Symbol.toPrimitive]:function(){throw 'unused';}};
var empty=sum(proxy,-0,-0),nested=grid([],key,-0,-0);
var past=sum([row(1,2)],5,7),nanStart=sum([row(1,2)],NaN,8);
trace==='length' && Object.is(empty[0],-0) && Object.is(empty[1],-0) &&
Object.is(nested[0],-0) && Object.is(nested[1],-0) && past[0]===7 && past[1]===5 &&
nanStart[0]===8 && Number.isNaN(nanStart[1]);
"#,
    );
}

#[test]
fn compact_reads_decline_body_writes_capture_mutation_and_changed_induction() {
    assert_rows(
        r#"
function writes(rows){var total=0;for(var i=0;i<rows.length;i++)total+=rows[i].x+(rows[i].y=7);return total;}
function captures(rows){var total=0,column=0;function next(){column++;return 1;}
  for(var i=0;i<rows.length;i++)total+=rows[i].values[column]+next();return [total,column];}
function changed(rows){var total=0;for(var i=0;i<rows.length;i++){total+=rows[i].x;i++;}return [total,i];}
var rows=[row(1,2),row(3,4),row(5,6)],written=writes(rows);
var captured=captures([gridRow([1,2]),gridRow([3,4])]),skipped=changed(rows);
written===30 && rows[0].y===7 && rows[1].y===7 && rows[2].y===7 &&
captured[0]===7 && captured[1]===2 && skipped[0]===6 && skipped[1]===4;
"#,
    );
}

#[test]
fn compact_prefix_miss_publishes_completed_writes_before_an_abrupt_fallback() {
    assert_rows(
        r#"
function caught(rows){
  var total=1,index=0,stamp=0;
  try{
    for(;index<rows.length;index++){
      total+=rows[index].x+rows[index].y;
      stamp=total+index;
    }
  }catch(error){return [total,index,stamp,error];}
  return [total,index,stamp,null];
}
var marker={},calls=0,rows=[row(1,2),row(3,4),row(5,6)];
Object.defineProperty(rows[2],'y',{get:function(){calls++;rows[0].x=99;throw marker;}});
var result=caught(rows);
result[0]===11 && result[1]===2 && result[2]===12 && result[3]===marker &&
calls===1 && rows[0].x===99;
"#,
    );
}
