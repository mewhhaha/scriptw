//! A read that starts at the Global Environment Record answers from its
//! site's record in P: the global object's dictionary entry record for the
//! name, which holds the property's descriptor for as long as the property
//! lives and a dead accessor descriptor once it is deleted. The site checks
//! that it runs for the record's Environment, that the Environment has no
//! declarative names, and that the entry's descriptor is a data descriptor
//! holding a value. A `typeof` site also answers a name that is nowhere,
//! from a witness of the global object's dictionary table and length and of
//! its one ordinary prototype. An `instanceof` site whose right-hand side is
//! such a read keeps the value cell its own record was made for, and answers
//! without reading the global while the property still holds that cell.
//! Each script warms sites and then changes one of those facts between
//! executions of the same sites. Expectations are Node 24's, taken by
//! running each body as a Script, in a sloppy and in a strict function.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, module: bool, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let options = CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    };
    let run = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    };
    let observed = if module {
        engine.observe_module(source, options, run)
    } else {
        engine.observe_script(source, options, run)
    }
    .expect("the source executes compiled Wasm");
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

/// A Script, the same body as sloppy function code, and a module.
fn run_as_script_function_and_module(source: &str, expected: &str) {
    run(source, false, expected);
    run(&format!("(function () {{\n{source}}})();"), false, expected);
    run(source, true, expected);
}

#[test]
fn a_site_record_answers_only_while_its_entry_holds_the_binding() {
    // One name read as a value, as a `typeof` operand, as a `typeof` guard
    // and as a callee, at sites that first run while it is absent. It is
    // then defined, written through the global object and by assignment,
    // deleted, defined again, redefined as an accessor (whose calls are
    // counted), as an own `undefined` and as a non-writable function,
    // inherited from Object.prototype instead, and shadowed by a later
    // Script's global lexical declaration.
    run_as_script_function_and_module(
        r#"
var log = [];
var counter = 0;
var strictProbe = function (x) { "use strict"; return "f" + x + String(this); };
function read() { try { var v = probe; return v === strictProbe ? "fn" : String(v); } catch (e) { return e.constructor.name; } }
function kind() { return typeof probe; }
function guard() { return typeof probe == "function" ? 1 : 0; }
function call() { try { return probe(1); } catch (e) { return e.constructor.name; } }
function all(label) { log.push(label + ":" + read() + "/" + kind() + "/" + guard() + "/" + call()); }
all("absent"); all("absent");
globalThis.probe = strictProbe;
all("defined"); all("defined"); all("defined");
globalThis.probe = 7;
all("number");
probe = "assigned";
all("assigned");
delete globalThis.probe;
all("deleted"); all("deleted");
globalThis.probe = "back";
all("back"); all("back");
Object.defineProperty(globalThis, "probe", { get: function () { counter++; return "g" + counter; }, configurable: true });
all("getter"); all("getter");
Object.defineProperty(globalThis, "probe", { value: undefined, writable: true, configurable: true });
all("undefined");
Object.defineProperty(globalThis, "probe", { value: strictProbe, writable: false, enumerable: false, configurable: true });
all("frozen");
try { (function () { "use strict"; probe = 1; })(); } catch (e) { log.push(e.constructor.name); }
all("still");
delete globalThis.probe;
Object.prototype.probe = "inherited";
all("inherited"); all("inherited");
delete Object.prototype.probe;
all("gone");
globalThis.probe = strictProbe;
all("again");
__scriptwRealmEvalScript("let probe = 'lexical';");
all("lexical"); all("lexical");
log.push(globalThis.probe === strictProbe, counter);
print(log.join());
"#,
        "absent:ReferenceError/undefined/0/ReferenceError,absent:ReferenceError/undefined/0/ReferenceError,defined:fn/function/1/f1undefined,defined:fn/function/1/f1undefined,defined:fn/function/1/f1undefined,number:7/number/0/TypeError,assigned:assigned/string/0/TypeError,deleted:ReferenceError/undefined/0/ReferenceError,deleted:ReferenceError/undefined/0/ReferenceError,back:back/string/0/TypeError,back:back/string/0/TypeError,getter:g1/string/0/TypeError,getter:g5/string/0/TypeError,undefined:undefined/undefined/0/TypeError,frozen:fn/function/1/f1undefined,TypeError,still:fn/function/1/f1undefined,inherited:inherited/string/0/TypeError,inherited:inherited/string/0/TypeError,gone:ReferenceError/undefined/0/ReferenceError,again:fn/function/1/f1undefined,lexical:lexical/string/0/TypeError,lexical:lexical/string/0/TypeError,true,8",
    );
}

