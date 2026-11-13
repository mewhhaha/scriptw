//! Reads that no site record answers walk the receiver's live chain, and
//! computed-key reads and writes run without converting a String, Symbol or
//! index key first. These scripts change, between two executions of the same
//! site, each fact such a walk or fast path relies on. Expected lines are
//! Node's.
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
        "{:?}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn a_static_read_walk_follows_every_change_on_the_live_chain() {
    // One site sees own data, compact, dictionary, function, Array and Map
    // receivers that lack the key, then data, an accessor and a Proxy on
    // their chains.
    run(
        r#"
var log = [];
function rd(o) { return o.zz; }
function Comp() {}
var shaped = { zz: 1, b: 2 };
log.push(rd(shaped), rd({ a: 1 }), rd({ c: 1, d: 2 }), rd({ e: 1, f: 2, g: 3, h: 4 }));
log.push(rd(Comp), rd(function () {}), rd([1, 2]), rd(new Map()), rd(new WeakMap()));
var dict = { a: 1, zz: 5 }; delete dict.a;
log.push(rd(dict), rd(shaped));
var lacking = { a: 1 }; delete lacking.a;
log.push(rd(lacking));
Object.prototype.zz = "proto";
log.push(rd({ a: 1 }), rd(Comp), rd([1]), rd(lacking), rd(shaped));
Object.defineProperty(Object.prototype, "zz", {
  get: function () { return "getter:" + (this === lacking); },
  configurable: true
});
log.push(rd(lacking), rd({ a: 1 }));
delete Object.prototype.zz;
log.push(rd(lacking), rd({ a: 1 }), rd(Comp));
Function.prototype.zz = "fn";
log.push(rd(Comp), rd({ a: 1 }));
delete Function.prototype.zz;
Comp.zz = "own";
log.push(rd(Comp), rd(function () {}));
var protoHolder = { zz: "holder" };
var child = Object.create(protoHolder);
log.push(rd(child), rd(Object.create(child)));
protoHolder.zz = "changed";
log.push(rd(child));
delete protoHolder.zz;
log.push(rd(child), rd(Object.create(null)));
var proxied = Object.create(new Proxy({}, { get: function (t, k, r) { return "trap:" + String(k) + ":" + (r === proxied); } }));
log.push(rd(proxied), rd({ a: 2 }));
Array.prototype.zz = "arr";
log.push(rd([3]), rd({ b: 1 }));
delete Array.prototype.zz;
log.push(rd([3]));
var m = new Map();
m.zz = "map own";
log.push(rd(m), rd(new Map()));
print(log.join());
"#,
        "1,,,,,,,,,5,1,,proto,proto,proto,proto,1,getter:true,getter:false,,,,fn,,own,,holder,holder,changed,,,trap:zz:true,,arr,,,map own,",
    );
}

#[test]
fn primitive_receivers_read_their_realm_prototypes_without_wrappers() {
    // Number, String, Boolean, Symbol and BigInt receivers read through their
    // prototypes, which gain and lose data properties and getters.
    run(
        r#"
"use strict";
var log = [];
function rd(o) { return o.zz; }
function fix(o) { return o.toFixed; }
function hex(n) { return n.toString(16); }
for (var i = 250; i < 253; i++) log.push(hex(i));
log.push(rd(1), rd("s"), rd(true), rd(Symbol("q")), rd(10n), fix(1) === Number.prototype.toFixed);
Number.prototype.zz = "num";
String.prototype.zz = "str";
log.push(rd(1), rd("s"), rd(true), rd(2.5));
Object.defineProperty(String.prototype, "zz", { get: function () { return typeof this; }, configurable: true });
Object.defineProperty(Boolean.prototype, "zz", { get: function () { return typeof this; }, configurable: true });
log.push(rd("s"), rd(false), rd(new String("w")));
delete String.prototype.zz;
delete Boolean.prototype.zz;
delete Number.prototype.zz;
log.push(rd(1), rd("s"), rd(false));
Object.prototype.zz = "obj";
log.push(rd(1), rd("s"), rd(Symbol()), rd(1n));
delete Object.prototype.zz;
function len(s) { return s.length; }
function first(s) { return s["0"]; }
log.push(len("abc"), first("abc"), first(5));
Number.prototype.toString = function () { return "patched"; };
log.push(hex(255));
print(log.join());
"#,
        "fa,fb,fc,,,,,,true,num,str,,num,string,boolean,object,,,,obj,obj,obj,obj,3,a,,patched",
    );
}

