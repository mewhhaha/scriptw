//! The `instanceof` operator decides a plain function constructor without
//! its state machine, reading the constructor's own "prototype" through a
//! per-site record, and a global identifier read answers from the global
//! object without a Reference, including an unresolvable `typeof` operand.
//! Every use re-proves what it relies on; these scripts change each fact
//! between two executions of the same site. Expectations are Node 24's.
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

/// Script code and a function body read globals through different helpers;
/// strict and sloppy code through different helper variants.
fn run_in_every_scope(source: &str, expected: &str) {
    run(source, expected);
    run(&format!("\"use strict\";\n{source}"), expected);
    run(&format!("(function () {{\n{source}}})();"), expected);
    run(
        &format!("(function () {{\n\"use strict\";\n{source}}})();"),
        expected,
    );
}

#[test]
fn an_instanceof_site_follows_the_constructor_and_the_candidate() {
    // The constructor's "prototype" replaced or made a primitive, an own
    // @@hasInstance added, the constructor's [[Prototype]] replaced, and the
    // candidate's chain changed between uses of one site.
    run_in_every_scope(
        r#"
var log = [];
function test(v, C) { return v instanceof C; }
function C() {}
var c = new C();
log.push(test(c, C), test({}, C), test(1, C));
C.prototype = {};
log.push(test(c, C));
Object.setPrototypeOf(c, C.prototype);
log.push(test(c, C));
Object.defineProperty(C, Symbol.hasInstance, { value: function (v) { return v === 1; } });
log.push(test(1, C), test(c, C));
function D() {}
var d = new D();
log.push(test(d, D));
Object.setPrototypeOf(D, { [Symbol.hasInstance]: function () { return "custom"; } });
log.push(test({}, D), typeof D);
function E() {}
log.push(test({}, E));
E.prototype = 3;
log.push(test(1, E));
try { test({}, E); log.push("no error"); } catch (error) { log.push(error.constructor === TypeError); }
print(log.join());
"#,
        "true,false,false,false,true,true,false,true,true,function,false,false,true",
    );
}

#[test]
fn one_instanceof_site_serves_every_kind_of_constructor_and_candidate() {
    // Dictionary, intrinsic and derived constructors, bound functions, a
    // Proxy on the chain (its trap runs), another Realm's constructors and
    // a non-callable right-hand side, all through one site.
    run_in_every_scope(
        r#"
var log = [];
function test(v, C) { return v instanceof C; }
function C() {}
class A {}
class B extends A {}
log.push(test(new Map(), Map), test({}, Map), test([], Map), test(new Int8Array(1), Int8Array),
  test(new Uint8Array(1), Int8Array), test([], Array), test([], Object), test(test, Function),
  test(Object.create(null), Object), test(new C(), C), test(new B(), A), test(new B(), B),
  test(new A(), B), test(new C(), C.bind(null)), test(new Date(0), Date), test(new Error("x"), Error));
var trapped = [];
var proxy = new Proxy({}, { getPrototypeOf: function () { trapped.push("trap"); return C.prototype; } });
log.push(test(proxy, C), trapped.length);
var behind = Object.create(proxy);
log.push(test(behind, C), trapped.length);
var realm = __scriptwCreateRealm(), FM = realm.global.Map;
log.push(test(new FM(), FM), test(new Map(), FM), test(new FM(), Map), test(new Map(), Map));
var foreignFunction = realm.evalScript("(function F() {})");
log.push(test(new foreignFunction(), foreignFunction), test({}, foreignFunction));
try { test({}, {}); log.push("no error"); } catch (error) { log.push(error.constructor === TypeError); }
print(log.join());
"#,
        "true,false,false,true,false,true,true,true,false,true,true,true,false,true,true,true,true,1,true,2,true,false,false,true,true,false,true",
    );
}