#[test]
fn entry_records_move_with_compaction_and_end_with_their_property() {
    // Warmed sites for an early, a middle and a late global. Deletions
    // leave holes before their entries and growth compacts the table, so
    // the live entries move; a deleted name is unresolvable, and the same
    // name defined again holds a new entry at the end of the table, more
    // often than a site asks for records.
    run_as_script_function_and_module(
        r#"
var log = [];
var g = globalThis;
for (var i = 0; i < 40; i++) g["moved" + i] = "v" + i;
function early() { try { return moved3; } catch (e) { return e.constructor.name; } }
function middle() { try { return moved20; } catch (e) { return e.constructor.name; } }
function late() { try { return moved39; } catch (e) { return e.constructor.name; } }
function kinds() { return typeof moved3 + typeof moved20 + typeof moved39; }
function all() { log.push(early() + "/" + middle() + "/" + late() + "/" + kinds()); }
all(); all();
// Holes before the warmed entries, then growth: live entries move.
delete g.moved0; delete g.moved1; delete g.moved2; delete g.moved10;
all();
for (var j = 0; j < 300; j++) g["grown" + j] = j;
all(); all();
g.moved20 = "rewritten";
delete g.moved3;
all();
// The same name again takes a new entry at the end of the table.
g.moved3 = "returned";
all(); all();
for (var k = 0; k < 300; k++) delete g["grown" + k];
for (var m = 0; m < 700; m++) g["again" + m] = m;
all();
delete g.moved39;
g.moved39 = 39;
all(); all();
// More replacements than a site asks records for.
for (var n = 0; n < 8; n++) { delete g.moved20; log.push(middle()); g.moved20 = "r" + n; log.push(middle(), typeof moved20); }
print(log.join());
"#,
        "v3/v20/v39/stringstringstring,v3/v20/v39/stringstringstring,v3/v20/v39/stringstringstring,v3/v20/v39/stringstringstring,v3/v20/v39/stringstringstring,ReferenceError/rewritten/v39/undefinedstringstring,returned/rewritten/v39/stringstringstring,returned/rewritten/v39/stringstringstring,returned/rewritten/v39/stringstringstring,returned/rewritten/39/stringstringnumber,returned/rewritten/39/stringstringnumber,ReferenceError,r0,string,ReferenceError,r1,string,ReferenceError,r2,string,ReferenceError,r3,string,ReferenceError,r4,string,ReferenceError,r5,string,ReferenceError,r6,string,ReferenceError,r7,string",
    );
}