#[test]
fn computed_reads_answer_from_the_live_chain() {
    // Equal Strings of different identity, dictionaries, inherited data and
    // accessors, Array indices and "length", Symbols, Proxies and key
    // conversion reach one computed-key read site.
    run(
        r#"
var log = [];
function get(o, k) { return o[k]; }
var key = "ab";
var built = "a" + "b".repeat(1);
var lit = { ab: 1, c: 2 };
log.push(get(lit, key), get(lit, built), get(lit, "c"), get(lit, "zz"));
var wide = { a: 1, b: 2, c: 3, d: 4, e: 5 };
log.push(get(wide, "d"), get(wide, "q"), get(wide, built));
var dict = { a: 1, ab: 7 }; delete dict.a;
log.push(get(dict, key), get(dict, built), get(dict, "a"));
function P() {} P.prototype.m = "inherited";
var made = new P();
log.push(get(made, "m"), get(made, "toString") === Object.prototype.toString);
Object.defineProperty(P.prototype, "acc", { get: function () { return this === made; } });
log.push(get(made, "acc"));
log.push(get(made, "__proto__") === P.prototype, get({ __proto__: null }, "__proto__"));
var arr = [10, 20, 30];
log.push(get(arr, "1"), get(arr, "length"), get(arr, 2), get(arr, "push") === Array.prototype.push, get(arr, "missing"));
arr.extra = "x";
log.push(get(arr, "extra"), get("str", "length"), get("str", "1"), get("str", 1));
var sym = Symbol("s");
var holder = { a: 1 };
log.push(get(holder, Symbol.iterator), get(arr, Symbol.iterator) === Array.prototype[Symbol.iterator], get(holder, sym));
holder[sym] = "own symbol";
log.push(get(holder, sym), get({ a: 1 }, sym));
Object.prototype[Symbol.iterator] = "proto iterator";
log.push(get(holder, Symbol.iterator), get({ b: 1 }, Symbol.iterator), get(arr, Symbol.iterator) === Array.prototype[Symbol.iterator]);
delete Object.prototype[Symbol.iterator];
log.push(get(holder, Symbol.iterator));
var px = new Proxy({ ab: "target" }, { get: function (t, k, r) { return "trap " + String(k); } });
log.push(get(px, key), get(Object.create(px), "ab"), get(Object.create(px), sym));
log.push(get(function f() {}, "name"), get(Math, "PI") === Math.PI, get(JSON, "zz"));
var counted = { toString: function () { log.push("toString"); return "ab"; } };
log.push(get(lit, counted));
print(log.join());
"#,
        "1,1,2,,4,,,7,7,,inherited,true,true,true,,20,3,30,true,,x,3,t,t,,true,,own symbol,,proto iterator,proto iterator,true,,trap ab,trap ab,trap Symbol(s),f,true,,toString,1",
    );
}

#[test]
fn computed_writes_add_only_where_no_prototype_intervenes() {
    // Adds by transition, setters and read-only properties on the chain,
    // "__proto__", frozen, sealed and non-extensible objects, dictionaries,
    // Proxies, Symbols, primitive bases and the order of key conversion.
    run(
        r#"
var log = [];
function put(o, k, v) { o[k] = v; return o[k]; }
function copy(src) { var out = {}; for (var k in src) out[k] = src[k]; return out; }
var a = copy({ x: 1, y: 2 }), b = copy({ x: 3, y: 4 }), c = copy({ y: 5, x: 6 });
log.push(Object.keys(a).join(""), Object.keys(c).join(""), a.y + b.y + c.y);
var built = "x" + "y".repeat(0);
log.push(put(a, built, 9), a.x, Object.keys(a).join(""));
log.push(put({}, "2", "two"), Object.keys(copy({ b: 1, 2: 2, a: 3 })).join(""));
var withSetter = Object.create({ set s(v) { log.push("setter " + v + " " + (this === withSetter)); } });
put(withSetter, "s", 1);
log.push(Object.keys(withSetter).length);
var readOnly = Object.create(Object.defineProperty({}, "r", { value: "fixed", writable: false }));
log.push(put(readOnly, "r", 2), Object.keys(readOnly).length);
var target = {};
put(target, "__proto__", { inherited: "yes" });
log.push(target.inherited, Object.keys(target).length);
log.push(put(Object.freeze({ f: 1 }), "f", 2), put(Object.seal({ f: 1 }), "f", 3), put(Object.seal({ f: 1 }), "g", 4));
log.push(put(Object.preventExtensions({}), "n", 5), put(Object.defineProperty({}, "w", { value: 1, writable: false }), "w", 6));
var d = { a: 1, b: 2 }; delete d.a;
log.push(put(d, "b", 7), put(d, "c", 8), Object.keys(d).join(""));
Object.prototype.inh = "proto data";
log.push(put({}, "inh", "own"), ({}).inh);
Object.defineProperty(Object.prototype, "inh", { set: function (v) { log.push("proto setter " + v); }, configurable: true });
put({ q: 1 }, "inh", "via setter");
delete Object.prototype.inh;
log.push(put({ q: 1 }, "inh", "after delete"));
var px = new Proxy({}, { set: function (t, k, v, r) { log.push("trap " + k + "=" + v); t[k] = v; return true; } });
put(px, "p", 1);
put(Object.create(px), "p2", 2);
var sym = Symbol("w");
log.push(put({}, sym, "sym value"), put("str", "x", 1), put(5, "y", 2));
var order = [];
var key = { toString: function () { order.push("key"); return "late"; } };
var obj = {};
obj[key] = (order.push("rhs"), "v");
log.push(order.join(">"), obj.late);
try { null[(log.push("key evaluated"), "k")] = (log.push("rhs evaluated"), 1); } catch (e) { log.push(e.constructor.name); }
print(log.join());
"#,
        "xy,yx,11,9,9,xy,two,2ba,setter 1 true,0,fixed,0,yes,0,1,3,,,1,7,8,bc,own,proto data,proto setter via setter,after delete,trap p=1,trap p2=2,sym value,,,rhs>key,v,key evaluated,rhs evaluated,TypeError",
    );
}

