//! Stores and allocations that answer their common case before the general
//! kernel: a strict function that creates closures omits an arguments object
//! no child can name; an Array element is published into the packed backing
//! it already has room in; a fresh literal stores a value without a scalar
//! word into its reference backing; and a prototype of a usual layout
//! becomes the typed [[Prototype]] edge by one inline layout test. Each script warms its sites
//! with the admitted case and then runs the same sites with every excluded
//! one. Expectations are Node 24's, taken by running each body as a Script,
//! as function code and as a module.
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
fn strict_child_callables_observe_arguments_only_through_a_captured_binding() {
    // A strict function that creates closures omits its arguments object
    // when no nested function names `arguments` or holds a direct eval.
    // Children that capture only other bindings, children that name their
    // own `arguments` (ordinary functions, generators, their arrows and
    // evals) and arrow functions that name this activation's, directly or
    // through a direct eval, all run here. Activations are called with
    // fewer, exactly as many and more arguments than parameters, and strict
    // functions stay opaque to `.arguments` and `.caller`.
    run_as_script_function_and_module(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
// Ordinary child functions bind their own `arguments`; these arrows name none.
function plain(x) { "use strict"; var f = function () { return x; }; return f(); }
function ownArgs(x) { "use strict"; var f = function () { return arguments.length + ":" + arguments[0]; }; return f(x, 2, 3); }
function arrowCapture(x) { "use strict"; var f = () => x + 1; return f(); }
function nestedArrowCapture(x) { "use strict"; return (() => () => x * 2)()(); }
function record(x) { "use strict"; var r = { id: x, ping: function () { return r.id; }, next: null }; return r.ping(); }
function wrap(i) { "use strict"; var o = i[Symbol.iterator]; var n = {}; n[Symbol.iterator] = function () { return o.call(i); }; return n; }
function innerArrowOwn(x) { "use strict"; return function () { return () => arguments[0]; }(x + 1)(); }
function innerEvalOwn(x) { "use strict"; return function () { return eval("arguments[0] + arguments.length"); }(x, 0); }
function generatorChild(x) { "use strict"; var g = function* () { yield arguments.length; yield x; }; return Array.from(g(1, 2)).join("/"); }
function loopClosures(n) { "use strict"; var fs = []; for (var i = 0; i < n; i++) fs.push(function () { return i + n; }); return fs.map(function (f) { return f(); }).join(""); }
function catchClosure(x) { "use strict"; try { throw x; } catch (e) { return function () { return e; }; } }
function reflect(cb) { "use strict"; var keep = function () { return cb; }; return cb(reflect, keep); }
// An arrow function, or a direct eval in one, names this activation's binding.
function arrowArgs(x) { "use strict"; var f = () => arguments[0] + ":" + arguments.length; return f(); }
function nestedArrowArgs(x) { "use strict"; return (() => () => arguments[1])()(); }
function arrowEval(x) { "use strict"; return (() => eval("arguments[0] + arguments.length"))(); }
function arrowEvalText(x, text) { "use strict"; return (() => eval(text))(); }
function nestedArrowEvalText(x, text) { "use strict"; return (() => () => eval(text))()(); }
function arrowTypeof(x) { "use strict"; return (() => typeof arguments)(); }
function arrowWrite(x) { "use strict"; var f = () => { arguments[0] = 9; return arguments[0] + ":" + x; }; return f(); }
function arrowEscapes(x) { "use strict"; return () => arguments; }
function mixed(x) { "use strict"; var a = function () { return x; }; var b = () => arguments.length; return a() + ":" + b(); }
function own(x) { "use strict"; var f = function () { return x; }; return arguments.length + ":" + arguments[0] + ":" + f(); }
function ownEval(x) { "use strict"; var f = function () { return x; }; return eval("arguments.length") + ":" + f(); }
function ownEvalText(x, text) { "use strict"; var f = function () { return x; }; return eval(text) + ":" + f(); }
function calleeThrows(x) { "use strict"; var f = () => arguments.callee; return f; }
function literalArrow(x) { "use strict"; return [x, { deep: () => arguments.length }][1].deep(); }
function shadowed(arguments_) { "use strict"; var f = function (arguments_) { return arguments_; }; return f(arguments_ + 1); }
for (var i = 0; i < 4; i++) {
  log.push(plain(i), ownArgs(i), arrowCapture(i), nestedArrowCapture(i), record(i), innerArrowOwn(i), innerEvalOwn(i), generatorChild(i), loopClosures(i), catchClosure(i)());
  log.push([...wrap([i, i + 1, i + 2])].join("+"), Array.from(wrap("ab" + i)).join("+"));
  log.push(arrowArgs(i), arrowArgs(i, 1, 2), arrowArgs(), nestedArrowArgs(i, "n" + i), arrowEval(i), arrowEval(i, 1), arrowEvalText(i, "arguments[1].length + arguments.length"), nestedArrowEvalText(i, "typeof arguments + arguments.length", 3));
  log.push(arrowTypeof(i), arrowWrite(i), arrowEscapes(i, "e")[1], arrowEscapes().length, mixed(i, 1), own(i, 1, 2), ownEval(i, 1), ownEvalText(i, "arguments[0] + arguments.length"), literalArrow(i, 2, 3, 4), shadowed(i));
}
log.push(show(calleeThrows(1)), show(function () { return plain.arguments; }), show(function () { return plain.caller; }), Object.hasOwn(plain, "arguments"), Object.hasOwn(wrap, "caller"));
log.push(show(function () { return reflect(function (f) { return f.arguments; }); }), show(function () { return reflect(function (f) { return f.caller; }); }), show(function () { return reflect(function (f, keep) { return keep()() === f; }); }), show(function () { return reflect(function (f, keep) { return typeof keep() + arguments.length; }); }));
var a = arrowEscapes(1, 2, 3);
log.push(Object.prototype.toString.call(a()), a().length, a() === a(), Object.getOwnPropertyNames(a()).join("|"), typeof Object.getOwnPropertyDescriptor(a(), "callee").get);
print(log.join());
"##,
        r##"0,3:0,1,0,0,1,2,2/0,,0,0+1+2,a+b+0,0:1,0:3,undefined:0,n0,1,2,40,object3,object,9:0,,0,0:2,3:0:0,2:0,2:0,4,1,1,3:1,2,2,1,2,3,2/1,2,1,1+2+3,a+b+1,1:1,1:3,undefined:0,n1,2,3,40,object3,object,9:1,,0,1:2,3:1:1,2:1,3:1,4,2,2,3:2,3,4,2,3,4,2/2,44,2,2+3+4,a+b+2,2:1,2:3,undefined:0,n2,3,4,40,object3,object,9:2,,0,2:2,3:2:2,2:2,4:2,4,3,3,3:3,4,6,3,4,5,2/3,666,3,3+4+5,a+b+3,3:1,3:3,undefined:0,n3,4,5,40,object3,object,9:3,,0,3:2,3:3:3,2:3,5:3,4,4,TypeError,TypeError,TypeError,false,false,TypeError,TypeError,TypeError,function2,[object Arguments],3,true,0|1|2|length|callee,function"##,
    );
}

