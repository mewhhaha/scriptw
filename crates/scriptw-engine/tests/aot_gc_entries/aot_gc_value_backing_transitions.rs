//! Observable transitions across packed values and lazily allocated references.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion, RealmBuilder,
    RunOptions, SnapshotLimits, SnapshotOutcome,
};
use scriptw_runtime::rooted_snapshot::{SnapshotDescriptor, SnapshotKey, SnapshotValue};

fn check(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for directive in ["", "'use strict';\n"] {
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &format!("{directive}function assert(v,m){{if(!v)throw new Error(m);}}\n{source}"),
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
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            observed.completion
        );
    }
}

#[test]
fn mixed_array_values_keep_holes_undefined_and_all_value_tags_distinct() {
    check(
        r#"
var token={mark:11},symbol=Symbol('kept'),call=function(){return token;};
var a=[-0,NaN,undefined,,null,true,'text',12345678901234567890n,token,symbol,call];
gc();
assert(a.length===11 && Object.is(a[0],-0) && Number.isNaN(a[1]),'number bits in mixed storage');
assert(Object.hasOwn(a,'2') && a[2]===undefined && !Object.hasOwn(a,'3'),'undefined is occupied; hole is absent');
assert(a[4]===null && a[5]===true && a[6]==='text' && a[7]===12345678901234567890n,'primitive tags');
assert(a[8]===token && a[9]===symbol && a[10]()===token,'reference identity and callable tag');
assert(Object.keys(a).join(',')==='0,1,2,4,5,6,7,8,9,10','own keys skip only the hole');
var d=Object.getOwnPropertyDescriptor(a,'2');
assert(d.value===undefined && d.writable && d.enumerable && d.configurable,'implicit default attributes reflect fully');
a[2]=token;a[3]=undefined;delete a[8];gc();
assert(d.value===undefined && a[2]===token && Object.hasOwn(a,'3') && !(8 in a),'descriptor snapshot, rewrite, deletion');
a[8]=null;a[8]=token;delete a[3];gc();
assert(a[8]===token && !(3 in a) && a[9]===symbol && a[10]()===token,'hole reuse preserves surrounding references');
"#,
    );
}

#[test]
fn mixed_array_growth_keeps_only_live_slots_and_roots_through_gc() {
    check(
        r#"
var a=[{id:0},undefined];
for(var i=2;i<150;i++)a[i]={id:i};
delete a[63];delete a[64];delete a[127];
a[62]=undefined;a[65]=null;a[128]=-0;a[129]=NaN;a[130]=7n;
gc();
assert(a[0].id===0 && a[149].id===149 && a[126].id===126,'grown backing roots unaliased objects');
assert(!(63 in a) && !(64 in a) && !(127 in a) && Object.hasOwn(a,'62') && a[62]===undefined,'holes across growth boundaries');
assert(a[65]===null && Object.is(a[128],-0) && Number.isNaN(a[129]) && a[130]===7n,'replacement tags after growth');
a[64]=a;a[127]=a[149];gc();
assert(a[64]===a && a[127]===a[149],'cycles and shared identity');
a.length=66;gc();assert(a.length===66 && a[64]===a && !Object.hasOwn(a,'127'),'successful shrink removes high references');
a[127]=undefined;gc();assert(a.length===128 && Object.hasOwn(a,'127') && !(126 in a),'regrowth does not resurrect retired values');
"#,
    );
}

#[test]
fn mixed_array_set_keeps_inherited_setters_and_distinct_receivers() {
    check(
        r#"
var token={},other={},a=[token,undefined,,null],proto=Object.create(Array.prototype),events=[];
Object.defineProperty(proto,'1',{set:function(){events.push('wrong');},configurable:true});
Object.defineProperty(proto,'2',{set:function(value){events.push(this,value);gc();},configurable:true});
Object.setPrototypeOf(a,proto);
assert(Reflect.set(a,'1',other) && a[1]===other && events.length===0,'own undefined shadows inherited setter');
assert(Reflect.set(a,'2',token) && !Object.hasOwn(a,'2') && events[0]===a && events[1]===token,'hole uses inherited setter');
var receiver={};assert(Reflect.set(a,'0',other,receiver) && receiver[0]===other && a[0]===token,'existing own data uses actual receiver');
assert(Reflect.set(a,'2',other,receiver) && events[2]===receiver && events[3]===other && !Object.hasOwn(receiver,'2'),'inherited setter receives explicit receiver');
Object.defineProperty(receiver,'1',{value:7,writable:false,configurable:true});
assert(!Reflect.set(a,'1',token,receiver) && receiver[1]===7 && a[1]===other,'receiver own nonwritable property rejects');
var getter=function(){gc();return other;};Object.defineProperty(a,'0',{get:getter,configurable:true});
assert(!Reflect.set(a,'0',token,receiver) && a[0]===other && receiver[0]===other,'accessor transition changes Set semantics');
"#,
    );
}