#[test]
fn computed_writes_fail_loudly_in_strict_code() {
    // Each refused computed-key Set throws in strict code.
    run(
        r#"
"use strict";
var log = [];
function put(o, k, v) { o[k] = v; return o[k]; }
function attempt(f) { try { return f(); } catch (e) { return e.constructor.name; } }
log.push(attempt(function () { return put(Object.freeze({ f: 1 }), "f", 2); }));
log.push(attempt(function () { return put(Object.preventExtensions({}), "g", 2); }));
log.push(attempt(function () { return put(Object.create(Object.freeze({ r: 1 })), "r", 2); }));
log.push(attempt(function () { return put("str", "x", 1); }));
log.push(attempt(function () { return put(undefined, "x", 1); }));
log.push(attempt(function () { return put({ get g() { return 1; } }, "g", 2); }));
log.push(attempt(function () { return put([], 0, "ok"); }));
log.push(attempt(function () { return put(Object.freeze([1]), 0, 2); }));
log.push(attempt(function () { return put(Object.preventExtensions([1]), 1, 2); }));
var fixedLength = [1, 2];
Object.defineProperty(fixedLength, "length", { writable: false });
log.push(attempt(function () { return put(fixedLength, 2, 3); }), attempt(function () { return put(fixedLength, 1, 9); }), fixedLength.join());
print(log.join());
"#,
        "TypeError,TypeError,TypeError,TypeError,TypeError,TypeError,ok,TypeError,TypeError,TypeError,9,1,9",
    );
}

