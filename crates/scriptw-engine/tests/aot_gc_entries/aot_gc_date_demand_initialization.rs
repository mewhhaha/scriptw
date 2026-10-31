use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostClock, HostOutputEvent, HostSurfacePolicy,
    MonotonicClockInstant, ObservedCompletion, ObservedJsValue, ObservedNumber, Realm,
    RealmBuilder, RunOptions, UtcEpochMilliseconds,
};
use scriptw_intl::ConfiguredSystemTimeZone;

#[derive(Default)]
struct HostReads {
    wall: AtomicU64,
    zone: AtomicU64,
    monotonic: AtomicU64,
    order: Mutex<Vec<&'static str>>,
}

struct FixedClock(Arc<HostReads>);

impl HostClock for FixedClock {
    fn utc_epoch_milliseconds(&self) -> UtcEpochMilliseconds {
        self.0.wall.fetch_add(1, Ordering::Relaxed);
        self.0.order.lock().unwrap().push("clock");
        UtcEpochMilliseconds::new(1234).expect("fixed valid Date clock")
    }

    fn monotonic_instant(&self) -> MonotonicClockInstant {
        MonotonicClockInstant::new(self.0.monotonic.fetch_add(1, Ordering::Relaxed))
    }
}

fn realm(reads: &Arc<HostReads>) -> Realm {
    RealmBuilder::new()
        .with_host_clock(Box::new(FixedClock(Arc::clone(reads))))
        .with_system_time_zone(ConfiguredSystemTimeZone::resolve("+01:00").unwrap())
        .build()
}

fn compile_options() -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    }
}

fn run_options() -> RunOptions {
    RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(120_000),
        ..RunOptions::default()
    }
}

fn assert_modes(source: &str, expected_clock_reads: u64) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    for directive in ["", "'use strict';\n"] {
        let reads = Arc::new(HostReads::default());
        let source = format!("{directive}{source}");
        let observed = Engine::new(realm(&reads))
            .observe_script(&source, compile_options(), run_options())
            .unwrap_or_else(|error| panic!("Date demand initialization: {error}\n{source}"));
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0))),
            "{source}",
        );
        assert_eq!(
            observed.output_events,
            [HostOutputEvent::PrintLine("ok".into())]
        );
        assert_eq!(
            reads.wall.load(Ordering::Relaxed),
            expected_clock_reads,
            "{source}"
        );
    }
}

