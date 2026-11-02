use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

fn compile_options() -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    }
}

fn assert_modes(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{source}");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                compile_options(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(60_000),
                    ..RunOptions::default()
                },
            )
            .unwrap_or_else(|error| panic!("RegExp demand initialization: {error}\n{source}"));
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
    }
}

#[test]
fn first_regexp_descriptor_completes_canonical_methods_and_shared_legacy_captures() {
    assert_modes(
        r#"
function check(value,label){if(!value)throw label;}
var constructors=[],methods=[];
for(var global of [globalThis,__scriptwCreateRealm().global]){
  var before=Reflect.ownKeys(global).join('|');
  check(Object.hasOwn(global,'RegExp') && 'RegExp' in global &&
    !Object.prototype.propertyIsEnumerable.call(global,'RegExp'),'unobserved global membership');
  gc();
  var descriptor=Object.getOwnPropertyDescriptor(global,'RegExp'),C=descriptor.value,P=C.prototype;
  check(C===global.RegExp && typeof C==='function' && descriptor.writable &&
    !descriptor.enumerable && descriptor.configurable && !Object.hasOwn(descriptor,'get'),
    'first descriptor retains ordinary data attributes');
  check(C.name==='RegExp' && C.length===2 && P.constructor===C &&
    Object.getPrototypeOf(C)===global.Function.prototype &&
    Object.getPrototypeOf(P)===global.Object.prototype,'completed defining-Realm cycle');
  var prototype=Object.getOwnPropertyDescriptor(C,'prototype');
  check(prototype.value===P && !prototype.writable && !prototype.enumerable &&
    !prototype.configurable,'constructor prototype descriptor');
  for(var pair of [['input','$_'],['lastMatch','$&'],['lastParen','$+'],
    ['leftContext','$`'],['rightContext',"$'"]]){
    var first=Object.getOwnPropertyDescriptor(C,pair[0]);
    var alias=Object.getOwnPropertyDescriptor(C,pair[1]);
    check(first.get===alias.get && first.set===alias.set &&
      first.get.name==='get '+pair[0] && first.get.length===0 &&
      Object.getPrototypeOf(first.get)===global.Function.prototype &&
      !first.enumerable && first.configurable,'shared legacy getter '+pair[0]);
    if(pair[0]==='input')check(typeof first.set==='function' && first.set.name==='set input' &&
      first.set.length===1,'one shared input setter');
    else check(first.set===undefined,'getter-only legacy slot');
    check(C[pair[0]]==='' && C[pair[1]]==='','initial empty legacy state');
  }
  for(var symbol of [Symbol.match,Symbol.matchAll,Symbol.search]){
    var method=Object.getOwnPropertyDescriptor(P,symbol);
    check(typeof method.value==='function' && method.value===P[symbol] && method.writable &&
      !method.enumerable && method.configurable &&
      Object.getPrototypeOf(method.value)===global.Function.prototype,'canonical symbol callable');
  }
  check(P.source==='(?:)' && P.flags==='','prototype accessors see the complete family');
  var input=Object.getOwnPropertyDescriptor(C,'input');
  input.set.call(C,{toString(){gc();return 'assigned';}});
  check(input.get.call(C)==='assigned' && C.$_==='assigned','captured input state survives GC');
  check(Reflect.ownKeys(global).join('|')===before,'demand preserves global key order');
  constructors.push(C);methods.push(P[Symbol.match]);
}
gc();
check(constructors[0]!==constructors[1] && constructors[0].prototype!==constructors[1].prototype &&
  methods[0]!==methods[1],'separate canonical identities per Realm');
print('ok');262;
"#,
    );
}

#[test]
fn literal_and_internal_creation_ignore_unread_public_regexp_mutations() {
    assert_modes(
        r#"
function check(value,label){if(!value)throw label;}
check(Reflect.deleteProperty(globalThis,'RegExp'),'delete before literal demand');
gc();
var literal=/(a)(b)/g,P=Object.getPrototypeOf(literal),C=P.constructor;
var lastIndex=Object.getOwnPropertyDescriptor(literal,'lastIndex');
check(!Object.hasOwn(globalThis,'RegExp') && C.name==='RegExp' && C.prototype===P &&
  lastIndex.value===0 && lastIndex.writable && !lastIndex.enumerable && !lastIndex.configurable,
  'literal creates the private graph without resurrecting the public binding');
check(literal.exec('zabx')[0]==='ab' && C.$1==='a' && C.$2==='b' && C.lastMatch==='ab' &&
  C.leftContext==='z' && C.rightContext==='x','first literal match updates legacy captures');
for(var mode of ['delete','assign','accessor']){
  var global=__scriptwCreateRealm().global,marker={},gets=0,sets=0;
  var match=global.String.prototype.match,target=global.Array.bind(null);
  Object.defineProperty(target,'prototype',{value:17});
  if(mode==='delete')delete global.RegExp;
  else if(mode==='assign')global.RegExp=marker;
  else Object.defineProperty(global,'RegExp',{
    get(){gets++;throw 'public RegExp getter';},set(value){sets++;},configurable:true});
  gc();
  var result=match.call('zabx','(a)(b)');
  check(result[0]==='ab' && result[1]==='a' && result[2]==='b' && result.index===1 &&
    Object.getPrototypeOf(result)===global.Array.prototype && gets===0 && sets===0,
    'borrowed String match privately demands its defining Realm');
  var foreignRx=Reflect.construct(C,['z'],target),foreignP=Object.getPrototypeOf(foreignRx),
    foreignC=foreignP.constructor;
  check(foreignC!==C && foreignC.prototype===foreignP &&
    Object.getPrototypeOf(foreignC)===global.Function.prototype && foreignC.$1==='a' &&
    foreignC.$2==='b','newTarget default selects the same private foreign graph');
  var getter=Object.getOwnPropertyDescriptor(foreignC,'input').get,caught;
  try{getter.call(C);}catch(error){caught=error;}
  check(Object.getPrototypeOf(caught)===global.TypeError.prototype,'legacy brand error uses getter Realm');
  gc();
  var descriptor=Object.getOwnPropertyDescriptor(global,'RegExp');
  if(mode==='delete')check(descriptor===undefined && !('RegExp' in global),'deleted binding stays absent');
  else if(mode==='assign')check(descriptor.value===marker,'replacement stays installed');
  else check(typeof descriptor.get==='function' && typeof descriptor.set==='function',
    'replacement accessor stays installed');
  check(gets===0 && sets===0 && foreignP.constructor===foreignC,'private reads do not invoke public hooks');
}
print('ok');262;
"#,
    );
}

