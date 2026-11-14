//! An `instanceof` source site answers from its own record in P: made once
//! for the first right-hand side the site meets, usable for a function whose
//! [[Prototype]] is %Function.prototype% and whose own "prototype" is a
//! non-writable, non-configurable data property holding an object. Each
//! script warms sites and then changes what a record relies on, or meets a
//! case the record leaves to the entry and the HasInstance helper: a Proxy
//! candidate, a writable "prototype", another right-hand side, a bound,
//! Proxy or non-callable one, a changed [[Prototype]] and @@hasInstance
//! defined after warming. Expectations are Node 24's.
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

fn run_at_top_level_and_in_a_function(source: &str, expected: &str) {
    run(source, expected);
    run(&format!("(function () {{\n{source}}})();"), expected);
}

#[test]
fn site_records_answer_hits_misses_and_primitives_and_leave_every_other_case() {
    // Hits at depth one and two, misses, null prototypes and primitives at
    // warmed sites; builtin right-hand sides; Proxy and moving candidates; a
    // non-writable and a writable "prototype"; changing, bound, Proxy and
    // non-callable right-hand sides; the constructor's [[Prototype]]
    // changed, made a Proxy and restored after warming; and a subclass and a
    // typed array constructor, one function object from Function.prototype,
    // whose edge changes the same way.
    let source = r#"
var log = [];
class K {}
class L extends K {}
function isK(v) { return v instanceof K; }
// Hits at depth one and two, misses, a null-prototype object and primitives.
var nullProto = Object.create(null);
for (var i = 0; i < 3; i++) {
  log.push([isK(new K()), isK(new L()), isK({}), isK([]), isK(nullProto), isK(1), isK("s"), isK(undefined), isK(null), isK(Symbol.iterator), isK(K), isK(1n)].map(Number).join(""));
}
// Builtins whose "prototype" is permanent, at sites of their own.
function isArray(v) { return v instanceof Array; }
function isObject(v) { return v instanceof Object; }
function isFunction(v) { return v instanceof Function; }
function isMap(v) { return v instanceof Map; }
function isDate(v) { return v instanceof Date; }
var values = [[], {}, function () {}, new Map(), new Date(0), nullProto, 0, Object.create([]), Object.create(Object.create({})), class {}, new Uint8Array(1)];
for (var round = 0; round < 2; round++) {
  log.push([isArray, isObject, isFunction, isMap, isDate].map(function (site) {
    return values.map(function (v) { return site(v) ? 1 : 0; }).join("");
  }).join(" "));
}
// A Proxy candidate, and a candidate inheriting from one: the trap runs once per test.
var traps = 0;
var proxy = new Proxy(new L(), { getPrototypeOf: function (t) { traps++; return Reflect.getPrototypeOf(t); } });
log.push(isK(proxy), traps, isK(Object.create(proxy)), traps, isK(proxy), traps);
// A candidate whose chain changes between evaluations.
var moving = new K();
log.push(isK(moving));
Object.setPrototypeOf(moving, null);
log.push(isK(moving));
Object.setPrototypeOf(moving, L.prototype);
log.push(isK(moving));
// A class's "prototype" is not writable.
K.prototype = {};
log.push(isK(new K()), isK(moving));
try { (function () { "use strict"; K.prototype = {}; })(); log.push("assigned"); } catch (e) { log.push(e.constructor === TypeError); }
// An ordinary function's "prototype" is writable: reassigned between two
// evaluations of one site.
function F() {}
function isF(v) { return v instanceof F; }
var f1 = new F();
log.push(isF(f1), isF({}));
var old = F.prototype;
F.prototype = {};
var f2 = new F();
log.push(isF(f1), isF(f2));
F.prototype = old;
log.push(isF(f1), isF(f2));
// A right-hand side that changes between evaluations of one site.
function test(v, C) { return v instanceof C; }
log.push(test(new K(), K), test(new K(), L), test(new L(), L), test([], Array), test([], K), test(new K(), K), test(f1, F));
// Bound and Proxy right-hand sides.
var BK = K.bind(null);
log.push(test(new K(), BK), test({}, BK), test(new L(), new Proxy(K, {})), test(new L(), BK.bind(null)));
// Non-callable right-hand sides throw a TypeError, and so does a callable
// one whose "prototype" is not an object, unless the candidate is primitive.
[{}, { prototype: {} }, 1, null, undefined, "K", Symbol.iterator].forEach(function (C) {
  try { test({}, C); log.push("no error"); } catch (e) { log.push(e.constructor === TypeError); }
});
function N() {}
N.prototype = 3;
try { test({}, N); log.push("no error"); } catch (e) { log.push(e.constructor === TypeError); }
log.push(test(3, N), test(null, N));
// The constructor's [[Prototype]] changed after its site was warmed, and
// restored.
Object.setPrototypeOf(K, Object.prototype);
log.push(isK(new K()), isK({}), typeof K);
Object.setPrototypeOf(K, null);
log.push(isK(new L()), isK(1));
var gets = [];
Object.setPrototypeOf(K, new Proxy(Function.prototype, { get: function (t, key, r) { gets.push(String(key)); return Reflect.get(t, key, r); } }));
log.push(isK(new K()), isK(1), gets.join("/"));
Object.setPrototypeOf(K, Function.prototype);
log.push(isK(new K()), isK(new L()), isK({}), gets.length);
// One function object between the constructor and Function.prototype: a
// subclass and typed array constructors; then that function's own
// [[Prototype]] changed after warming, and restored.
class S extends K {}
function isS(v) { return v instanceof S; }
function isU8(v) { return v instanceof Uint8Array; }
var u8 = new Uint8Array(2), s = new S();
for (var r = 0; r < 2; r++) {
  log.push([isS(s), isS(new K()), isS({}), isS(1), isU8(u8), isU8(new Int8Array(1)), isU8([]), isU8(Object.create(u8))].map(Number).join(""));
}
var TypedArray = Object.getPrototypeOf(Uint8Array);
var hasInstanceGets = 0;
Object.setPrototypeOf(TypedArray, new Proxy(Function.prototype, { get: function (t, key, r) { if (key === Symbol.hasInstance) hasInstanceGets++; return Reflect.get(t, key, r); } }));
log.push(isU8(u8), isU8({}), hasInstanceGets);
Object.setPrototypeOf(TypedArray, Function.prototype);
log.push(isU8(u8), isU8({}), hasInstanceGets);
Object.setPrototypeOf(K, null);
log.push(isS(s), isS({}));
Object.setPrototypeOf(K, Function.prototype);
log.push(isS(s), isS({}));
print(log.join());
"#;
    let expected = "110000000000,110000000000,110000000000,10000001000 11111001111 00100000010 00010000000 00001000000,10000001000 11111001111 00100000010 00010000000 00001000000,true,1,true,2,true,3,true,false,true,true,true,true,true,false,false,true,true,false,true,false,true,true,false,true,true,true,false,true,true,true,true,true,true,true,true,true,true,false,false,true,false,function,true,false,true,false,Symbol(Symbol.hasInstance)/Symbol(Symbol.hasInstance),true,true,false,2,10001001,10001001,true,false,2,true,false,2,true,false,true,false";
    run_at_top_level_and_in_a_function(source, expected);
}