#[test]
fn first_date_descriptor_publishes_complete_realm_graph_and_shared_utc_gmt_alias() {
    assert_modes(
        r#"
function check(value,label){if(!value)throw label;}
var methods=('constructor|getTime|setTime|valueOf|getFullYear|getUTCFullYear|getMonth|getUTCMonth|'+
  'getDate|getUTCDate|getDay|getUTCDay|getHours|getUTCHours|getMinutes|getUTCMinutes|'+
  'getSeconds|getUTCSeconds|getMilliseconds|getUTCMilliseconds|getTimezoneOffset|getYear|setYear|'+
  'setFullYear|setUTCFullYear|setMonth|setUTCMonth|setDate|setUTCDate|setHours|setUTCHours|'+
  'setMinutes|setUTCMinutes|setSeconds|setUTCSeconds|setMilliseconds|setUTCMilliseconds|'+
  'toISOString|toJSON|toDateString|toLocaleDateString|toLocaleString|toLocaleTimeString|'+
  'toTemporalInstant|toTimeString|toString|toUTCString|toGMTString').split('|');
var constructors=[],aliases=[];
for(var global of [globalThis,__scriptwCreateRealm().global,__scriptwCreateRealm().global]){
  var before=Reflect.ownKeys(global);
  check(before.indexOf('Date')>=0 && 'Date' in global && Object.hasOwn(global,'Date') &&
    !Object.prototype.propertyIsEnumerable.call(global,'Date'),'unread global membership');
  gc();
  var descriptor=Object.getOwnPropertyDescriptor(global,'Date'),C=descriptor.value,P=C.prototype;
  check(typeof C==='function' && C===global.Date && C===Reflect.get(global,'Date') &&
    descriptor.writable && !descriptor.enumerable && descriptor.configurable &&
    !Object.hasOwn(descriptor,'get'),'first observation is an ordinary data descriptor');
  check(C.name==='Date' && C.length===7 && P.constructor===C &&
    Object.getPrototypeOf(C)===global.Function.prototype && Object.getPrototypeOf(P)===global.Object.prototype,
    'complete constructor and prototype cycle');
  check(Reflect.ownKeys(C).join('|')==='length|name|prototype|now|parse|UTC',
    'constructor method order');
  var ownPrototype=Object.getOwnPropertyDescriptor(C,'prototype');
  check(ownPrototype.value===P && !ownPrototype.writable && !ownPrototype.enumerable &&
    !ownPrototype.configurable,'constructor prototype attributes');
  for(var key of ['name','length']){
    var metadata=Object.getOwnPropertyDescriptor(C,key);
    check(!metadata.writable && !metadata.enumerable && metadata.configurable,'constructor metadata attributes');
  }
  var keys=Reflect.ownKeys(P);
  check(keys.slice(0,-1).join('|')===methods.join('|') && keys[keys.length-1]===Symbol.toPrimitive,
    'prototype String and Symbol key order');
  var gmt=P.toGMTString;gc();var utc=P.toUTCString;
  check(gmt===utc && utc.name==='toUTCString' && utc.length===0 &&
    Object.getPrototypeOf(utc)===global.Function.prototype,'UTC/GMT shared deferred callable');
  for(var key of methods){
    var member=Object.getOwnPropertyDescriptor(P,key);
    check(typeof member.value==='function' && member.writable && !member.enumerable && member.configurable &&
      Object.getPrototypeOf(member.value)===global.Function.prototype,key+' descriptor and defining Realm');
  }
  var primitive=Object.getOwnPropertyDescriptor(P,Symbol.toPrimitive);
  check(typeof primitive.value==='function' && !primitive.writable && !primitive.enumerable &&
    primitive.configurable,'Symbol.toPrimitive attributes');
  Object.defineProperty(P,'toGMTString',{value:function replacement(){return 'changed';}});
  check(P.toUTCString===utc && P.toGMTString!==utc,'alias descriptors are independently mutable');
  delete P.toGMTString;gc();
  check(!Object.hasOwn(P,'toGMTString') && utc.call(new C(0))==='Thu, 01 Jan 1970 00:00:00 GMT',
    'deleting one alias retains the other callable');
  check(Reflect.ownKeys(global).join('|')===before.join('|'),'global insertion order survives demand');
  constructors.push(C);aliases.push(utc);
}
gc();
check(constructors[0]!==constructors[1] && constructors[1]!==constructors[2] &&
  constructors[0].prototype!==constructors[1].prototype && aliases[0]!==aliases[1] &&
  aliases[1]!==aliases[2],'separate Realm constructors, prototypes and alias caches');
for(var mutation of ['delete','replace']){
  var global=__scriptwCreateRealm().global,P=global.Date.prototype,replacement=function(){return 'changed';};
  if(mutation==='delete')delete P.toGMTString;
  else P.toUTCString=replacement;
  gc();
  var survivor=mutation==='delete'?P.toUTCString:P.toGMTString;
  check(survivor.name==='toUTCString' && survivor!==replacement &&
    survivor.call(new global.Date(0))==='Thu, 01 Jan 1970 00:00:00 GMT',
    'mutating an unread alias leaves the sibling recipe alive');
}
print('ok');262;
"#,
        0,
    );
}

