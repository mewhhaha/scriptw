//! Guarded immutable returns retain actual callable identity and source entry rules.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, RealmBuilder, RunOptions,
};

fn assert_constant_leaf(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
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
        .unwrap_or_else(|error| panic!("constant leaf execution failed: {error}\n{source}"));
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        outcome.note.contains("boolean(true)"),
        "{}\n{source}",
        outcome.note
    );
}

#[test]
fn primitive_constant_returns_keep_utf16_values_identity_and_construction() {
    assert_constant_leaf(
        r#"
function text(){return 'a\ud800'+'\udc00b'+'\u0000c';}
function number(){return 9007199254740991;}
function yes(){return true;}
function no(){return false;}
function nil(){return null;}
function absent(){return;}
function negativeZero(){return -0;}
function notNumber(){return NaN;}
var value=text(), again=text();
var created=new text();
gc();
value==='a\ud800\udc00b\u0000c' && again===value && value.length===6 &&
value.charCodeAt(1)===55296 && value.charCodeAt(2)===56320 && value.charCodeAt(4)===0 &&
number()===9007199254740991 && yes()===true && no()===false && nil()===null && absent()===undefined &&
Object.is(negativeZero(),-0) && Number.isNaN(notNumber()) &&
Object.getPrototypeOf(created)===text.prototype && text.length===0 && text.name==='text' &&
text.toString().includes('return') && text.arguments===null && text.caller===null;
"#,
    );
}

#[test]
fn constant_leaf_calls_keep_patched_bound_proxy_and_once_evaluated_argument_paths() {
    assert_constant_leaf(
        r#"
function text(){return 'original';}
function invoke(){var value=text();return value;}
var original=text,events='';
var first=invoke();
text=function(){events+='M';return this.mark;};
globalThis.mark='patched';
var changed=invoke();
text=original.bind(null);
var bound=invoke();
text=new Proxy(original,{apply(target,self,args){events+='P';return Reflect.apply(target,self,args)+'!';}});
var proxied=invoke();
var receiver={mark:'receiver'};
var called=text.call(receiver);
text=original;
var extras=text((events+='L',gc(),1),(events+='R',2));
var spread=text(...{[Symbol.iterator](){events+='I';return [1,2][Symbol.iterator]();}});
var marker={},abrupt=false;
function stop(){events+='T';throw marker;}
try{text(stop());}catch(error){abrupt=error===marker;}
first==='original' && changed==='patched' && bound==='original' && proxied==='original!' &&
called==='original!' && extras==='original' && spread==='original' && abrupt && events==='MPPLRIT';
"#,
    );
}

#[test]
fn constant_leaf_fallbacks_keep_formals_effects_captures_and_execution_protocols() {
    assert_constant_leaf(
        r#"
var events='';
function defaults(value=(events+='D')){return 'default';}
function rest(...values){return 'rest';}
function argumentsObserver(){return arguments.length;}
function callerObserver(){return callerObserver.caller;}
function observeCaller(){var value=callerObserver();return value===observeCaller;}
function outer(value){return function captured(){return value;};}
function effect(){events+='E';return 'effect';}
function object(){return {};}
function* generated(){return 'generator';}
async function asynchronous(){return 'async';}
class Classed {constructor(){return 'class';}}
var captured=outer('captured'),first=object(),second=object();
var iterator=generated(),step=iterator.next();
var illegal=false;
try{Classed();}catch(error){illegal=error instanceof TypeError;}
var instance=new Classed();
var promise=asynchronous();
defaults()==='default' && rest()==='rest' && argumentsObserver(1,2)===2 && observeCaller() &&
captured()==='captured' && effect()==='effect' && events==='DE' && first!==second &&
step.done && step.value==='generator' && illegal && Object.getPrototypeOf(instance)===Classed.prototype &&
promise instanceof Promise;
"#,
    );
}

#[test]
fn constant_leaf_code_and_literal_pool_belong_to_the_actual_program_and_realm() {
    assert_constant_leaf(
        r#"
function text(){return 'local\ud800';}
function invoke(){var value=text();return value;}
var original=text;
var foreign=__scriptwCreateRealm().global;
var other=foreign.eval('(function text(){return "foreign\\udc00";})');
text=other;
var borrowed=invoke();
var source='function text(){return "another-program\\u0000";}';
(0,eval)(source);
var replaced=invoke();
gc();
borrowed==='foreign\udc00' && replaced==='another-program\u0000' && original()==='local\ud800' &&
other()===borrowed && Object.getPrototypeOf({})===Object.prototype && original.arguments===null;
"#,
    );
}

#[test]
fn constant_leaf_source_depth_guard_matches_a_generic_call_and_recovers_after_throw() {
    assert_constant_leaf(
        r#"
function constant(){return 'token';}
function generic(){var unused=0;return 'token';}
var count=0;
function dive(selected){
  var value;
  if(selected)value=constant();else value=generic();
  if(value!=='token')throw 'value';
  count++;
  dive(selected);
}
function attempt(selected){
  count=0;
  try{dive(selected);}catch(error){if(Object.getPrototypeOf(error)!==RangeError.prototype)throw error;}
  return count;
}
var specialized=attempt(true),ordinary=attempt(false),repeated=attempt(true);
specialized>0 && specialized===ordinary && repeated===ordinary && constant()==='token' &&
constant.arguments===null && constant.caller===null && dive.arguments===null && dive.caller===null;
"#,
    );
}