#[test]
fn mixed_array_descriptor_sparse_freeze_and_failed_shrink_transitions_are_lossless() {
    check(
        r#"
var token={},symbol=Symbol('s'),a=[-0,NaN,undefined,token,7n,symbol,null];
var saved=Object.getOwnPropertyDescriptor(a,'3');
Object.defineProperty(a,'2',{enumerable:false});
a[1000000]=token;gc();
assert(Object.is(a[0],-0) && Number.isNaN(a[1]) && a[3]===token && a[4]===7n && a[5]===symbol,'sparse transition retains each tag');
assert(Object.hasOwn(a,'2') && a[2]===undefined && !Object.getOwnPropertyDescriptor(a,'2').enumerable,'nondefault undefined descriptor');
Object.defineProperty(a,'3',{get:function(){gc();return symbol;},configurable:true});
assert(a[3]===symbol && saved.value===token,'accessor conversion preserves saved descriptor identity');
Object.freeze(a);gc();
assert(Object.isFrozen(a) && !Reflect.set(a,'0',token) && !Reflect.deleteProperty(a,'1000000') && a[1000000]===token,'freeze uses real sparse/default descriptor state');
var b=[-0,undefined,token,NaN,7n,symbol,null];
Object.defineProperty(b,'2',{configurable:false});
var caught=false;try{Object.defineProperty(b,'length',{value:1,writable:false});}catch(e){caught=e instanceof TypeError;}
gc();
assert(caught && b.length===3 && b[2]===token && Object.is(b[0],-0) && Object.hasOwn(b,'1'),'failed shrink rolls length back to blocked index plus one');
assert(!(3 in b) && !(4 in b) && !(5 in b) && !(6 in b),'failed shrink keeps preceding descending deletions');
assert(!Object.getOwnPropertyDescriptor(b,'length').writable && !Reflect.set(b,'3',token),'failed shrink still applies requested nonwritable length');
"#,
    );
}

#[test]
fn scalar_compact_literals_materialize_reference_storage_without_losing_scalars() {
    check(
        r#"
function make(){return {zero:-0,nan:NaN,missing:undefined,nil:null,truth:true,count:7};}
var a=make(),b=make(),token={id:17},symbol=Symbol('late');
a.count=token;gc();
assert(a.count===token && b.count===7 && Object.is(a.zero,-0) && Number.isNaN(a.nan),'first reference at nonzero slot preserves other scalar backing and separate instances');
assert(Object.hasOwn(a,'missing') && a.missing===undefined && a.nil===null && a.truth===true,'first reference preserves present undefined/null/boolean');
a.nil=symbol;a.truth=12345678901234567890n;gc();
assert(a.nil===symbol && a.truth===12345678901234567890n && a.count===token,'later reference kinds share no stale slot tags');
var saved=Object.getOwnPropertyDescriptor(a,'count');
a.count=null;a.nil=null;a.truth=null;gc();
assert(a.count===null && a.nil===null && a.truth===null && saved.value===token && saved.value.id===17,'null overwrites do not corrupt independently retained descriptor root');
a.missing=saved.value;gc();assert(a.missing===token && Object.is(a.zero,-0) && Number.isNaN(a.nan),'reference rewrite after all references replaced');
Object.defineProperty(a,'zero',{enumerable:false});delete a.count;a.count=undefined;gc();
assert(!Object.getOwnPropertyDescriptor(a,'zero').enumerable && Object.is(a.zero,-0) && Number.isNaN(a.nan),'dictionary transition preserves scalar bits');
assert(a.missing===token && Object.hasOwn(a,'count') && a.count===undefined && b.count===7 && b.nil===null,'transition preserves reference and rewritten undefined without sharing backing');
"#,
    );
}

#[test]
fn first_reference_store_rechecks_reentrant_keys_and_accessor_transition() {
    check(
        r#"
var a={x:0,y:undefined,z:null},token={},key={},calls=0;
key[Symbol.toPrimitive]=function(hint){assert(hint==='string','key hint');calls++;a.z=token;gc();return 'y';};
a[key]=token;gc();assert(calls===1 && a.y===token && a.z===token && a.x===0,'key effects may allocate reference backing before assignment');
var before=Object.getOwnPropertyDescriptor(a,'y');
Object.defineProperty(a,'y',{get:function(){gc();return a.z;},set:function(v){a.z=v;gc();},configurable:true});
var next={};a.y=next;assert(a.y===next && a.z===next && before.value===token,'setter retains target and old descriptor through GC');
a.z=null;gc();assert(a.y===null && before.value===token,'null overwrite after accessor transition');
"#,
    );
}