#[test]
fn unread_date_mutations_and_locked_globals_keep_private_foreign_defaults() {
    assert_modes(
        r#"
function check(value,label){if(!value)throw label;}
for(var mode of ['delete','assign','define','accessor','attributes','seal','freeze']){
  var global=__scriptwCreateRealm().global,marker={},gets=0,sets=0;
  var target=global.Array.bind(null),functionPrototype=global.Function.prototype,objectPrototype=global.Object.prototype;
  Object.defineProperty(target,'prototype',{value:17});
  if(mode==='delete')check(Reflect.deleteProperty(global,'Date'),'delete unread Date');
  else if(mode==='assign')global.Date=marker;
  else if(mode==='define')Object.defineProperty(global,'Date',{value:marker,writable:false,configurable:false});
  else if(mode==='accessor')Object.defineProperty(global,'Date',{
    get(){gets++;return marker;},set(value){sets++;},configurable:true});
  else if(mode==='attributes')Object.defineProperty(global,'Date',{enumerable:true,writable:false,configurable:false});
  else Object[mode](global);
  gc();
  var instance=Reflect.construct(Date,[0],target),P=Object.getPrototypeOf(instance),C=P.constructor;
  check(C.name==='Date' && C!==Date && C.prototype===P &&
    Object.getPrototypeOf(C)===functionPrototype && Object.getPrototypeOf(P)===objectPrototype &&
    Date.prototype.getTime.call(instance)===0,'canonical foreign Date default');
  check(gets===0 && sets===0,'private demand never reads or writes the substituted accessor');
  var descriptor=Object.getOwnPropertyDescriptor(global,'Date');
  if(mode==='delete')check(descriptor===undefined && !('Date' in global),'deleted global remains absent');
  else if(mode==='accessor'){
    check(typeof descriptor.get==='function' && typeof descriptor.set==='function','accessor remains installed');
    check(global.Date===marker && gets===1 && Reflect.set(global,'Date',C) && sets===1,
      'ordinary explicit access still invokes the replacement accessor');
  }else if(mode==='assign' || mode==='define'){
    check(descriptor.value===marker && descriptor.writable===(mode==='assign'),'public replacement survives demand');
  }else{
    check(descriptor.value===C && !Object.hasOwn(descriptor,'get') && !descriptor.configurable &&
      descriptor.enumerable===(mode==='attributes') && descriptor.writable===(mode==='seal'),
      'attribute-only, seal and freeze preserve the original Date value');
    check(Reflect.defineProperty(global,'Date',{value:C}) && !Reflect.deleteProperty(global,'Date'),
      'locked data property accepts the completed canonical SameValue');
    if(mode!=='seal')check(!Reflect.set(global,'Date',marker) &&
      !Reflect.defineProperty(global,'Date',{value:marker}),'readonly global rejects non-SameValue replacement');
  }
  var reads=0,proxy=new Proxy(target,{get(t,key,receiver){
    if(key==='prototype'){reads++;return 17;}return Reflect.get(t,key,receiver);
  }});
  gc();
  check(Object.getPrototypeOf(Reflect.construct(Date,[1],proxy))===P && reads===1 &&
    P.constructor===C,'proxy/bound newTarget retains the same private Realm after GC');
  check(gets===(mode==='accessor'?1:0) && sets===(mode==='accessor'?1:0),
    'repeated private defaults preserve public accessor counts');
}
var global=__scriptwCreateRealm().global,target=global.Array.bind(null),custom=Object.create(null),gets=0;
Object.defineProperty(global,'Date',{get(){gets++;throw 'public Date accessor';},configurable:true});
Object.defineProperty(target,'prototype',{value:custom});
check(Object.getPrototypeOf(Reflect.construct(Date,[0],target))===custom && gets===0,
  'object-valued newTarget prototype bypasses the Date default');
var abruptTarget=global.Array.bind(null),trace='',marker={},caught;
Object.defineProperty(abruptTarget,'prototype',{get(){trace+='p';gc();throw marker;}});
try{Reflect.construct(Date,[{valueOf(){trace+='a';return 0;}}],abruptTarget);}catch(error){caught=error;}
check(caught===marker && trace==='ap' && gets===0,'argument conversion then one abrupt prototype Get');
var revoked=Proxy.revocable(global.Array,{});revoked.revoke();caught=undefined;
try{Reflect.construct(Date,[0],revoked.proxy);}catch(error){caught=error;}
check(caught instanceof TypeError && gets===0,'revoked newTarget preserves abrupt Get without public Date access');
print('ok');262;
"#,
        0,
    );
}

