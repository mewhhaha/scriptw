//! A static read site keeps a bounded chain of records, each a complete set
//! of proofs for the site's key: an own slot or absence before a closed tail
//! by the receiver's shape, an own dictionary entry or intrinsic row, and a
//! place on the receiver's live prototype. The hit entry asks each record in
//! turn and answers only what the live receiver, its prototype and its
//! storage re-prove; a receiver no record answers goes to the miss kernel
//! while the site still learns and through the call-free walk afterwards.
//! Each script warms one site with several receivers and then changes, one
//! at a time, a fact some record's answer rested on. Expectations are Node
//! 24's, taken by running each body as a Script, as function code and as a
//! module, with `vm` contexts for the second Realm.
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
fn one_site_keeps_a_proof_for_each_receiver_shape_until_a_fact_changes() {
    // One site reads an absent key from five shapes and an own value from
    // three more, so its chain holds an own slot, absences before the
    // %Object.prototype% tail and nothing for the rest. The key is then
    // defined and written on receivers, defined on %Object.prototype% as
    // data and as a counted getter and deleted again, defined on a
    // receiver as a counted getter and as a read-only value, and found
    // through a replaced prototype, a deeper one and a null one. The last
    // statement confirms that %Object.prototype% keeps its null prototype.
    run_as_script_function_and_module(
        r#"
var log = [];
var count = 0;
function read(o) { return o.probe; }
function show(label, list) {
  var out = [];
  for (var i = 0; i < list.length; i++) {
    var v;
    try { v = read(list[i]); } catch (e) { v = e.constructor.name; }
    out.push(typeof v === "function" ? "fn" : String(v));
  }
  log.push(label + ":" + out.join("/"));
}
var a = {}, b = { x: 1 }, c = { x: 1, y: 2 }, d = { x: 1, y: 2, z: 3 };
var e = { probe: "own-e", q: 1, r: 2, s: 3 }, f = { p1: 1, p2: 2, p3: 3, p4: 4, p5: 5 };
var g = { probe: "own-g" }, h = { q: 1, r: 2, s: 3, probe: "own-h" };
var list = [a, b, c, d, e, f, g, h];
show("warm", list); show("warm", list); show("warm", list);
c.probe = "c"; show("own-c", list);
h.probe = "h2"; e.probe = 0; show("written", list);
Object.prototype.probe = "proto"; show("proto-data", list); show("proto-data", list);
Object.defineProperty(Object.prototype, "probe", { get: function () { count++; return "g" + count; }, configurable: true });
show("getter", list); show("getter", list);
delete Object.prototype.probe; show("deleted", list); show("deleted", list);
delete c.probe; delete g.probe; show("own-deleted", list);
Object.defineProperty(d, "probe", { get: function () { count++; return "own-get" + count; }, configurable: true });
show("own-getter", list); show("own-getter", list);
Object.defineProperty(d, "probe", { value: "fixed", writable: false, configurable: true });
show("own-fixed", list);
delete d.probe; show("own-gone", list);
var proto = { probe: "from-proto" };
Object.setPrototypeOf(b, proto); show("setproto", list); show("setproto", list);
proto.probe = "changed"; show("proto-changed", list);
delete proto.probe; show("proto-deleted", list);
Object.prototype.probe = "deep"; show("deep", list);
Object.setPrototypeOf(b, null); show("null-proto", list);
delete Object.prototype.probe; show("null-proto-clean", list);
Object.setPrototypeOf(b, Object.prototype); show("restored", list);
Object.setPrototypeOf(Object.prototype, null);
log.push(count);
print(log.join(";"));
"#,
        r#"warm:undefined/undefined/undefined/undefined/own-e/undefined/own-g/own-h;warm:undefined/undefined/undefined/undefined/own-e/undefined/own-g/own-h;warm:undefined/undefined/undefined/undefined/own-e/undefined/own-g/own-h;own-c:undefined/undefined/c/undefined/own-e/undefined/own-g/own-h;written:undefined/undefined/c/undefined/0/undefined/own-g/h2;proto-data:proto/proto/c/proto/0/proto/own-g/h2;proto-data:proto/proto/c/proto/0/proto/own-g/h2;getter:g1/g2/c/g3/0/g4/own-g/h2;getter:g5/g6/c/g7/0/g8/own-g/h2;deleted:undefined/undefined/c/undefined/0/undefined/own-g/h2;deleted:undefined/undefined/c/undefined/0/undefined/own-g/h2;own-deleted:undefined/undefined/undefined/undefined/0/undefined/undefined/h2;own-getter:undefined/undefined/undefined/own-get9/0/undefined/undefined/h2;own-getter:undefined/undefined/undefined/own-get10/0/undefined/undefined/h2;own-fixed:undefined/undefined/undefined/fixed/0/undefined/undefined/h2;own-gone:undefined/undefined/undefined/undefined/0/undefined/undefined/h2;setproto:undefined/from-proto/undefined/undefined/0/undefined/undefined/h2;setproto:undefined/from-proto/undefined/undefined/0/undefined/undefined/h2;proto-changed:undefined/changed/undefined/undefined/0/undefined/undefined/h2;proto-deleted:undefined/undefined/undefined/undefined/0/undefined/undefined/h2;deep:deep/deep/deep/deep/0/deep/deep/h2;null-proto:deep/undefined/deep/deep/0/deep/deep/h2;null-proto-clean:undefined/undefined/undefined/undefined/0/undefined/undefined/h2;restored:undefined/undefined/undefined/undefined/0/undefined/undefined/h2;10"#,
    );
}

