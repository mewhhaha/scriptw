//! Shortcuts in front of the general property and call paths that a retained
//! server request reaches: the proof that an immutable intrinsic layout lacks
//! a Symbol key, stores whose receiver is a collection or a Promise, named
//! reads from Arrays in the general Get, ArraySpeciesCreate for an untouched
//! species, the final "length" store of the mutating Array methods, and
//! strict functions whose parameters are object binding patterns. Each script
//! warms its sites with the admitted case and then runs the same sites with
//! every excluded one. Expectations are Node 24's, taken by running each
//! body as a Script, as function code and as a module.
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
        timeout_ms: Some(60_000),
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
fn symbol_keys_ask_an_intrinsic_layout_for_its_symbol_rows() {
    // A Symbol key is absent from an immutable intrinsic layout that holds
    // no Symbol row, which one word after the layout's String index says.
    // Twelve Symbols are read from thirty-three kinds of receiver, twice
    // each, and then again after Object.prototype gains a Symbol key (and
    // stops being a layout), after accessors under Symbol keys appear on
    // Array.prototype and Function.prototype, after their deletion, after
    // Symbol rows of the layouts themselves are redefined and deleted, with
    // a Proxy on the chain, and with own Symbol properties on every storage
    // form across a collection.
    run_as_script_function_and_module(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
function churn() { var keep = []; for (var i = 0; i < 40000; i++) keep[i & 63] = { i: i, a: [i, i + 1] }; return keep.length; }
function kind(v) { return v === undefined ? "u" : v === null ? "n" : typeof v === "function" ? "f" : typeof v === "object" ? "o" : typeof v === "symbol" ? "s" : String(v); }
function read(o, k) { return o[k]; }
var mine = Symbol("mine"), other = Symbol("other");
var plain = { a: 1 }, fn = function () {}, child = Object.create(plain);
function subjects() {
  return [plain, [1, 2], fn, new Map(), new Set(), Promise.resolve(1), new Date(0), /x/, Object("s"), Object(1), new Error("e"), new Uint8Array(2), (function* () {})(), (function () { return arguments; })(1), fn.bind(null), Object.create(null), child, new WeakMap(), new WeakSet(), Math, JSON, Reflect, Array.prototype, Object.prototype, Function.prototype, Symbol.prototype, Promise, Array, Map, "text", 7, true, mine];
}
var keys = [mine, other, Symbol.iterator, Symbol.asyncIterator, Symbol.toStringTag, Symbol.toPrimitive, Symbol.hasInstance, Symbol.species, Symbol.unscopables, Symbol.isConcatSpreadable, Symbol.match, Symbol.for("react.element")];
function sweep(tag) {
  var list = subjects(), rows = [];
  for (var r = 0; r < 2; r++) {
    rows.length = 0;
    for (var i = 0; i < list.length; i++) {
      var line = "";
      for (var j = 0; j < keys.length; j++) line += show(function () { return kind(read(list[i], keys[j])); }) + (j + 1 < keys.length ? "," : "");
      rows.push(line);
    }
  }
  log.push(tag + "=" + rows.join("/"));
}
sweep("warm");
churn();
// Object.prototype stops being its immutable layout and gains a Symbol key.
Object.prototype[mine] = "op";
sweep("object-prototype");
// An accessor under a Symbol key sees the original receiver.
var seen = [];
Object.defineProperty(Array.prototype, other, { get: function () { seen.push(kind(this) + ":" + (Array.isArray(this) ? this.length : "-")); return "ag"; }, configurable: true });
Object.defineProperty(Function.prototype, mine, { get: function () { return typeof this; }, configurable: true });
sweep("accessors");
log.push(seen.length, seen.slice(0, 4).join("|"));
delete Object.prototype[mine];
delete Array.prototype[other];
delete Function.prototype[mine];
sweep("deleted");
// Symbol rows of the intrinsic layouts themselves change.
var iterator = Array.prototype[Symbol.iterator];
Array.prototype[Symbol.iterator] = 5;
Object.defineProperty(Map.prototype, Symbol.toStringTag, { value: "Changed", configurable: true });
delete Math[Symbol.toStringTag];
log.push(kind(read([], Symbol.iterator)), read(new Map(), Symbol.toStringTag), kind(read(Math, Symbol.toStringTag)), Object.prototype.toString.call(new Map()), Object.prototype.toString.call(Math));
Array.prototype[Symbol.iterator] = iterator;
log.push(kind(read([], Symbol.iterator)), [...[1, 2]].join(""));
// A Proxy on the chain is asked for the Symbol, once per read.
var traps = [];
var proxy = new Proxy({}, { get: function (t, k, r) { traps.push(typeof k === "symbol" ? String(k.description) : k); return k === mine ? "px" : undefined; } });
var under = Object.create(proxy), deep = Object.create(under);
under[other] = "own";
log.push(read(deep, mine), read(deep, other), kind(read(deep, Symbol.iterator)), traps.join("|"));
// Own Symbol properties, on every storage form, shadow and are deleted.
var bag = {}, big = {};
for (var n = 0; n < 40; n++) big["k" + n] = n;
[bag, big, [], fn, new Map(), Object.create(null)].forEach(function (o, i) {
  log.push(kind(read(o, mine)));
  o[mine] = "own" + i;
  log.push(read(o, mine));
  churn();
  log.push(read(o, mine), delete o[mine], kind(read(o, mine)));
});
// Symbols that were never used as a key, and a registered one.
log.push(kind(read(plain, Symbol())), kind(read([], Symbol("fresh"))), kind(read(Math, Symbol.for("x"))), kind(read(fn, Symbol.for("x"))));
print(log.join());
"##,
        r##"warm=u,u,u,u,u,u,u,u,u,u,u,u/u,u,f,u,u,u,u,u,o,u,u,u/u,u,u,u,u,u,f,u,u,u,u,u/u,u,f,u,Map,u,u,u,u,u,u,u/u,u,f,u,Set,u,u,u,u,u,u,u/u,u,u,u,Promise,u,u,u,u,u,u,u/u,u,u,u,u,f,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,f,u/u,u,f,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,f,u,Uint8Array,u,u,u,u,u,u,u/u,u,f,u,Generator,u,u,u,u,u,u,u/u,u,f,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,f,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,WeakMap,u,u,u,u,u,u,u/u,u,u,u,WeakSet,u,u,u,u,u,u,u/u,u,u,u,Math,u,u,u,u,u,u,u/u,u,u,u,JSON,u,u,u,u,u,u,u/u,u,u,u,Reflect,u,u,u,u,u,u,u/u,u,f,u,u,u,u,u,o,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,f,u,u,u,u,u/u,u,u,u,Symbol,f,u,u,u,u,u,u/u,u,u,u,u,u,f,f,u,u,u,u/u,u,u,u,u,u,f,f,u,u,u,u/u,u,u,u,u,u,f,f,u,u,u,u/u,u,f,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,Symbol,f,u,u,u,u,u,u,object-prototype=op,u,u,u,u,u,u,u,u,u,u,u/op,u,f,u,u,u,u,u,o,u,u,u/op,u,u,u,u,u,f,u,u,u,u,u/op,u,f,u,Map,u,u,u,u,u,u,u/op,u,f,u,Set,u,u,u,u,u,u,u/op,u,u,u,Promise,u,u,u,u,u,u,u/op,u,u,u,u,f,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,f,u/op,u,f,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/op,u,f,u,Uint8Array,u,u,u,u,u,u,u/op,u,f,u,Generator,u,u,u,u,u,u,u/op,u,f,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,f,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/op,u,u,u,WeakMap,u,u,u,u,u,u,u/op,u,u,u,WeakSet,u,u,u,u,u,u,u/op,u,u,u,Math,u,u,u,u,u,u,u/op,u,u,u,JSON,u,u,u,u,u,u,u/op,u,u,u,Reflect,u,u,u,u,u,u,u/op,u,f,u,u,u,u,u,o,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,f,u,u,u,u,u/op,u,u,u,Symbol,f,u,u,u,u,u,u/op,u,u,u,u,u,f,f,u,u,u,u/op,u,u,u,u,u,f,f,u,u,u,u/op,u,u,u,u,u,f,f,u,u,u,u/op,u,f,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/op,u,u,u,Symbol,f,u,u,u,u,u,u,accessors=op,u,u,u,u,u,u,u,u,u,u,u/op,ag,f,u,u,u,u,u,o,u,u,u/function,u,u,u,u,u,f,u,u,u,u,u/op,u,f,u,Map,u,u,u,u,u,u,u/op,u,f,u,Set,u,u,u,u,u,u,u/op,u,u,u,Promise,u,u,u,u,u,u,u/op,u,u,u,u,f,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,f,u/op,u,f,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/op,u,f,u,Uint8Array,u,u,u,u,u,u,u/op,u,f,u,Generator,u,u,u,u,u,u,u/op,u,f,u,u,u,u,u,u,u,u,u/function,u,u,u,u,u,f,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/op,u,u,u,WeakMap,u,u,u,u,u,u,u/op,u,u,u,WeakSet,u,u,u,u,u,u,u/op,u,u,u,Math,u,u,u,u,u,u,u/op,u,u,u,JSON,u,u,u,u,u,u,u/op,u,u,u,Reflect,u,u,u,u,u,u,u/op,ag,f,u,u,u,u,u,o,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/function,u,u,u,u,u,f,u,u,u,u,u/op,u,u,u,Symbol,f,u,u,u,u,u,u/function,u,u,u,u,u,f,f,u,u,u,u/function,u,u,u,u,u,f,f,u,u,u,u/function,u,u,u,u,u,f,f,u,u,u,u/op,u,f,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/op,u,u,u,u,u,u,u,u,u,u,u/op,u,u,u,Symbol,f,u,u,u,u,u,u,4,o:2|o:0|o:2|o:0,deleted=u,u,u,u,u,u,u,u,u,u,u,u/u,u,f,u,u,u,u,u,o,u,u,u/u,u,u,u,u,u,f,u,u,u,u,u/u,u,f,u,Map,u,u,u,u,u,u,u/u,u,f,u,Set,u,u,u,u,u,u,u/u,u,u,u,Promise,u,u,u,u,u,u,u/u,u,u,u,u,f,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,f,u/u,u,f,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,f,u,Uint8Array,u,u,u,u,u,u,u/u,u,f,u,Generator,u,u,u,u,u,u,u/u,u,f,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,f,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,WeakMap,u,u,u,u,u,u,u/u,u,u,u,WeakSet,u,u,u,u,u,u,u/u,u,u,u,Math,u,u,u,u,u,u,u/u,u,u,u,JSON,u,u,u,u,u,u,u/u,u,u,u,Reflect,u,u,u,u,u,u,u/u,u,f,u,u,u,u,u,o,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,f,u,u,u,u,u/u,u,u,u,Symbol,f,u,u,u,u,u,u/u,u,u,u,u,u,f,f,u,u,u,u/u,u,u,u,u,u,f,f,u,u,u,u/u,u,u,u,u,u,f,f,u,u,u,u/u,u,f,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,u,u,u,u,u,u,u,u/u,u,u,u,Symbol,f,u,u,u,u,u,u,5,Changed,u,[object Changed],[object Object],f,12,px,own,u,mine|Symbol.iterator,u,own0,own0,true,u,u,own1,own1,true,u,u,own2,own2,true,u,u,own3,own3,true,u,u,own4,own4,true,u,u,own5,own5,true,u,u,u,u,u"##,
    );
}