#[test]
fn date_first_publication_ignores_inherited_hooks_and_preserves_call_order_and_realm() {
    assert_modes(
        r#"
function check(value,label){if(!value)throw label;}
var global=__scriptwCreateRealm().global,hooks=0,objectPrototype=global.Object.prototype,
  functionPrototype=global.Function.prototype,errorPrototype=global.TypeError.prototype;
function forbidden(){hooks++;throw 'Date installation invoked user code';}
Object.defineProperty(global,'Object',{get:forbidden,configurable:true});
Object.defineProperty(global,'Function',{get:forbidden,configurable:true});
for(var key of ['constructor','now','parse','UTC','getTime','toUTCString','toGMTString','toLocaleString','toTemporalInstant'])
  Object.defineProperty(objectPrototype,key,{get:forbidden,set:forbidden,configurable:true});
for(var key of ['name','length','prototype'])
  Object.defineProperty(functionPrototype,key,{get:forbidden,set:forbidden,configurable:true});
var C=Object.getOwnPropertyDescriptor(global,'Date').value,P=C.prototype;
check(C.name==='Date' && C.length===7 && P.constructor===C && Object.getPrototypeOf(C)===functionPrototype &&
  Object.getPrototypeOf(P)===objectPrototype && P.toGMTString===P.toUTCString && hooks===0,
  'first own publication ignores inherited setters and replaced public parents');
var events=[],target=global.Array.bind(null);
Object.defineProperty(target,'prototype',{get(){events.push('prototype');gc();return P;}});
function argument(name,value){return {valueOf(){events.push(name);gc();return value;}};}
var date=Reflect.construct(C,[argument('year',1970),argument('month',0),argument('day',1),
  argument('hour',1),argument('minute',2),argument('second',3),argument('ms',4)],target);
check(events.join('|')==='year|month|day|hour|minute|second|ms|prototype' && date.getTime()===123004 &&
  date.getHours()===1,'constructor coercion and prototype Get retain their order and configured zone');
events=[];
var primitive={};primitive[Symbol.toPrimitive]=function(hint){events.push(hint);gc();return 1234;};
check(new C(primitive).getTime()===1234 && events.join('|')==='default','one-argument default-hint conversion');
var ignored={valueOf(){throw 'Date call coerced an ignored argument';}},current=new C();
check(current.getTime()===1234 && typeof C(ignored)==='string' && C.now()===1234,
  'Date construction, ordinary call and now read the injected clock only when called');
var retained=P.toUTCString;delete global.Date;global.TypeError=function replacement(){};gc();
var caught,marker={},later=0;
try{P.getTime.call({$DateValue:0});}catch(error){caught=error;}
check(caught!==undefined && Object.getPrototypeOf(caught)===errorPrototype,'brand errors use the called Date Realm');
caught=undefined;
try{C.UTC({valueOf(){gc();return Symbol('bad');}},0);}catch(error){caught=error;}
check(caught!==undefined && Object.getPrototypeOf(caught)===errorPrototype,'numeric coercion errors keep the defining Realm');
caught=undefined;
try{new C({valueOf(){throw marker;}},{valueOf(){later++;return 0;}});}catch(error){caught=error;}
check(caught===marker && later===0 && retained.call(new C(0))==='Thu, 01 Jan 1970 00:00:00 GMT' &&
  !Object.hasOwn(global,'Date') && hooks===0,'retained methods, arbitrary abrupt values and public deletion');
print('ok');262;
"#,
        3,
    );
}

