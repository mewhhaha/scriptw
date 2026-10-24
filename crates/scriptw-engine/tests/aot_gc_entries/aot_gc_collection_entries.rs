use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion, RealmBuilder,
    RunOptions,
};
fn assert_collections(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
    for strict in [false, true] {
        let source = format!(
            "{}var $262={{createRealm:__scriptwCreateRealm,gc:gc}};function assert(v,m){{if(!v)throw new Error(m);}}\n{source}",
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
            .expect("collection control uses Wasm AOT");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            observed.completion
        );
    }
}
#[test]
fn string_hashes_preserve_content_across_properties_collections_and_gc() {
    assert_collections(include_str!(
        "../../../scriptw-cli/tests/fixtures/wasm_cached_string_hash.js"
    ));
}
#[test]
fn keyed_equality_keeps_content_identity_nan_and_positive_zero() {
    assert_collections(
        r#"
var object={}, other={}, symbol=Symbol('same'), otherSymbol=Symbol('same');
var map=new Map([[NaN,1],[-0,2],['ab',3],[1n,4],[object,5],[symbol,6]]);
assert(map.get(0)===2 && map.get(Number('bad'))===1,'SameValueZero');
assert(map.get('a'+'b')===3 && map.get(BigInt('1'))===4,'content hashes');
assert(map.get(object)===5 && !map.has(other) && map.get(symbol)===6 && !map.has(otherSymbol),'identities');
assert(map.size===6 && 1/Array.from(map.keys())[1]===Infinity,'canonical +0');
var roots=[], set=new Set();
for(var i=0;i<80;i++){var key={i:i};roots.push(key);set.add(key);map.set(key,i);}
for(var i=0;i<40;i++){map.delete(roots[i]);set.delete(roots[i]);}
$262.gc();
for(var i=40;i<80;i++)assert(map.get(roots[i])===i && set.has(roots[i]),'growth/unlink roots');
assert(map.get(object)===5 && map.get('ab')===3,'surviving distinct hash domains');
assert(new Set([-0,0,NaN,Number('bad')]).size===2,'Set numeric equality');
"#,
    );
}
#[test]
fn live_cursors_keep_history_through_clear_growth_and_reinsertion() {
    assert_collections(
        r#"
var map=new Map([[1,'a'],[2,'b'],[3,'c']]), cursor=map.entries();
assert(cursor.next().value.join(',')==='1,a','first');map.delete(2);map.set(1,'updated');map.set(4,'d');
assert(cursor.next().value.join(',')==='3,c','skip deleted');map.clear();map.set(5,'e');
for(var i=6;i<24;i++)map.set(i,String(i));$262.gc();
assert(cursor.next().value.join(',')==='5,e','Clear preserves history');
for(var i=6;i<24;i++)assert(cursor.next().value[0]===i,'fresh grown table');
assert(cursor.next().done,'exhaustion');map.set(25,'later');assert(cursor.next().done,'exhaustion is permanent');
var set=new Set([1,2]), it=set.values();assert(it.next().value===1,'Set first');set.delete(2);set.add(2);set.clear();set.add(3);
assert(it.next().value===3 && it.next().done,'Set Clear and insertion');
var pair=new Set([{}]).entries().next().value;assert(pair[0]===pair[1],'Set pair identity');
"#,
    );
}
#[test]
fn foreach_observes_live_mutation_whole_arguments_and_throws() {
    assert_collections(
        r#"
var map=new Map([[1,'a'],[2,'b']]), seen=[], context={};
map.forEach(function(v,k,m){assert(this===context && m===map,'receiver');seen.push(k+':'+v);if(k===1 && v==='a'){map.delete(2);map.set(3,'c');map.delete(1);map.set(1,'new');}else if(k===3)map.set(4,'d');},context);
assert(seen.join(',')==='1:a,3:c,1:new,4:d','live ordered callback mutation');
var set=new Set([1,2]), order=[];set.forEach(function(v,k,s){assert(v===k && s===set,'Set args');order.push(v);if(v===1){set.clear();set.add(3);}});
assert(order.join(',')==='1,3','Set live Clear');
var token={};try{map.forEach(function(){throw token;});throw new Error('missing throw');}catch(e){assert(e===token,'whole callback throw');}
"#,
    );
}
#[test]
fn constructors_acquire_adders_before_iterators_and_close_owned_failures() {
    assert_collections(
        r#"
var order=[], token={}, closeCount=0;
var iterable={[Symbol.iterator]:function(){order.push('iterator');return{next:function(){order.push('next');return{value:{get 0(){order.push('key');return 1;},get 1(){order.push('value');throw token;}},done:false};},return:function(){closeCount++;throw new Error('secondary');}};}};
function Target(){};
Target.prototype=Object.create(Map.prototype,{set:{get:function(){order.push('set');return Map.prototype.set;}}});
try{Reflect.construct(Map,[iterable],Target);throw new Error('missing entry throw');}catch(e){assert(e===token && closeCount===1,'original Throw wins Close');}
assert(order.join(',')==='set,iterator,next,key,value','construction order');
var called=0;Target.prototype=Object.create(Map.prototype,{set:{value:null}});
try{Reflect.construct(Map,[{get [Symbol.iterator](){called++;throw token;}}],Target);throw new Error('bad adder');}catch(e){assert(e instanceof TypeError && called===0,'adder before iterator');}
assert(Reflect.construct(Map,[null],Target).size===0,'nullish skips adder');
var stepClose=0;try{new Set({[Symbol.iterator]:function(){return{next:function(){throw token;},return:function(){stepClose++;return{};}};}});}catch(e){assert(e===token && stepClose===0,'step failure has no Close');}
"#,
    );
}
#[test]
fn get_or_insert_validates_callback_and_reprobes_after_reentrant_changes() {
    assert_collections(
        r#"
var token={}, map=new Map([[1,undefined]]), calls=0;
assert(map.getOrInsert(1,token)===undefined && map.size===1,'existing undefined');
try{map.getOrInsertComputed(1,null);throw new Error('invalid callback');}catch(e){assert(e instanceof TypeError,'validation before existing lookup');}
var value=map.getOrInsertComputed(-0,function(k){calls++;assert(1/k===Infinity,'canonical key');map.clear();map.set(0,'inner');map.set(2,'other');$262.gc();return token;});
assert(value===token && map.get(0)===token && calls===1 && map.size===2,'computed value replaces callback insertion');
assert(Array.from(map.keys()).join(',')==='0,2','replacement preserves insertion order');
assert(map.getOrInsertComputed(0,function(){throw new Error('called existing callback');})===token,'existing skips callback');
try{map.getOrInsertComputed(3,function(){throw token;});throw new Error('missing callback Throw');}catch(e){assert(e===token && !map.has(3),'no insertion after Throw');}
"#,
    );
}
#[test]
fn from_entries_and_group_by_keep_order_keys_groups_and_close() {
    assert_collections(
        r#"
var symbol=Symbol('k'), order=[], key={[Symbol.toPrimitive]:function(h){order.push('coerce:'+h);return symbol;}};
var result=Object.fromEntries([{get 0(){order.push('key');return key;},get 1(){order.push('value');return 7;}},['__proto__',9],[symbol,8]]);
assert(order.join(',')==='key,value,coerce:string' && result[symbol]===8,'entry order and overwrite');
assert(Object.getPrototypeOf(result)===Object.prototype && result.__proto__===9,'own __proto__');
var zero=Map.groupBy([1,2,3],function(v,i){assert(i===v-1,'callback index');return i===0?-0:0;});
assert(zero.size===1 && zero.get(0).join(',')==='1,2,3' && 1/Array.from(zero.keys())[0]===Infinity,'canonical grouping');
var groups=Object.groupBy([1,2,3],function(v){return v===2?symbol:'__proto__';});
assert(Object.getPrototypeOf(groups)===null && groups.__proto__.join(',')==='1,3' && groups[symbol][0]===2,'null prototype and Symbol groups');
var token={}, closes=0;
try{Map.groupBy({[Symbol.iterator]:function(){return{next:function(){return{value:1,done:false};},return:function(){closes++;return{};}};}},function(){throw token;});throw new Error('callback throw');}catch(e){assert(e===token && closes===1,'GroupBy owns Close');}
"#,
    );
}
#[test]
fn set_algebra_snapshots_records_orders_branches_and_duplicate_keys() {
    assert_collections(
        r#"
var order=[], source=new Set([1,2]), other={get size(){order.push('size');return{valueOf:function(){order.push('number');return 3;}};},get has(){order.push('has');return function(v){order.push('call:'+v);if(v===1){source.delete(2);source.add(3);}return true;};},get keys(){order.push('keys');return function(){throw new Error('unused keys');};}};
var intersection=source.intersection(other);
assert(order.join(',')==='size,number,has,keys,call:1,call:3' && Array.from(intersection).join(',')==='1,3','ordered record/live branch');
var base=new Set([1,2]), bag={size:1,has:function(){throw new Error('unused has');},keys:function(){return [2,3,3].values();}};
assert(Array.from(base.intersection(bag)).join(',')==='2','other order and duplicate suppression');
assert(Array.from(base.symmetricDifference(bag)).join(',')==='1,3','duplicates do not toggle');
assert(Array.from(base.union(bag)).join(',')==='1,2,3','union order');
assert(Array.from(base.difference(bag)).join(',')==='1','difference');
var copied=new Set([1,2]), changes={size:3,keys:function(){throw new Error('unused');},has:function(v){if(v===1){copied.clear();copied.add(9);}return v===1;}};
assert(Array.from(copied.difference(changes)).join(',')==='2','difference private copy');
var beforeKeys=new Set([1]), mutating={size:0,has:function(){return false;},keys:function(){beforeKeys.add(2);return{get next(){beforeKeys.add(3);return function(){return{done:true};};}};}};
assert(Array.from(beforeKeys.union(mutating)).join(',')==='1,2,3','keys and next Get before copy');
"#,
    );
}
#[test]
fn set_predicates_close_only_early_other_iteration_and_preserve_close_throw() {
    assert_collections(
        r#"
var closes=0, token={}, bag={size:1,has:function(){return true;},keys:function(){return{next:function(){return{value:2,done:false};},return:function(){closes++;return{};}};}};
assert(!new Set([1,2]).isDisjointFrom(bag) && closes===1,'early Disjoint closes');
assert(!new Set([1]).isSupersetOf(bag) && closes===2,'early Superset closes');
assert(new Set([1]).isSubsetOf({size:Infinity,has:function(){return true;},keys:function(){throw new Error('unused');}}),'Infinity size');
var records=0;assert(!new Set([1,2]).isSubsetOf({size:1,has:function(){return true;},keys:function(){records++;return [].values();}}) && records===0,'size fast path');
bag.keys=function(){return{next:function(){return{value:2,done:false};},return:function(){throw token;}};};
try{new Set([1]).isSupersetOf(bag);throw new Error('lost close failure');}catch(e){assert(e===token,'Normal early Close can throw');}
var reached=0;try{new Set().union({size:NaN,get has(){reached++;return function(){};},keys:function(){}});throw new Error('NaN');}catch(e){assert(e instanceof TypeError && reached===0,'NaN before has');}
try{new Set().union({size:-1,get has(){reached++;return function(){};},keys:function(){}});throw new Error('negative');}catch(e){assert(e instanceof RangeError && reached===0,'negative before has');}
"#,
    );
}
#[test]
fn borrowed_collection_builtins_choose_their_realm_and_reject_proxies() {
    assert_collections(
        r#"
var realm=__scriptwCreateRealm(), other=realm.global, map=new Map([[1,2]]), set=new Set([1]);
var iterator=other.Map.prototype.entries.call(map), entry=iterator.next();
assert(Object.getPrototypeOf(iterator)===Object.getPrototypeOf(new other.Map().entries()),'iterator Realm');
assert(Object.getPrototypeOf(entry)===other.Object.prototype && Object.getPrototypeOf(entry.value)===other.Array.prototype,'iterator result and pair Realm');
var result=other.Set.prototype.union.call(set,new Set([2]));
assert(Object.getPrototypeOf(result)===other.Set.prototype && Array.from(result).join(',')==='1,2','intrinsic result Realm');
var reads=0, proxy=new Proxy(map,{get:function(){reads++;throw new Error('brand should not Get');}});
try{other.Map.prototype.get.call(proxy,1);throw new Error('proxy brand');}catch(e){assert(e instanceof other.TypeError && reads===0,'direct GC brand and callee Realm');}
"#,
    );
}