#[test]
fn stores_on_collections_and_promises_define_on_their_ordinary_header() {
    // A store whose receiver is a Map, Set, WeakMap, WeakSet or Promise
    // defines the property on its ordinary header, and reaches that
    // definition without asking each prototype when the chain walk proves
    // that none has the key. Every store here is strict, so a refused one
    // throws. After warming: frozen, sealed and non-extensible receivers,
    // non-writable and accessor own properties, setters and data properties
    // that appear on the prototypes, a non-writable property and a Symbol
    // accessor on Object.prototype, Proxies above the receiver (asked once
    // per store, and one that refuses), an Array, a typed array, a String
    // wrapper, a function, null and another collection as the prototype,
    // receivers that differ from the object the store starts at, a `super`
    // store, and forty properties with a deletion across a collection.
    run_as_script_function_and_module(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
function churn() { var keep = []; for (var i = 0; i < 40000; i++) keep[i & 63] = { i: i, a: [i, i + 1] }; return keep.length; }
function put(o, k, v) { "use strict"; o[k] = v; return o[k]; }
function putStrict(o, k, v) { "use strict"; o[k] = v; return o[k]; }
function named(o, v) { "use strict"; o.status = v; return o.status; }
var sym = Symbol("s");
class P extends Promise {}
class M extends Map { put(v) { super.viaSuper = v; return this.viaSuper + ":" + Object.hasOwn(this, "viaSuper") + ":" + Object.hasOwn(Map.prototype, "viaSuper"); } }
function make() {
  return [new Map(), new Set(), new WeakMap(), new WeakSet(), Promise.resolve(1), new P(function () {}), new M(), {}, [], strictFunction(), Object("s")];
}
function strictFunction() { "use strict"; return function () {}; }
function keysOf(o) { return Reflect.ownKeys(o).map(function (k) { return typeof k === "symbol" ? "@" : k; }).join("."); }
function describe(o, k) { var d = Object.getOwnPropertyDescriptor(o, k); return d === undefined ? "none" : ("value" in d ? String(d.value) : "accessor") + (d.writable ? "w" : "") + (d.enumerable ? "e" : "") + (d.configurable ? "c" : ""); }
function round(tag, objects) {
  var out = [], previous = "";
  for (var i = 0; i < objects.length; i++) {
    (function (o) {
      var row = [show(function () { return put(o, "status", "pending"); }), show(function () { return named(o, "done"); }), show(function () { return putStrict(o, "value", "v"); }), show(function () { return put(o, sym, "sym"); }), show(function () { return put(o, "0", "zero"); }), show(function () { return putStrict(o, "length", 3); }), show(function () { return put(o, "status", "last"); }), show(function () { return keysOf(o); }), show(function () { return describe(o, "status") + "/" + describe(o, "value") + "/" + describe(o, sym); })].join(";");
      out.push(row === previous ? "=" : row);
      previous = row;
    })(objects[i]);
  }
  log.push(tag + "#" + out.join(" "));
}
round("warm", make());
churn();
round("again", make());
// Receivers that take no new property, and ones whose property is fixed.
round("frozen", make().map(function (o) { return Object.freeze(o); }));
round("sealed", make().map(function (o) { o.status = "kept"; return Object.seal(o); }));
round("closed", make().map(function (o) { o.value = "kept"; return Object.preventExtensions(o); }));
round("fixed", make().map(function (o) { Object.defineProperty(o, "status", { value: "ro", writable: false, configurable: true }); return o; }));
var hits = [];
round("accessor", make().map(function (o) { Object.defineProperty(o, "status", { get: function () { return "g"; }, set: function (v) { hits.push(typeof v + (this === o)); }, configurable: true }); return o; }));
log.push(hits.length, hits.slice(0, 3).join("|"));
round("getter-only", make().map(function (o) { Object.defineProperty(o, "value", { get: function () { return "g"; }, configurable: true }); return o; }));
// The chain above a collection changes after its sites are warm.
var sets = [];
Object.defineProperty(Promise.prototype, "status", { set: function (v) { sets.push("P:" + v + ":" + (this instanceof Promise)); this.inner = v; churn(); }, get: function () { return "pg"; }, configurable: true });
Object.defineProperty(Map.prototype, "value", { set: function (v) { sets.push("M:" + v); }, configurable: true });
round("prototype-setters", make());
log.push(sets.join("|"));
delete Promise.prototype.status;
delete Map.prototype.value;
Object.defineProperty(Object.prototype, sym, { get: function () { return "symget"; }, configurable: true });
Object.defineProperty(Object.prototype, "value", { value: "inherited", writable: false, configurable: true });
round("object-prototype-fixed", make());
delete Object.prototype.value;
delete Object.prototype[sym];
Set.prototype.status = "proto-data";
round("prototype-data", make());
delete Set.prototype.status;
round("restored", make());
// A Proxy above the receiver is asked to set, once per store.
var asked = [];
var proxy = new Proxy({}, { set: function (t, k, v, r) { asked.push(String(k)); return Reflect.set(t, k, v, r); } });
round("proxy-prototype", make().map(function (o) { return Object.setPrototypeOf(o, proxy); }));
log.push(asked.length, asked.slice(0, 7).join("|"));
var refuse = new Proxy({}, { set: function () { return false; } });
round("refusing-proxy", make().map(function (o) { return Object.setPrototypeOf(o, refuse); }));
// Exotic and unusual prototypes: an Array, a typed array, a String wrapper, a function, null.
round("array-prototype", make().map(function (o) { return Object.setPrototypeOf(o, [7, 8]); }));
round("typed-prototype", make().map(function (o) { return Object.setPrototypeOf(o, new Uint8Array(2)); }));
round("string-prototype", make().map(function (o) { return Object.setPrototypeOf(o, Object("ab")); }));
round("function-prototype", make().map(function (o) { return Object.setPrototypeOf(o, function f(a, b) {}); }));
round("null-prototype", make().map(function (o) { return Object.setPrototypeOf(o, null); }));
round("collection-prototype", make().map(function (o) { var parent = new Map(); parent.status = "parent"; return Object.setPrototypeOf(o, parent); }));
// The receiver differs from the object the store starts at.
var map = new Map(), promise = Promise.resolve(2), holder = { k: 0 };
log.push(Reflect.set({}, "k", 1, map), map.k, Reflect.set(map, "k", 2, holder), holder.k, map.k, Reflect.set(Object.create(map), "k", 3, promise), promise.k, Reflect.set(holder, "k", 4, new Set()), Reflect.set(map, "k", 5, 7), Reflect.set(map, "k", 6, Object.freeze(new Map())));
log.push(new M().put("s"), keysOf(Map.prototype).indexOf("viaSuper"));
// Many properties, deletion and re-adding keep insertion order.
var wide = new Map();
for (var n = 0; n < 40; n++) put(wide, "k" + n, n);
delete wide.k3; put(wide, "k3", "back"); churn();
log.push(Object.keys(wide).length, Object.keys(wide).slice(-3).join("|"), wide.k3, wide.k39, wide.size);
print(log.join());
"##,
        r##"warm#pending;done;v;sym;zero;3;last;0.status.value.length.@;lastwec/vwec/symwec = = = = = = = pending;done;v;sym;zero;3;last;0.length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;zero;TypeError;last;0.length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,again#pending;done;v;sym;zero;3;last;0.status.value.length.@;lastwec/vwec/symwec = = = = = = = pending;done;v;sym;zero;3;last;0.length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;zero;TypeError;last;0.length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,frozen#TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;;none/none/none = = = = = = = TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;length;none/none/none TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;length.name.prototype;none/none/none TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;0.length;none/none/none,sealed#pending;done;TypeError;TypeError;TypeError;TypeError;last;status;lastwe/none/none = = = = = = = pending;done;TypeError;TypeError;TypeError;3;last;length.status;lastwe/none/none pending;done;TypeError;TypeError;TypeError;TypeError;last;length.name.prototype.status;lastwe/none/none pending;done;TypeError;TypeError;TypeError;TypeError;last;0.length.status;lastwe/none/none,closed#TypeError;TypeError;v;TypeError;TypeError;TypeError;TypeError;value;none/vwec/none = = = = = = = TypeError;TypeError;v;TypeError;TypeError;3;TypeError;length.value;none/vwec/none TypeError;TypeError;v;TypeError;TypeError;TypeError;TypeError;length.name.prototype.value;none/vwec/none TypeError;TypeError;v;TypeError;TypeError;TypeError;TypeError;0.length.value;none/vwec/none,fixed#TypeError;TypeError;v;sym;zero;3;TypeError;0.status.value.length.@;roc/vwec/symwec = = = = = = = TypeError;TypeError;v;sym;zero;3;TypeError;0.length.status.value.@;roc/vwec/symwec TypeError;TypeError;v;sym;zero;TypeError;TypeError;0.length.name.prototype.status.value.@;roc/vwec/symwec TypeError;TypeError;v;sym;TypeError;TypeError;TypeError;0.length.status.value.@;roc/vwec/symwec,accessor#g;g;v;sym;zero;3;g;0.status.value.length.@;accessorc/vwec/symwec = = = = = = = g;g;v;sym;zero;3;g;0.length.status.value.@;accessorc/vwec/symwec g;g;v;sym;zero;TypeError;g;0.length.name.prototype.status.value.@;accessorc/vwec/symwec g;g;v;sym;TypeError;TypeError;g;0.length.status.value.@;accessorc/vwec/symwec,33,stringtrue|stringtrue|stringtrue,getter-only#pending;done;TypeError;sym;zero;3;last;0.value.status.length.@;lastwec/accessorc/symwec = = = = = = = pending;done;TypeError;sym;zero;3;last;0.length.value.status.@;lastwec/accessorc/symwec pending;done;TypeError;sym;zero;TypeError;last;0.length.name.prototype.value.status.@;lastwec/accessorc/symwec pending;done;TypeError;sym;TypeError;TypeError;last;0.length.value.status.@;lastwec/accessorc/symwec,prototype-setters#pending;done;undefined;sym;zero;3;last;0.status.length.@;lastwec/none/symwec pending;done;v;sym;zero;3;last;0.status.value.length.@;lastwec/vwec/symwec = = pg;pg;v;sym;zero;3;pg;0.inner.value.length.@;none/vwec/symwec = pending;done;undefined;sym;zero;3;last;0.status.length.@;lastwec/none/symwec pending;done;v;sym;zero;3;last;0.status.value.length.@;lastwec/vwec/symwec pending;done;v;sym;zero;3;last;0.length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;zero;TypeError;last;0.length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,M:v|P:pending:true|P:done:true|P:last:true|P:pending:true|P:done:true|P:last:true|M:v,object-prototype-fixed#pending;done;TypeError;TypeError;zero;3;last;0.status.length;lastwec/none/none = = = = = = = pending;done;TypeError;TypeError;zero;3;last;0.length.status;lastwec/none/none pending;done;TypeError;TypeError;zero;TypeError;last;0.length.name.prototype.status;lastwec/none/none pending;done;TypeError;TypeError;TypeError;TypeError;last;0.length.status;lastwec/none/none,prototype-data#pending;done;v;sym;zero;3;last;0.status.value.length.@;lastwec/vwec/symwec = = = = = = = pending;done;v;sym;zero;3;last;0.length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;zero;TypeError;last;0.length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,restored#pending;done;v;sym;zero;3;last;0.status.value.length.@;lastwec/vwec/symwec = = = = = = = pending;done;v;sym;zero;3;last;0.length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;zero;TypeError;last;0.length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,proxy-prototype#pending;done;v;sym;zero;3;last;0.status.value.length.@;lastwec/vwec/symwec = = = = = = = pending;done;v;sym;zero;3;last;0.length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;zero;TypeError;last;0.length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,51,status|value|Symbol(s)|0|length|status|value,refusing-proxy#TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;;none/none/none = = = = = = = TypeError;TypeError;TypeError;TypeError;TypeError;3;TypeError;length;none/none/none TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;length.name.prototype;none/none/none TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;TypeError;0.length;none/none/none,array-prototype#pending;done;v;sym;zero;3;last;0.status.value.length.@;lastwec/vwec/symwec = = = = = = = pending;done;v;sym;zero;3;last;0.length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;zero;TypeError;last;0.length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,typed-prototype#pending;done;v;sym;zero;TypeError;last;0.status.value.@;lastwec/vwec/symwec = = = = = = = pending;done;v;sym;zero;3;last;0.length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;zero;TypeError;last;0.length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,string-prototype#pending;done;v;sym;TypeError;TypeError;last;status.value.@;lastwec/vwec/symwec = = = = = = = pending;done;v;sym;TypeError;3;last;length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,function-prototype#pending;done;v;sym;zero;TypeError;last;0.status.value.@;lastwec/vwec/symwec = = = = = = = pending;done;v;sym;zero;3;last;0.length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;zero;TypeError;last;0.length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,null-prototype#pending;done;v;sym;zero;3;last;0.status.value.length.@;lastwec/vwec/symwec = = = = = = = pending;done;v;sym;zero;3;last;0.length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;zero;TypeError;last;0.length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,collection-prototype#pending;done;v;sym;zero;3;last;0.status.value.length.@;lastwec/vwec/symwec = = = = = = = pending;done;v;sym;zero;3;last;0.length.status.value.@;lastwec/vwec/symwec pending;done;v;sym;zero;TypeError;last;0.length.name.prototype.status.value.@;lastwec/vwec/symwec pending;done;v;sym;TypeError;TypeError;last;0.length.status.value.@;lastwec/vwec/symwec,true,1,true,2,1,true,3,true,false,false,s:true:false,-1,40,k38|k39|k3,back,39,0"##,
    );
}

