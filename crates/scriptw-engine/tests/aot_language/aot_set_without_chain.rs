//! OrdinarySet skips asking the prototype chain when the chain provably lacks
//! the key: a plain object above an unmodified %Object.prototype%, or an Array
//! index above an element-free Array prototype with that same tail. These
//! scripts add setters and read-only properties to each parent between uses.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn set_asks_a_parent_again_once_it_could_hold_the_key() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_script(
            r#"
var log = [];
function setKey(o, k, v) { o[k] = v; return o[k]; }
function setIndex(a, i, v) { a[i] = v; return a.length + ":" + a[i]; }
var o = {};
log.push(setKey(o, "x", 1), setKey({}, "x", 2), setKey({ y: 0 }, "x", 2.5));
var a = [];
log.push(setIndex(a, 0, "p"), setIndex(a, 1, "q"), setIndex([], 3, "r"));
Object.defineProperty(Object.prototype, "x", {
  set: function (v) { log.push("setter:" + v); },
  get: function () { return "inherited"; },
  configurable: true
});
log.push(setKey({}, "x", 3), setKey({ y: 0 }, "x", 3.5));
delete Object.prototype.x;
Object.defineProperty(Object.prototype, "x", { value: "ro", writable: false, configurable: true });
log.push(setKey({}, "x", 4));
delete Object.prototype.x;
log.push(setKey({}, "x", 5));
Object.defineProperty(Array.prototype, "2", {
  set: function (v) { log.push("index setter:" + v); },
  get: function () { return "proto"; },
  configurable: true
});
var b = [1, 2];
log.push(setIndex(b, 2, "z"));
delete Array.prototype[2];
Object.defineProperty(Object.prototype, "2", {
  set: function (v) { log.push("object index setter:" + v); },
  configurable: true
});
var c = [1, 2];
log.push(setIndex(c, 2, "y"));
delete Object.prototype[2];
var d = [1, 2];
log.push(setIndex(d, 2, "w"));
var frozen = Object.freeze([]);
log.push(setIndex(frozen, 0, "f"));
var e = [];
Object.setPrototypeOf(e, { set 0(v) { log.push("custom:" + v); } });
e[0] = 9;
log.push(e.length);
var sealed = Object.preventExtensions({});
log.push(String(setKey(sealed, "x", 6)));
print(log.join());
"#,
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
        .expect("the script executes compiled Wasm");
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine("1,2,2.5,1:p,2:q,4:r,setter:3,setter:3.5,inherited,inherited,ro,5,index setter:z,2:proto,object index setter:y,2:undefined,3:w,0:undefined,custom:9,0,undefined".into())]
    );
}
