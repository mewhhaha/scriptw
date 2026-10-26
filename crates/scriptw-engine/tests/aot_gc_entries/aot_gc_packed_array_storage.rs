use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion, RealmBuilder,
    RunOptions, SnapshotLimits, SnapshotOutcome,
};
use scriptw_runtime::rooted_snapshot::{SnapshotDescriptor, SnapshotKey, SnapshotValue};

fn check(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for directive in ["", "'use strict';"] {
        let result = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &format!("{directive}function assert(v,m){{if(!v)throw new Error(m);}}{source}"),
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
            .unwrap();
        assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(result.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            result.completion
        );
    }
}

#[test]
fn number_bits_holes_and_bitmap_word_growth_remain_independent() {
    check(
        r#"
var a=[NaN,-0,Infinity,-Infinity,1.25,,7];
for(var i=7;i<258;i++)a[i]=i+0.5;
delete a[0];delete a[63];delete a[64];delete a[127];delete a[255];
assert(!(0 in a) && !(5 in a) && !(63 in a) && !(64 in a) && !(127 in a) && !(255 in a),'independent hole bitmap');
a[0]=NaN;a[64]=-0;a[127]=Infinity;a[255]=-Infinity;gc();
assert(Number.isNaN(a[0]) && Object.is(a[1],-0) && Object.is(a[64],-0),'all Number bits retained');
assert(a[2]===Infinity && a[3]===-Infinity && a[127]===Infinity && a[255]===-Infinity && a[257]===257.5,'growth and replacement');
assert(a.length===258 && !(63 in a),'delete never changes length');
var keys=Object.keys(a);assert(keys.length===256 && keys[0]==='0' && keys[keys.length-1]==='257','occupied keys only');
a[5]=undefined;assert(5 in a && a[5]===undefined && !(63 in a) && Object.is(a[64],-0),'nonnumber transition preserves present undefined and holes');
for(var j=0;j<258;j++)delete a[j];gc();a[0]=-0;assert(Object.is(a[0],-0) && Object.keys(a).join(',')==='0','reuse retired backing');
"#,
    );
}

#[test]
fn array_iteration_presence_keeps_live_own_and_inherited_semantics() {
    check(
        r#"
var a=[-0,1,2,,NaN], gets=0, seen=[], proto=Object.create(Array.prototype);
Object.defineProperty(proto,'2',{value:22,configurable:true});
Object.setPrototypeOf(a,proto);
var result=a.map(function(value,index){
  seen.push(index,value);
  if(index===0){
    Object.defineProperty(a,'1',{get:function(){gets++;gc();return undefined;},configurable:true});
    delete a[2];
  }
  return value;
});
assert(seen.length===8 && seen[0]===0 && Object.is(seen[1],-0),'first numeric own value');
assert(seen[2]===1 && seen[3]===undefined && gets===1,'accessor Get runs once after presence');
assert(seen[4]===2 && seen[5]===22 && seen[6]===4 && Number.isNaN(seen[7]),'later presence sees deletion and prototype');
assert(Object.is(result[0],-0) && 1 in result && result[1]===undefined && result[2]===22 && !(3 in result) && Number.isNaN(result[4]),'map preserves holes and Number bits');
var b=[-0,,NaN], bp=Object.create(Array.prototype);
Object.defineProperty(bp,'1',{value:7,configurable:true});Object.setPrototypeOf(b,bp);
var copy=b.slice();assert(Object.is(copy[0],-0) && copy[1]===7 && Number.isNaN(copy[2]),'slice copies inherited indices');
var holes=[1,,3].slice();assert(holes.length===3 && !(1 in holes),'slice retains an absent index');
var wide={length:4294967296};wide['4294967295']=17;
assert(Array.prototype.indexOf.call(wide,17,4294967295)===4294967295,'generic indices above Array index range');
"#,
    );
}