#[test]
fn later_programs_and_retained_calls_keep_foreign_date_demand_and_host_read_boundaries() {
    use scriptw_engine::{EmbeddedEvaluation, EmbeddedRuntime, JsCompletion, JsValue};

    fn normal(completion: JsCompletion) -> JsValue {
        match completion {
            JsCompletion::Normal(value) => value,
            JsCompletion::Throw(value) => panic!("retained Date operation threw: {value:?}"),
        }
    }

    // Independently compiled NativeHost programs share the captured Realm, not
    // the first ProgramInstance or a subsequently replaced public eval/Date.
    assert_modes(
        r#"
function check(value,label){if(!value)throw label;}
var realm=__scriptwCreateRealm(),global=realm.global,evaluate=global.eval;
var first=evaluate("var target=Array.bind(null);Object.defineProperty(target,'prototype',{value:17});var marker={};Date=marker;[target,marker];");
global.parentDate=Date;gc();
var second=evaluate("var instance=Reflect.construct(parentDate,[0],target);var saved=Object.getPrototypeOf(instance).constructor;var utc=saved.prototype.toUTCString;[saved,saved.prototype,utc,Date];");
var C=second[0],P=second[1],utc=second[2];
check(second[3]===first[1] && C!==Date && P.constructor===C &&
  Object.getPrototypeOf(C)===global.Function.prototype,'later program privately demands the captured Realm');
delete global.Date;global.eval=function replacement(){throw 'replacement eval';};
realm.evalScript=function replacement(){throw 'replacement evalScript';};gc();
var third=evaluate("[saved,saved.prototype,utc,saved.prototype.toGMTString,utc.call(new saved(0)),Object.hasOwn(globalThis,'Date')];");
check(third[0]===C && third[1]===P && third[2]===utc && third[3]===utc &&
  third[4]==='Thu, 01 Jan 1970 00:00:00 GMT' && third[5]===false,
  'third program preserves constructor cycle, aliases and captured callable after GC');
check(Object.getPrototypeOf({})===Object.prototype,'caller Realm restored after foreign evaluation');
print('ok');262;
"#,
        0,
    );

    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(
            r#"
export const foreign=__scriptwCreateRealm().global;
const marker={};
const target=foreign.Array.bind(null);
Object.defineProperty(target,'prototype',{value:undefined});
foreign.Date=marker;
let savedC,savedDate;
export function metadata(){
  const C=Object.getOwnPropertyDescriptor(globalThis,'Date').value;
  return C.name==='Date' && C.prototype.constructor===C &&
    C.prototype.toUTCString===C.prototype.toGMTString && foreign.Date===marker;
}
export function demand(){
  savedDate=Reflect.construct(Date,[0],target);
  savedC=Object.getPrototypeOf(savedDate).constructor;
  return savedC;
}
export function date(){return savedDate;}
export function inspect(C,utc,gmt){
  return C===savedC && C!==Date && C.prototype.constructor===C && utc===gmt &&
    utc===C.prototype.toUTCString && Object.getPrototypeOf(C)===foreign.Function.prototype &&
    Object.getPrototypeOf(C.prototype)===foreign.Object.prototype && foreign.Date===marker;
}
export function mutate(){
  delete foreign.Date;delete savedC.prototype.toGMTString;
  savedC.prototype.toUTCString=function replacement(){return 'changed';};
  gc();return true;
}
export function inspectAfter(utc){
  const value=Reflect.construct(Date,[1],target);
  return Object.getPrototypeOf(value)===savedC.prototype &&
    savedC.prototype.constructor===savedC && !Object.hasOwn(foreign,'Date') &&
    !Object.hasOwn(savedC.prototype,'toGMTString') && utc!==savedC.prototype.toUTCString &&
    utc.call(savedDate)==='Thu, 01 Jan 1970 00:00:00 GMT';
}
export function formatNow(){
  const ignored={valueOf(){throw 'Date call coerced argument';}};
  return savedC(ignored).indexOf('GMT+0100')>=0;
}
export function localTime(){return savedDate.getHours()===1 && savedDate.getTimezoneOffset()===-60;}
"#,
            compile_options(),
        )
        .unwrap();
    let artifact = compiler.emit_wasm(&unit).unwrap();
    let mut config = wasmtime::Config::new();
    scriptw_engine::configure_wasm_runtime(&mut config);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let runtime = EmbeddedRuntime::compile(&engine, &artifact).unwrap();
    for _ in 0..2 {
        let reads = Arc::new(HostReads::default());
        let state = runtime.new_state(realm(&reads), ()).unwrap();
        let mut store = wasmtime::Store::new(&engine, state);
        store.limiter(|state| state.limits_mut());
        store.set_epoch_deadline(u64::MAX / 2);
        let mut linker = wasmtime::Linker::new(&engine);
        runtime.register(&mut store, &mut linker).unwrap();

        // Delegate to the actual registered callback with its exact FuncType.
        // This observes zone reads without fabricating a GC byte-array result.
        let zone = scriptw_aot_wasm::GcHostImport::SystemTimeZoneSnapshot;
        let original = linker
            .get(&mut store, zone.module(), zone.name())
            .unwrap()
            .into_func()
            .unwrap();
        let signature = original.ty(&store);
        let probe = Arc::clone(&reads);
        linker.allow_shadowing(true);
        linker
            .func_new(
                zone.module(),
                zone.name(),
                signature,
                move |mut caller, inputs, outputs| {
                    probe.zone.fetch_add(1, Ordering::Relaxed);
                    probe.order.lock().unwrap().push("zone");
                    original.call(&mut caller, inputs, outputs)
                },
            )
            .unwrap();
        linker.allow_shadowing(false);

        let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
        assert!(matches!(
            instance.initialize(&mut store).unwrap(),
            EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
        ));
        let metadata = normal(instance.get_export(&mut store, "metadata").unwrap());
        store.gc(None).unwrap();
        assert_eq!(
            normal(instance.call(&mut store, &metadata, None, &[]).unwrap()).as_boolean(),
            Some(true)
        );
        assert_eq!(reads.wall.load(Ordering::Relaxed), 0);
        assert_eq!(
            reads.zone.load(Ordering::Relaxed),
            0,
            "metadata demand must not consult the zone"
        );

        let demand = normal(instance.get_export(&mut store, "demand").unwrap());
        let constructor = normal(instance.call(&mut store, &demand, None, &[]).unwrap());
        let prototype = normal(
            instance
                .get_property(
                    &mut store,
                    &constructor,
                    &"prototype".encode_utf16().collect::<Vec<_>>(),
                )
                .unwrap(),
        );
        let gmt = normal(
            instance
                .get_property(
                    &mut store,
                    &prototype,
                    &"toGMTString".encode_utf16().collect::<Vec<_>>(),
                )
                .unwrap(),
        );
        store.gc(None).unwrap();
        let utc = normal(
            instance
                .get_property(
                    &mut store,
                    &prototype,
                    &"toUTCString".encode_utf16().collect::<Vec<_>>(),
                )
                .unwrap(),
        );
        let inspect = normal(instance.get_export(&mut store, "inspect").unwrap());
        assert_eq!(
            normal(
                instance
                    .call(
                        &mut store,
                        &inspect,
                        None,
                        &[constructor.clone(), utc.clone(), gmt]
                    )
                    .unwrap()
            )
            .as_boolean(),
            Some(true)
        );
        let date = normal(instance.get_export(&mut store, "date").unwrap());
        let date = normal(instance.call(&mut store, &date, None, &[]).unwrap());
        store.gc(None).unwrap();
        let text = normal(instance.call(&mut store, &utc, Some(&date), &[]).unwrap());
        assert_eq!(
            String::from_utf16(&text.to_utf16(&mut store).unwrap()).unwrap(),
            "Thu, 01 Jan 1970 00:00:00 GMT"
        );
        assert_eq!(reads.wall.load(Ordering::Relaxed), 0);
        assert_eq!(reads.zone.load(Ordering::Relaxed), 0);

        let now = normal(
            instance
                .get_property(
                    &mut store,
                    &constructor,
                    &"now".encode_utf16().collect::<Vec<_>>(),
                )
                .unwrap(),
        );
        assert_eq!(
            normal(instance.call(&mut store, &now, None, &[]).unwrap()).as_number(),
            Some(1234.0)
        );
        assert_eq!(reads.wall.load(Ordering::Relaxed), 1);
        assert_eq!(reads.zone.load(Ordering::Relaxed), 0);
        reads.order.lock().unwrap().clear();
        let format_now = normal(instance.get_export(&mut store, "formatNow").unwrap());
        assert_eq!(
            normal(instance.call(&mut store, &format_now, None, &[]).unwrap()).as_boolean(),
            Some(true)
        );
        let order = reads.order.lock().unwrap().clone();
        assert_eq!(order.first(), Some(&"clock"));
        assert!(order.len() > 1 && order[1..].iter().all(|event| *event == "zone"));
        assert_eq!(reads.wall.load(Ordering::Relaxed), 2);
        let local_time = normal(instance.get_export(&mut store, "localTime").unwrap());
        assert_eq!(
            normal(instance.call(&mut store, &local_time, None, &[]).unwrap()).as_boolean(),
            Some(true)
        );
        assert!(reads.zone.load(Ordering::Relaxed) > 0);

        let mutate = normal(instance.get_export(&mut store, "mutate").unwrap());
        assert_eq!(
            normal(instance.call(&mut store, &mutate, None, &[]).unwrap()).as_boolean(),
            Some(true)
        );
        store.gc(None).unwrap();
        let inspect_after = normal(instance.get_export(&mut store, "inspectAfter").unwrap());
        assert_eq!(
            normal(
                instance
                    .call(&mut store, &inspect_after, None, &[utc])
                    .unwrap()
            )
            .as_boolean(),
            Some(true)
        );
        assert_eq!(reads.wall.load(Ordering::Relaxed), 2);
        assert!(instance.take_output(&mut store).unwrap().is_empty());
    }
}