#[test]
fn one_site_reads_ordinary_exotic_and_primitive_receivers_exactly() {
    // The same site meets ordinary objects, dictionary-mode objects with and
    // without the key, one of them below a holder that has it, an Array,
    // functions, one with defined own properties, every primitive, a Proxy
    // and an object whose prototype is one (each trap is counted),
    // collections, a Promise, a Date, a RegExp, a typed array, wrappers, a
    // null-prototype object and null and undefined. Each intrinsic
    // prototype then gains the key, the String and Number prototypes as
    // strict getters that report their primitive receiver, and loses it
    // again; dictionary receivers gain and lose it, as a value and as a
    // counted getter; an Array and functions gain it as an own property;
    // %Object.prototype% gains it; and the holder's value changes and goes.
    run_as_script_function_and_module(
        r#"
var log = [];
var hits = 0, count = 0;
function read(o) { return o.probe; }
function show(label, list) {
  var out = [];
  for (var i = 0; i < list.length; i++) {
    var v;
    try { v = read(list[i]); } catch (e) { v = e.constructor.name; }
    out.push(typeof v === "function" ? "fn" : String(v));
  }
  log.push(label + ":" + out.join("/"));
}
var target = { probe: "t" };
var proxy = new Proxy(target, { get: function (t, k, r) { hits++; return k === "probe" ? "proxied" + hits : t[k]; } });
var viaProxy = Object.create(proxy);
var dict = {}, dict2 = {}, i;
for (i = 0; i < 40; i++) { dict["k" + i] = i; dict2["k" + i] = i; }
delete dict.k3; delete dict2.k5; dict2.probe = "dict-own";
var arr = [1, 2, 3], fn = function () {}, bound = fn.bind(null), sym = Symbol("s"), big = 10n;
var tagged = Object.defineProperties(function () {}, { mark: { value: 1 }, other: { value: 2 } });
var holder = { probe: "held" }, dictChild = Object.create(holder);
for (i = 0; i < 40; i++) dictChild["c" + i] = i;
delete dictChild.c1;
var list = [{}, arr, fn, "str", 5, true, sym, big, dict, dict2, viaProxy, proxy, new Map(), new WeakMap(),
  new Set(), new WeakSet(), Promise.resolve(1), new Date(0), /x/, bound, Object.create(null), new Uint8Array(2),
  new String("boxed"), new Number(1), null, undefined, [], function named() {}, tagged, dictChild];
show("warm", list); show("warm", list); show("warm", list);
Array.prototype.probe = "A"; show("array-proto", list);
Function.prototype.probe = "F"; show("function-proto", list);
String.prototype.probe = "S"; Number.prototype.probe = "N"; Boolean.prototype.probe = "B";
Symbol.prototype.probe = "Y"; BigInt.prototype.probe = "G"; show("primitive-protos", list);
Object.defineProperty(String.prototype, "probe", { get: function () { "use strict"; count++; return typeof this + count; }, configurable: true });
Object.defineProperty(Number.prototype, "probe", { get: function () { "use strict"; count++; return typeof this + count; }, configurable: true });
show("primitive-getters", list); show("primitive-getters", list);
delete Array.prototype.probe; delete Function.prototype.probe; delete String.prototype.probe;
delete Number.prototype.probe; delete Boolean.prototype.probe; delete Symbol.prototype.probe; delete BigInt.prototype.probe;
show("cleared", list);
dict.probe = "late"; show("dict-late", list); delete dict.probe; show("dict-gone", list);
delete dict2.probe; show("dict2-gone", list); dict2.probe = "dict2-back"; show("dict2-back", list);
arr.probe = "own-arr"; fn.probe = "own-fn"; show("own-exotic", list);
delete arr.probe; delete fn.probe; show("own-exotic-gone", list);
Map.prototype.probe = "M"; WeakMap.prototype.probe = "W"; Set.prototype.probe = "T"; Promise.prototype.probe = "P";
show("collections", list);
delete Map.prototype.probe; delete WeakMap.prototype.probe; delete Set.prototype.probe; delete Promise.prototype.probe;
Object.prototype.probe = "O"; show("object-proto", list); show("object-proto", list);
target.probe = "t2"; show("target", list);
holder.probe = "held2"; show("holder-write", list);
delete holder.probe; show("holder-delete", list);
Object.defineProperty(dictChild, "probe", { get: function () { count++; return "child-get" + count; }, configurable: true });
show("dictionary-getter", list); show("dictionary-getter", list);
delete dictChild.probe; holder.probe = "held3";
Object.defineProperty(tagged, "probe", { value: "tagged-own", configurable: true }); show("tagged-own", list);
delete tagged.probe; show("tagged-gone", list);
delete Object.prototype.probe; show("end", list);
log.push(hits, count);
print(log.join(";"));
"#,
        r#"warm:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/dict-own/proxied1/proxied2/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held;warm:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/dict-own/proxied3/proxied4/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held;warm:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/dict-own/proxied5/proxied6/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held;array-proto:undefined/A/undefined/undefined/undefined/undefined/undefined/undefined/undefined/dict-own/proxied7/proxied8/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/A/undefined/undefined/held;function-proto:undefined/A/F/undefined/undefined/undefined/undefined/undefined/undefined/dict-own/proxied9/proxied10/undefined/undefined/undefined/undefined/undefined/undefined/undefined/F/undefined/undefined/undefined/undefined/TypeError/TypeError/A/F/F/held;primitive-protos:undefined/A/F/S/N/B/Y/G/undefined/dict-own/proxied11/proxied12/undefined/undefined/undefined/undefined/undefined/undefined/undefined/F/undefined/undefined/S/N/TypeError/TypeError/A/F/F/held;primitive-getters:undefined/A/F/string1/number2/B/Y/G/undefined/dict-own/proxied13/proxied14/undefined/undefined/undefined/undefined/undefined/undefined/undefined/F/undefined/undefined/object3/object4/TypeError/TypeError/A/F/F/held;primitive-getters:undefined/A/F/string5/number6/B/Y/G/undefined/dict-own/proxied15/proxied16/undefined/undefined/undefined/undefined/undefined/undefined/undefined/F/undefined/undefined/object7/object8/TypeError/TypeError/A/F/F/held;cleared:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/dict-own/proxied17/proxied18/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held;dict-late:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/late/dict-own/proxied19/proxied20/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held;dict-gone:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/dict-own/proxied21/proxied22/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held;dict2-gone:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/proxied23/proxied24/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held;dict2-back:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/dict2-back/proxied25/proxied26/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held;own-exotic:undefined/own-arr/own-fn/undefined/undefined/undefined/undefined/undefined/undefined/dict2-back/proxied27/proxied28/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held;own-exotic-gone:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/dict2-back/proxied29/proxied30/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held;collections:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/dict2-back/proxied31/proxied32/M/W/T/undefined/P/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held;object-proto:O/O/O/O/O/O/O/O/O/dict2-back/proxied33/proxied34/O/O/O/O/O/O/O/O/undefined/O/O/O/TypeError/TypeError/O/O/O/held;object-proto:O/O/O/O/O/O/O/O/O/dict2-back/proxied35/proxied36/O/O/O/O/O/O/O/O/undefined/O/O/O/TypeError/TypeError/O/O/O/held;target:O/O/O/O/O/O/O/O/O/dict2-back/proxied37/proxied38/O/O/O/O/O/O/O/O/undefined/O/O/O/TypeError/TypeError/O/O/O/held;holder-write:O/O/O/O/O/O/O/O/O/dict2-back/proxied39/proxied40/O/O/O/O/O/O/O/O/undefined/O/O/O/TypeError/TypeError/O/O/O/held2;holder-delete:O/O/O/O/O/O/O/O/O/dict2-back/proxied41/proxied42/O/O/O/O/O/O/O/O/undefined/O/O/O/TypeError/TypeError/O/O/O/O;dictionary-getter:O/O/O/O/O/O/O/O/O/dict2-back/proxied43/proxied44/O/O/O/O/O/O/O/O/undefined/O/O/O/TypeError/TypeError/O/O/O/child-get9;dictionary-getter:O/O/O/O/O/O/O/O/O/dict2-back/proxied45/proxied46/O/O/O/O/O/O/O/O/undefined/O/O/O/TypeError/TypeError/O/O/O/child-get10;tagged-own:O/O/O/O/O/O/O/O/O/dict2-back/proxied47/proxied48/O/O/O/O/O/O/O/O/undefined/O/O/O/TypeError/TypeError/O/O/tagged-own/held3;tagged-gone:O/O/O/O/O/O/O/O/O/dict2-back/proxied49/proxied50/O/O/O/O/O/O/O/O/undefined/O/O/O/TypeError/TypeError/O/O/O/held3;end:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/dict2-back/proxied51/proxied52/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/TypeError/TypeError/undefined/undefined/undefined/held3;52;10"#,
    );
}