#[test]
fn array_iteration_presence_preserves_proxy_traps_and_own_shadowing() {
    check(
        r#"
var inheritedHas=[],a=[5,,7],proto=new Proxy(Array.prototype,{
  has:function(target,key){inheritedHas.push(key);return Reflect.has(target,key);}
});Object.setPrototypeOf(a,proto);
var calls=0;assert(a.every(function(){calls++;return true;}) && calls===2,'own data and hole iteration');
assert(inheritedHas.join(',')==='1','own indices never consult prototype has trap');
var events=[],target=[3,,5],proxy=new Proxy(target,{
  has:function(t,k){events.push('has:'+k);return Reflect.has(t,k);},
  get:function(t,k,r){if(k!=='length' && k!=='every')events.push('get:'+k);return Reflect.get(t,k,r);}
});
assert(Array.prototype.every.call(proxy,function(){return true;}),'Proxy array iteration result');
assert(events.join(',')==='has:0,get:0,has:1,has:2,get:2','Proxy has precedes Get and visits holes');
var error={},visited=0,abrupt=new Proxy([1,2],{
  has:function(t,k){if(k==='1')throw error;return Reflect.has(t,k);}
});
var caught=false;try{Array.prototype.every.call(abrupt,function(){visited++;return true;});}catch(e){caught=e===error;}
assert(caught && visited===1,'abrupt Proxy has propagates before callback');
"#,
    );
}

#[test]
fn inherited_setters_distinct_receivers_and_proxy_traps_keep_set_semantics() {
    check(
        r#"
var calls=[],a=[11],proto=Object.create(Array.prototype);
Object.defineProperty(proto,'1',{set:function(v){calls.push(this,v);gc();},configurable:true});Object.setPrototypeOf(a,proto);
assert(a.push(22)===2 && calls.length===2 && calls[0]===a && calls[1]===22 && !Object.prototype.hasOwnProperty.call(a,'1'),'push sees inherited setter');
var receiver={};assert(Reflect.set(a,'0',33,receiver) && receiver[0]===33 && a[0]===11,'distinct receiver is not direct own write');
var seen=[],p=new Proxy(a,{set:function(t,k,v,r){seen.push(k,v,r);return Reflect.set(t,k,v,r);},deleteProperty:function(t,k){seen.push('delete',k);return Reflect.deleteProperty(t,k);}});
p[0]=44;assert(a[0]===44 && seen[0]==='0' && seen[1]===44 && seen[2]===p,'actual proxy set trap');
delete p[0];assert(!(0 in a) && seen[3]==='delete' && seen[4]==='0','actual proxy delete trap');
var b=[1],key={};key[Symbol.toPrimitive]=function(h){assert(h==='string','key hint');b[0]={mark:7};gc();return '0';};
b[key]=5;assert(b[0]===5,'key effects precede storage guard');
var error={},c=[-0,NaN],cp=Object.create(Array.prototype);Object.defineProperty(cp,'2',{get:function(){gc();throw error;},configurable:true});Object.setPrototypeOf(c,cp);c.length=3;
var caught=false;try{c[2];}catch(e){caught=e===error;}assert(caught && Object.is(c[0],-0) && Number.isNaN(c[1]),'inherited abrupt getter preserves exception and packed roots');
"#,
    );
}

#[test]
fn cold_descriptors_accessors_freeze_and_shrink_keep_actual_mutable_records() {
    check(
        r#"
var token={},a=[0,1,2,3,4,5,6,7];Object.defineProperty(a,'3',{value:-0,writable:false,enumerable:false,configurable:false});
assert(Object.is(a[3],-0) && !Reflect.set(a,'3',8),'changed descriptor survives packed transition');
var receiver={token:token};Object.defineProperty(a,'6',{get:function(){gc();return this.token;},configurable:true,enumerable:true});
assert(Reflect.get(a,'6',receiver)===token,'cold accessor preserves receiver and GC roots');
var result=Reflect.defineProperty(a,'length',{value:1,writable:false});
assert(result===false && a.length===4 && !(4 in a) && !(6 in a) && Object.is(a[3],-0),'descending shrink rolls back at nonconfigurable entry');
assert(Object.getOwnPropertyDescriptor(a,'length').writable===false,'failed shrink final readonly length');
var b=[NaN,-0,2];Object.seal(b);assert(Reflect.set(b,'1',3) && !Reflect.deleteProperty(b,'1'),'seal retains writable only');
Object.freeze(b);assert(!Reflect.set(b,'1',4) && !Reflect.defineProperty(b,'0',{value:1}) && Number.isNaN(b[0]),'freeze prevents packed writes');
"#,
    );
}