#[test]
fn named_reads_on_arrays_answer_from_the_header_and_the_chain() {
    // The general Get reads a key that is neither an array index nor
    // "length" from an Array's ordinary header and walks on from there.
    // Twenty-six keys, among them index-like and "length"-like Strings and
    // Symbols, are read from fifteen receivers, twice each, and again after
    // data properties and accessors appear on Array.prototype and
    // Object.prototype; the inherited "constructor" is replaced, becomes an
    // accessor and is deleted; a Proxy sits above an Array; and own named
    // properties are added, redefined and deleted on every Array storage
    // form across a collection.
    run_as_script_function_and_module(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
function churn() { var keep = []; for (var i = 0; i < 40000; i++) keep[i & 63] = { i: i, a: [i, i + 1] }; return keep.length; }
function kind(v) { return v === undefined ? "u" : v === null ? "n" : typeof v === "function" ? "f" + v.name : typeof v === "object" ? "o" : typeof v === "symbol" ? "s" : String(v); }
function read(o, k) { return o[k]; }
function ctor(o) { return o.constructor; }
function def(o, k, v) { Object.defineProperty(o, k, { value: v, writable: true, enumerable: true, configurable: true }); }
var mine = Symbol("mine");
class Sub extends Array {}
function subjects() {
  var own = [5, 6]; def(own, "foo", "own-foo"); def(own, "constructor", "own-ctor"); def(own, mine, "own-sym");
  var wide = [1]; for (var n = 0; n < 40; n++) def(wide, "k" + n, n); def(wide, "foo", "wide-foo");
  var bare = [3]; Object.setPrototypeOf(bare, null);
  return [[], [1, 2, 3], new Array(5), Array.prototype, new Sub(1, 2), Object.create([7, 8]), own, wide, Object.freeze([9]), bare, (function () { return arguments; })(4, 5), new Uint8Array([6, 7]), Object("st"), Array.of(1.5, 2.5), [, "hole"]];
}
var keys = ["constructor", "length", "0", "1", "-0", "01", "1.0", "4294967294", "4294967295", "foo", "push", "toString", "at", "hasOwnProperty", "", "l", "lengthy", "k7", Symbol.iterator, Symbol.unscopables, Symbol.toStringTag, mine, 0, 1, 1.5, -1];
function sweep(tag) {
  var list = subjects(), rows = [];
  for (var r = 0; r < 2; r++) {
    rows.length = 0;
    for (var i = 0; i < list.length; i++) {
      var line = "";
      for (var j = 0; j < keys.length; j++) line += show(function () { return kind(read(list[i], keys[j])); }) + (j + 1 < keys.length ? "," : "");
      rows.push(line + "," + show(function () { return kind(ctor(list[i])); }));
    }
  }
  log.push(tag + "=" + rows.join("/"));
}
sweep("warm");
churn();
// Named properties appear on the prototypes after the reads are warm.
Array.prototype.foo = "array-foo";
Object.prototype.k7 = "object-k7";
Array.prototype[mine] = "array-sym";
sweep("prototype-data");
// An accessor on the chain sees the Array it was read from.
var seen = [];
Object.defineProperty(Array.prototype, "foo", { get: function () { seen.push((Array.isArray(this) ? "A" : typeof this) + this.length); return "got-foo"; }, configurable: true });
Object.defineProperty(Object.prototype, "lengthy", { get: function () { return "lengthy:" + (this === Array.prototype); }, configurable: true });
sweep("prototype-accessors");
log.push(seen.length, seen.slice(0, 5).join("|"));
delete Array.prototype.foo; delete Object.prototype.k7; delete Array.prototype[mine]; delete Object.prototype.lengthy;
// The constructor an Array inherits is replaced, becomes an accessor, and goes.
var original = Object.getOwnPropertyDescriptor(Array.prototype, "constructor");
Array.prototype.constructor = Sub;
log.push(kind(ctor([])), kind(ctor(new Sub())), kind([1].map(function (x) { return x; }).constructor));
var count = 0;
Object.defineProperty(Array.prototype, "constructor", { get: function () { count++; return Array; }, configurable: true });
log.push(kind(ctor([])), kind(read([], "constructor")), count);
delete Array.prototype.constructor;
log.push(kind(ctor([])), kind(ctor(Array.prototype)));
Object.defineProperty(Array.prototype, "constructor", original);
sweep("restored");
// A Proxy above an Array is asked for named keys only when the Array lacks them.
var asked = [];
var proxy = new Proxy({}, { get: function (t, k, r) { asked.push(typeof k === "symbol" ? "sym" : k); return k === "foo" ? "proxy-foo:" + Array.isArray(r) : undefined; } });
var under = [1, 2]; Object.setPrototypeOf(under, proxy);
log.push(read(under, "foo"), read(under, "length"), read(under, "0"), kind(read(under, "5")), kind(read(under, "constructor")), kind(read(under, mine)), asked.join("|"));
// Own named properties on every Array storage form: added, read, redefined, deleted.
[[], [1, 2], new Array(100), [1.5], [, 1], Object.freeze([1])].forEach(function (a, i) {
  log.push(show(function () { "use strict"; a.name = "n" + i; return a.name; }), kind(read(a, "name")));
  show(function () { Object.defineProperty(a, "acc", { get: function () { return "acc" + this.length; }, configurable: true }); });
  churn();
  log.push(kind(read(a, "acc")), show(function () { "use strict"; return delete a.name; }), kind(read(a, "name")), a.length);
});
// Keys that look like indices or "length" stay with the Array itself.
var odd = [10, 11];
odd["01"] = "padded"; odd["-0"] = "minus"; odd["1.0"] = "dotted"; odd["4294967295"] = "max"; odd[" 1"] = "spaced";
log.push(odd.length, read(odd, "01"), read(odd, "-0"), read(odd, "1.0"), read(odd, "4294967295"), read(odd, " 1"), read(odd, "1"), read(odd, 1), read(odd, "length"), Object.keys(odd).join("|"));
print(log.join());
"##,
        r##"warm=fArray,0,u,u,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,u,u,u,u,fArray/fArray,3,1,2,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,1,2,u,u,fArray/fArray,5,u,u,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,u,u,u,u,fArray/fArray,0,u,u,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,u,u,u,u,fArray/fSub,2,1,2,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,1,2,u,u,fSub/fArray,2,7,8,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,7,8,u,u,fArray/own-ctor,2,5,6,u,u,u,u,u,own-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,own-sym,5,6,u,u,own-ctor/fArray,1,1,u,u,u,u,u,u,wide-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,7,fvalues,o,u,u,1,u,u,u,fArray/fArray,1,9,u,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,9,u,u,u,fArray/u,1,3,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,3,u,u,u,u/fObject,2,4,5,u,u,u,u,u,u,u,ftoString,u,fhasOwnProperty,u,u,u,u,fvalues,u,u,u,4,5,u,u,fObject/fUint8Array,2,6,7,u,u,u,u,u,u,u,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,u,Uint8Array,u,6,7,u,u,fUint8Array/fString,2,s,t,u,u,u,u,u,u,u,ftoString,fat,fhasOwnProperty,u,u,u,u,f[Symbol.iterator],u,u,u,s,t,u,u,fString/fArray,2,1.5,2.5,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,1.5,2.5,u,u,fArray/fArray,2,u,hole,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,u,hole,u,u,fArray,prototype-data=fArray,0,u,u,u,u,u,u,u,array-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,fvalues,o,u,array-sym,u,u,u,u,fArray/fArray,3,1,2,u,u,u,u,u,array-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,fvalues,o,u,array-sym,1,2,u,u,fArray/fArray,5,u,u,u,u,u,u,u,array-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,fvalues,o,u,array-sym,u,u,u,u,fArray/fArray,0,u,u,u,u,u,u,u,array-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,fvalues,o,u,array-sym,u,u,u,u,fArray/fSub,2,1,2,u,u,u,u,u,array-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,fvalues,o,u,array-sym,1,2,u,u,fSub/fArray,2,7,8,u,u,u,u,u,array-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,fvalues,o,u,array-sym,7,8,u,u,fArray/own-ctor,2,5,6,u,u,u,u,u,own-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,fvalues,o,u,own-sym,5,6,u,u,own-ctor/fArray,1,1,u,u,u,u,u,u,wide-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,7,fvalues,o,u,array-sym,1,u,u,u,fArray/fArray,1,9,u,u,u,u,u,u,array-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,fvalues,o,u,array-sym,9,u,u,u,fArray/u,1,3,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,3,u,u,u,u/fObject,2,4,5,u,u,u,u,u,u,u,ftoString,u,fhasOwnProperty,u,u,u,object-k7,fvalues,u,u,u,4,5,u,u,fObject/fUint8Array,2,6,7,u,u,u,u,u,u,u,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,fvalues,u,Uint8Array,u,6,7,u,u,fUint8Array/fString,2,s,t,u,u,u,u,u,u,u,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,f[Symbol.iterator],u,u,u,s,t,u,u,fString/fArray,2,1.5,2.5,u,u,u,u,u,array-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,fvalues,o,u,array-sym,1.5,2.5,u,u,fArray/fArray,2,u,hole,u,u,u,u,u,array-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,object-k7,fvalues,o,u,array-sym,u,hole,u,u,fArray,prototype-accessors=fArray,0,u,u,u,u,u,u,u,got-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,object-k7,fvalues,o,u,array-sym,u,u,u,u,fArray/fArray,3,1,2,u,u,u,u,u,got-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,object-k7,fvalues,o,u,array-sym,1,2,u,u,fArray/fArray,5,u,u,u,u,u,u,u,got-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,object-k7,fvalues,o,u,array-sym,u,u,u,u,fArray/fArray,0,u,u,u,u,u,u,u,got-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,lengthy:true,object-k7,fvalues,o,u,array-sym,u,u,u,u,fArray/fSub,2,1,2,u,u,u,u,u,got-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,object-k7,fvalues,o,u,array-sym,1,2,u,u,fSub/fArray,2,7,8,u,u,u,u,u,got-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,object-k7,fvalues,o,u,array-sym,7,8,u,u,fArray/own-ctor,2,5,6,u,u,u,u,u,own-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,object-k7,fvalues,o,u,own-sym,5,6,u,u,own-ctor/fArray,1,1,u,u,u,u,u,u,wide-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,7,fvalues,o,u,array-sym,1,u,u,u,fArray/fArray,1,9,u,u,u,u,u,u,got-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,object-k7,fvalues,o,u,array-sym,9,u,u,u,fArray/u,1,3,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,3,u,u,u,u/fObject,2,4,5,u,u,u,u,u,u,u,ftoString,u,fhasOwnProperty,u,u,lengthy:false,object-k7,fvalues,u,u,u,4,5,u,u,fObject/fUint8Array,2,6,7,u,u,u,u,u,u,u,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,object-k7,fvalues,u,Uint8Array,u,6,7,u,u,fUint8Array/fString,2,s,t,u,u,u,u,u,u,u,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,object-k7,f[Symbol.iterator],u,u,u,s,t,u,u,fString/fArray,2,1.5,2.5,u,u,u,u,u,got-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,object-k7,fvalues,o,u,array-sym,1.5,2.5,u,u,fArray/fArray,2,u,hole,u,u,u,u,u,got-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,lengthy:false,object-k7,fvalues,o,u,array-sym,u,hole,u,u,fArray,18,A0|A3|A5|A0|A2,fSub,fSub,fSub,fArray,fArray,2,fObject,fObject,restored=fArray,0,u,u,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,u,u,u,u,fArray/fArray,3,1,2,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,1,2,u,u,fArray/fArray,5,u,u,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,u,u,u,u,fArray/fArray,0,u,u,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,u,u,u,u,fArray/fSub,2,1,2,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,1,2,u,u,fSub/fArray,2,7,8,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,7,8,u,u,fArray/own-ctor,2,5,6,u,u,u,u,u,own-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,own-sym,5,6,u,u,own-ctor/fArray,1,1,u,u,u,u,u,u,wide-foo,fpush,ftoString,fat,fhasOwnProperty,u,u,u,7,fvalues,o,u,u,1,u,u,u,fArray/fArray,1,9,u,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,9,u,u,u,fArray/u,1,3,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,u,3,u,u,u,u/fObject,2,4,5,u,u,u,u,u,u,u,ftoString,u,fhasOwnProperty,u,u,u,u,fvalues,u,u,u,4,5,u,u,fObject/fUint8Array,2,6,7,u,u,u,u,u,u,u,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,u,Uint8Array,u,6,7,u,u,fUint8Array/fString,2,s,t,u,u,u,u,u,u,u,ftoString,fat,fhasOwnProperty,u,u,u,u,f[Symbol.iterator],u,u,u,s,t,u,u,fString/fArray,2,1.5,2.5,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,1.5,2.5,u,u,fArray/fArray,2,u,hole,u,u,u,u,u,u,fpush,ftoString,fat,fhasOwnProperty,u,u,u,u,fvalues,o,u,u,u,hole,u,u,fArray,proxy-foo:true,2,1,u,u,u,foo|5|constructor|sym,n0,n0,acc0,true,u,0,n1,n1,acc2,true,u,2,n2,n2,acc100,true,u,100,n3,n3,acc1,true,u,1,n4,n4,acc2,true,u,2,TypeError,u,u,true,u,1,2,padded,minus,dotted,max,spaced,11,11,2,0|1|01|-0|1.0|4294967295| 1"##,
    );
}

