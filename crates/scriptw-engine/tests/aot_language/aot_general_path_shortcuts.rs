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
