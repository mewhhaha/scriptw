//! Set-receiver data definitions preserve Array coercion, publication and errors.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion, RealmBuilder,
    RunOptions,
};

fn check(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
    for directive in ["", "'use strict';\n"] {
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &format!("{directive}function assert(v,m){{if(!v)throw m;}}\n{source}"),
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
            .expect("Array descriptor control executes through Wasm AOT");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            observed.completion
        );
    }
}

#[test]
fn length_coercions_finish_before_current_writability_is_read() {
    check(
        r#"
var a=[1,2,3],calls=0,value={valueOf:function(){
 calls++;gc();if(calls===1)Object.defineProperty(a,'length',{writable:false});return 2;
}};
assert(Reflect.set(a,'length',value)===false,'length became readonly during first coercion');
assert(calls===2 && a.length===3 && a[2]===3,'both conversions precede current readonly rejection');
var b=[1,2,3],events=[],v={valueOf:function(){
 events.push('convert');gc();if(events.length===1){b.push(4);delete b[2];}return 1;
}};
assert(Reflect.set(b,'length',v) && events.length===2 && b.length===1 && b[0]===1,
 'length and deletions use storage after both coercion callbacks');
"#,
    );
}

#[test]
fn mismatched_length_conversions_and_original_throws_keep_their_realms() {
    check(
        r#"
var foreign=__scriptwCreateRealm().global,a=[1,2,3],calls=0,caught;
var value={valueOf:function(){calls++;gc();if(calls===1){Object.defineProperty(a,'length',{writable:false});return 1;}return 2;}};
try{foreign.Reflect.set(a,'length',value);}catch(e){caught=e;}
assert(caught instanceof foreign.RangeError && calls===2 && a.length===3,
 'mismatch precedes readonly rejection and uses executing Reflect Realm');
var marker={},b=[1,2,3],count=0;
var throwing={valueOf:function(){count++;gc();if(count===2)throw marker;return 1;}};
try{Reflect.set(b,'length',throwing);throw 'missing coercion failure';}catch(e){assert(e===marker,'original coercion throw identity');}
assert(count===2 && b.length===3 && b[2]===3,'abrupt conversion publishes no length or deletion');
"#,
    );
}

#[test]
fn distinct_array_receiver_uses_partial_updates_and_complete_new_data_fields() {
    check(
        r#"
var source={0:0,1:0},token={},a=[1];
Object.defineProperty(a,'0',{writable:true,enumerable:false,configurable:false});
Object.preventExtensions(a);gc();
assert(Reflect.set(source,'0',token,a) && a[0]===token,'existing data value can change on nonextensible Array');
var d=Object.getOwnPropertyDescriptor(a,'0');
assert(d.writable && !d.enumerable && !d.configurable,'value-only update preserves attributes');
assert(!Reflect.set(source,'1',token,a) && a.length===1 && !Object.hasOwn(a,'1'),'new index rejects without extension');
var b=[],proto=Object.create(Array.prototype),setterCalls=0;
Object.defineProperty(proto,'0',{set:function(){setterCalls++;},configurable:true});Object.setPrototypeOf(b,proto);
assert(Reflect.set(source,'0',token,b) && b[0]===token && b.length===1 && setterCalls===0,
 'ordinary source defines on own Array receiver without consulting receiver prototype setter');
var made=Object.getOwnPropertyDescriptor(b,'0');
assert(made.writable && made.enumerable && made.configurable,'new index has complete data attributes');
var symbol=Symbol('key');
assert(Reflect.set({},symbol,token,b) && Reflect.set({},'named',token,b),'new named and symbol receiver properties');
for(var key of [symbol,'named']){var p=Object.getOwnPropertyDescriptor(b,key);assert(p.value===token && p.writable && p.enumerable && p.configurable,'new header attributes');}
gc();assert(b[symbol]===token && b.named===token,'whole value edges survive GC');
"#,
    );
}

#[test]
fn shrink_failure_keeps_completed_deletions_and_outer_set_owns_strict_errors() {
    check(
        r#"
var a=[0,1,2,3,4];Object.defineProperty(a,'2',{configurable:false});
assert(!Reflect.set(a,'length',1),'nonconfigurable shrink returns false');
assert(a.length===3 && a[2]===2 && !Object.hasOwn(a,'3') && !Object.hasOwn(a,'4'),
 'rollback retains failed index and preceding descending deletions');
var b=[];Object.defineProperty(b,'length',{writable:false});
assert(!Reflect.set(b,'0',7) && b.length===0 && !Object.hasOwn(b,'0'),'new-index failure is Boolean false');
var caught;function strictWrite(){'use strict';b[0]=7;}
try{strictWrite();}catch(e){caught=e;}
assert(caught instanceof TypeError && b.length===0 && !Object.hasOwn(b,'0'),'outer strict Set creates TypeError');
var wide=[];wide.length=4294967295;var range;
try{wide.push('last-named');}catch(e){range=e;}
assert(range instanceof RangeError && wide.length===4294967295 && wide['4294967295']==='last-named',
 'named boundary publication precedes final length RangeError');
"#,
    );
}