#[test]
fn strict_child_declarations_methods_and_classes_keep_their_parents_arguments_exact() {
    // The other forms a child callable takes: hoisted and block-level
    // declarations (called before and after their text, recursive, escaping),
    // a named function expression, object methods and accessors, class
    // methods, an async child, a child created under try/finally and one
    // whose default parameter holds an arrow function.
    run_as_script_function_and_module(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
function declCapture(x) { "use strict"; function inner() { return x + 1; } return inner(); }
function declOwn(x) { "use strict"; function inner() { return arguments.length + ":" + x; } return inner(1, 2); }
function declHoisted(x) { "use strict"; var r = inner(); function inner() { return typeof x; } return r; }
function declArrow(x) { "use strict"; function inner() { return (() => arguments[0])(); } return inner(x * 2); }
function declEscapes(x) { "use strict"; function inner() { return x; } return inner; }
function declRecursive(n) { "use strict"; function down(k) { return k ? down(k - 1) + 1 : n; } return down(n); }
function declBlock(x) { "use strict"; { function inner() { return x; } var f = inner; } return f(); }
function declAndArrow(x) { "use strict"; function inner() { return x; } var a = () => arguments.length; return inner() + ":" + a(); }
function namedExpr(x) { "use strict"; var f = function self(n) { return n ? self(n - 1) : x; }; return f(3); }
function method(x) { "use strict"; var o = { m() { return x; }, get g() { return x + 1; }, n: function () { return arguments.length; } }; return o.m() + ":" + o.g + ":" + o.n(1, 2, 3); }
function klass(x) { "use strict"; class K { m() { return x; } static s() { return arguments.length; } } return new K().m() + ":" + K.s(1, 2); }
function asyncChild(x) { "use strict"; var f = async function () { return x; }; return typeof f(); }
function tryClosure(x) { "use strict"; try { return (function () { return x; })(); } finally { log.push("f" + x); } }
function defaultChild(x) { "use strict"; var f = function (y = () => x) { return y(); }; return f(); }
for (var i = 0; i < 3; i++) log.push(declCapture(i), declOwn(i), declHoisted(i), declArrow(i), declEscapes(i)(), declRecursive(i), declBlock(i), declAndArrow(i, 1), namedExpr(i), method(i), klass(i), asyncChild(i), tryClosure(i), defaultChild(i));
log.push(show(function () { return declCapture.arguments; }), show(function () { return declEscapes(1).caller; }));
print(log.join());
"##,
        r##"f0,1,2:0,number,0,0,0,0,0:2,0,0:1:3,0:2,object,0,0,f1,2,2:1,number,2,1,2,1,1:2,1,1:2:3,1:2,object,1,1,f2,3,2:2,number,4,2,4,2,2:2,2,2:3:3,2:2,object,2,2,TypeError,TypeError"##,
    );
}

