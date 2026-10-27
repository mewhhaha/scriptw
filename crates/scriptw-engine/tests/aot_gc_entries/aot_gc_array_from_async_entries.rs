use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

pub(super) fn assert_async(source: &str, marker: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
    for strict in [false, true] {
        let source = format!(
            "{}var $262={{createRealm:__scriptwCreateRealm}};function assert(v,m){{if(!v)throw new Error(m);}}\n{source}",
            if strict { "\"use strict\";\n" } else { "" }
        );
        let observed = Engine::new(RealmBuilder::new().build())
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
            .expect("real Array.fromAsync compiles through Wasm AOT");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            observed.completion
        );
        // A pending or failed async assertion cannot silently pass this control.
        assert_eq!(
            observed.output_events,
            vec![HostOutputEvent::PrintLine(marker.into())]
        );
    }
}

#[test]
fn array_like_values_and_mapper_results_are_awaited_in_order() {
    assert_async(
        r#"
var log=[],token={},symbol=Symbol('v'),source={get length(){log.push('length');return 3;},get 0(){log.push('get0');return {then:function(resolve){log.push('await0');resolve(token);}};},get 1(){log.push('get1');return symbol;},get 2(){log.push('get2');return 7n;}};
Object.defineProperty(source,Symbol.asyncIterator,{get:function(){log.push('async');return undefined;}});
Object.defineProperty(source,Symbol.iterator,{get:function(){log.push('sync');return undefined;}});
var receiver={};
Array.fromAsync.call({},source,function(value,index){'use strict';assert(this===receiver && arguments.length===2,'mapper Call');log.push('map'+index);return Promise.resolve(value);},receiver).then(function(result){
 assert(result.length===3 && result[0]===token && result[1]===symbol && result[2]===7n,'whole awaited values');
 assert(log.join(',')==='async,sync,length,get0,await0,map0,get1,map1,get2,map2','sequential observations');
 print('arraylike-order');
});
"#,
        "arraylike-order",
    );
}

#[test]
fn iterator_acquisition_precedes_construct_and_array_like_length_is_cached() {
    assert_async(
        r#"
var log=[],target,source={};
source[Symbol.asyncIterator]=function(){log.push('iterator');var count=0;return {get next(){log.push('next-get');return function(){log.push('next');return Promise.resolve(count++ ? {done:true}:{done:false,value:5});};}};};
function C(){log.push('construct:'+arguments.length);target=this;}
Array.fromAsync.call(C,source).then(function(result){
 assert(result===target && target[0]===5 && target.length===1,'custom iterable target');
 assert(log.join(',')==='iterator,next-get,construct:0,next,next','acquire before constructor');
 log=[];var gets=0,like={get length(){gets++;log.push('length');return 2;},0:8,1:9};
 function D(length){log.push('construct:'+length+':'+arguments.length);like[2]=10;}
 return Array.fromAsync.call(D,like).then(function(result){assert(gets===1 && result.length===2 && !(2 in result),'cached length');assert(log.join(',')==='length,construct:2:1','length before custom target');print('acquisition-order');});
});
"#,
        "acquisition-order",
    );
}

#[test]
fn async_iterator_values_remain_whole_until_mapping_and_done_omits_value() {
    assert_async(
        r#"
var token={},wrapped=Promise.resolve(token),gets=0,source={};
source[Symbol.asyncIterator]=function(){var index=0;return {get next(){gets++;return function(){return Promise.resolve(index++ ? {get done(){return true;},get value(){throw new Error('final value Get');}}:{done:false,value:wrapped});};}};};
Array.fromAsync(source).then(function(result){
 assert(result[0]===wrapped && result.length===1 && gets===1,'unmapped async value retained');
 return Array.fromAsync(source,function(value,index){assert(value===wrapped && index===0,'mapper receives unawaited value');return Promise.resolve(7n);});
}).then(function(result){assert(result[0]===7n && gets===2,'mapped result awaited once');print('async-value-policy');});
"#,
        "async-value-policy",
    );
}

#[test]
fn sync_done_values_are_awaited_and_value_rejection_closes_once() {
    assert_async(
        r#"
var log=[],source={};source[Symbol.iterator]=function(){return {next:function(){return {done:true,get value(){log.push('done-value');return {then:function(resolve){log.push('done-await');resolve(0);}};}};}};};
Array.fromAsync(source).then(function(result){
 assert(result.length===0 && log.join(',')==='done-value,done-await','sync done value awaited');
 var marker={},closes=0,iterable={};iterable[Symbol.iterator]=function(){return {next:function(){return {done:false,value:Promise.reject(marker)};},return:function(){closes++;return {done:true};}};};
 return Array.fromAsync(iterable).then(function(){throw new Error('missing rejection');},function(error){assert(error===marker && closes===1,'underlying close only once');print('sync-value-policy');});
});
"#,
        "sync-value-policy",
    );
}

