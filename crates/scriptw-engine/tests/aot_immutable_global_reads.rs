use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn check(source: &str, expected: &[&str]) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
    let source = format!(
        "function assert(value, message) {{ if (!value) throw new Error(message); }}\n{source}"
    );
    let outcome = Engine::new(RealmBuilder::new().build())
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
        .expect("immutable global reads execute through Wasm AOT");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "completion: {:?}; source:\n{source}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        expected
            .iter()
            .map(|line| HostOutputEvent::PrintLine((*line).into()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn source_and_named_function_parent_environments_read_canonical_values() {
    check(
        r#"
assert(undefined===void 0 && Number.isNaN(NaN) && Infinity===1/0,'direct source reads');
var read=(function namedOuter(){
  return function namedInner(){return [undefined,NaN,Infinity];};
})();
for(var i=0;i<8;i++){
  var values=read();
  assert(values[0]===void 0 && Number.isNaN(values[1]) && values[2]===1/0,'named closure parent reads');
}
gc();
assert(read()[0]===void 0 && read()[2]===1/0,'canonical source owner survives GC');
print('canonical');
"#,
        &["canonical"],
    );
}

#[test]
fn parameters_locals_and_named_function_bindings_shadow_immutable_globals() {
    check(
        r#"
var marker={id:17};
var parameter=(function(undefined,NaN,Infinity){
  return function(){return [undefined,NaN,Infinity];};
})(marker,23,29)();
assert(parameter[0]===marker && parameter[1]===23 && parameter[2]===29,'parameter shadows preserve whole values');
var local=(function(){
  var undefined=marker,NaN=31,Infinity=37;
  return (function nested(){return [undefined,NaN,Infinity];})();
})();
assert(local[0]===marker && local[1]===31 && local[2]===37,'local shadows survive named parent environment');
var named=(function undefined(){return undefined;})();
assert(typeof named==='function' && named()===named,'named function self binding shadows global undefined');
assert(undefined===void 0 && Number.isNaN(NaN) && Infinity===1/0,'shadows do not change source globals');
print('shadowed');
"#,
        &["shadowed"],
    );
}

#[test]
fn with_bindings_preserve_unscopables_and_value_getter_order() {
    check(
        r#"
var marker={},events=[],mask={},scope={},read;
Object.defineProperty(mask,'undefined',{get:function(){events.push('mask:undefined');return false;}});
Object.defineProperty(mask,'NaN',{get:function(){events.push('mask:NaN');return true;}});
Object.defineProperty(mask,'Infinity',{get:function(){events.push('mask:Infinity');return false;}});
Object.defineProperty(scope,Symbol.unscopables,{configurable:true,get:function(){events.push('unscopables');return mask;}});
Object.defineProperty(scope,'undefined',{get:function(){events.push('value:undefined');return marker;}});
Object.defineProperty(scope,'NaN',{get:function(){throw new Error('blocked value getter');}});
Object.defineProperty(scope,'Infinity',{get:function(){events.push('value:Infinity');return 41;}});
with(scope){read=function namedRead(){return [undefined,NaN,Infinity];};}
events.length=0;
var values=read();
assert(values[0]===marker && Number.isNaN(values[1]) && values[2]===41,'with bindings and blocked global fallback');
assert(events.join(',')==='unscopables,mask:undefined,value:undefined,unscopables,mask:NaN,unscopables,mask:Infinity,value:Infinity','lookup effects run in identifier order');
Object.defineProperty(scope,Symbol.unscopables,{get:function(){throw marker;}});
var caught=false;try{read();}catch(error){caught=error===marker;}
assert(caught,'unscopables abrupt completion is preserved');
print('with order');
"#,
        &["with order"],
    );
}

#[test]
fn lexical_shadows_throw_in_the_tdz_before_returning_initialized_values() {
    check(
        r#"
var marker={},failures=0;
{
  var readUndefined=function(){return undefined;};
  try{readUndefined();}catch(error){assert(error instanceof ReferenceError,'undefined TDZ');failures++;}
  try{typeof undefined;}catch(error){assert(error instanceof ReferenceError,'typeof undefined TDZ');failures++;}
  let undefined=marker;
  assert(readUndefined()===marker,'initialized lexical undefined preserves object');
}
{
  var readNaN=function(){return NaN;};
  try{readNaN();}catch(error){assert(error instanceof ReferenceError,'NaN TDZ');failures++;}
  let NaN=43;
  assert(readNaN()===43,'initialized lexical NaN');
}
{
  var readInfinity=function(){return Infinity;};
  try{readInfinity();}catch(error){assert(error instanceof ReferenceError,'Infinity TDZ');failures++;}
  const Infinity=47;
  assert(readInfinity()===47,'initialized lexical Infinity');
}
assert(failures===4 && undefined===void 0 && Number.isNaN(NaN) && Infinity===1/0,'all lexical reads retain TDZ and source scope');
print('tdz');
"#,
        &["tdz"],
    );
}

#[cfg(feature = "native-source-compiler")]
#[test]
fn runtime_global_lexical_insertion_and_eval_shadows_preserve_warmed_reads() {
    check(
        r#"
function readCanonical(){return (function named(){return [undefined,NaN,Infinity];})();}
function readLate(){return immutableReadLate;}
for(var i=0;i<4;i++)assert(readCanonical()[0]===void 0,'warm source read');
var missing=false;try{readLate();}catch(error){missing=error instanceof ReferenceError;}
assert(missing,'late global lexical initially absent');
__scriptwRealmEvalScript(['let immutableReadLate','=53;'].join(''));
gc();
var values=readCanonical();
assert(readLate()===53 && values[0]===void 0 && Number.isNaN(values[1]) && values[2]===1/0,'unrelated global lexical retains source lookup');
var shadowed=(0,eval)('let undefined={marker:59}; const NaN=61; let Infinity=67; [undefined,NaN,Infinity];');
assert(shadowed[0].marker===59 && shadowed[1]===61 && shadowed[2]===67,'eval lexical owner shadows canonical names');
var rejected=false;
try{__scriptwRealmEvalScript('let undefined=71;');}catch(error){rejected=error instanceof SyntaxError;}
assert(rejected && readCanonical()[0]===void 0,'restricted global lexical is rejected without changing reads');
print('late lexical');
"#,
        &["late lexical"],
    );
}

#[cfg(feature = "native-source-compiler")]
#[test]
fn foreign_source_owners_and_immutable_global_descriptors_remain_canonical() {
    check(
        r#"
var realm=__scriptwCreateRealm(),foreign=realm.global;
var read=realm.evalScript('(function foreignRead(){return (function nested(){return [undefined,NaN,Infinity];})();});');
var values=(function(undefined,NaN,Infinity){return read();})({},73,79);
assert(values[0]===void 0 && Number.isNaN(values[1]) && values[2]===1/0,'foreign source closure ignores caller shadows');
for(var owner of [globalThis,foreign]){
  for(var name of ['undefined','NaN','Infinity']){
    var descriptor=Object.getOwnPropertyDescriptor(owner,name);
    assert(!descriptor.writable && !descriptor.configurable && !descriptor.enumerable,'canonical descriptor attributes');
    assert(!Reflect.set(owner,name,{}) && !Reflect.deleteProperty(owner,name),'immutable property rejects write and deletion');
    assert(!Reflect.defineProperty(owner,name,{value:{}}),'immutable property rejects redefinition');
  }
}
gc();values=read();
assert(values[0]===foreign.undefined && Number.isNaN(values[1]) && values[2]===foreign.Infinity,'foreign source owner remains valid after rejected mutation');
print('foreign immutable');
"#,
        &["foreign immutable"],
    );
}