#[test]
fn a_typeof_site_answers_an_absent_name_only_while_its_witness_holds() {
    // `typeof` of a name that is nowhere, alone and as the guard of an
    // `instanceof` test, beside a throwing value read. Another global is
    // added; Object.prototype gains the name as data and as a counted
    // accessor and loses it; the name itself is defined and deleted more
    // often than a site asks for records; and a later Script declares it
    // lexically.
    run_as_script_function_and_module(
        r#"
var log = [];
var g = globalThis;
function kind() { return typeof nowhere; }
function guard() { return typeof nowhere == "function" && nowhere instanceof Object; }
function read() { try { return String(nowhere); } catch (e) { return e.constructor.name; } }
function all(label) { log.push(label + ":" + kind() + "/" + guard() + "/" + read()); }
all("absent"); all("absent"); all("absent");
// Another global grows the dictionary the witness names.
g.unrelated1 = 1;
all("grown"); all("grown");
// The prototype of the global object gains the name, as data and as an
// accessor, and loses it.
Object.prototype.nowhere = "proto";
all("inherited"); all("inherited");
delete Object.prototype.nowhere;
all("absent");
var gets = 0;
Object.defineProperty(Object.prototype, "nowhere", { get: function () { gets++; return gets; }, configurable: true });
all("getter");
delete Object.prototype.nowhere;
all("absent"); all("absent");
// The name itself, defined and deleted more often than a site asks for
// records.
for (var i = 0; i < 8; i++) {
  g.nowhere = function () {};
  log.push(kind(), guard());
  delete g.nowhere;
  log.push(kind(), guard(), read());
}
// A later global lexical declaration.
g.unrelated2 = 2;
all("absent");
__scriptwRealmEvalScript("let nowhere = 1n;");
all("lexical"); all("lexical");
log.push(gets, "nowhere" in g);
print(log.join());
"#,
        "absent:undefined/false/ReferenceError,absent:undefined/false/ReferenceError,absent:undefined/false/ReferenceError,grown:undefined/false/ReferenceError,grown:undefined/false/ReferenceError,inherited:string/false/proto,inherited:string/false/proto,absent:undefined/false/ReferenceError,getter:number/false/3,absent:undefined/false/ReferenceError,absent:undefined/false/ReferenceError,function,true,undefined,false,ReferenceError,function,true,undefined,false,ReferenceError,function,true,undefined,false,ReferenceError,function,true,undefined,false,ReferenceError,function,true,undefined,false,ReferenceError,function,true,undefined,false,ReferenceError,function,true,undefined,false,ReferenceError,function,true,undefined,false,ReferenceError,absent:undefined/false/ReferenceError,lexical:bigint/false/1,lexical:bigint/false/1,3,false",
    );
}

#[test]
fn an_absence_witness_follows_the_global_objects_prototype_chain() {
    // Each function is one `typeof` site, warmed under the chain it then
    // sees change: the global object's prototype is replaced by an object
    // that has the name; by a dictionary object with a null prototype that
    // gains and loses the name and then gets a prototype of its own that
    // has it; and by a Proxy whose `has` trap counts each HasProperty.
    run_as_script_function_and_module(
        r#"
var log = [];
var g = globalThis;
var original = Object.getPrototypeOf(g);
function a() { return typeof phantom; }
function b() { return typeof phantom; }
function c() { return typeof phantom; }
function d() { return typeof phantom; }
function e() { try { return String(phantom); } catch (err) { return err.constructor.name; } }
// The global object's own prototype is replaced after warming.
log.push(a(), a(), e());
Object.setPrototypeOf(g, { phantom: "from prototype" });
log.push(a(), a(), e());
// A dictionary prototype with a null prototype of its own gains and loses
// the name.
var mid = Object.create(null);
mid.first = 1; mid.second = 2; delete mid.first;
Object.setPrototypeOf(g, mid);
log.push(a(), b(), b(), e());
mid.phantom = "inserted";
log.push(a(), b(), b(), e());
delete mid.phantom;
log.push(b(), c(), c());
// That prototype gets a prototype of its own that has the name.
Object.setPrototypeOf(mid, { phantom: "deeper" });
log.push(b(), c(), c(), e());
Object.setPrototypeOf(mid, null);
log.push(c(), d(), d());
// A Proxy prototype observes each HasProperty.
var seen = 0;
Object.setPrototypeOf(g, new Proxy({}, { has: function (target, key) { if (key === "phantom") seen++; return false; } }));
log.push(c(), d(), d(), e(), seen);
Object.setPrototypeOf(g, original);
log.push(a(), b(), c(), d(), e());
print(log.join());
"#,
        "undefined,undefined,ReferenceError,string,string,from prototype,undefined,undefined,undefined,ReferenceError,string,string,string,inserted,undefined,undefined,undefined,string,string,string,deeper,undefined,undefined,undefined,undefined,undefined,undefined,ReferenceError,4,undefined,undefined,undefined,undefined,ReferenceError",
    );
}