#[test]
fn native_snapshots_preserve_tags_and_aliases_before_and_after_transitions() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for (setup, transition) in [
        ("var v={zero:-0,nan:NaN,missing:undefined,big:0,first:null,second:null,nil:null};v.big=7n;v.first={};v.second=v.first;", ""),
        ("var v={zero:-0,nan:NaN,missing:undefined,big:0,first:null,second:null,nil:null};v.big=7n;v.first={};v.second=v.first;", "Object.defineProperty(v,'zero',{enumerable:false});"),
        ("var token={};var v=[-0,NaN,undefined,7n,token,token,null,,undefined];", ""),
        ("var token={};var v=[-0,NaN,undefined,7n,token,token,null,,undefined];", "Object.defineProperty(v,'0',{enumerable:false});"),
    ] {
        let observed = Engine::new(RealmBuilder::new().build()).observe_script_graph(
            &format!("{setup}{transition}gc();v;"),
            CompileOptions { host_surface_policy: HostSurfacePolicy::Test262, ..CompileOptions::default() },
            RunOptions { backend: ExecutionBackend::WasmAot, timeout_ms: Some(30_000), ..RunOptions::default() },
            SnapshotLimits::default(),
        ).unwrap();
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        let SnapshotOutcome::Captured { graph } = observed.completion.outcome else { panic!("native backing snapshot rejected: {setup}{transition}") };
        let node = &graph.nodes()[0];
        if setup.contains("var v=[") {
            let property = |key: &str| node.properties.iter().find(|property| {
                matches!(&property.key, SnapshotKey::String { units }
                    if String::from_utf16(units.units()).unwrap() == key)
            });
            assert!(property("7").is_none(), "snapshot must not invent a hole property");
            assert!(matches!(&property("8").unwrap().descriptor,
                SnapshotDescriptor::Data { value: SnapshotValue::Undefined {}, .. }));
        }
        let values = node.properties.iter().take(7).map(|property| match &property.descriptor {
            SnapshotDescriptor::Data { value, .. } => value,
            _ => panic!("data property changed to accessor in snapshot"),
        }).collect::<Vec<_>>();
        assert_eq!(values.len(), 7);
        assert!(matches!(values[0], SnapshotValue::Number { bits } if *bits == (-0.0f64).to_bits()));
        assert!(matches!(values[1], SnapshotValue::Number { bits } if f64::from_bits(*bits).is_nan()));
        assert!(matches!(values[2], SnapshotValue::Undefined {}));
        assert!(matches!(values[3], SnapshotValue::BigInt { decimal } if decimal.decimal() == "7"));
        let (SnapshotValue::Object { id: first }, SnapshotValue::Object { id: second }) = (values[4], values[5]) else { panic!("object aliases lost") };
        assert_eq!(first, second, "snapshot must intern the same actual object once");
        assert!(matches!(values[6], SnapshotValue::Null {}));
        assert!(matches!(&node.properties[0].descriptor, SnapshotDescriptor::Data { enumerable, .. } if *enumerable == transition.is_empty()));
    }
}

#[test]
fn numeric_holes_survive_value_promotion_before_descriptor_materialization() {
    check(
        r#"
var a=[-0,,NaN,,5],token={id:41};
// Both holes predate the first non-Number store. Keep cold descriptor reads last.
a[4]=token;a.push(undefined);gc();
assert(a.length===6 && Object.is(a[0],-0) && Number.isNaN(a[2]) && a[4]===token,'promotion preserves Number payloads and first reference');
assert(!(1 in a) && !(3 in a) && a[5]===undefined,'old holes differ from newly present undefined');
assert(Reflect.ownKeys(a).join(',')==='0,2,4,5,length','promotion retains exact own occupancy');
a[1]=undefined;delete a[4];a[3]=token;gc();
assert(a[1]===undefined && !(4 in a) && a[3]===token && a[5]===undefined,'mixed writes and deletion keep independent holes');
assert(Object.is(a[0],-0) && Number.isNaN(a[2]) && Reflect.ownKeys(a).join(',')==='0,1,2,3,5,length','no stale Number bitmap or value slots');
var absent=Object.getOwnPropertyDescriptor(a,'4'),present=Object.getOwnPropertyDescriptor(a,'1');
assert(absent===undefined && present.value===undefined && present.writable && present.enumerable && present.configurable,'final descriptors distinguish absent from present undefined');
"#,
    );
}