#[test]
fn a_global_read_follows_the_global_object_and_its_chain() {
    // A builtin global replaced, deleted (its read then throws), redefined
    // as an accessor and shadowed by a later global lexical declaration; an
    // absent name later inherited, own, deleted and then asked through a
    // Proxy prototype.
    run_in_every_scope(
        r#"
var log = [];
var nativeMap = Map;
function read() { return Map; }
function readLate() { return Late; }
function typeLate() { return typeof Late == "string"; }
function readMissing() { try { return String(Missing); } catch (error) { return error.constructor.name; } }
function typeMissing() { return typeof Missing == "undefined"; }
log.push(read() === nativeMap, read() === nativeMap);
globalThis.Map = 5;
log.push(read());
delete globalThis.Map;
log.push(readDeleted());
function readDeleted() { try { return String(Map); } catch (error) { return error.constructor.name; } }
Object.defineProperty(globalThis, "Map", { get: function () { return this === globalThis ? "getter" : "wrong"; }, configurable: true });
log.push(read(), typeof Map);
Object.defineProperty(globalThis, "Map", { value: nativeMap, writable: true, configurable: true });
log.push(read() === nativeMap, Map === nativeMap);
globalThis.Late = "object";
log.push(readLate(), typeLate());
__scriptwRealmEvalScript("let Late = 2;");
log.push(readLate(), typeLate(), globalThis.Late);
log.push(readMissing(), typeMissing());
Object.prototype.Missing = "inherited";
log.push(readMissing(), typeMissing());
delete Object.prototype.Missing;
log.push(readMissing(), typeMissing());
globalThis.Missing = 4;
log.push(readMissing(), typeMissing());
delete globalThis.Missing;
var traps = [];
var original = Object.getPrototypeOf(globalThis);
Object.setPrototypeOf(globalThis, new Proxy(original, {
  has: function (target, key) { if (key === "Missing") traps.push("has"); return Reflect.has(target, key); },
}));
log.push(typeMissing(), readMissing(), traps.join("/"));
Object.setPrototypeOf(globalThis, original);
log.push(typeMissing(), traps.length);
print(log.join());
"#,
        "true,true,5,ReferenceError,getter,string,true,true,object,true,2,false,object,ReferenceError,true,inherited,false,ReferenceError,true,4,false,true,ReferenceError,has/has,true,2",
    );
}

#[test]
fn a_global_read_sees_eval_vars_with_objects_and_its_own_realm() {
    // Sloppy direct eval adds a var that shadows the global, a `with` object
    // answers the name, and another Realm's global answers its own reads.
    let source = r#"
var log = [];
var nativeMap = Map;
function outer(code) {
  function read() { return Map === nativeMap ? "global" : Map; }
  function check() { return typeof Map == "function"; }
  var result = [read(), check()];
  eval(code);
  result.push(read(), check());
  return result.join("/");
}
log.push(outer(""), outer("var Map = 7"), outer(""));
function within(object) {
  with (object) {
    return [Map === nativeMap ? "global" : Map, typeof Map == "function", typeof Absent == "undefined"].join("/");
  }
}
log.push(within({}), within({ Map: 1, Absent: 2 }), within({}));
var realm = __scriptwCreateRealm();
var foreignRead = realm.evalScript("(function () { return typeof Foreign == 'number' ? Foreign : typeof Foreign; })");
log.push(foreignRead());
realm.global.Foreign = 9;
log.push(foreignRead(), typeof Foreign);
print(log.join());
"#;
    let expected = "global/true/global/true,global/true/7/false,global/true/global/true,global/true/true,1/false/false,global/true/true,undefined,9,undefined";
    run(source, expected);
    run(&format!("(function () {{\n{source}}})();"), expected);
}

#[test]
fn a_named_binding_read_keeps_its_initialization_and_shadowing() {
    // A binding found by name is read without a Reference only once it is
    // initialized; before that, ResolveBinding throws. A captured binding
    // shadows the global of the same name.
    let source = r#"
var log = [];
function scope(code) {
  eval(code);
  function read() { try { return String(later); } catch (error) { return error.constructor.name; } }
  function kind() { try { return typeof later; } catch (error) { return error.constructor.name; } }
  var result = [read(), kind()];
  let later = 3;
  result.push(read(), kind());
  later = "changed";
  result.push(read(), kind());
  return result.join("/");
}
log.push(scope(""), scope("var later2 = 1"));
var captured = (function () {
  eval("");
  var names = [];
  for (var i = 0; i < 40; i++) names.push("v" + i);
  var Map = "shadow";
  return function () { return Map; };
})();
log.push(captured());
print(log.join());
"#;
    let expected = "ReferenceError/ReferenceError/3/number/changed/string,ReferenceError/ReferenceError/3/number/changed/string,shadow";
    run(source, expected);
    run(&format!("(function () {{\n{source}}})();"), expected);
}