#[test]
fn instanceof_sites_follow_referenceless_and_global_right_hand_sides() {
    // Sites that first meet a right-hand side that is no function, most
    // with no heap reference, then constructors. Sites against global
    // constructors, which answer without reading the global while its
    // property holds the value cell their record was made for: the global
    // is replaced, restored, deleted, defined again, written back as the
    // same function, made an accessor (whose calls are counted) and data
    // again; a bound function and a Proxy at a warmed site; the typed array
    // constructors' edges and @@hasInstance changed after warming; and a
    // later Script's lexical binding that shadows a global constructor.
    run_as_script_function_and_module(
        r#"
var log = [];
function f1(v, c) { try { return v instanceof c; } catch (e) { return e.constructor.name; } }
function f2(v, c) { try { return v instanceof c; } catch (e) { return e.constructor.name; } }
function f3(v, c) { try { return v instanceof c; } catch (e) { return e.constructor.name; } }
function f4(v, c) { try { return v instanceof c; } catch (e) { return e.constructor.name; } }
function f5(v, c) { try { return v instanceof c; } catch (e) { return e.constructor.name; } }
function f6(v, c) { try { return v instanceof c; } catch (e) { return e.constructor.name; } }
function f7(v, c) { try { return v instanceof c; } catch (e) { return e.constructor.name; } }
// Each site first meets a right-hand side that is no function, most of
// them with no heap reference at all, then a constructor, then another.
var firsts = [1, undefined, null, true, "str", Symbol("s"), 1n];
var sites = [f1, f2, f3, f4, f5, f6, f7];
function K() {}
var k = new K();
for (var i = 0; i < sites.length; i++) {
  var site = sites[i];
  log.push([site(k, firsts[i]), site(k, firsts[i]), site(k, K), site({}, K), site(1, K), site(k, firsts[(i + 1) % 7]), site(k, K)].join("/"));
}
// Global constructors at warmed sites; the global is replaced, restored,
// deleted and defined again.
function isMap(v) { return v instanceof Map; }
function isU8(v) { return v instanceof Uint8Array; }
function isErr(v) { return v instanceof Error; }
var m = new Map(), u8 = new Uint8Array(1), err = new TypeError("x"), plain = {};
function row() { try { return [isMap(m), isMap(plain), isU8(u8), isU8(plain), isU8(m), isErr(err), isErr(plain), isErr(null)].map(Number).join(""); } catch (e) { return e.constructor.name; } }
log.push(row(), row());
var NativeMap = Map;
globalThis.Map = function Map() {};
log.push(row(), isMap(new Map()), isMap(new NativeMap()));
globalThis.Map = NativeMap;
log.push(row());
delete globalThis.Map;
log.push(row(), typeof Map);
globalThis.Map = NativeMap;
log.push(row());
// The same function written back is a new value cell; the global then
// becomes an accessor, whose calls are counted, and data again.
globalThis.Map = NativeMap;
log.push(row(), row());
var gets = 0;
Object.defineProperty(globalThis, "Map", { get: function () { gets++; return NativeMap; }, configurable: true });
log.push(row(), row(), gets);
Object.defineProperty(globalThis, "Map", { value: NativeMap, writable: true, configurable: true });
log.push(row(), typeof Map == "function" && m instanceof Map, typeof Nowhere == "function" && m instanceof Nowhere);
// A bound function and a Proxy as the right-hand side of a warmed site.
function is(v, c) { return v instanceof c; }
log.push(is(m, Map), is(m, Map), is(plain, Map));
var bound = Map.bind(null);
log.push(is(m, bound), is(plain, bound));
var traps = [];
var proxied = new Proxy(Map, { get: function (t, key, r) { traps.push(typeof key === "symbol" ? "symbol" : key); return Reflect.get(t, key, r); } });
log.push(is(m, proxied), is(plain, proxied), traps.join("/"), is(m, Map));
// The typed array constructor's edge to %TypedArray% changes and returns.
var TypedArray = Object.getPrototypeOf(Uint8Array);
Object.setPrototypeOf(Uint8Array, Function.prototype);
log.push(row());
Object.setPrototypeOf(Uint8Array, TypedArray);
log.push(row());
// %TypedArray%'s own edge to Function.prototype changes: a null edge has
// no @@hasInstance, so the constructor's own "prototype" decides.
Object.setPrototypeOf(TypedArray, null);
log.push(row());
Object.setPrototypeOf(TypedArray, Function.prototype);
log.push(row());
// @@hasInstance defined on %TypedArray% after warming, and removed.
Object.defineProperty(TypedArray, Symbol.hasInstance, { value: function (v) { return v === plain; }, configurable: true });
log.push(row());
delete TypedArray[Symbol.hasInstance];
log.push(row());
// A later Script shadows a global constructor with a lexical binding.
__scriptwRealmEvalScript("let Error = function Shadow() {};");
log.push(row(), isErr(err), isErr(new Error()), row());
print(log.join());
"#,
        "TypeError/TypeError/true/false/false/TypeError/true,TypeError/TypeError/true/false/false/TypeError/true,TypeError/TypeError/true/false/false/TypeError/true,TypeError/TypeError/true/false/false/TypeError/true,TypeError/TypeError/true/false/false/TypeError/true,TypeError/TypeError/true/false/false/TypeError/true,TypeError/TypeError/true/false/false/TypeError/true,10100100,10100100,00100100,true,false,10100100,ReferenceError,undefined,10100100,10100100,10100100,10100100,10100100,4,10100100,true,false,true,true,false,true,false,true,false,symbol/prototype/symbol/prototype,true,10100100,10100100,10100100,10100100,10010100,10100100,10100000,false,true,10100000",
    );
}