#[test]
fn array_species_and_final_length_stores_keep_every_observable_step() {
    // ArraySpeciesCreate creates a plain Array without the three lookups
    // when the source is an Array with no own "constructor" whose prototype
    // is the realm's untouched %Array.prototype% and whose %Array% is
    // untouched; and the final Set of "length" is skipped when the target
    // is an Array whose writable length already has that value. Ten
    // species consumers run on plain Arrays and then with every kind of own
    // "constructor", subclass instances, other prototypes, an array-like, a
    // Proxy, and with Array.prototype.constructor and Array[@@species]
    // replaced, made accessors, deleted and restored after warming. Twelve
    // mutating methods run on plain, frozen, sealed and non-extensible
    // Arrays, Arrays with a non-writable length or element, sparse and
    // presized Arrays, a subclass, a Proxy that records every store, an
    // array-like with a length setter and an Array of the greatest length.
    run_as_script_function_and_module(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
function churn() { var keep = []; for (var i = 0; i < 40000; i++) keep[i & 63] = { i: i, a: [i, i + 1] }; return keep.length; }
class Sub extends Array {}
function tag(v) {
  var p = Object.getPrototypeOf(v);
  return (p === Array.prototype ? "A" : p === Sub.prototype ? "S" : p === Number.prototype ? "N" : p === null ? "0" : "?") + (Array.isArray(v) ? "" : "!") + v.length + "[" + show(function () { return Array.prototype.join.call(v, ""); }) + "]";
}
function ops(make) {
  var out = [];
  out.push(show(function () { var a = make(); return tag(a.splice(1, 1)) + tag(a); }));
  out.push(show(function () { var a = make(); return tag(a.splice(0, 0)) + tag(a); }));
  out.push(show(function () { var a = make(); return tag(a.splice(0)) + tag(a); }));
  out.push(show(function () { var a = make(); return tag(a.splice(1, 1, "x", "y")) + tag(a); }));
  out.push(show(function () { return tag(make().map(function (x) { return x; })); }));
  out.push(show(function () { return tag(make().filter(function () { return true; })); }));
  out.push(show(function () { return tag(make().slice(1)); }));
  out.push(show(function () { return tag(make().concat(["c"])); }));
  out.push(show(function () { return tag(make().flat()); }));
  out.push(show(function () { return tag(make().flatMap(function (x) { return [x, x]; })); }));
  return out.join(" ");
}
function plain() { return ["a", "b", "c"]; }
function line(name, make) { log.push(name + "=" + ops(make)); }
line("plain", plain); line("plain-again", plain);
churn();
// An own "constructor" of every kind.
function withCtor(value) { return function () { var a = plain(); a.constructor = value; return a; }; }
var speciesHolder = {}; speciesHolder[Symbol.species] = Sub;
var nullSpecies = {}; nullSpecies[Symbol.species] = null;
var undefinedSpecies = function () {}; undefinedSpecies[Symbol.species] = undefined;
var arrowSpecies = {}; arrowSpecies[Symbol.species] = function () { return [].slice; }();
line("own-undefined", withCtor(undefined)); line("own-null", withCtor(null)); line("own-number", withCtor(5)); line("own-string", withCtor("s"));
line("own-Sub", withCtor(Sub)); line("own-Array", withCtor(Array)); line("own-Object", withCtor(Object)); line("own-species-Sub", withCtor(speciesHolder));
line("own-species-null", withCtor(nullSpecies)); line("own-species-undefined", withCtor(undefinedSpecies)); line("own-species-not-constructor", withCtor(arrowSpecies));
line("own-accessor", function () { var a = plain(); Object.defineProperty(a, "constructor", { get: function () { log.push("own-get"); return Sub; } }); return a; });
// Arrays whose prototype is not this realm's %Array.prototype%.
line("subclass", function () { return Sub.from(plain()); });
line("sub-prototype", function () { return Object.setPrototypeOf(plain(), Sub.prototype); });
line("null-prototype", function () { var a = plain(); Object.setPrototypeOf(a, null); a.splice = [].splice; a.map = [].map; a.filter = [].filter; a.slice = [].slice; a.concat = [].concat; a.flat = [].flat; a.flatMap = [].flatMap; return a; });
line("middle-prototype", function () { return Object.setPrototypeOf(plain(), Object.create(Array.prototype)); });
line("middle-constructor", function () { var middle = Object.create(Array.prototype); middle.constructor = Sub; return Object.setPrototypeOf(plain(), middle); });
// Receivers that are not Arrays, and a Proxy for one.
line("array-like", function () { var o = { length: 3, 0: "a", 1: "b", 2: "c" }; ["splice", "map", "filter", "slice", "concat", "flat", "flatMap"].forEach(function (n) { o[n] = Array.prototype[n]; }); return o; });
var traps = [];
line("proxy", function () { return new Proxy(plain(), { get: function (t, k, r) { if (k === "constructor") traps.push("get"); return Reflect.get(t, k, r); } }); });
log.push(traps.length);
line("revoked-constructor-proxy", withCtor(function () { var p = Proxy.revocable(function () {}, {}); p.revoke(); return p.proxy; }()));
// The species itself changes after every site is warm, and is restored.
var prototypeConstructor = Object.getOwnPropertyDescriptor(Array.prototype, "constructor");
var species = Object.getOwnPropertyDescriptor(Array, Symbol.species);
Array.prototype.unrelated = 1;
line("unrelated-property", plain);
delete Array.prototype.unrelated;
Array.prototype.constructor = Sub;
line("prototype-constructor-Sub", plain);
var gets = 0;
Object.defineProperty(Array.prototype, "constructor", { get: function () { gets++; return Array; }, configurable: true });
line("prototype-constructor-accessor", plain);
log.push(gets);
delete Array.prototype.constructor;
line("prototype-constructor-deleted", plain);
Object.defineProperty(Array.prototype, "constructor", prototypeConstructor);
var asked = 0;
Object.defineProperty(Array, Symbol.species, { get: function () { asked++; return Sub; }, configurable: true });
line("species-Sub", plain);
log.push(asked);
Object.defineProperty(Array, Symbol.species, { value: null, configurable: true });
line("species-null", plain);
Object.defineProperty(Array, Symbol.species, { value: 7, configurable: true });
line("species-number", plain);
Object.defineProperty(Array, Symbol.species, species);
Array.extra = 1; delete Array.extra;
line("restored", plain);
// The final Set of "length" in the mutating methods.
function mutate(make) {
  var out = [];
  var P = Array.prototype;
  [function (a) { return P.push.call(a); }, function (a) { return P.push.call(a, "p"); }, function (a) { return P.pop.call(a); }, function (a) { return P.shift.call(a); }, function (a) { return P.unshift.call(a); }, function (a) { return P.unshift.call(a, "u"); }, function (a) { return tag(P.splice.call(a, 0, 0)); }, function (a) { return tag(P.splice.call(a, 1, 1)); }, function (a) { return tag(P.splice.call(a, 1, 0, "i")); }, function (a) { return P.reverse.call(a).length; }, function (a) { return P.fill.call(a, "f").length; }, function (a) { return P.copyWithin.call(a, 0, 1).length; }].forEach(function (operation) {
    var a;
    out.push(show(function () { a = make(); return operation(a); }) + ">" + show(function () { return tag(a); }));
  });
  return out.join(" ");
}
function lengths(name, make) { log.push(name + "=" + mutate(make)); }
lengths("plain", plain); lengths("empty", function () { return []; });
lengths("frozen", function () { return Object.freeze(plain()); });
lengths("sealed", function () { return Object.seal(plain()); });
lengths("closed", function () { return Object.preventExtensions(plain()); });
lengths("fixed-length", function () { var a = plain(); Object.defineProperty(a, "length", { writable: false }); return a; });
lengths("fixed-empty", function () { var a = []; Object.defineProperty(a, "length", { writable: false }); return a; });
lengths("holes", function () { return ["a", , "c"]; });
lengths("presized", function () { return new Array(3); });
lengths("numbers", function () { return [1.5, 2.5, 3.5]; });
lengths("subclass", function () { return Sub.from(plain()); });
lengths("fixed-element", function () { var a = plain(); Object.defineProperty(a, "1", { value: "b", writable: false, configurable: false }); return a; });
var writes = [];
lengths("proxy", function () { return new Proxy(plain(), { set: function (t, k, v, r) { writes.push(String(k) + "=" + v); return Reflect.set(t, k, v, r); }, deleteProperty: function (t, k) { writes.push("-" + String(k)); return Reflect.deleteProperty(t, k); } }); });
log.push(writes.length, writes.slice(0, 8).join("|"));
lengths("array-like", function () { return { length: 3, 0: "a", 1: "b", 2: "c" }; });
lengths("length-setter", function () { var store = 2, o = { 0: "a", 1: "b" }; Object.defineProperty(o, "length", { get: function () { return store; }, set: function (v) { writes.push("len" + v); store = v; } }); return o; });
log.push(writes.slice(-4).join("|"));
var big = []; big.length = 4294967295;
log.push(show(function () { return big.push(); }), show(function () { return big.push(1); }), big.length, show(function () { return tag(big.splice(4294967294, 1)); }), big.length);
print(log.join());
"##,
        r##"plain=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],plain-again=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],own-undefined=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],own-null=TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError,own-number=TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError,own-string=TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError,own-Sub=S1[b]A2[ac] S0[]A3[abc] S3[abc]A0[] S1[b]A4[axyc] S3[abc] S3[abc] S2[bc] S4[abcc] S3[abc] S6[aabbcc],own-Array=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],own-Object=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],own-species-Sub=S1[b]A2[ac] S0[]A3[abc] S3[abc]A0[] S1[b]A4[axyc] S3[abc] S3[abc] S2[bc] S4[abcc] S3[abc] S6[aabbcc],own-species-null=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],own-species-undefined=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],own-species-not-constructor=TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError,own-get,own-get,own-get,own-get,own-get,own-get,own-get,own-get,own-get,own-get,own-accessor=S1[b]A2[ac] S0[]A3[abc] S3[abc]A0[] S1[b]A4[axyc] S3[abc] S3[abc] S2[bc] S4[abcc] S3[abc] S6[aabbcc],subclass=S1[b]S2[ac] S0[]S3[abc] S3[abc]S0[] S1[b]S4[axyc] S3[abc] S3[abc] S2[bc] S4[abcc] S3[abc] S6[aabbcc],sub-prototype=S1[b]S2[ac] S0[]S3[abc] S3[abc]S0[] S1[b]S4[axyc] S3[abc] S3[abc] S2[bc] S4[abcc] S3[abc] S6[aabbcc],null-prototype=A1[b]02[ac] A0[]03[abc] A3[abc]00[] A1[b]04[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],middle-prototype=A1[b]?2[ac] A0[]?3[abc] A3[abc]?0[] A1[b]?4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],middle-constructor=S1[b]?2[ac] S0[]?3[abc] S3[abc]?0[] S1[b]?4[axyc] S3[abc] S3[abc] S2[bc] S4[abcc] S3[abc] S6[aabbcc],array-like=A1[b]?!2[ac] A0[]?!3[abc] A3[abc]?!0[] A1[b]?!4[axyc] A3[abc] A3[abc] A2[bc] A2[[object Object]c] A3[abc] A6[aabbcc],proxy=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],10,revoked-constructor-proxy=TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError,unrelated-property=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],prototype-constructor-Sub=S1[b]A2[ac] S0[]A3[abc] S3[abc]A0[] S1[b]A4[axyc] S3[abc] S3[abc] S2[bc] S4[abcc] S3[abc] S6[aabbcc],prototype-constructor-accessor=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],10,prototype-constructor-deleted=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],species-Sub=S1[b]A2[ac] S0[]A3[abc] S3[abc]A0[] S1[b]A4[axyc] S3[abc] S3[abc] S2[bc] S4[abcc] S3[abc] S6[aabbcc],10,species-null=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],species-number=TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError TypeError,restored=A1[b]A2[ac] A0[]A3[abc] A3[abc]A0[] A1[b]A4[axyc] A3[abc] A3[abc] A2[bc] A4[abcc] A3[abc] A6[aabbcc],plain=3>A3[abc] 4>A4[abcp] c>A2[ab] a>A2[bc] 3>A3[abc] 4>A4[uabc] A0[]>A3[abc] A1[b]>A2[ac] A0[]>A4[aibc] 3>A3[cba] 3>A3[fff] 3>A3[bcc],empty=0>A0[] 1>A1[p] undefined>A0[] undefined>A0[] 0>A0[] 1>A1[u] A0[]>A0[] A0[]>A0[] A0[]>A1[i] 0>A0[] 0>A0[] 0>A0[],frozen=TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[abc],sealed=3>A3[abc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[bcc] 3>A3[abc] TypeError>A3[abc] A0[]>A3[abc] TypeError>A3[acc] TypeError>A3[abc] 3>A3[cba] 3>A3[fff] 3>A3[bcc],closed=3>A3[abc] TypeError>A3[abc] c>A2[ab] a>A2[bc] 3>A3[abc] TypeError>A3[abc] A0[]>A3[abc] A1[b]>A2[ac] TypeError>A3[abc] 3>A3[cba] 3>A3[fff] 3>A3[bcc],fixed-length=TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[ab] TypeError>A3[bc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[abc] TypeError>A3[ac] TypeError>A3[abc] 3>A3[cba] 3>A3[fff] 3>A3[bcc],fixed-empty=TypeError>A0[] TypeError>A0[] TypeError>A0[] TypeError>A0[] TypeError>A0[] TypeError>A0[] TypeError>A0[] TypeError>A0[] TypeError>A0[] 0>A0[] 0>A0[] 0>A0[],holes=3>A3[ac] 4>A4[acp] c>A2[a] a>A2[c] 3>A3[ac] 4>A4[uac] A0[]>A3[ac] A1[]>A2[ac] A0[]>A4[aic] 3>A3[ca] 3>A3[fff] 3>A3[cc],presized=3>A3[] 4>A4[p] undefined>A2[] undefined>A2[] 3>A3[] 4>A4[u] A0[]>A3[] A1[]>A2[] A0[]>A4[i] 3>A3[] 3>A3[fff] 3>A3[],numbers=3>A3[1.52.53.5] 4>A4[1.52.53.5p] 3.5>A2[1.52.5] 1.5>A2[2.53.5] 3>A3[1.52.53.5] 4>A4[u1.52.53.5] A0[]>A3[1.52.53.5] A1[2.5]>A2[1.53.5] A0[]>A4[1.5i2.53.5] 3>A3[3.52.51.5] 3>A3[fff] 3>A3[2.53.53.5],subclass=3>S3[abc] 4>S4[abcp] c>S2[ab] a>S2[bc] 3>S3[abc] 4>S4[uabc] S0[]>S3[abc] S1[b]>S2[ac] S0[]>S4[aibc] 3>S3[cba] 3>S3[fff] 3>S3[bcc],fixed-element=3>A3[abc] 4>A4[abcp] c>A2[ab] TypeError>A3[bbc] 3>A3[abc] TypeError>A4[abbc] A0[]>A3[abc] TypeError>A3[abc] TypeError>A4[abbc] 3>A3[cba] TypeError>A3[fbc] TypeError>A3[bbc],proxy=3>A3[abc] 4>A4[abcp] c>A2[ab] a>A2[bc] 3>A3[abc] 4>A4[uabc] A0[]>A3[abc] A1[b]>A2[ac] A0[]>A4[aibc] 3>A3[cba] 3>A3[fff] 3>A3[bcc],30,length=3|3=p|length=4|-2|length=2|0=b|1=c|-2,array-like=3>?!3[abc] 4>?!4[abcp] c>?!2[ab] a>?!2[bc] 3>?!3[abc] 4>?!4[uabc] A0[]>?!3[abc] A1[b]>?!2[ac] A0[]>?!4[aibc] 3>?!3[cba] 3>?!3[fff] 3>?!3[bcc],length-setter=2>?!2[ab] 3>?!3[abp] b>?!1[a] a>?!1[b] 2>?!2[ab] 3>?!3[uab] A0[]>?!2[ab] A1[b]>?!1[a] A0[]>?!3[aib] 2>?!2[ba] 2>?!2[ff] 2>?!2[bb],len3|len2|len1|len3,4294967295,RangeError,4294967295,A1[],4294967294"##,
    );
}

