//! An `instanceof` source site enters a call-free entry that answers from
//! the site's record: the constructor's own "prototype" at a recorded
//! dictionary position or intrinsic value group, the default @@hasInstance
//! proved through at most three function objects, and the candidate's chain
//! through ordinary, function and Array headers. Each script changes what a
//! warmed site relies on between two evaluations. Expectations are Node 24's.
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

#[test]
fn a_warmed_instanceof_site_follows_every_fact_its_entry_reads() {
    // A configurable "prototype" moved, made an accessor (its getter runs
    // once per test with the constructor as receiver) and made a primitive;
    // dictionary constructors with "prototype" at different positions and
    // intrinsic constructors alternating at one site, one later converted
    // to a dictionary; chains of three and four function objects; a
    // candidate chain through a function, an Array and a Proxy; both
    // [[Prototype]] edges changed; and other kinds of constructor.
    let source = r#"
var log = [];
function test(v, C) { return v instanceof C; }
// A configurable own "prototype" that moves between dictionary positions,
// becomes an accessor and comes back, all at one warmed site.
var m = ({ f() {} }).f;
var P1 = {}, P2 = {};
Object.defineProperty(m, "prototype", { value: P1, configurable: true, writable: true });
var a = Object.create(P1), b = Object.create(P2);
log.push(test(a, m), test(b, m), test(a, m));
delete m.prototype;
for (var i = 0; i < 6; i++) m["x" + i] = i;
Object.defineProperty(m, "prototype", { value: P2, configurable: true });
log.push(test(a, m), test(b, m));
var reads = 0, receivers = [];
Object.defineProperty(m, "prototype", { get: function () { reads++; receivers.push(this === m); return P1; }, configurable: true });
log.push(test(a, m), test(b, m), reads, receivers.join("/"));
Object.defineProperty(m, "prototype", { value: P2, configurable: true, writable: true });
log.push(test(a, m), test(b, m), reads);
m.prototype = 5;
try { test(a, m); log.push("no error"); } catch (error) { log.push(error.constructor === TypeError); }
log.push(test(1, m), test("s", m));
// Two dictionary constructors with "prototype" at different positions.
function F() {}
var G = function () {};
G.extra = 1;
var fs = [new F(), new G(), {}];
for (var round = 0; round < 3; round++) {
  log.push(fs.map(function (v) { return [test(v, F), test(v, G)].join(""); }).join(":"));
}
// Intrinsic constructors alternating at one site, one of them later
// converted to a dictionary by an own property.
var builtins = [Map, Set, Int8Array, Date, Array, Promise, RegExp];
var samples = [new Map(), new Set(), new Int8Array(1), new Date(0), [], Promise.resolve(), /x/, {}];
function row() {
  return builtins.map(function (C) {
    return samples.map(function (v) { return test(v, C) ? 1 : 0; }).join("");
  }).join(" ");
}
log.push(row());
Map.extra = 1;
Set.prototype.extra = 2;
log.push(row());
delete Map.extra;
log.push(row());
// A function chain of three and of four function objects before
// %Function.prototype%.
class A1 {}
class A2 extends A1 {}
class A3 extends A2 {}
class A4 extends A3 {}
log.push(test(new A4(), A1), test(new A4(), A3), test(new A3(), A4), test(new A4(), A4), test(new A1(), A4));
Object.defineProperty(A1, Symbol.hasInstance, { value: function (v) { return v === 7; } });
log.push(test(7, A4), test(new A4(), A4), test(7, A1), test(new A4(), A2));
// The candidate's chain through a function, an Array and a Proxy.
function H() {}
var fn = function () {};
Object.setPrototypeOf(fn, H.prototype);
var arr = [];
Object.setPrototypeOf(arr, H.prototype);
var traps = 0;
var proxied = Object.create(new Proxy(H.prototype, { getPrototypeOf: function (t) { traps++; return Reflect.getPrototypeOf(t); } }));
log.push(test(fn, H), test(arr, H), test(proxied, H), traps, test(proxied, Object), traps, test(proxied, Map), traps);
// The candidate's [[Prototype]] and the constructor's [[Prototype]] changed
// between evaluations.
function J() {}
var j = new J();
log.push(test(j, J));
Object.setPrototypeOf(J, Object.prototype);
log.push(test(j, J), typeof J);
Object.setPrototypeOf(J, Function.prototype);
log.push(test(j, J));
Object.setPrototypeOf(j, null);
log.push(test(j, J), test(j, Object));
// Function.prototype's own method is fixed; writing it does nothing.
Function.prototype[Symbol.hasInstance] = function () { return "replaced"; };
try { Object.defineProperty(Function.prototype, Symbol.hasInstance, { value: 1 }); log.push("redefined"); } catch (error) { log.push(error.constructor === TypeError); }
log.push(test(new J(), J), test({}, J));
// A Proxy, a bound function and a non-callable object as the constructor.
var PJ = new Proxy(J, {});
log.push(test(new J(), PJ), test(new J(), J.bind(null)));
try { test({}, { prototype: {} }); log.push("no error"); } catch (error) { log.push(error.constructor === TypeError); }
print(log.join());
"#;
    let expected = "true,false,true,false,true,true,false,2,true/true,false,true,2,true,false,false,truefalse:falsetrue:falsefalse,truefalse:falsetrue:falsefalse,truefalse:falsetrue:falsefalse,10000000 01000000 00100000 00010000 00001000 00000100 00000010,10000000 01000000 00100000 00010000 00001000 00000100 00000010,10000000 01000000 00100000 00010000 00001000 00000100 00000010,true,true,false,true,false,true,false,true,false,true,true,false,1,true,2,false,3,true,true,function,true,false,false,true,true,false,true,true,true";
    run(source, expected);
    run(&format!("(function () {{\n{source}}})();"), expected);
}
