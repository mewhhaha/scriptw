use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn check(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!(
            "{directive}function check(value,label){{if(!value)throw new Error(label);}}\n{source}\nprint('ok');"
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
            .unwrap_or_else(|error| panic!("intrinsic data observation: {error}\n{source}"));
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            observed.completion,
        );
        assert_eq!(
            observed.output_events,
            [HostOutputEvent::PrintLine("ok".into())]
        );
    }
}

#[test]
fn observed_intrinsic_data_keeps_live_mutations_descriptor_flags_and_same_value() {
    check(
        r#"
function readConstructor(){return RegExp;}
var locked=__scriptwCreateRealm().global;
Object.defineProperty(locked,'RegExp',{writable:false,configurable:false});
gc();
var lockedConstructor=locked.RegExp;
check(typeof lockedConstructor==='function' && locked.RegExp===lockedConstructor &&
  !Reflect.set(locked,'RegExp',17),'first observation behind readonly attributes');
var nativeConstructor=RegExp;
for(var index=0;index<32;index++)check(readConstructor()===nativeConstructor,'repeated observed global');
check(Object.getOwnPropertyDescriptor(globalThis,'RegExp').value===nativeConstructor,'global descriptor identity');
globalThis.RegExp=17;
gc();
check(readConstructor()===17,'global overwrite invalidates observed native');
delete globalThis.RegExp;
check(typeof RegExp==='undefined','global delete after observation');
Object.defineProperty(globalThis,'RegExp',{value:nativeConstructor,writable:true,configurable:true});
for(var index=0;index<32;index++)check(readConstructor()===nativeConstructor,'global redefine after delete');
Object.defineProperty(globalThis,'RegExp',{value:17});
check(readConstructor()===17,'global define invalidates observed native');
Object.defineProperty(globalThis,'RegExp',{value:nativeConstructor,writable:false,configurable:false});
gc();
Object.defineProperty(globalThis,'RegExp',{value:nativeConstructor});
check(readConstructor()===nativeConstructor && !Reflect.set(globalThis,'RegExp',17),'global SameValue and readonly binding');
var original=Math.abs;
original.marker={retained:17};
var snapshot=Object.getOwnPropertyDescriptor(Math,'abs');
gc();
check(Math.abs===original && snapshot.value===original && Math.abs.marker.retained===17,'stable observed data');
check(snapshot.writable && !snapshot.enumerable && snapshot.configurable,'initial attributes');
var replacement=function(value){return value+100;};
Math.abs=replacement;
gc();
check(Math.abs===replacement && Math.abs(3)===103 && snapshot.value===original,'live assignment after observation');
delete Math.abs;
check(!Object.hasOwn(Math,'abs'),'delete observed slot');
Object.defineProperty(Math,'abs',{value:original,writable:true,enumerable:true,configurable:true});
var names=Object.getOwnPropertyNames(Math);
check(names[names.length-1]==='abs' && Math.abs===original,'reinsert order and identity');
Object.defineProperty(Math,'abs',{value:replacement});
check(Math.abs===replacement && Object.getOwnPropertyDescriptor(Math,'abs').value===replacement,'define updates live value');
Object.defineProperty(Math,'abs',{value:original,writable:false,configurable:false});
Object.freeze(Math);
gc();
Object.defineProperty(Math,'abs',{value:original});
check(Object.isFrozen(Math) && Math.abs===original && !Reflect.set(Math,'abs',replacement),'frozen SameValue accepted');
var threw=false;
try{Object.defineProperty(Math,'abs',{value:replacement});}catch(error){threw=error instanceof TypeError;}
check(threw && Math.abs===original && original.marker.retained===17,'different frozen value rejected');
"#,
    );
}

