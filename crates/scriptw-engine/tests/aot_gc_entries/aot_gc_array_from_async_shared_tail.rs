use super::aot_gc_array_from_async_entries::assert_async;

#[test]
fn all_source_modes_commit_each_awaited_mapping_once_and_keep_index_order_through_gc() {
    assert_async(
        r#"
function run(mode){
 var tokens=[{},{}],inputs=[],inputGets=0,inputCalls=0,mapGets=0,mapCalls=0,events=[],next=0;
 for(var i=0;i<2;i++)(function(index){inputs.push({get then(){inputGets++;gc();return function(resolve,reject){inputCalls++;gc();resolve(tokens[index]);resolve({});reject({});throw {};};}});})(i);
 var source;
 if(mode==='like')source={length:2,get 0(){events.push('get0');return inputs[0];},get 1(){events.push('get1');return inputs[1];}};
 else{source={};source[mode==='async'?Symbol.asyncIterator:Symbol.iterator]=function(){return {next:function(){var i=next++;if(i===2)return mode==='async'?Promise.resolve({done:true}):{done:true};events.push('get'+i);var row={done:false,value:inputs[i]};return mode==='async'?Promise.resolve(row):row;}};};}
 var receiver={},target;
 function C(){target=new Proxy({}, {defineProperty:function(object,key,desc){events.push('store'+key);assert(desc.writable&&desc.enumerable&&desc.configurable,'default data attributes');gc();return Reflect.defineProperty(object,key,desc);}});return target;}
 return Array.fromAsync.call(C,source,function(value,index){'use strict';assert(this===receiver&&arguments.length===2,'mapper call contract');assert(value===(mode==='async'?inputs[index]:tokens[index]),'source-specific Await policy');events.push('map'+index);return {get then(){mapGets++;gc();return function(resolve,reject){mapCalls++;gc();resolve(tokens[index]);resolve({});reject({});throw {};};}};},receiver).then(function(result){
  gc();assert(result===target&&result.length===2&&result[0]===tokens[0]&&result[1]===tokens[1],'one commit per fulfilled value');
  assert(events.join(',')==='get0,map0,store0,get1,map1,store1,storelength','source Get, mapping and publication order');
  assert(inputGets===(mode==='async'?0:2)&&inputCalls===inputGets&&mapGets===2&&mapCalls===2,'thenables are observed exactly once');
  assert(next===(mode==='like'?0:3),'no duplicate drive');
 });
}
run('like').then(function(){return run('sync');}).then(function(){return run('async');}).then(function(){print('shared-await-commit-order');});
"#,
        "shared-await-commit-order",
    );
}

#[test]
fn resumed_mapping_failures_close_once_keep_prefix_and_preserve_original_error() {
    assert_async(
        r#"
function run(mode,rejectMap,closeKind){
 var marker={},closeMarker={},next=0,gets=0,calls=0,stores=[],target={},source={};
 source[mode==='async'?Symbol.asyncIterator:Symbol.iterator]=function(){
  var iterator={next:function(){var row={done:false,value:next++};return mode==='async'?Promise.resolve(row):row;}};
  Object.defineProperty(iterator,'return',{get:function(){gets++;gc();if(closeKind==='get')throw closeMarker;return function(){calls++;gc();if(closeKind==='throw')throw closeMarker;return mode==='async'?Promise.reject(closeMarker):{done:true,value:Promise.reject(closeMarker)};};}});
  return iterator;
 };
 function C(){return new Proxy(target,{defineProperty:function(object,key,desc){stores.push(key);return Reflect.defineProperty(object,key,desc);}});}
 return Array.fromAsync.call(C,source,function(value,index){assert(index===value,'resumed index');gc();if(index===0)return Promise.resolve('prefix');if(rejectMap)return Promise.reject(marker);throw marker;}).then(function(){throw new Error('missing mapping failure');},function(error){
  gc();assert(error===marker,'mapping error outranks every close error');assert(gets===1&&calls===(closeKind==='get'?0:1)&&next===2,'close once, never drive after failure');
  assert(stores.join(',')==='0'&&target[0]==='prefix'&&!('1'in target)&&!('length'in target),'no duplicate or late publication');
 });
}
run('async',true,'get').then(function(){return run('async',false,'reject');}).then(function(){return run('sync',true,'throw');}).then(function(){return run('sync',false,'reject');}).then(function(){print('shared-close-precedence');});
"#,
        "shared-close-precedence",
    );
}

#[test]
fn resumed_foreign_realm_continuations_keep_result_and_error_realms_after_gc() {
    assert_async(
        r#"
var other=$262.createRealm().global,token=new other.Object(),receiver={},maps=0;
var promise=other.Array.fromAsync.call({}, {0:Promise.resolve(token),length:1},function(value,index){'use strict';gc();assert(this===receiver&&value===token&&index===0,'foreign resumed mapper');maps++;return other.Promise.resolve(value);},receiver);
assert(promise instanceof other.Promise&&!(promise instanceof Promise),'defining Realm Promise');
promise.then(function(result){
 gc();assert(result instanceof other.Array&&!(result instanceof Array)&&result[0]===token&&maps===1,'defining Realm Array and live value');
 var gets=0,calls=0,steps=0,source={},target=Object.preventExtensions({});
 source[Symbol.asyncIterator]=function(){var iterator={next:function(){steps++;return Promise.resolve({done:false,value:token});}};Object.defineProperty(iterator,'return',{get:function(){gets++;return function(){calls++;gc();return Promise.resolve({done:true});};}});return iterator;};
 function C(){return target;}
 return other.Array.fromAsync.call(C,source).then(function(){throw new Error('missing definition error');},function(error){
  gc();assert(error instanceof other.TypeError&&!(error instanceof TypeError),'postAwait commit error uses defining Realm');
  assert(gets===1&&calls===1&&steps===1&&Reflect.ownKeys(target).length===0,'failed un-mapped async commit closes once');
  print('shared-tail-foreign-realm');
 });
});
"#,
        "shared-tail-foreign-realm",
    );
}