#[test]
fn mapper_and_definition_failures_await_close_and_keep_the_original_error() {
    assert_async(
        r#"
var marker={},closeMarker={},log=[],source={};
source[Symbol.asyncIterator]=function(){return {next:function(){return Promise.resolve({done:false,value:1});},return:function(){log.push('return');return {then:function(resolve,reject){log.push('close-await');reject(closeMarker);}};}};};
Array.fromAsync(source,function(){return Promise.reject(marker);}).then(function(){throw new Error('missing mapper rejection');},function(error){
 assert(error===marker && log.join(',')==='return,close-await','original mapper rejection after awaited close');
 var closes=0,target={},defineMarker={},values={};Object.defineProperty(target,'0',{value:9,writable:false,configurable:false});
 values[Symbol.iterator]=function(){return {next:function(){return {done:false,value:5};},return:function(){closes++;throw defineMarker;}};};
 function C(){return target;}
 return Array.fromAsync.call(C,values).then(function(){throw new Error('missing define rejection');},function(error){assert(error instanceof TypeError && closes===1 && target[0]===9,'failed definition closes and retains error');print('close-error-policy');});
});
"#,
        "close-error-policy",
    );
}

#[test]
fn entry_and_final_length_errors_reject_the_defining_realm_promise() {
    assert_async(
        r#"
var other=$262.createRealm().global,reads=0,source={get length(){reads++;throw new Error('late length');}};
var rejected=other.Array.fromAsync.call({},source,0);
assert(rejected instanceof other.Promise && !(rejected instanceof Promise) && reads===0,'defining Realm intrinsic Promise before mapper admission');
rejected.then(function(){throw new Error('missing mapper rejection');},function(error){
 assert(error instanceof other.TypeError && !(error instanceof TypeError),'defining Realm mapper error');
 var token={},count=0,iterable={};iterable[Symbol.asyncIterator]=function(){return {next:function(){return Promise.resolve({done:true});},return:function(){count++;return {};}};};
 function C(){return new Proxy({}, {set:function(target,key,value){if(key==='length')throw token;return true;}});}
 return other.Array.fromAsync.call(C,iterable).then(function(){throw new Error('missing length rejection');},function(error){assert(error===token && count===0,'final length failure has original identity and no close');print('realm-rejection');});
});
"#,
        "realm-rejection",
    );
}

#[test]
fn async_iterator_protocol_failures_reject_without_observing_return() {
    assert_async(
        r#"
function checkFailure(kind){
 var marker={},nextCalls=0,returnGets=0,returnCalls=0,source={};
 source[Symbol.asyncIterator]=function(){
  var iterator={next:function(){
   nextCalls++;
   if(kind==='call')throw marker;
   if(kind==='await')return Promise.reject(marker);
   if(kind==='done')return Promise.resolve({get done(){throw marker;},get value(){throw new Error('value after done error');}});
   return Promise.resolve({done:false,get value(){throw marker;}});
  }};
  Object.defineProperty(iterator,'return',{get:function(){returnGets++;return function(){returnCalls++;return Promise.resolve({done:true});};}});
  return iterator;
 };
 return Array.fromAsync(source).then(function(){throw new Error('missing '+kind+' rejection');},function(error){
  assert(error===marker && nextCalls===1 && returnGets===0 && returnCalls===0,'protocol cutoff '+kind);
 });
}
Promise.resolve().then(function(){return checkFailure('call');}).then(function(){return checkFailure('await');}).then(function(){return checkFailure('done');}).then(function(){return checkFailure('value');}).then(function(){print('async-protocol-cutoffs');});
"#,
        "async-protocol-cutoffs",
    );
}

#[test]
fn synchronous_await_setup_failures_close_only_after_mapping() {
    assert_async(
        r#"
var mapperMarker={},mapperPromise=Promise.resolve(5),mapperConstructorGets=0,mapperCloseGets=0,mapperCloseCalls=0,nextCalls=0,source={};
Object.defineProperty(mapperPromise,'constructor',{get:function(){mapperConstructorGets++;throw mapperMarker;}});
source[Symbol.asyncIterator]=function(){
 var iterator={next:function(){nextCalls++;return Promise.resolve({done:false,value:3});}};
 Object.defineProperty(iterator,'return',{get:function(){mapperCloseGets++;return function(){mapperCloseCalls++;return Promise.resolve({done:true});};}});
 return iterator;
};
Array.fromAsync(source,function(value,index){assert(value===3 && index===0,'mapper arguments before Await');return mapperPromise;}).then(function(){throw new Error('missing mapper Await setup rejection');},function(error){
 assert(error===mapperMarker && mapperConstructorGets===1 && mapperCloseGets===1 && mapperCloseCalls===1 && nextCalls===1,'mapper Await setup closes once');
 var iteratorMarker={},nextPromise=Promise.resolve({done:false,value:8}),nextConstructorGets=0,returnGets=0,iterable={};
 Object.defineProperty(nextPromise,'constructor',{get:function(){nextConstructorGets++;throw iteratorMarker;}});
 iterable[Symbol.asyncIterator]=function(){
  var iterator={next:function(){return nextPromise;}};
  Object.defineProperty(iterator,'return',{get:function(){returnGets++;throw new Error('unexpected close Get');}});
  return iterator;
 };
 return Array.fromAsync(iterable).then(function(){throw new Error('missing iterator Await setup rejection');},function(error){
  assert(error===iteratorMarker && nextConstructorGets===1 && returnGets===0,'iterator Await setup rejects without close');
  print('await-setup-cutoffs');
 });
});
"#,
        "await-setup-cutoffs",
    );
}