#[test]
fn delete_all_then_number_reuse_and_reference_repromotion_do_not_resurrect_values() {
    check(
        r#"
var a=[-0,,NaN,5],old={id:17};a[3]=old;a[7]=undefined;gc();
// Do not acquire present-slot descriptors before empty reset and re-promotion.
for(var i=0;i<8;i++)delete a[i];
gc();assert(a.length===8 && Reflect.ownKeys(a).join(',')==='length' && !(0 in a) && !(3 in a) && !(7 in a),'delete-all keeps length but retires occupancy');
a[2]=-0;a[6]=NaN;gc();
assert(Object.is(a[2],-0) && Number.isNaN(a[6]) && !(0 in a) && !(3 in a) && !(7 in a),'empty backing accepts fresh Number writes without old references');
assert(Reflect.ownKeys(a).join(',')==='2,6,length','Number restart has only newly written indices');
var next={id:29};a[5]=next;a[7]=undefined;gc();
assert(a[5]===next && a[5]!==old && Object.is(a[2],-0) && Number.isNaN(a[6]),'second promotion retains new Number bits and new identity');
assert(!(0 in a) && !(1 in a) && !(3 in a) && !(4 in a) && a[7]===undefined,'re-promotion never resurrects retired slots');
assert(Reflect.ownKeys(a).join(',')==='2,5,6,7,length','second promotion has exact occupied count');
a.length=7;gc();
assert(a.length===7 && !(7 in a) && a[5]===next && Reflect.ownKeys(a).join(',')==='2,5,6,length','shrink clears present undefined after restart');
var d=Object.getOwnPropertyDescriptor(a,'5');
assert(d.value===next && d.writable && d.enumerable && d.configurable && Object.getOwnPropertyDescriptor(a,'3')===undefined,'final reflection preserves new identity and old hole');
"#,
    );
}

#[test]
fn intrinsic_aliases_share_functions_but_never_mutable_property_descriptors() {
    check(
        r#"
var proto=Array.prototype,key=Symbol.iterator,original=proto.values,token={};
assert(original===proto[key],'intrinsic aliases have the same function identity');
var named=Object.getOwnPropertyDescriptor(proto,'values'),symbol=Object.getOwnPropertyDescriptor(proto,key);
assert(named!==symbol && named.value===original && symbol.value===original,'reflection creates independent descriptors with shared function');
assert(named.writable && !named.enumerable && named.configurable && symbol.writable && !symbol.enumerable && symbol.configurable,'both installed alias flags');
named.value=null;named.writable=false;symbol.enumerable=true;gc();
assert(proto.values===original && proto[key]===original,'mutating reflected snapshots does not mutate installed aliases');
var replacement=function(){return token;};proto.values=replacement;gc();
assert(proto.values===replacement && proto[key]===original,'overwriting one alias does not overwrite its sibling');
var iterator=proto[key].call([token]);gc();
assert(iterator.next().value===token && iterator.next().done,'unmodified alias retains actual iterator behavior');
assert(delete proto.values,'configurable named alias can be deleted');gc();
assert(!Object.hasOwn(proto,'values') && proto[key]===original,'deleting one installed descriptor retains sibling function');
Object.defineProperty(proto,'values',{value:original,writable:false,enumerable:false,configurable:false});gc();
var fixed=Object.getOwnPropertyDescriptor(proto,'values'),live=Object.getOwnPropertyDescriptor(proto,key);
assert(!fixed.writable && !fixed.configurable && live.writable && live.configurable && !live.enumerable,'freezing one alias descriptor leaves sibling attributes independent');
assert(!Reflect.set(proto,'values',replacement) && !Reflect.deleteProperty(proto,'values'),'fixed alias rejects mutation and deletion');
assert(Reflect.set(proto,key,replacement) && proto[key]===replacement && proto.values===original,'sibling remains writable after named alias freezes');
assert(Reflect.deleteProperty(proto,key) && !Object.hasOwn(proto,key) && proto.values===original,'sibling deletion cannot retire the frozen alias');
gc();assert(fixed.value===original && live.value===original && live.writable && live.configurable,'previous reflected snapshots remain independent of later alias changes');
var foreign=__scriptwCreateRealm().global,other=foreign.Array.prototype;
assert(other.values===other[foreign.Symbol.iterator] && other.values!==original,'created Realm owns its own canonical alias function');
assert(other.values.call([token]).next().value===token,'fresh Realm alias accepts original Realm receiver');
"#,
    );
}