#[test]
fn far_indices_and_whole_value_transitions_are_occupied_bounded_and_rooted() {
    check(
        r#"
var token={mark:9},symbol=Symbol('s'),a=new Array(4294967295);
a[0]=NaN;a[7]=-0;a[4294967294]=Infinity;gc();
assert(a.length===4294967295 && Number.isNaN(a[0]) && Object.is(a[7],-0) && a[4294967294]===Infinity,'far index transition never allocates logical extent');
a[7]=token;a[11]=symbol;a[13]=7n;gc();
assert(a[7]===token && a[11]===symbol && a[13]===7n && a[4294967294]===Infinity,'whole values remain strong roots');
assert(Object.keys(a).join(',')==='0,7,11,13,4294967294','sparse keys sorted after transition');
delete a[0];delete a[7];delete a[11];delete a[13];delete a[4294967294];gc();
assert(Object.keys(a).length===0 && a.length===4294967295,'final sparse deletion retires storage');
a[4294967294]=-0;assert(Object.is(a[4294967294],-0) && Object.keys(a).join(',')==='4294967294','empty sparse mode reinsertion');
"#,
    );
}

#[test]
fn graph_snapshot_reads_packed_numbers_and_empty_sparse_mode_without_mutation() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for (source, expected_keys) in [
        (
            "var a=[NaN,-0,,Infinity];gc();a;",
            vec!["0", "1", "3", "length"].into_iter().map(str::to_owned).collect::<Vec<_>>(),
        ),
        (
            "var a=[];a[4294967294]=1;delete a[4294967294];gc();a;",
            vec!["length".to_owned()],
        ),
        ("var a=[];for(var i=0;i<130;i++)a.push(i+0.5);delete a[63];delete a[64];a.length=4294967295;gc();a;",
            (0..130).filter(|index| *index!=63 && *index!=64).map(|index| index.to_string()).chain(["length".to_owned()]).collect()),
    ] {
        let result = Engine::new(RealmBuilder::new().build())
            .observe_script_graph(
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
                SnapshotLimits::default(),
            )
            .unwrap();
        assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
        let SnapshotOutcome::Captured { graph } = result.completion.outcome else {
            panic!("snapshot rejected");
        };
        let node = &graph.nodes()[0];
        let keys = node
            .properties
            .iter()
            .map(|property| match &property.key {
                SnapshotKey::String { units } => String::from_utf16(units.units()).unwrap(),
                _ => panic!("unexpected Symbol"),
            })
            .collect::<Vec<_>>();
        assert_eq!(keys, expected_keys);
        if source.contains("NaN") {
            assert!(
                matches!(&node.properties[0].descriptor, SnapshotDescriptor::Data {
                value: SnapshotValue::Number { bits }, writable: true, enumerable: true, configurable: true
            } if f64::from_bits(*bits).is_nan())
            );
            assert!(
                matches!(&node.properties[1].descriptor, SnapshotDescriptor::Data {
                value: SnapshotValue::Number { bits }, writable: true, enumerable: true, configurable: true
            } if *bits == (-0f64).to_bits())
            );
        } else if node.properties.len() > 1 {
            assert!(matches!(&node.properties[63].descriptor, SnapshotDescriptor::Data {
                value: SnapshotValue::Number { bits }, writable: true, enumerable: true, configurable: true
            } if *bits == 65.5f64.to_bits()));
            assert!(matches!(&node.properties.last().unwrap().descriptor, SnapshotDescriptor::Data {
                value: SnapshotValue::Number { bits }, writable: true, enumerable: false, configurable: false
            } if *bits == 4294967295f64.to_bits()));
        }
    }
}