#[test]
fn one_site_follows_prototype_places_and_more_shapes_than_it_records() {
    // Instances read through their constructor's prototype one, two and
    // three objects up, beside twelve literal shapes, more than a site
    // chains, of which every third owns the key. The holder's value
    // changes, is shadowed, becomes a counted getter, moves into a
    // dictionary-mode holder, is written and deleted there, and the chain
    // is cut. A second site reads the method `get` from a Map, a WeakMap,
    // an object that owns it and receivers that lack it, while the method
    // is shadowed by an own property, replaced, removed, inherited from
    // %Object.prototype% and made a counted getter.
    run_as_script_function_and_module(
        r#"
var log = [];
var count = 0;
function read(o) { return o.probe; }
function method(o) { return o.get; }
function show(label, list, reader) {
  var out = [];
  for (var i = 0; i < list.length; i++) {
    var v;
    try { v = reader(list[i]); } catch (e) { v = e.constructor.name; }
    out.push(typeof v === "function" ? "fn:" + v.name : String(v));
  }
  log.push(label + ":" + out.join("/"));
}
function P() {} P.prototype.probe = "P";
function Q() {} Q.prototype = Object.create(P.prototype);
function R() { this.a = 1; this.b = 2; this.c = 3; } R.prototype = Object.create(Q.prototype);
var p1 = new P(), p2 = new P(), q = new Q(), r = new R(); p2.other = 1;
var many = [], i, j;
for (i = 0; i < 12; i++) { var o = {}; for (j = 0; j <= i; j++) o["m" + i + "_" + j] = j; if (i % 3 === 0) o.probe = "own" + i; many.push(o); }
var list = [p1, p2, q, r].concat(many);
show("warm", list, read); show("warm", list, read); show("warm", list, read); show("warm", list, read);
P.prototype.probe = "P2"; show("value", list, read);
Q.prototype.probe = "Q"; show("shadow", list, read); delete Q.prototype.probe; show("unshadow", list, read);
p1.probe = "own"; show("own", list, read); delete p1.probe; show("own-gone", list, read);
Object.defineProperty(P.prototype, "probe", { get: function () { count++; return "get" + count; }, configurable: true });
show("getter", list, read); show("getter", list, read);
for (i = 0; i < 40; i++) P.prototype["d" + i] = i; delete P.prototype.d7;
Object.defineProperty(P.prototype, "probe", { value: "dict", writable: true, configurable: true });
show("dictionary", list, read); show("dictionary", list, read);
P.prototype.probe = "dict2"; show("dictionary-write", list, read);
delete P.prototype.probe; show("dictionary-delete", list, read);
Object.prototype.probe = "O"; show("object-proto", list, read);
many[1].probe = "late1"; many[4].probe = "late4"; delete many[0].probe; show("late", list, read);
Object.setPrototypeOf(Q.prototype, null); show("cut", list, read);
delete Object.prototype.probe; show("end", list, read);
var map = new Map(), weak = new WeakMap(), plain = { get: 1 }, own = new Map();
var gets = [map, weak, plain, own, new Set(), {}, [], "s"];
show("methods", gets, method); show("methods", gets, method); show("methods", gets, method);
own.get = "own-get"; show("own-method", gets, method);
var original = Map.prototype.get;
Map.prototype.get = function replaced() {}; show("replaced", gets, method);
delete Map.prototype.get; show("removed", gets, method);
Object.prototype.get = "O-get"; show("inherited", gets, method);
Map.prototype.get = original; delete Object.prototype.get; delete own.get; show("restored", gets, method);
Object.defineProperty(WeakMap.prototype, "get", { get: function () { count++; return "weak-getter" + count; }, configurable: true });
show("weak-getter", gets, method);
log.push(count);
print(log.join(";"));
"#,
        r#"warm:P/P/P/P/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;warm:P/P/P/P/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;warm:P/P/P/P/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;warm:P/P/P/P/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;value:P2/P2/P2/P2/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;shadow:P2/P2/Q/Q/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;unshadow:P2/P2/P2/P2/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;own:own/P2/P2/P2/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;own-gone:P2/P2/P2/P2/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;getter:get1/get2/get3/get4/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;getter:get5/get6/get7/get8/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;dictionary:dict/dict/dict/dict/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;dictionary:dict/dict/dict/dict/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;dictionary-write:dict2/dict2/dict2/dict2/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;dictionary-delete:undefined/undefined/undefined/undefined/own0/undefined/undefined/own3/undefined/undefined/own6/undefined/undefined/own9/undefined/undefined;object-proto:O/O/O/O/own0/O/O/own3/O/O/own6/O/O/own9/O/O;late:O/O/O/O/O/late1/O/own3/late4/O/own6/O/O/own9/O/O;cut:O/O/undefined/undefined/O/late1/O/own3/late4/O/own6/O/O/own9/O/O;end:undefined/undefined/undefined/undefined/undefined/late1/undefined/own3/late4/undefined/own6/undefined/undefined/own9/undefined/undefined;methods:fn:get/fn:get/1/fn:get/undefined/undefined/undefined/undefined;methods:fn:get/fn:get/1/fn:get/undefined/undefined/undefined/undefined;methods:fn:get/fn:get/1/fn:get/undefined/undefined/undefined/undefined;own-method:fn:get/fn:get/1/own-get/undefined/undefined/undefined/undefined;replaced:fn:replaced/fn:get/1/own-get/undefined/undefined/undefined/undefined;removed:undefined/fn:get/1/own-get/undefined/undefined/undefined/undefined;inherited:O-get/fn:get/1/own-get/O-get/O-get/O-get/O-get;restored:fn:get/fn:get/1/fn:get/undefined/undefined/undefined/undefined;weak-getter:fn:get/weak-getter9/1/fn:get/undefined/undefined/undefined/undefined;9"#,
    );
}