#[test]
fn compacted_collection_cursors_resume_by_ordinal_and_exhaust_permanently() {
    assert_collections(
        r#"
var map=new Map(),set=new Set(),mapEarly=map.keys(),setEarly=set.values();
for(var i=0;i<96;i++){map.set(i,{n:i});set.add(i);}
var mapMiddle=map.keys(),setMiddle=set.values();
for(var i=0;i<48;i++){assert(mapMiddle.next().value===i,'Map prefix');assert(setMiddle.next().value===i,'Set prefix');}
for(var i=0;i<95;i++){map.delete(i);set.delete(i);}
// Repeated append/delete forces physical compaction while old cursors remain.
for(var i=96;i<400;i++){map.set(i,i);set.add(i);if(i!==399){map.delete(i);set.delete(i);}}
$262.gc();
assert(mapEarly.next().value===95 && mapMiddle.next().value===95,'Map stale physical positions');
assert(setEarly.next().value===95 && setMiddle.next().value===95,'Set stale physical positions');
assert(mapMiddle.next().value===399 && setMiddle.next().value===399,'surviving appended tail');
map.delete(399);set.delete(399);map.set(400,'last');set.add(400);
assert(mapMiddle.next().value===400 && setMiddle.next().value===400,'deleted tail followed by append');
assert(mapMiddle.next().done && setMiddle.next().done,'cursor exhausted');
map.clear();set.clear();map.set(401,'new');set.add(401);
assert(mapMiddle.next().done && setMiddle.next().done,'exhausted cursors never resurrect');
assert(mapEarly.next().value===401 && setEarly.next().value===401,'unexhausted cursor survives clear');
var untouchedMap=map.keys(),untouchedSet=set.values();
for(var i=0;i<256;i++){map.clear();set.clear();map.set(i,i);set.add(i);}
assert(untouchedMap.next().value===255 && untouchedSet.next().value===255,'clear drops history without dropping live cursors');
assert(Array.from(map.keys()).join(',')==='255' && Array.from(set.values()).join(',')==='255','fresh cursors begin at current first entry');
map.clear();map.clear();set.clear();set.clear();map.set(-0,NaN);set.add(-0);
assert(1/map.keys().next().value===Infinity && Number.isNaN(map.get(0)) && 1/set.values().next().value===Infinity,'clear preserves canonical keys and values');
"#,
    );
}