#[test]
fn site_records_follow_has_instance_defined_after_warming() {
    // Function.prototype's own method stays fixed; an own @@hasInstance
    // defined on and removed from a warmed constructor; one inherited
    // through a subclass; and a getter on Object.prototype, reached only
    // once the constructor no longer inherits from Function.prototype.
    let source = r#"
var log = [];
class K {}
function isK(v) { return v instanceof K; }
var k = new K();
log.push(isK(k), isK({}), isK(1));
// Function.prototype's own @@hasInstance is fixed.
Function.prototype[Symbol.hasInstance] = function () { return "replaced"; };
try { Object.defineProperty(Function.prototype, Symbol.hasInstance, { value: function () { return true; } }); log.push("redefined"); } catch (e) { log.push(e.constructor === TypeError); }
K[Symbol.hasInstance] = function () { return true; };
log.push(isK(k), isK({}), Object.getOwnPropertySymbols(K).length);
// An own @@hasInstance defined on a warmed site's constructor, and removed.
var calls = [];
Object.defineProperty(K, Symbol.hasInstance, { value: function (v) { calls.push(this === K); return v === 1; }, configurable: true });
log.push(isK(k), isK(1), isK({}), calls.join("/"));
delete K[Symbol.hasInstance];
log.push(isK(k), isK(1), calls.length);
// An @@hasInstance on an object between a constructor and Function.prototype.
class M extends K {}
function isM(v) { return v instanceof M; }
log.push(isM(new M()), isM(k));
Object.defineProperty(K, Symbol.hasInstance, { value: function (v) { return v === k; }, configurable: true });
log.push(isM(new M()), isM(k), isK(k), isK(new M()));
// A getter for it on Function.prototype's own prototype is never reached.
Object.defineProperty(Object.prototype, Symbol.hasInstance, { get: function () { log.push("getter"); return function () { return true; }; }, configurable: true });
class Q {}
function isQ(v) { return v instanceof Q; }
log.push(isQ(new Q()), isQ({}), isQ(new Q()));
Object.setPrototypeOf(Q, Object.prototype);
log.push(isQ(new Q()), isQ({}));
print(log.join());
"#;
    let expected = "true,false,false,true,true,false,0,false,true,false,true/true/true,true,false,3,true,false,false,true,true,false,true,false,true,getter,getter,true,true";
    run_at_top_level_and_in_a_function(source, expected);
}