#[test]
fn strict_object_pattern_parameters_keep_binding_initialization_exact() {
    // A strict function whose parameters are object binding patterns omits
    // its arguments object when nothing names it, and in a module reads the
    // names a pattern without expressions binds through their cells. The
    // body is strict in all three forms. Patterns are read, written,
    // captured, nested, repeated, used with `arguments` (directly, through
    // an arrow function and through a direct eval), with defaults, a
    // temporal dead zone, computed keys, rest properties, array patterns
    // and list defaults; called with primitives, null and nothing; and
    // given getters, a Proxy, inherited properties, a getter that re-enters
    // the function and one that collects.
    run_as_script_function_and_module(
        r##"
"use strict";
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
function churn() { var keep = []; for (var i = 0; i < 40000; i++) keep[i & 63] = { i: i, a: [i, i + 1] }; return keep.length; }
var tick = 0;
// One object pattern, read, written and captured.
function row({ id }) { return "row" + id + ":" + id; }
function pair({ a, b }, extra) { return a + "+" + b + "+" + extra; }
function renamed({ a: first, b: second }) { first = first + 1; return first + "/" + second; }
function nested({ outer: { inner }, flat }) { return inner + "|" + flat; }
function capture({ id }) { return function () { id = id + 1; return id; }; }
function arrowCapture({ id }) { var next = () => ++id; next(); return [id, next()].join(); }
function count({ id }) { return arguments.length + ":" + (arguments[0].id === id) + ":" + typeof arguments[1]; }
function own({ id }) { arguments[0] = { id: "changed" }; return id + ":" + arguments[0].id; }
function arrowArguments({ id }) { return (() => arguments[0].id + arguments.length)(); }
function evalArguments({ id }) { return eval("arguments[0].id + id"); }
function shadowFunction({ id }) { function id() {} return typeof id; }
function recursive({ n, acc }) { return n ? recursive({ n: n - 1, acc: acc + n }) : acc; }
function reenter({ id, again }) { var mine = id; if (again) reenter({ id: id + 100, again: false }); return mine === id; }
function twice({ id }, { id: other }) { return id + "&" + other; }
function thisAndNew({ id }) { return (this === undefined) + ":" + (new.target === undefined) + ":" + id; }
function Make({ id }) { this.id = id; this.read = function () { return id; }; }
var arrow = ({ id }) => id * 2;
var arrowNested = ({ a }) => ({ b }) => a + b;
var method = { m({ id }) { return id + (this === method); } };
class K { constructor({ id }) { this.id = id; } static s({ id }) { return id; } get g() { return this.id; } }
function* generator({ id }) { yield id; yield id + 1; }
async function asynchronous({ id }) { return id; }
// Patterns that hold expressions keep their separate records.
function defaults({ a = 1, b = a + 1 }) { return a + ":" + b; }
function tdz({ a = b, b }) { return a + ":" + b; }
function computed({ ["k" + ++tick]: v }) { return v; }
function rest({ a, ...others }) { return a + ":" + Object.keys(others).join(""); }
function closureDefault({ a, f = () => a }) { a = a + 1; return f(); }
function listDefault({ id } = { id: "dflt" }) { return id; }
function argumentsDefault({ id = arguments.length }) { return id; }
function array([first, second]) { return first + ":" + second; }
function mixed({ a }, [b], c = a + b, ...d) { return [a, b, c, d.length].join(); }
for (var i = 0; i < 3; i++) {
  log.push(row({ id: i }), pair({ a: i, b: 2 }, "e"), renamed({ a: i, b: "s" }), nested({ outer: { inner: i }, flat: "f" }), capture({ id: i })(), arrowCapture({ id: i }), count({ id: i }), count({ id: i }, 1), own({ id: i }), arrowArguments({ id: "a" }), evalArguments({ id: i }), shadowFunction({ id: i }), recursive({ n: 4, acc: i }), reenter({ id: i, again: true }), twice({ id: i }, { id: "o" }), thisAndNew({ id: i }), new Make({ id: i }).read(), arrow({ id: i }), arrowNested({ a: i })({ b: 10 }), method.m({ id: i }), new K({ id: i }).g, K.s({ id: i }), Array.from(generator({ id: i })).join(""), typeof asynchronous({ id: i }));
  log.push(defaults({}), defaults({ a: 5 }), defaults({ b: 7 }), show(function () { return tdz({}); }), tdz({ a: 1, b: 2 }), computed({ k1: "one", k2: "two", k3: "three" }), rest({ a: 1, b: 2, c: 3 }), closureDefault({ a: i }), listDefault(), listDefault({ id: i }), argumentsDefault({}, 2, 3), array([i, "s"]), array("xy"), mixed({ a: 1 }, [2]), mixed({ a: 1 }, [2], 9, 8, 7));
}
churn();
// Arguments that are not plain objects.
log.push(show(function () { return row(); }), show(function () { return row(null); }), show(function () { return row(undefined); }), row(7), row("str"), row(true), row([]), row(function id() {}).length > 8, row(Symbol.iterator), show(function () { return nested({ flat: 1 }); }), show(function () { return nested({ outer: null }); }));
// Getters, Proxies and prototypes on the argument run in source order, once each.
var order = [];
var source = { get a() { order.push("a"); return 1; }, get b() { order.push("b"); return 2; } };
log.push(pair(source, "x"), renamed(source), order.join(""));
var traps = [];
var proxy = new Proxy({ id: "p", a: 1, b: 2 }, { get: function (t, k, r) { traps.push(String(k)); return Reflect.get(t, k, r); }, has: function (t, k) { traps.push("has:" + String(k)); return Reflect.has(t, k); }, ownKeys: function (t) { traps.push("keys"); return Reflect.ownKeys(t); }, getOwnPropertyDescriptor: function (t, k) { traps.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); } });
log.push(row(proxy), pair(proxy, "y"), rest(proxy), traps.join("|"));
var inherited = Object.create({ id: "proto", a: "pa" }); inherited.b = "own";
log.push(row(inherited), pair(inherited, "z"));
Object.prototype.id = "object-prototype";
log.push(row({}), row(7), row([]));
delete Object.prototype.id;
log.push(row({}), show(function () { return row({ get id() { throw new RangeError("r"); } }); }));
// A getter that re-enters the same function, and one that collects.
var depth = 0;
var reentrant = { get id() { return depth++ < 2 ? row(reentrant) : "base"; } };
log.push(row(reentrant), row({ get id() { churn(); return "after-collection"; } }), capture({ get id() { churn(); return 40; } })());
// Strict functions stay opaque, and their parameters stay their own.
log.push(show(function () { return row.arguments; }), show(function () { return row.caller; }), row.length, pair.length, mixed.length, listDefault.length, Object.hasOwn(row, "arguments"));
var captured = capture({ id: 0 }), other = capture({ id: 100 });
log.push(captured(), captured(), other(), captured());
var made = new Make({ id: "kept" });
log.push(made.id, made.read(), show(function () { return Make({ id: 1 }); }), show(function () { return new K(); }), show(function () { return K({ id: 1 }); }));
print(log.join());
"##,
        r##"row0:0,0+2+e,1/s,0|f,1,1,2,1:true:undefined,2:true:number,0:changed,a1,0,function,10,true,0&o,true:true:0,0,0,10,1,0,0,01,object,1:2,5:6,1:7,ReferenceError,1:2,one,1:bc,1,dflt,0,3,0:s,x:y,1,2,3,0,1,2,9,2,row1:1,1+2+e,2/s,1|f,2,2,3,1:true:undefined,2:true:number,1:changed,a1,2,function,11,true,1&o,true:true:1,1,2,11,2,1,1,12,object,1:2,5:6,1:7,ReferenceError,1:2,two,1:bc,2,dflt,1,3,1:s,x:y,1,2,3,0,1,2,9,2,row2:2,2+2+e,3/s,2|f,3,3,4,1:true:undefined,2:true:number,2:changed,a1,4,function,12,true,2&o,true:true:2,2,4,12,3,2,2,23,object,1:2,5:6,1:7,ReferenceError,1:2,three,1:bc,3,dflt,2,3,2:s,x:y,1,2,3,0,1,2,9,2,TypeError,TypeError,TypeError,rowundefined:undefined,rowundefined:undefined,rowundefined:undefined,rowundefined:undefined,true,rowundefined:undefined,TypeError,TypeError,1+2+x,2/2,abab,rowp:p,1+2+y,1:idb,id|a|b|a|keys|gopd:id|id|gopd:b|b,rowproto:proto,pa+own+z,rowobject-prototype:object-prototype,rowobject-prototype:object-prototype,rowobject-prototype:object-prototype,rowundefined:undefined,RangeError,rowrowrowbase:base:rowbase:base:rowrowbase:base:rowbase:base,rowafter-collection:after-collection,41,TypeError,TypeError,1,2,2,0,false,1,2,101,3,kept,kept,TypeError,TypeError,TypeError"##,
    );
}