#[test]
fn native_source_first_demand_and_later_programs_keep_the_defining_regexp_graph() {
    assert_modes(
        r#"
function check(value,label){if(!value)throw label;}
for(var route of ['literal','function','program']){
  var realm=__scriptwCreateRealm(),global=realm.global,evaluate=global.eval,make=global.Function;
  delete global.RegExp;gc();
  var rx;
  if(route==='literal')rx=evaluate('/(q)/g');
  else if(route==='function')rx=make('return /(q)/g;')();
  else rx=evaluate('var first=/(q)/g;first;');
  var P=Object.getPrototypeOf(rx),C=P.constructor,exec=P.exec;
  check(C.name==='RegExp' && C.prototype===P && Object.getPrototypeOf(C)===global.Function.prototype &&
    !Object.hasOwn(global,'RegExp'),'first native source creation owns the captured Realm');
  check(exec.call(rx,'xqz')[0]==='q' && C.$1==='q' && C.input==='xqz',
    'first native source match updates its own legacy state');
  global.savedC=C;global.savedRx=rx;global.savedExec=exec;
  var input=Object.getOwnPropertyDescriptor(C,'input').get;
  global.eval=function(){throw 'replaced public eval';};
  realm.evalScript=function(){throw 'replaced evalScript';};
  delete P.exec;gc();
  var later=evaluate('savedRx.lastIndex=0;[savedC,savedExec,savedExec.call(savedRx,"aqb")[0],savedC.$1,Object.hasOwn(globalThis,"RegExp")];');
  check(later[0]===C && later[1]===exec && later[2]==='q' && later[3]==='q' && later[4]===false &&
    input.call(C)==='aqb' && !Object.hasOwn(P,'exec'),
    'later compiled program retains private graph and captured callable after public mutation and GC');
}
print('ok');262;
"#,
    );
}

#[test]
fn retained_embedding_calls_demand_once_and_survive_public_deletion_and_gc() {
    use scriptw_engine::{EmbeddedEvaluation, EmbeddedRuntime, JsCompletion, JsValue};

    fn normal(completion: JsCompletion) -> JsValue {
        match completion {
            JsCompletion::Normal(value) => value,
            JsCompletion::Throw(value) => panic!("retained RegExp operation threw: {value:?}"),
        }
    }

    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(
            r#"
const marker={};
globalThis.RegExp=marker;
let saved;
export function demand(){saved=/(a)/;return Object.getPrototypeOf(saved).constructor;}
export function inspect(C,exec){
  return C===Object.getPrototypeOf(saved).constructor && C.prototype.constructor===C &&
    C!==marker && globalThis.RegExp===marker && exec===C.prototype.exec;
}
export function mutate(){delete globalThis.RegExp;delete Object.getPrototypeOf(saved).exec;gc();}
export function useRetained(C,exec){
  return exec.call(saved,'xab')[0]==='a' && C.$1==='a' && C.input==='xab' &&
    !Object.hasOwn(globalThis,'RegExp') && !Object.hasOwn(C.prototype,'exec');
}
"#,
            compile_options(),
        )
        .unwrap();
    let artifact = compiler.emit_wasm(&unit).unwrap();
    let mut config = wasmtime::Config::new();
    scriptw_engine::configure_wasm_runtime(&mut config);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let runtime = EmbeddedRuntime::compile(&engine, &artifact).unwrap();
    let state = runtime.new_state(RealmBuilder::new().build(), ()).unwrap();
    let mut store = wasmtime::Store::new(&engine, state);
    store.limiter(|state| state.limits_mut());
    store.set_epoch_deadline(u64::MAX / 2);
    let mut linker = wasmtime::Linker::new(&engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    let demand = normal(instance.get_export(&mut store, "demand").unwrap());
    store.gc(None).unwrap();
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
    let exec = normal(
        instance
            .get_property(
                &mut store,
                &prototype,
                &"exec".encode_utf16().collect::<Vec<_>>(),
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
                    &[constructor.clone(), exec.clone()]
                )
                .unwrap()
        )
        .as_boolean(),
        Some(true)
    );
    let mutate = normal(instance.get_export(&mut store, "mutate").unwrap());
    normal(instance.call(&mut store, &mutate, None, &[]).unwrap());
    store.gc(None).unwrap();
    let retained = normal(instance.get_export(&mut store, "useRetained").unwrap());
    assert_eq!(
        normal(
            instance
                .call(&mut store, &retained, None, &[constructor, exec])
                .unwrap()
        )
        .as_boolean(),
        Some(true)
    );
}