#[test]
fn rooted_snapshot_resolves_unread_date_and_preserves_cycle_flags_and_utc_gmt_identity() {
    use scriptw_engine::{SnapshotLimits, SnapshotOutcome};
    use scriptw_runtime::rooted_snapshot::{
        SnapshotDescriptor, SnapshotKey, SnapshotNode, SnapshotObjectKind, SnapshotValue,
    };

    fn property<'a>(node: &'a SnapshotNode, name: &str) -> &'a SnapshotDescriptor {
        let units = name.encode_utf16().collect::<Vec<_>>();
        &node.properties.iter().find(|property| {
            matches!(&property.key, SnapshotKey::String { units: key } if key.units() == units)
        }).unwrap_or_else(|| panic!("snapshot missing {name}")).descriptor
    }

    fn object_id(descriptor: &SnapshotDescriptor) -> u32 {
        match descriptor {
            SnapshotDescriptor::Data {
                value: SnapshotValue::Object { id },
                ..
            } => *id,
            other => panic!("expected object-valued Date descriptor: {other:?}"),
        }
    }

    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let reads = Arc::new(HostReads::default());
    let outcome = Engine::new(realm(&reads))
        .observe_script_graph(
            r#"
(function(){
  const root=__scriptwCreateRealm().global,keys=Reflect.ownKeys,remove=Reflect.deleteProperty,collect=gc;
  globalThis.Date={replacement:true};
  Object.defineProperty(root,'Date',{writable:false});
  Object.setPrototypeOf(root,null);
  Object.setPrototypeOf(root.Function.prototype,null);
  delete root.Function.prototype.constructor;
  delete root.Object.prototype.constructor;
  for(const key of keys(root))if(key!=='Date')remove(root,key);
  collect();return root;
})();
"#,
            compile_options(),
            run_options(),
            SnapshotLimits::default(),
        )
        .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
        panic!("Date snapshot rejected: {:?}", outcome.completion.outcome);
    };
    let root = &graph.nodes()[0];
    assert_eq!(
        root.properties.len(),
        4,
        "Date and the three nonconfigurable scalar globals remain exposed"
    );
    for name in ["Date", "Infinity", "NaN", "undefined"] {
        property(root, name);
    }
    let descriptor = property(root, "Date");
    assert!(matches!(
        descriptor,
        SnapshotDescriptor::Data {
            writable: false,
            enumerable: false,
            configurable: true,
            ..
        }
    ));
    let constructor_id = object_id(descriptor);
    let constructor = &graph.nodes()[constructor_id as usize];
    let SnapshotObjectKind::Function {
        constructable: true,
        realm: date_realm,
    } = constructor.kind
    else {
        panic!("Date descriptor must resolve to its complete constructor");
    };
    assert!(
        matches!(property(constructor, "name"), SnapshotDescriptor::Data {
        value: SnapshotValue::String { units }, writable: false, enumerable: false, configurable: true
    } if units.units() == "Date".encode_utf16().collect::<Vec<_>>())
    );
    let prototype_descriptor = property(constructor, "prototype");
    assert!(matches!(
        prototype_descriptor,
        SnapshotDescriptor::Data {
            writable: false,
            enumerable: false,
            configurable: false,
            ..
        }
    ));
    let prototype = &graph.nodes()[object_id(prototype_descriptor) as usize];
    assert_eq!(prototype.kind, SnapshotObjectKind::Ordinary {});
    assert_eq!(
        object_id(property(prototype, "constructor")),
        constructor_id
    );
    let utc_id = object_id(property(prototype, "toUTCString"));
    assert_eq!(
        utc_id,
        object_id(property(prototype, "toGMTString")),
        "one recipe cache backs both aliases"
    );
    for name in ["toUTCString", "toGMTString"] {
        assert!(matches!(
            property(prototype, name),
            SnapshotDescriptor::Data {
                writable: true,
                enumerable: false,
                configurable: true,
                ..
            }
        ));
    }
    assert_eq!(
        graph.nodes()[utc_id as usize].kind,
        SnapshotObjectKind::Function {
            constructable: false,
            realm: date_realm
        }
    );
    assert_eq!(
        reads.wall.load(Ordering::Relaxed),
        0,
        "snapshot resolution is metadata-only"
    );
    assert!(outcome.output_events.is_empty());
}