#[test]
fn object_pattern_parameters_in_sloppy_code_keep_their_arguments_object() {
    // The same patterns in code that is sloppy as a Script and as function
    // code and strict as a module; only what both modes agree on is
    // observed.
    run_as_script_function_and_module(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
// The same patterns in code that is sloppy as a Script and as function code
// and strict as a module: only what both modes agree on is observed.
function row({ id }) { return "row" + id + ":" + id; }
function count({ id }) { return arguments.length + ":" + (arguments[0].id === id); }
function own({ id }) { arguments[0] = { id: "changed" }; id = id + "!"; return id + ":" + arguments[0].id; }
function capture({ id }) { return function () { id = id + 1; return id; }; }
function arrowArguments({ id }) { return (() => arguments[0].id + arguments.length)(); }
function evalBody({ id }) { return eval("id + arguments.length"); }
function nested({ outer: { inner }, flat }, extra) { return inner + "|" + flat + "|" + extra; }
function defaults({ a = 1, b = a + 1 }) { return a + ":" + b; }
function rest({ a, ...others }) { return a + ":" + Object.keys(others).join(""); }
var arrow = ({ id }) => id * 2;
for (var i = 0; i < 3; i++) log.push(row({ id: i }), count({ id: i }, 1), own({ id: i }), capture({ id: i })(), arrowArguments({ id: "a" }), evalBody({ id: i }), nested({ outer: { inner: i }, flat: "f" }), defaults({ b: i }), rest({ a: i, z: 1 }), arrow({ id: i }));
var order = [];
log.push(row({ get id() { order.push("g"); return "got"; } }), order.join(""), show(function () { return row(); }), show(function () { return nested({}); }), row(7), row.length, nested.length);
var kept = capture({ id: 10 });
log.push(kept(), kept(), capture({ id: 0 })(), kept());
print(log.join());
"##,
        r##"row0:0,2:true,0!:changed,1,a1,1,0|f|undefined,1:0,0:z,0,row1:1,2:true,1!:changed,2,a1,2,1|f|undefined,1:1,1:z,2,row2:2,2:true,2!:changed,3,a1,3,2|f|undefined,1:2,2:z,4,rowgot:got,g,TypeError,TypeError,rowundefined:undefined,1,2,11,12,1,13"##,
    );
}