#[test]
fn array_elements_publish_into_the_packed_backing_they_have_room_in() {
    // Literal elements and appends through `c[f] = v` publish into the
    // packed backing without a copy when it has room, and a first element
    // creates the backing (one slot per index below a short length). Every
    // site is warmed with references and then given every kind of value,
    // holes, literals longer than the born table, a value that throws, and
    // presized arrays on both sides of the bound; the arrays are then
    // mutated. The same store site meets frozen, sealed, non-extensible and
    // fixed-length arrays, typed arrays, a Proxy, an arguments object, a
    // setter and primitives, and indexed properties that appear on
    // Array.prototype and Object.prototype after warming: a literal defines
    // its own elements and a store asks the chain.
    run_as_script_function_and_module(
        r##"
var log = [];
function text(v) { return Object.is(v, -0) ? "-0" : typeof v === "symbol" ? "sym" : typeof v === "bigint" ? v + "n" : typeof v === "function" ? "fn" : typeof v === "object" && v !== null ? (Array.isArray(v) ? "[" + v.length + "]" : "obj") : String(v); }
function describe(a) {
  var holes = "", parts = "";
  for (var i = 0; i < a.length; i++) if (!(i in a)) holes += i;
  for (var j = 0; j < a.length; j++) parts += (j ? " " : "") + text(a[j]);
  return a.length + "[" + parts + "]k" + Object.keys(a).join("") + "h" + holes;
}
function lit1(a) { return [a]; }
function lit4(a, b, c, d) { return [a, b, c, d]; }
function lit5(a, b, c, d, e) { return [a, b, c, d, e]; }
function litHoles(a, b) { return [a, , b, , ]; }
function litLeadingHoles(a) { return [, , , , , a]; }
function lit9(v) { return [v, v, v, v, v, v, v, v, v]; }
function litLong(v) { return [v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v,v]; }
function litThrow(a, f) { return [a, f(), a, a]; }
function litNested(a) { return [[a, a], [[a]], a, [a, , a]]; }
function append(n, make) { "use strict"; var c = []; for (var f = 0; f < n; f++) c[f] = make(f); return c; }
function presized(n, at, v) { "use strict"; var c = new Array(n); try { c[at] = v; } catch (e) { c.error = e.constructor.name; } return c; }
function storeAt(c, i, v) { "use strict"; try { c[i] = v; return "ok"; } catch (e) { return e.constructor.name; } }
var sym = Symbol("s"), obj = { o: 1 }, fn = function () {};
var kinds = ["s", obj, sym, null, undefined, true, false, 10n, fn, [1, 2], 1, 0, -0, NaN, 1.5, -7, 2 ** 53, Infinity];
// Warm every site with references, then give each every kind of value.
for (var w = 0; w < 3; w++) { lit1("w"); lit4("a", obj, null, sym); lit5("a", "b", "c", "d", "e"); litHoles("a", obj); lit9(obj); append(6, function (i) { return "v" + i; }); }
for (var k = 0; k < kinds.length; k++) {
  var v = kinds[k], u = kinds[(k + 7) % kinds.length];
  log.push(describe(lit1(v)), describe(lit4(v, u, v, u)), describe(lit4(u, v, 1, "t")), describe(lit5(1, 2, v, 4, u)), describe(litHoles(v, u)), describe(litLeadingHoles(v)), describe(lit9(v)));
  log.push(describe(append(1, function () { return v; })), describe(append(5, function (i) { return i % 2 ? v : u; })), describe(append(9, function (i) { return i < 4 ? i : v; })));
}
var longArray = litLong(obj); log.push(longArray.length, longArray[69] === obj, 70 in longArray, describe(litLong(7)).length, describe(litLong("z")).slice(0, 12));
log.push(describe(litNested("n")[3]), litNested(obj)[1][0][0] === obj, describe(append(33, function (i) { return i === 32 ? obj : "x"; })).length);
// A value that throws leaves no array; later literals are unaffected.
try { litThrow("a", function () { throw new RangeError("stop"); }); } catch (e) { log.push(e.constructor.name); }
log.push(describe(litThrow("a", function () { return obj; })));
// An index below a short length is born into one table; others keep their backing.
log.push(describe(presized(5, 3, "p")), describe(presized(5, 0, obj)), describe(presized(64, 63, "q")).slice(0, 4), describe(presized(65, 64, "r")).slice(0, 4), describe(presized(100, 99, sym)).slice(0, 5), describe(presized(5, 4, 2.5)), describe(presized(3, 7, "past")), describe(presized(0, 0, "zero")), describe(presized(4, 2, -0)));
var filled = presized(6, 5, "last"); filled[0] = "first"; filled[2] = 2; filled[3] = obj; log.push(describe(filled), filled.indexOf("last"), filled.lastIndexOf(2), JSON.stringify(filled));
// Later mutation of arrays born through these sites.
var m = lit4("a", "b", "c", "d");
m.push("e", 6); delete m[1]; m[9] = "far"; log.push(describe(m)); m.length = 3; log.push(describe(m)); m.reverse(); log.push(describe(m));
m[1] = obj; m.unshift(0); log.push(describe(m), JSON.stringify(Object.getOwnPropertyDescriptor(m, 2)), JSON.stringify(Object.getOwnPropertyDescriptor(lit4(1, "x", 2, 3), 1)));
var n = append(4, function (i) { return "n" + i; }); delete n[0]; delete n[1]; delete n[2]; delete n[3]; n[0] = 1; n[1] = "one"; log.push(describe(n), n.concat(lit1(sym)).length, lit4(3, 1, "b", "a").sort().join(""), lit4("x", undefined, 1, null).join("-"));
Object.defineProperty(m, 0, { get: function () { return "getter"; }, configurable: true }); log.push(m[0], describe(lit4(m[0], m, 1, 2)));
// Receivers that take no append: frozen, sealed, non-extensible, a fixed length.
var frozen = Object.freeze(lit4("a", "b", "c", "d")), sealed = Object.seal(lit1("s")), closed = Object.preventExtensions(append(2, function (i) { return "c" + i; })), fixed = append(2, String);
Object.defineProperty(fixed, "length", { writable: false });
log.push(storeAt(frozen, 4, "x"), storeAt(frozen, 0, "x"), storeAt(sealed, 1, "x"), storeAt(sealed, 0, "y"), storeAt(closed, 2, "x"), storeAt(closed, 1, "z"), storeAt(fixed, 2, "x"), storeAt(fixed, 1, "w"), describe(frozen), describe(sealed), describe(closed), describe(fixed));
// The same store site with receivers that are not packed arrays.
var typed = new Uint8Array(2), trapped = "", proxy = new Proxy([], { set: function (t, key, value, r) { trapped += String(key) + "=" + text(value) + "|"; return Reflect.set(t, key, value, r); } });
var args = (function () { return arguments; })("a0"), plainObject = {}, wrapper = new String("ab"), holder = { set 2(value) { trapped += "setter=" + text(value) + "|"; } };
log.push(storeAt(typed, 0, 300), storeAt(typed, 5, 1), storeAt(typed, 1, "7"), typed.join(), storeAt(proxy, 0, "p"), storeAt(proxy, 1, obj), proxy.length, storeAt(args, 0, "A"), storeAt(args, 3, "D"), args.length, args[3], storeAt(plainObject, 0, "zero"), Object.keys(plainObject).join(), storeAt(wrapper, 0, "x"), storeAt(wrapper, 5, "y"), wrapper[5], storeAt(holder, 2, "h"), storeAt(null, 0, 1), storeAt(undefined, 0, 1), storeAt(5, 0, 1), trapped);
// Indexed properties that appear on the prototypes after warming: a literal
// defines its own elements, and a store asks the chain.
var seen = "";
Object.defineProperty(Array.prototype, "2", { set: function (value) { seen += "set2=" + text(value) + "|"; }, get: function () { return "proto2"; }, configurable: true });
var defined = lit4("a", "b", "c", "d"), stored = append(4, function (i) { return "s" + i; });
log.push(describe(defined), Object.hasOwn(defined, 2), describe(stored), Object.hasOwn(stored, 2), stored.length, seen, describe(presized(4, 2, "own?")), describe(litHoles("a", "b")));
delete Array.prototype[2];
Object.defineProperty(Object.prototype, "1", { value: "readonly", writable: false, configurable: true });
var blocked = [];
log.push(storeAt(blocked, 0, "zero"), storeAt(blocked, 1, "one"), describe(blocked), describe(lit4("a", "b", "c", "d")), describe(presized(3, 1, "x")));
delete Object.prototype[1];
Object.prototype[3] = "inherited";
var over = append(5, function (i) { return "o" + i; }); log.push(describe(over), Object.hasOwn(over, 3), describe(litLeadingHoles("z")).slice(0, 14));
delete Object.prototype[3];
log.push(describe(append(4, function (i) { return "s" + i; })), describe(lit4(1, 2, 3, 4)), seen.length);
// A subclass and a foreign length.
class Sub extends Array {}
var sub = new Sub(); log.push(storeAt(sub, 0, "sub"), storeAt(sub, 1, obj), sub.length, sub instanceof Sub, describe(sub));
print(log.join(";"));
"##,
        r##"1[s]k0h;4[s 10n s 10n]k0123h;4[10n s 1 t]k0123h;5[1 2 s 4 10n]k01234h;4[s undefined 10n undefined]k02h13;6[undefined undefined undefined undefined undefined s]k5h01234;9[s s s s s s s s s]k012345678h;1[s]k0h;5[10n s 10n s 10n]k01234h;9[0 1 2 3 s s s s s]k012345678h;1[obj]k0h;4[obj fn obj fn]k0123h;4[fn obj 1 t]k0123h;5[1 2 obj 4 fn]k01234h;4[obj undefined fn undefined]k02h13;6[undefined undefined undefined undefined undefined obj]k5h01234;9[obj obj obj obj obj obj obj obj obj]k012345678h;1[obj]k0h;5[fn obj fn obj fn]k01234h;9[0 1 2 3 obj obj obj obj obj]k012345678h;1[sym]k0h;4[sym [2] sym [2]]k0123h;4[[2] sym 1 t]k0123h;5[1 2 sym 4 [2]]k01234h;4[sym undefined [2] undefined]k02h13;6[undefined undefined undefined undefined undefined sym]k5h01234;9[sym sym sym sym sym sym sym sym sym]k012345678h;1[sym]k0h;5[[2] sym [2] sym [2]]k01234h;9[0 1 2 3 sym sym sym sym sym]k012345678h;1[null]k0h;4[null 1 null 1]k0123h;4[1 null 1 t]k0123h;5[1 2 null 4 1]k01234h;4[null undefined 1 undefined]k02h13;6[undefined undefined undefined undefined undefined null]k5h01234;9[null null null null null null null null null]k012345678h;1[null]k0h;5[1 null 1 null 1]k01234h;9[0 1 2 3 null null null null null]k012345678h;1[undefined]k0h;4[undefined 0 undefined 0]k0123h;4[0 undefined 1 t]k0123h;5[1 2 undefined 4 0]k01234h;4[undefined undefined 0 undefined]k02h13;6[undefined undefined undefined undefined undefined undefined]k5h01234;9[undefined undefined undefined undefined undefined undefined undefined undefined undefined]k012345678h;1[undefined]k0h;5[0 undefined 0 undefined 0]k01234h;9[0 1 2 3 undefined undefined undefined undefined undefined]k012345678h;1[true]k0h;4[true -0 true -0]k0123h;4[-0 true 1 t]k0123h;5[1 2 true 4 -0]k01234h;4[true undefined -0 undefined]k02h13;6[undefined undefined undefined undefined undefined true]k5h01234;9[true true true true true true true true true]k012345678h;1[true]k0h;5[-0 true -0 true -0]k01234h;9[0 1 2 3 true true true true true]k012345678h;1[false]k0h;4[false NaN false NaN]k0123h;4[NaN false 1 t]k0123h;5[1 2 false 4 NaN]k01234h;4[false undefined NaN undefined]k02h13;6[undefined undefined undefined undefined undefined false]k5h01234;9[false false false false false false false false false]k012345678h;1[false]k0h;5[NaN false NaN false NaN]k01234h;9[0 1 2 3 false false false false false]k012345678h;1[10n]k0h;4[10n 1.5 10n 1.5]k0123h;4[1.5 10n 1 t]k0123h;5[1 2 10n 4 1.5]k01234h;4[10n undefined 1.5 undefined]k02h13;6[undefined undefined undefined undefined undefined 10n]k5h01234;9[10n 10n 10n 10n 10n 10n 10n 10n 10n]k012345678h;1[10n]k0h;5[1.5 10n 1.5 10n 1.5]k01234h;9[0 1 2 3 10n 10n 10n 10n 10n]k012345678h;1[fn]k0h;4[fn -7 fn -7]k0123h;4[-7 fn 1 t]k0123h;5[1 2 fn 4 -7]k01234h;4[fn undefined -7 undefined]k02h13;6[undefined undefined undefined undefined undefined fn]k5h01234;9[fn fn fn fn fn fn fn fn fn]k012345678h;1[fn]k0h;5[-7 fn -7 fn -7]k01234h;9[0 1 2 3 fn fn fn fn fn]k012345678h;1[[2]]k0h;4[[2] 9007199254740992 [2] 9007199254740992]k0123h;4[9007199254740992 [2] 1 t]k0123h;5[1 2 [2] 4 9007199254740992]k01234h;4[[2] undefined 9007199254740992 undefined]k02h13;6[undefined undefined undefined undefined undefined [2]]k5h01234;9[[2] [2] [2] [2] [2] [2] [2] [2] [2]]k012345678h;1[[2]]k0h;5[9007199254740992 [2] 9007199254740992 [2] 9007199254740992]k01234h;9[0 1 2 3 [2] [2] [2] [2] [2]]k012345678h;1[1]k0h;4[1 Infinity 1 Infinity]k0123h;4[Infinity 1 1 t]k0123h;5[1 2 1 4 Infinity]k01234h;4[1 undefined Infinity undefined]k02h13;6[undefined undefined undefined undefined undefined 1]k5h01234;9[1 1 1 1 1 1 1 1 1]k012345678h;1[1]k0h;5[Infinity 1 Infinity 1 Infinity]k01234h;9[0 1 2 3 1 1 1 1 1]k012345678h;1[0]k0h;4[0 s 0 s]k0123h;4[s 0 1 t]k0123h;5[1 2 0 4 s]k01234h;4[0 undefined s undefined]k02h13;6[undefined undefined undefined undefined undefined 0]k5h01234;9[0 0 0 0 0 0 0 0 0]k012345678h;1[0]k0h;5[s 0 s 0 s]k01234h;9[0 1 2 3 0 0 0 0 0]k012345678h;1[-0]k0h;4[-0 obj -0 obj]k0123h;4[obj -0 1 t]k0123h;5[1 2 -0 4 obj]k01234h;4[-0 undefined obj undefined]k02h13;6[undefined undefined undefined undefined undefined -0]k5h01234;9[-0 -0 -0 -0 -0 -0 -0 -0 -0]k012345678h;1[-0]k0h;5[obj -0 obj -0 obj]k01234h;9[0 1 2 3 -0 -0 -0 -0 -0]k012345678h;1[NaN]k0h;4[NaN sym NaN sym]k0123h;4[sym NaN 1 t]k0123h;5[1 2 NaN 4 sym]k01234h;4[NaN undefined sym undefined]k02h13;6[undefined undefined undefined undefined undefined NaN]k5h01234;9[NaN NaN NaN NaN NaN NaN NaN NaN NaN]k012345678h;1[NaN]k0h;5[sym NaN sym NaN sym]k01234h;9[0 1 2 3 NaN NaN NaN NaN NaN]k012345678h;1[1.5]k0h;4[1.5 null 1.5 null]k0123h;4[null 1.5 1 t]k0123h;5[1 2 1.5 4 null]k01234h;4[1.5 undefined null undefined]k02h13;6[undefined undefined undefined undefined undefined 1.5]k5h01234;9[1.5 1.5 1.5 1.5 1.5 1.5 1.5 1.5 1.5]k012345678h;1[1.5]k0h;5[null 1.5 null 1.5 null]k01234h;9[0 1 2 3 1.5 1.5 1.5 1.5 1.5]k012345678h;1[-7]k0h;4[-7 undefined -7 undefined]k0123h;4[undefined -7 1 t]k0123h;5[1 2 -7 4 undefined]k01234h;4[-7 undefined undefined undefined]k02h13;6[undefined undefined undefined undefined undefined -7]k5h01234;9[-7 -7 -7 -7 -7 -7 -7 -7 -7]k012345678h;1[-7]k0h;5[undefined -7 undefined -7 undefined]k01234h;9[0 1 2 3 -7 -7 -7 -7 -7]k012345678h;1[9007199254740992]k0h;4[9007199254740992 true 9007199254740992 true]k0123h;4[true 9007199254740992 1 t]k0123h;5[1 2 9007199254740992 4 true]k01234h;4[9007199254740992 undefined true undefined]k02h13;6[undefined undefined undefined undefined undefined 9007199254740992]k5h01234;9[9007199254740992 9007199254740992 9007199254740992 9007199254740992 9007199254740992 9007199254740992 9007199254740992 9007199254740992 9007199254740992]k012345678h;1[9007199254740992]k0h;5[true 9007199254740992 true 9007199254740992 true]k01234h;9[0 1 2 3 9007199254740992 9007199254740992 9007199254740992 9007199254740992 9007199254740992]k012345678h;1[Infinity]k0h;4[Infinity false Infinity false]k0123h;4[false Infinity 1 t]k0123h;5[1 2 Infinity 4 false]k01234h;4[Infinity undefined false undefined]k02h13;6[undefined undefined undefined undefined undefined Infinity]k5h01234;9[Infinity Infinity Infinity Infinity Infinity Infinity Infinity Infinity Infinity]k012345678h;1[Infinity]k0h;5[false Infinity false Infinity false]k01234h;9[0 1 2 3 Infinity Infinity Infinity Infinity Infinity]k012345678h;70;true;false;275;70[z z z z z;3[n undefined n]k02h1;true;129;RangeError;4[a obj a a]k0123h;5[undefined undefined undefined p undefined]k3h0124;5[obj undefined undefined undefined undefined]k0h1234;64[u;65[u;100[u;5[undefined undefined undefined undefined 2.5]k4h0123;8[undefined undefined undefined undefined undefined undefined undefined past]k7h0123456;1[zero]k0h;4[undefined undefined -0 undefined]k2h013;6[first undefined 2 obj undefined last]k0235h14;5;2;["first",null,2,{"o":1},null,"last"];10[a undefined c d e 6 undefined undefined undefined far]k023459h1678;3[a undefined c]k02h1;3[c undefined a]k02h1;4[0 c obj a]k0123h;{"value":{"o":1},"writable":true,"enumerable":true,"configurable":true};{"value":"x","writable":true,"enumerable":true,"configurable":true};4[1 one undefined undefined]k01h23;5;13ab;x--1-;getter;4[getter [4] 1 2]k0123h;TypeError;TypeError;TypeError;ok;TypeError;ok;TypeError;ok;4[a b c d]k0123h;1[y]k0h;2[c0 z]k01h;2[0 w]k01h;ok;ok;ok;44,7;ok;ok;2;ok;ok;1;D;ok;0;TypeError;ok;y;ok;TypeError;TypeError;TypeError;0=p|1=obj|setter=h|;4[a b c d]k0123h;true;4[s0 s1 proto2 s3]k013h;false;4;set2=s2|;4[undefined undefined proto2 undefined]kh013;4[a undefined b undefined]k02h13;ok;TypeError;1[zero]k0h;4[a b c d]k0123h;3[undefined readonly undefined]kerrorh02;5[o0 o1 o2 o3 o4]k01234h;true;6[undefined un;4[s0 s1 s2 s3]k0123h;4[1 2 3 4]k0123h;18;ok;ok;2;true;2[sub obj]k01h"##,
    );
}