#[test]
fn collection_callbacks_reload_compacted_and_cleared_storage() {
    assert_collections(
        r#"
var map=new Map([[0,'first'],[1,'removed']]),set=new Set([0,1]),mapSeen=[],setSeen=[];
map.forEach(function(value,key,owner){
  mapSeen.push(key);
  if(key===0){
    owner.delete(1);
    for(var i=2;i<40;i++){owner.set(i,i);owner.delete(i);}
    owner.set(40,'retained');$262.gc();
  }else if(key===40){owner.clear();owner.set(41,'after clear');}
});
set.forEach(function(value,key,owner){
  assert(value===key,'Set callback pair');setSeen.push(value);
  if(value===0){
    owner.delete(1);
    for(var i=2;i<40;i++){owner.add(i);owner.delete(i);}
    owner.add(40);$262.gc();
  }else if(value===40){owner.clear();owner.add(41);}
});
assert(mapSeen.join(',')==='0,40,41' && setSeen.join(',')==='0,40,41','forEach uses live ordinal across repack and clear');
var source=new Set([0,1]),hasCalls=[];
var other={size:100,has:function(value){
  hasCalls.push(value);
  if(value===0){source.clear();for(var j=2;j<20;j++){source.add(j);source.delete(j);}source.add(20);}
  return true;
},keys:function(){throw new Error('unexpected keys branch');}};
assert(Array.from(source.intersection(other)).join(',')==='0,20' && hasCalls.join(',')==='0,20','Set algebra callbacks observe replacement storage');
"#,
    );
}