#[test]
fn array_index_writes_respect_length_holes_and_prototypes() {
    // Appends, holes, the largest index, non-index Numbers, frozen, sealed
    // and non-extensible Arrays, a read-only length, indexed data and setters
    // on both prototypes, subclasses, sparse Arrays, typed arrays, arguments
    // objects and Proxies.
    run(
        r#"
var log = [];
function put(o, k, v) { o[k] = v; return o.length; }
function build(n) { var c = []; for (var f = 0; f < n; f++) c[f] = f * 2; return c; }
var x = build(5);
log.push(x.join(), x.length, build(0).length);
var h = [];
log.push(put(h, 3, "three"), 1 in h, h[3], Object.keys(h).join());
log.push(put(h, 1, "one"), Object.keys(h).join(), put(h, -0, "zero"), h[0]);
log.push(put(h, 4294967294, "max"), h.length, put(h, 4294967295, "not index"), h.length, h[4294967295]);
log.push(put(h, 1.5, "frac"), h["1.5"], put(h, NaN, "nan"), h.NaN, put(h, -1, "neg"), h["-1"]);
var arr = ["a", "b"];
log.push(put(arr, 0, 1), put(arr, 2, { o: 1 }), arr.length, typeof arr[2]);
var frozen = Object.freeze([1, 2]);
log.push(put(frozen, 0, 9), put(frozen, 2, 9), frozen.join());
var sealed = Object.seal([1, 2]);
log.push(put(sealed, 0, 9), put(sealed, 2, 9), sealed.join());
var closed = Object.preventExtensions([1]);
log.push(put(closed, 1, 2), closed.join());
var fixedLength = [1, 2];
Object.defineProperty(fixedLength, "length", { writable: false });
log.push(put(fixedLength, 2, 3), put(fixedLength, 0, 7), fixedLength.join());
Array.prototype[1] = "proto element";
var p = [];
log.push(put(p, 0, "z"), p[1], put(p, 1, "own"), Array.prototype[1], Object.keys(p).join());
delete Array.prototype[1];
Object.defineProperty(Array.prototype, 2, { set: function (v) { log.push("array setter " + v); }, configurable: true });
var q = [0, 1];
log.push(put(q, 2, "via setter"), q.length, 2 in q);
delete Array.prototype[2];
log.push(put(q, 2, "after"), q[2]);
Object.prototype[0] = "object proto";
var r = [];
log.push(put(r, 0, "own0"), r[0], Object.prototype[0]);
Object.defineProperty(Object.prototype, 0, { set: function (v) { log.push("object setter " + v); }, configurable: true });
var s = [];
log.push(put(s, 0, "via object setter"), 0 in s);
delete Object.prototype[0];
class Sub extends Array {}
var sub = new Sub();
log.push(put(sub, 0, 1), put(sub, 5, 2), sub.length);
var sparse = [];
sparse[1000000] = 1;
log.push(put(sparse, 5, 5), sparse.length, sparse[5]);
var ta = new Uint8Array(2);
log.push(put(ta, 0, 300), ta[0], put(ta, 5, 1), ta[5]);
(function () { log.push(put(arguments, 0, "arg"), arguments[0], put(arguments, 3, "more"), arguments.length); })(1, 2);
var proxyArr = new Proxy([], { set: function (t, k, v) { log.push("ptrap " + k); t[k] = v; return true; } });
proxyArr[0] = 1;
var protoArr = Object.create(Array.prototype);
log.push(put(protoArr, 0, 1), protoArr.length);
var withProtoArray = [];
Object.setPrototypeOf(withProtoArray, [9, 8, 7]);
log.push(put(withProtoArray, 1, "own"), withProtoArray.length, withProtoArray[2]);
var holes = [, , 3];
log.push(put(holes, 0, "filled"), holes.join("|"));
print(log.join());
"#,
        "0,2,4,6,8,5,0,4,false,three,3,4,1,3,4,zero,4294967295,4294967295,4294967295,4294967295,not index,4294967295,frac,4294967295,nan,4294967295,neg,2,3,3,object,2,2,1,2,2,2,9,2,1,1,2,2,7,2,1,proto element,2,proto element,0,1,array setter via setter,2,2,true,3,after,1,own0,object proto,object setter via object setter,0,true,1,6,6,1000001,1000001,5,2,44,2,,2,arg,2,2,ptrap 0,3,3,2,2,7,3,filled||3",
    );
}

#[test]
fn arguments_objects_answer_named_properties_from_their_header() {
    // "length", "callee" and absent keys of mapped and unmapped arguments
    // objects, after writes, deletion and a getter, while index keys keep
    // their exotic lookup.
    run(
        r#"
var log = [];
function len() { return arguments.length; }
function lenAfter(f) { f(arguments); return arguments.length; }
function zz() { return arguments.zz; }
function key(k) { return arguments[k]; }
function mapped(a, k) { a = "changed"; return arguments[k]; }
function strictCallee() { "use strict"; try { return arguments.callee; } catch (e) { return e.constructor.name; } }
log.push(len(1, 2, 3), len(), len(undefined));
log.push(lenAfter(function (a) { a.length = 7; }), lenAfter(function (a) { delete a.length; }));
log.push(lenAfter(function (a) { Object.defineProperty(a, "length", { get: function () { return "getter"; } }); }));
log.push(zz(1), strictCallee(), key("length", 2), key("0"), key("1", "x"), mapped("a", "0"), mapped("a", "length"));
Object.prototype.zz = "proto";
Object.prototype.length = "proto length";
log.push(zz(1), lenAfter(function (a) { delete a.length; }), key("zz"));
delete Object.prototype.zz;
delete Object.prototype.length;
log.push(zz(), key(Symbol.iterator) === Array.prototype.values);
print(log.join());
"#,
        "3,0,1,7,,getter,,TypeError,2,0,x,changed,2,proto,proto length,proto,,true",
    );
}