#[test]
fn literal_values_store_into_inline_slots_and_lazily_backed_arrays() {
    // Literals of one to seventeen static keys: inline slots; tag, scalar
    // and reference arrays, where a value without a scalar word is stored
    // inline once the reference backing exists (the scalar backing appears
    // with a first nonzero word, the reference backing with a first
    // reference); and the dictionary beyond sixteen keys. Every site is warmed with references
    // and then given every kind of value, including -0, NaN and zero words;
    // values evaluate in order, may throw between stores, re-enter the same
    // site and suspend in a generator. Objects born here keep default
    // attributes and key order through later structural changes.
    run_as_script_function_and_module(
        r##"
var log = [];
function text(v) { return Object.is(v, -0) ? "-0" : typeof v === "symbol" ? "sym" : typeof v === "bigint" ? v + "n" : typeof v === "function" ? "fn" : typeof v === "object" && v !== null ? (Array.isArray(v) ? "[" + v.length + "]" : "obj") : typeof v === "string" ? "'" + v + "'" : String(v); }
function dump(o) { var keys = Object.keys(o), out = ""; for (var i = 0; i < keys.length; i++) out += (i ? " " : "") + keys[i] + "=" + text(o[keys[i]]); return "{" + out + "}"; }
function three(a, b, c) { return { x: a, y: b, z: c }; }
function five(a, b, c, d, e) { return { $$typeof: a, type: b, key: c, ref: d, props: e }; }
function sixteen(v) { return { k0: v, k1: v, k2: v, k3: v, k4: v, k5: v, k6: v, k7: v, k8: v, k9: v, k10: v, k11: v, k12: v, k13: v, k14: v, k15: v }; }
function seventeen(v) { return { k0: v, k1: v, k2: v, k3: v, k4: v, k5: v, k6: v, k7: v, k8: v, k9: v, k10: v, k11: v, k12: v, k13: v, k14: v, k15: v, last: v }; }
function two(a, b) { return { first: a, second: b }; }
function nested(a) { return { outer: { p: a, q: a, r: a }, list: [a, { m: a, n: 1, o: a }], tail: a }; }
function thrower(a, f) { return { a: a, b: f(), c: a, d: a }; }
function ordered(trace) { return { a: trace("a"), b: trace("b"), c: trace("c"), d: trace("d") }; }
function recur(n) { return n === 0 ? { depth: 0, child: null, tag: "leaf" } : { depth: n, child: recur(n - 1), tag: "node" }; }
function reenter(n, o) { return { before: n, inner: n ? reenter(n - 1, o) : o, after: -n, same: o }; }
function* gen(a) { var o = { first: a, second: yield 1, third: yield 2, fourth: a }; return o; }
function numeric(i) { return { x: i, y: i * 0.5, z: -i, w: 0 }; }
function zeros() { return { a: 0, b: false, c: null, d: undefined, e: "" }; }
var sym = Symbol("s"), obj = { o: 1 }, fn = function () {};
var kinds = ["s", obj, sym, null, undefined, true, false, 10n, fn, [1, 2], 1, 0, -0, NaN, 1.5, -7, 2 ** 53, Infinity, ""];
// Warm every site with references, then give each every kind of value.
for (var w = 0; w < 3; w++) { three("a", obj, null); five(sym, "div", null, null, obj); sixteen(obj); two("a", obj); nested(obj); recur(2); }
for (var k = 0; k < kinds.length; k++) {
  var v = kinds[k], u = kinds[(k + 7) % kinds.length], t = kinds[(k + 13) % kinds.length];
  log.push(dump(three(v, u, t)), dump(three(t, v, v)), dump(five(v, u, t, v, u)), dump(five(1, v, 2, u, 3)), dump(two(v, u)), dump(nested(v).outer), dump(nested(v).list[1]));
}
var big = sixteen(-0); log.push(dump(big), Object.is(sixteen(NaN).k15, NaN), sixteen(sym).k7 === sym, Object.keys(seventeen(1)).length, dump(sixteen(obj)).length, seventeen(obj).last === obj);
// Values are the caller's, not copies; -0 and NaN keep their bits.
var same = five(obj, fn, sym, obj, kinds);
log.push(same.$$typeof === obj, same.type === fn, same.key === sym, same.ref === same.$$typeof, same.props === kinds, Object.is(three(-0, 0, -0).x, -0), Object.is(three(-0, 0, -0).y, 0), 1 / numeric(0).z, numeric(3).y, dump(numeric(-2)), dump(zeros()));
// Evaluation order, a value that throws between stores, and reentry of the same site.
var order = ""; log.push(dump(ordered(function (name) { order += name; return name.toUpperCase(); })), order);
try { thrower("a", function () { throw new RangeError("stop"); }); } catch (e) { log.push(e.constructor.name); }
log.push(dump(thrower("a", function () { return obj; })), dump(thrower(1, function () { return thrower(2, function () { return 3; }); }).b));
var deep = recur(5), depths = ""; for (var c = deep; c; c = c.child) depths += c.depth + c.tag[0]; log.push(depths);
var chain = reenter(3, obj), walk = ""; for (var r = chain; r !== obj; r = r.inner) walk += r.before + ":" + r.after + (r.same === obj ? "s" : "x") + " "; log.push(walk);
// A literal whose values suspend.
var it = gen("g"); log.push(it.next().value, it.next("two").value); var made = it.next(obj); log.push(made.done, dump(made.value), made.value.third === obj);
var other = gen(0); other.next(); other.next(1.5); log.push(dump(other.next(-0).value));
// Attributes, order and later structural changes of objects born here.
var o = five(1, "two", obj, null, undefined);
log.push(JSON.stringify(Object.getOwnPropertyDescriptor(o, "type")), JSON.stringify(Object.getOwnPropertyDescriptor(o, "$$typeof")), Object.keys(o).join(), JSON.stringify(o), Object.isExtensible(o), Object.getPrototypeOf(o) === Object.prototype);
o.key = 5; o.extra = "x"; delete o.type; o.type = "again"; log.push(dump(o));
Object.defineProperty(o, "ref", { get: function () { return "getter"; }, enumerable: true, configurable: true }); log.push(dump(o), dump({ ...o }));
var names = ""; for (var name in three(1, 2, 3)) names += name; log.push(names, dump(Object.assign(three(1, "b", obj), { y: sym, q: 1 })));
var frozen = Object.freeze(three("a", 2, obj)); try { (function () { "use strict"; frozen.x = "changed"; })(); } catch (e) { log.push(e.constructor.name); } log.push(dump(frozen), Object.isFrozen(frozen), dump(three("still", "fresh", 3)));
// Many objects of one shape keep their own values.
var rows = []; for (var i = 0; i < 40; i++) rows.push(five(sym, i % 2 ? "li" : fn, i % 3 ? "k" + i : null, null, { children: i, id: i * 1.5 }));
var total = 0, kindsSeen = ""; for (var j = 0; j < rows.length; j++) { total += rows[j].props.id; kindsSeen += text(rows[j].type)[0] + (rows[j].key === null ? "n" : rows[j].key.length); }
log.push(total, kindsSeen, rows[39].props.children, rows[0].props === rows[1].props);
print(log.join(";"));
"##,
        r##"{x='s' y=10n z=NaN};{x=NaN y='s' z='s'};{$$typeof='s' type=10n key=NaN ref='s' props=10n};{$$typeof=1 type='s' key=2 ref=10n props=3};{first='s' second=10n};{p='s' q='s' r='s'};{m='s' n=1 o='s'};{x=obj y=fn z=1.5};{x=1.5 y=obj z=obj};{$$typeof=obj type=fn key=1.5 ref=obj props=fn};{$$typeof=1 type=obj key=2 ref=fn props=3};{first=obj second=fn};{p=obj q=obj r=obj};{m=obj n=1 o=obj};{x=sym y=[2] z=-7};{x=-7 y=sym z=sym};{$$typeof=sym type=[2] key=-7 ref=sym props=[2]};{$$typeof=1 type=sym key=2 ref=[2] props=3};{first=sym second=[2]};{p=sym q=sym r=sym};{m=sym n=1 o=sym};{x=null y=1 z=9007199254740992};{x=9007199254740992 y=null z=null};{$$typeof=null type=1 key=9007199254740992 ref=null props=1};{$$typeof=1 type=null key=2 ref=1 props=3};{first=null second=1};{p=null q=null r=null};{m=null n=1 o=null};{x=undefined y=0 z=Infinity};{x=Infinity y=undefined z=undefined};{$$typeof=undefined type=0 key=Infinity ref=undefined props=0};{$$typeof=1 type=undefined key=2 ref=0 props=3};{first=undefined second=0};{p=undefined q=undefined r=undefined};{m=undefined n=1 o=undefined};{x=true y=-0 z=''};{x='' y=true z=true};{$$typeof=true type=-0 key='' ref=true props=-0};{$$typeof=1 type=true key=2 ref=-0 props=3};{first=true second=-0};{p=true q=true r=true};{m=true n=1 o=true};{x=false y=NaN z='s'};{x='s' y=false z=false};{$$typeof=false type=NaN key='s' ref=false props=NaN};{$$typeof=1 type=false key=2 ref=NaN props=3};{first=false second=NaN};{p=false q=false r=false};{m=false n=1 o=false};{x=10n y=1.5 z=obj};{x=obj y=10n z=10n};{$$typeof=10n type=1.5 key=obj ref=10n props=1.5};{$$typeof=1 type=10n key=2 ref=1.5 props=3};{first=10n second=1.5};{p=10n q=10n r=10n};{m=10n n=1 o=10n};{x=fn y=-7 z=sym};{x=sym y=fn z=fn};{$$typeof=fn type=-7 key=sym ref=fn props=-7};{$$typeof=1 type=fn key=2 ref=-7 props=3};{first=fn second=-7};{p=fn q=fn r=fn};{m=fn n=1 o=fn};{x=[2] y=9007199254740992 z=null};{x=null y=[2] z=[2]};{$$typeof=[2] type=9007199254740992 key=null ref=[2] props=9007199254740992};{$$typeof=1 type=[2] key=2 ref=9007199254740992 props=3};{first=[2] second=9007199254740992};{p=[2] q=[2] r=[2]};{m=[2] n=1 o=[2]};{x=1 y=Infinity z=undefined};{x=undefined y=1 z=1};{$$typeof=1 type=Infinity key=undefined ref=1 props=Infinity};{$$typeof=1 type=1 key=2 ref=Infinity props=3};{first=1 second=Infinity};{p=1 q=1 r=1};{m=1 n=1 o=1};{x=0 y='' z=true};{x=true y=0 z=0};{$$typeof=0 type='' key=true ref=0 props=''};{$$typeof=1 type=0 key=2 ref='' props=3};{first=0 second=''};{p=0 q=0 r=0};{m=0 n=1 o=0};{x=-0 y='s' z=false};{x=false y=-0 z=-0};{$$typeof=-0 type='s' key=false ref=-0 props='s'};{$$typeof=1 type=-0 key=2 ref='s' props=3};{first=-0 second='s'};{p=-0 q=-0 r=-0};{m=-0 n=1 o=-0};{x=NaN y=obj z=10n};{x=10n y=NaN z=NaN};{$$typeof=NaN type=obj key=10n ref=NaN props=obj};{$$typeof=1 type=NaN key=2 ref=obj props=3};{first=NaN second=obj};{p=NaN q=NaN r=NaN};{m=NaN n=1 o=NaN};{x=1.5 y=sym z=fn};{x=fn y=1.5 z=1.5};{$$typeof=1.5 type=sym key=fn ref=1.5 props=sym};{$$typeof=1 type=1.5 key=2 ref=sym props=3};{first=1.5 second=sym};{p=1.5 q=1.5 r=1.5};{m=1.5 n=1 o=1.5};{x=-7 y=null z=[2]};{x=[2] y=-7 z=-7};{$$typeof=-7 type=null key=[2] ref=-7 props=null};{$$typeof=1 type=-7 key=2 ref=null props=3};{first=-7 second=null};{p=-7 q=-7 r=-7};{m=-7 n=1 o=-7};{x=9007199254740992 y=undefined z=1};{x=1 y=9007199254740992 z=9007199254740992};{$$typeof=9007199254740992 type=undefined key=1 ref=9007199254740992 props=undefined};{$$typeof=1 type=9007199254740992 key=2 ref=undefined props=3};{first=9007199254740992 second=undefined};{p=9007199254740992 q=9007199254740992 r=9007199254740992};{m=9007199254740992 n=1 o=9007199254740992};{x=Infinity y=true z=0};{x=0 y=Infinity z=Infinity};{$$typeof=Infinity type=true key=0 ref=Infinity props=true};{$$typeof=1 type=Infinity key=2 ref=true props=3};{first=Infinity second=true};{p=Infinity q=Infinity r=Infinity};{m=Infinity n=1 o=Infinity};{x='' y=false z=-0};{x=-0 y='' z=''};{$$typeof='' type=false key=-0 ref='' props=false};{$$typeof=1 type='' key=2 ref=false props=3};{first='' second=false};{p='' q='' r=''};{m='' n=1 o=''};{k0=-0 k1=-0 k2=-0 k3=-0 k4=-0 k5=-0 k6=-0 k7=-0 k8=-0 k9=-0 k10=-0 k11=-0 k12=-0 k13=-0 k14=-0 k15=-0};true;true;17;119;true;true;true;true;true;true;true;true;-Infinity;1.5;{x=-2 y=-1 z=2 w=0};{a=0 b=false c=null d=undefined e=''};{a='A' b='B' c='C' d='D'};abcd;RangeError;{a='a' b=obj c='a' d='a'};{a=2 b=3 c=2 d=2};5n4n3n2n1n0l;3:-3s 2:-2s 1:-1s 0:0s ;1;2;true;{first='g' second='two' third=obj fourth='g'};true;{first=0 second=1.5 third=-0 fourth=0};{"value":"two","writable":true,"enumerable":true,"configurable":true};{"value":1,"writable":true,"enumerable":true,"configurable":true};$$typeof,type,key,ref,props;{"$$typeof":1,"type":"two","key":{"o":1},"ref":null};true;true;{$$typeof=1 key=5 ref=null props=undefined extra='x' type='again'};{$$typeof=1 key=5 ref='getter' props=undefined extra='x' type='again'};{$$typeof=1 key=5 ref='getter' props=undefined extra='x' type='again'};xyz;{x=1 y=sym z=obj q=1};TypeError;{x='a' y=2 z=obj};true;{x='still' y='fresh' z=3};1170;fn'2f2'nf2'2fn'2f2'nf3'3fn'3f3'nf3'3fn'3f3'nf3'3fn'3f3'nf3'3fn'3f3'nf3'3fn'3f3'n;39;false"##,
    );
}

