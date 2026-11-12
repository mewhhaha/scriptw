//! A name known at compile time as a builtin global such as `Map` is
//! resolved again on every use: once its configurable property is deleted,
//! the Reference is unresolvable. Reading it, calling it and compound
//! assignment throw a ReferenceError, typeof reads undefined, strict
//! assignment throws, and sloppy assignment creates the property.
//! Expectations are Node 24's except where noted.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_script(
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
        .expect("the script executes compiled Wasm");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}\n{source}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine(expected.into())],
        "{source}"
    );
}

/// Script code and function bodies, each sloppy and strict.
fn run_sloppy_and_strict(source: &str, sloppy: &str, strict: &str) {
    run(source, sloppy);
    run(&format!("\"use strict\";\n{source}"), strict);
    run(&format!("(function () {{\n{source}}})();"), sloppy);
    run(
        &format!("(function () {{\n\"use strict\";\n{source}}})();"),
        strict,
    );
}

#[test]
fn a_deleted_builtin_global_is_unresolvable() {
    // Before the deletion, after it, with an own `undefined` value (which
    // resolves and does not throw) and after restoring the constructor. A
    // throw before the read leaves the catch parameter a number; the read's
    // ReferenceError must reach the same catch as an object.
    let source = r#"
var log = [];
var nativeMap = Map;
function read() { try { return Map === nativeMap ? "native" : String(Map); } catch (e) { return e.constructor.name; } }
function kind() { return typeof Map; }
function isFunction() { return typeof Map == "function"; }
function call() { try { Map(); return "returned"; } catch (e) { return e.constructor.name; } }
function construct() { try { return new Map() instanceof nativeMap; } catch (e) { return e.constructor.name; } }
function compound() { try { Map += 1; return "assigned"; } catch (e) { return e.constructor.name; } }
function logical() { try { Map ||= 1; return "assigned"; } catch (e) { return e.constructor.name; } }
function caught(flag) { try { if (flag) throw 1; Map; return "none"; } catch (e) { return typeof e + ":" + (e instanceof ReferenceError); } }
log.push(read(), kind(), isFunction(), call(), construct(), caught(false), caught(true));
log.push(delete globalThis.Map, "Map" in globalThis);
log.push(read(), kind(), isFunction(), call(), construct(), compound(), logical(), caught(false), "Map" in globalThis);
globalThis.Map = undefined;
log.push(read(), kind(), isFunction(), call());
globalThis.Map = nativeMap;
log.push(read(), kind(), isFunction(), construct());
print(log.join());
"#;
    let expected = "native,function,true,TypeError,true,none,number:false,true,false,ReferenceError,undefined,false,ReferenceError,ReferenceError,ReferenceError,ReferenceError,object:true,false,undefined,undefined,false,TypeError,native,function,true,true";
    run_sloppy_and_strict(source, expected, expected);
}

#[test]
fn assignment_to_a_deleted_builtin_global_follows_put_value() {
    // An unresolvable Reference: strict code throws and creates nothing,
    // sloppy code creates an ordinary global property.
    let source = r#"
var log = [];
delete globalThis.Map;
function assign() { try { Map = 3; return String(globalThis.Map); } catch (e) { return e.constructor.name; } }
log.push(assign(), "Map" in globalThis);
print(log.join());
"#;
    run_sloppy_and_strict(source, "3,true", "ReferenceError,false");
}

#[test]
fn a_global_read_has_binding_then_get_binding_value() {
    // The global object lacks the name and its [[Prototype]] is a Proxy:
    // ResolveBinding's HasProperty, GetBindingValue's HasProperty, then Get.
    // When the second answer is false, strict code throws and sloppy code
    // reads undefined. A getter that deletes its own property answers once.
    // The specification's second HasProperty is not Node's: V8 omits it
    // (its queue then drifts), so these expectations follow ECMA-262.
    let source = r#"
var log = [];
var nativeMap = Map;
function read() { try { var v = Map; return v === nativeMap ? "native" : String(v); } catch (e) { return e.constructor.name; } }
function kind() { try { return typeof Map; } catch (e) { return e.constructor.name; } }
function isFunction() { try { return typeof Map == "function"; } catch (e) { return e.constructor.name; } }
var traps = [];
var answers = [];
var original = Object.getPrototypeOf(globalThis);
delete globalThis.Map;
Object.setPrototypeOf(globalThis, new Proxy(original, {
  has: function (target, key) {
    if (key !== "Map") return Reflect.has(target, key);
    traps.push("has");
    return answers.length ? answers.shift() : false;
  },
  get: function (target, key, receiver) {
    if (key === "Map") { traps.push("get"); return "proxied"; }
    return Reflect.get(target, key, receiver);
  },
}));
log.push(read(), traps.splice(0).join("/") || "-");
answers.push(true, true);
log.push(read(), traps.splice(0).join("/") || "-");
answers.push(true, false);
log.push(read(), traps.splice(0).join("/") || "-");
log.push(kind(), traps.splice(0).join("/") || "-");
answers.push(true, true);
log.push(kind(), traps.splice(0).join("/") || "-");
answers.push(true, true);
log.push(isFunction(), traps.splice(0).join("/") || "-");
Object.setPrototypeOf(globalThis, original);
Object.defineProperty(globalThis, "Map", { get: function () { traps.push("getter"); delete globalThis.Map; return "once"; }, configurable: true });
log.push(read(), traps.splice(0).join("/") || "-", read(), kind());
Object.defineProperty(globalThis, "Map", { value: undefined, writable: true, configurable: true });
log.push(read(), kind(), isFunction());
globalThis.Map = nativeMap;
log.push(read(), kind(), isFunction());
print(log.join());
"#;
    run_sloppy_and_strict(
        source,
        "ReferenceError,has,proxied,has/has/get,undefined,has/has,undefined,has,string,has/has/get,false,has/has/get,once,getter,ReferenceError,undefined,undefined,undefined,false,native,function,true",
        "ReferenceError,has,proxied,has/has/get,ReferenceError,has/has,undefined,has,string,has/has/get,false,has/has/get,once,getter,ReferenceError,undefined,undefined,undefined,false,native,function,true",
    );
}

#[test]
fn typeof_of_a_deleted_global_inside_with_reads_undefined() {
    // The `with` object is asked first; the global fallback keeps typeof's
    // unresolvable result while a value read throws.
    let source = r#"
var log = [];
function inside(object) { with (object) { return [typeof Map, typeof Map == "function"].join("/"); } }
function readInside(object) { with (object) { try { return String(Map); } catch (e) { return e.constructor.name; } } }
log.push(inside({}), inside({ Map: 1 }));
delete globalThis.Map;
log.push(inside({}), inside({ Map: "own" }), readInside({}), readInside({ Map: 2 }));
print(log.join());
"#;
    let expected = "function/true,number/false,undefined/false,string/false,ReferenceError,2";
    run(source, expected);
    run(&format!("(function () {{\n{source}}})();"), expected);
}