#[test]
fn observed_alias_groups_survive_promotion_and_independent_slot_replacement() {
    check(
        r#"
var prototype=String.prototype;
var fresh=__scriptwCreateRealm().global.String.prototype;
fresh.promotionProbe=1;
gc();
var freshLeft=fresh.trimLeft;
check(fresh.trimStart===freshLeft && freshLeft.name==='trimStart' &&
  freshLeft.call(' x ')==='x ','both aliases unread before promotion');
var start=prototype.trimStart;
Object.defineProperty(prototype,'promotionProbe',{value:1,configurable:true});
gc();
var left=prototype.trimLeft;
check(left===start && left.name==='trimStart' && left.length===0,'unobserved alias after promotion');
check(left.call(' x ')==='x ','retained alias callable');
prototype.trimStart=17;
check(prototype.trimStart===17 && prototype.trimLeft===left,'replacement is per property');
delete prototype.trimStart;
prototype.trimStart=function(){return 'replacement';};
check(prototype.trimStart() ==='replacement' && prototype.trimLeft===left,'reinsert does not overwrite alias');
Object.defineProperty(prototype,'trimLeft',{value:left,writable:false,configurable:false});
Object.freeze(prototype);
gc();
Object.defineProperty(prototype,'trimLeft',{value:start});
check(prototype.trimLeft===start && start.call(' y ')==='y ','frozen alias keeps original identity');
var values=Array.prototype.values;
Array.prototype.promotionProbe=1;
gc();
check(Array.prototype[Symbol.iterator]===values,'canonical Symbol alias after promotion');
Array.prototype.values=23;
check(Array.prototype[Symbol.iterator]===values && values.call([9]).next().value===9,'Symbol alias remains independent');
"#,
    );
}

#[test]
fn data_normalization_never_memoizes_accessor_results() {
    check(
        r#"
var firstParent={},secondParent={},object=Object.create(firstParent);
check(object.__proto__===firstParent,'initial native getter result');
Object.setPrototypeOf(object,secondParent);
gc();
check(object.__proto__===secondParent,'native getter reexecutes');
var reads=0,target={get value(){reads++;return {stamp:reads};}};
var first=target.value;
gc();
var second=target.value;
check(reads===2 && first!==second && first.stamp===1 && second.stamp===2,'fresh user getter results');
Object.defineProperty(target,'value',{get:function(){return ++reads;},configurable:true});
check(target.value===3 && target.value===4,'replacement getter stays live');
var expression=/a/;
check(expression.source==='a' && !expression.global,'initial RegExp getter results');
expression.compile('b','g');
gc();
check(expression.source==='b' && expression.global && expression.flags==='g','mutable native getter state');
RegExp.input='first';
check(RegExp.input==='first' && RegExp.$_==='first','first legacy accessor result');
RegExp.input='second';
gc();
check(RegExp.input==='second' && RegExp.$_==='second','legacy alias accessors reexecute');
"#,
    );
}

#[test]
fn observed_foreign_native_methods_keep_defining_realm_after_gc() {
    check(
        r#"
var foreign=__scriptwCreateRealm().global;
var functionPrototype=foreign.Function.prototype;
var publicReads=0;
Object.defineProperty(foreign,'Function',{get:function(){publicReads++;throw new Error('public Function lookup');},configurable:true});
var math=foreign.Math;
Object.defineProperty(math,'promotionProbe',{value:1,configurable:true});
gc();
var late=math.sqrt;
check(late(81)===9 && late.name==='sqrt' && late.length===1,'unobserved method metadata after promotion');
check(Object.getPrototypeOf(late)===functionPrototype && publicReads===0,'native allocation uses defining Realm slots');
var prototype=foreign.String.prototype;
var retained=prototype.trimStart;
prototype.promotionProbe=1;
gc();
check(prototype.trimLeft===retained && retained!==String.prototype.trimStart,'foreign alias identity');
prototype.trimStart=function(){return 'changed';};
check(prototype.trimStart()==='changed' && prototype.trimLeft===retained,'foreign replacement stays live');
check(Object.getPrototypeOf(retained)===functionPrototype,'original native prototype');
foreign=null;functionPrototype=null;math=null;late=null;prototype=null;
gc();
check(retained.call(' z ')==='z ','retained foreign native survives collection');
var threw=false;
try{retained.call(null);}catch(error){threw=error.name==='TypeError' && !(error instanceof TypeError);}
check(threw && publicReads===0,'retained callable throws in defining Realm');
"#,
    );
}