#[test]
fn a_prototype_of_any_object_layout_becomes_the_typed_edge() {
    // Object.create, `__proto__` in a literal, Object.setPrototypeOf, a
    // constructor's `prototype`, Reflect.construct of Array and `instanceof`
    // take a prototype of every object layout (ordinary, Array, function,
    // bound function, class, Proxy, typed array, collections, arguments,
    // wrappers, namespaces and the intrinsic prototypes), null, and
    // primitives after their sites were warmed with ordinary objects.
    run_as_script_function_and_module(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
function create(p) { return Object.create(p); }
function literal(p) { return { __proto__: p, a: 1, b: 2, c: 3 }; }
function small(p) { return { __proto__: p, only: 1 }; }
function retarget(p) { return Object.setPrototypeOf({ kept: 1 }, p); }
function Maker() { this.made = 1; }
function construct(p) { Maker.prototype = p; return new Maker(); }
function subclassed(p) { function Target() {} Target.prototype = p; return Reflect.construct(Array, [3], Target); }
function test(o, F, p) { F.prototype = p; return o instanceof F; }
function Probe() {}
var args = (function () { return arguments; })(1, 2);
var protos = [
  ["ordinary", { tag: "o" }], ["compact", { a: 1, b: 2, c: 3 }], ["array", [1, 2]], ["function", function named() {}], ["arrow", function () { return () => 1; }()],
  ["bound", function () {}.bind(null)], ["class", class K {}], ["proxy", new Proxy({ tag: "p" }, {})], ["typed", new Uint8Array(2)], ["map", new Map()],
  ["set", new Set()], ["date", new Date(0)], ["regexp", /x/], ["arguments", args], ["nullProto", Object.create(null)], ["string", new String("s")],
  ["number", new Number(1)], ["symbol", Object(Symbol.iterator)], ["promise", Promise.resolve()], ["generator", function* () {}()], ["error", new Error("e")],
  ["buffer", new ArrayBuffer(1)], ["weakmap", new WeakMap()], ["math", Math], ["json", JSON], ["objectPrototype", Object.prototype], ["arrayPrototype", Array.prototype], ["functionPrototype", Function.prototype],
];
// Warm every site with ordinary prototypes first.
for (var w = 0; w < 3; w++) { var q = { warm: w }; create(q); literal(q); small(q); retarget(q); construct(q); subclassed(q); test(create(q), Probe, q); }
for (var i = 0; i < protos.length; i++) {
  var name = protos[i][0], p = protos[i][1];
  var made = [create(p), literal(p), small(p), retarget(p), construct(p), subclassed(p)], line = name + ":";
  for (var j = 0; j < made.length; j++) line += (Object.getPrototypeOf(made[j]) === p ? "1" : "0") + (test(made[j], Probe, p) ? "i" : "n");
  line += Object.keys(made[1]).join("") + made[2].only + made[3].kept + made[4].made + made[5].length + (Array.isArray(made[5]) ? "A" : "-");
  line += test(p, Probe, p) ? "s" : "d";
  log.push(line);
}
// null and the values that are not objects.
log.push(Object.getPrototypeOf(create(null)) === null, Object.getPrototypeOf(literal(null)) === null, Object.getPrototypeOf(small(null)) === null, Object.getPrototypeOf(retarget(null)) === null, Object.getPrototypeOf(construct(null)) === Object.prototype, Object.getPrototypeOf(subclassed(null)) === Array.prototype);
var primitives = [1, "s", true, undefined, Symbol(), 1n];
for (var k = 0; k < primitives.length; k++) {
  var v = primitives[k];
  log.push(show(function () { return typeof create(v); }) + "/" + (Object.getPrototypeOf(literal(v)) === Object.prototype) + "/" + show(function () { return typeof retarget(v); }) + "/" + (Object.getPrototypeOf(construct(v)) === Object.prototype) + "/" + show(function () { return test({}, Probe, v); }));
}
// Inherited reads and writes go through the prototype given at birth.
var base = [10, 20], viaArray = create(base); log.push(viaArray[1], viaArray.length, viaArray.join === Array.prototype.join, Array.isArray(viaArray));
var viaFunction = literal(function f(a, b) {}); log.push(viaFunction.length, viaFunction.name, typeof viaFunction.call, typeof viaFunction);
var seen = "", viaProxy = small(new Proxy({ deep: "d" }, { get: function (t, key, r) { seen += String(key) + (r === viaProxy ? "!" : "?") + " "; return Reflect.get(t, key, r); }, has: function (t, key) { seen += "has:" + String(key) + " "; return key in t; } }));
log.push(viaProxy.deep, viaProxy.only, "deep" in viaProxy, "only" in viaProxy, seen);
var viaTyped = create(new Uint8Array([7, 8])); log.push(viaTyped[1], viaTyped[5], show(function () { return viaTyped.length; }));
print(log.join(";"));
"##,
        r##"ordinary:1i1i1i1i1i1iabc1113Ad;compact:1i1i1i1i1i1iabc1113Ad;array:1i1i1i1i1i1iabc1113Ad;function:1i1i1i1i1i1iabc1113Ad;arrow:1i1i1i1i1i1iabc1113Ad;bound:1i1i1i1i1i1iabc1113Ad;class:1i1i1i1i1i1iabc1113Ad;proxy:1i1i1i1i1i1iabc1113Ad;typed:1i1i1i1i1i1iabc1113Ad;map:1i1i1i1i1i1iabc1113Ad;set:1i1i1i1i1i1iabc1113Ad;date:1i1i1i1i1i1iabc1113Ad;regexp:1i1i1i1i1i1iabc1113Ad;arguments:1i1i1i1i1i1iabc1113Ad;nullProto:1i1i1i1i1i1iabc1113Ad;string:1i1i1i1i1i1iabc1113Ad;number:1i1i1i1i1i1iabc1113Ad;symbol:1i1i1i1i1i1iabc1113Ad;promise:1i1i1i1i1i1iabc1113Ad;generator:1i1i1i1i1i1iabc1113Ad;error:1i1i1i1i1i1iabc1113Ad;buffer:1i1i1i1i1i1iabc1113Ad;weakmap:1i1i1i1i1i1iabc1113Ad;math:1i1i1i1i1i1iabc1113Ad;json:1i1i1i1i1i1iabc1113Ad;objectPrototype:1i1i1i1i1i1iabc1113Ad;arrayPrototype:1i1i1i1i1i1iabc1113Ad;functionPrototype:1i1i1i1i1i1iabc1113Ad;true;true;true;true;true;true;TypeError/true/TypeError/true/TypeError;TypeError/true/TypeError/true/TypeError;TypeError/true/TypeError/true/TypeError;TypeError/true/TypeError/true/TypeError;TypeError/true/TypeError/true/TypeError;TypeError/true/TypeError/true/TypeError;20;2;true;false;2;f;function;object;d;1;true;true;deep! has:deep ;8;;TypeError"##,
    );
}
