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