#[test]
fn one_site_runs_for_two_realms() {
    // The same Script text makes a function in each of two Realms, so one
    // site's records meet two Global Environment Records whose global
    // objects then change independently.
    run_as_script_function_and_module(
        r#"
var log = [];
var first = __scriptwCreateRealm(), second = __scriptwCreateRealm();
first.global.shared = "one";
second.global.shared = "two";
first.global.only = 1;
function make(realm) {
  return realm.evalScript("(function () { var text; try { text = String(shared); } catch (e) { text = e instanceof ReferenceError ? 'missing' : 'other'; } return text + '/' + typeof shared + '/' + typeof only + '/' + ({} instanceof Object) + '/' + (typeof Map == 'function' && new Map() instanceof Map); })");
}
var readFirst = make(first), readSecond = make(second);
log.push(readFirst(), readSecond(), readFirst(), readSecond());
second.global.only = "now";
delete first.global.shared;
log.push(readFirst(), readSecond(), readFirst(), readSecond());
first.global.shared = "back";
delete second.global.only;
second.global.Map = 5;
log.push(readFirst(), readSecond(), readFirst(), readSecond());
log.push(typeof shared, typeof only);
print(log.join());
"#,
        "one/string/number/true/true,two/string/undefined/true/true,one/string/number/true/true,two/string/undefined/true/true,missing/undefined/number/true/true,two/string/string/true/true,missing/undefined/number/true/true,two/string/string/true/true,back/string/number/true/true,two/string/undefined/true/false,back/string/number/true/true,two/string/undefined/true/false,undefined,undefined",
    );
}