#[test]
fn one_site_serves_receivers_of_two_realms_by_their_own_prototypes() {
    // Objects, Arrays, functions and Maps of two Realms read through one
    // site, whose records name layouts and shapes that both Realms share.
    // Each Realm's prototypes then gain and lose the key separately, so an
    // answer for one Realm's receiver must not serve the other's. A
    // primitive reads through the running Realm's prototype whatever the
    // other Realm's holds, and receivers that exchange prototypes follow
    // their new chain.
    run_as_script_function_and_module(
        r#"
var log = [];
var count = 0;
var realm = __scriptwCreateRealm(), foreign = realm.global;
function read(o) { return o.probe; }
function show(label, list) {
  var out = [];
  for (var i = 0; i < list.length; i++) {
    var v;
    try { v = read(list[i]); } catch (e) { v = e.constructor.name; }
    out.push(typeof v === "function" ? "fn" : String(v));
  }
  log.push(label + ":" + out.join("/"));
}
var near = {}, far = new foreign.Object(), nearArr = [], farArr = new foreign.Array(), nearFn = function () {};
var farFn = foreign.Object, farMap = new foreign.Map(), nearMap = new Map(), farShaped = new foreign.Object();
farShaped.x = 1; farShaped.y = 2; farShaped.z = 3;
var nearShaped = { x: 1, y: 2, z: 3 };
var list = [near, far, nearArr, farArr, nearFn, farFn, nearMap, farMap, nearShaped, farShaped, "str", 5];
show("warm", list); show("warm", list); show("warm", list);
foreign.Object.prototype.probe = "far-O"; show("far-object", list); show("far-object", list);
Object.prototype.probe = "near-O"; show("both-object", list);
delete foreign.Object.prototype.probe; show("near-object", list);
delete Object.prototype.probe; show("neither", list);
foreign.Array.prototype.probe = "far-A"; foreign.Function.prototype.probe = "far-F"; foreign.Map.prototype.probe = "far-M";
show("far-specific", list);
Array.prototype.probe = "near-A"; Function.prototype.probe = "near-F"; Map.prototype.probe = "near-M";
show("both-specific", list);
foreign.String.prototype.probe = "far-S"; foreign.Number.prototype.probe = "far-N"; show("far-primitive", list);
String.prototype.probe = "near-S"; Number.prototype.probe = "near-N"; show("near-primitive", list);
Object.defineProperty(foreign.Object.prototype, "probe", { get: function () { count++; return "far-get" + count; }, configurable: true });
delete foreign.Array.prototype.probe; delete foreign.Function.prototype.probe; delete foreign.Map.prototype.probe;
delete Array.prototype.probe; delete Function.prototype.probe; delete Map.prototype.probe;
delete String.prototype.probe; delete Number.prototype.probe;
show("far-getter", list); show("far-getter", list);
delete foreign.Object.prototype.probe;
Object.setPrototypeOf(far, Object.prototype); Object.setPrototypeOf(near, foreign.Object.prototype);
foreign.Object.prototype.probe = "far-O2"; show("swapped", list);
delete foreign.Object.prototype.probe; delete foreign.String.prototype.probe; delete foreign.Number.prototype.probe;
show("end", list);
log.push(count);
print(log.join(";"));
"#,
        r#"warm:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined;warm:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined;warm:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined;far-object:undefined/far-O/undefined/far-O/undefined/far-O/undefined/far-O/undefined/far-O/undefined/undefined;far-object:undefined/far-O/undefined/far-O/undefined/far-O/undefined/far-O/undefined/far-O/undefined/undefined;both-object:near-O/far-O/near-O/far-O/near-O/far-O/near-O/far-O/near-O/far-O/near-O/near-O;near-object:near-O/undefined/near-O/undefined/near-O/undefined/near-O/undefined/near-O/undefined/near-O/near-O;neither:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined;far-specific:undefined/undefined/undefined/far-A/undefined/far-F/undefined/far-M/undefined/undefined/undefined/undefined;both-specific:undefined/undefined/near-A/far-A/near-F/far-F/near-M/far-M/undefined/undefined/undefined/undefined;far-primitive:undefined/undefined/near-A/far-A/near-F/far-F/near-M/far-M/undefined/undefined/undefined/undefined;near-primitive:undefined/undefined/near-A/far-A/near-F/far-F/near-M/far-M/undefined/undefined/near-S/near-N;far-getter:undefined/far-get1/undefined/far-get2/undefined/far-get3/undefined/far-get4/undefined/far-get5/undefined/undefined;far-getter:undefined/far-get6/undefined/far-get7/undefined/far-get8/undefined/far-get9/undefined/far-get10/undefined/undefined;swapped:far-O2/undefined/undefined/far-O2/undefined/far-O2/undefined/far-O2/undefined/far-O2/undefined/undefined;end:undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined/undefined;10"#,
    );
}
