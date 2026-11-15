//! JSON.stringify without a replacer function performs each member's Get in
//! its container walk: a plain data object's keys and values come from its
//! compact storage while that storage is still the object's, primitive
//! members are appended in place (strings through the quoting helper,
//! integral Numbers as digits), `toJSON` is read by the call-free chain walk
//! when it can answer, and everything else takes the general algorithm. The
//! script serializes plain values and then every case those paths must leave
//! alone or re-check: escapes and lone surrogates, Number spellings, skipped
//! members, index, Symbol, hidden and inherited keys, dictionary objects,
//! accessors, `toJSON` in every place it can live, BigInt, wrappers, a holder
//! mutated by an earlier member, Arrays that change length, cycles, Proxies,
//! replacer functions and lists, raw JSON and gaps.
//! Expectations are Node 24's for the same script.
use scriptw_engine::{CompileOptions, Engine, ExecutionBackend, RealmBuilder, RunOptions};

/// Runs `source`, which leaves its report in `result`, and compares the
/// report line by line with Node 24's for the same source.
fn assert_node_report(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let mut literal = String::new();
    for character in expected.chars() {
        assert!(character == '\n' || (' '..='~').contains(&character));
        match character {
            '\\' => literal.push_str("\\\\"),
            '"' => literal.push_str("\\\""),
            '\n' => literal.push_str("\\n"),
            other => literal.push(other),
        }
    }
    let script = format!(
        "{source}\nvar expected = \"{literal}\".split(\"\\n\"), actual = result.split(\"\\n\"), wrong = [];\n\
         for (var line = 0; line < Math.max(expected.length, actual.length); line++)\n\
           if (expected[line] !== actual[line]) wrong.push(\"got \" + actual[line] + \" for \" + expected[line]);\n\
         if (wrong.length) throw new Error(wrong.join(\"; \"));\ntrue;\n"
    );
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
            &script,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn json_stringify_matches_node_for_plain_values_and_every_excluded_case() {
    assert_node_report(
        r###"
var log = [];
function esc(s) { return String(s).replace(/[^\x20-\x7e]/g, function (c) { return "\\u" + ("0000" + c.charCodeAt(0).toString(16)).slice(-4); }); }
function show(label, make) {
  var text;
  try { text = make(); text = text === undefined ? "undefined" : esc(text); } catch (e) { text = e.constructor.name; }
  log.push(label + "=" + text);
}
var nested = { a: 1, b: "x", c: [1, "two", null, true, false, { d: -0, e: 1.5, f: 1e21, g: NaN, h: Infinity, i: 2147483648, j: -5, k: 123456789 }], m: {} , n: [] };
show("nested", function () { return JSON.stringify(nested); });
show("nested2", function () { return JSON.stringify(nested, null, 2); });
show("nestedTab", function () { return JSON.stringify(nested, null, "\t"); });
show("nested20", function () { return JSON.stringify({ a: [1, { b: 2 }] }, null, 20); });
show("numbers", function () { return JSON.stringify([0, -0, 1, 9, 10, 99, 100, 2147483647, 2147483648, 4294967295, -1, -2147483648, 0.1, 1e-7, 123456789012345680000, 1e21, -1e-7, 5e-324, 1.7976931348623157e308]); });
show("strings", function () { return JSON.stringify(["", "plain", "\"q\"", "back\\slash", "\b\f\n\r\t", "\u0000\u0001\u001f\u007f", "\ud800", "\udc00", "a\ud800", "\udc00b", "😀", "\ude00\ud83d", "  ", "￿，", "café"]); });
show("keys", function () { var o = {}; o["k\"\n"] = 1; o["\ud800"] = 2; o[""] = 3; o["café"] = 4; return JSON.stringify(o); });
var long = ""; for (var i = 0; i < 300; i++) long += String.fromCharCode(32 + (i % 90));
show("long", function () { var t = JSON.stringify({ long: long, again: [long, long] }); return t.length + ":" + t.slice(0, 40) + ":" + t.slice(-12); });
show("skips", function () { return JSON.stringify({ u: undefined, f: function () {}, s: Symbol("s"), k: 1, a: [undefined, function () {}, Symbol("t")] }); });
show("onlySkips", function () { return JSON.stringify({ u: undefined }, null, 1); });
show("rootUndefined", function () { return JSON.stringify(undefined) + "/" + JSON.stringify(function () {}) + "/" + JSON.stringify(Symbol()); });
show("indexKeys", function () { return JSON.stringify({ b: 1, 2: "x", 1: "y", a: 3 }); });
show("symbolKeys", function () { var o = { a: 1 }; o[Symbol("s")] = 2; return JSON.stringify(o); });
show("hidden", function () { var o = { a: 1, b: 2, c: 3 }; Object.defineProperty(o, "b", { enumerable: false }); return JSON.stringify(o); });
show("frozen", function () { return JSON.stringify(Object.freeze({ a: 1, b: [2] })); });
show("nullProto", function () { var o = Object.create(null); o.a = 1; o.b = "c"; return JSON.stringify(o); });
show("inherited", function () { var o = Object.create({ p: 1 }); o.own = 2; return JSON.stringify(o); });
show("many", function () { var o = {}; for (var i = 0; i < 100; i++) o["k" + i] = i; var t = JSON.stringify(o); return t.length + ":" + t.slice(0, 30) + ":" + t.slice(-20); });
show("deep", function () { var v = 1; for (var i = 0; i < 12; i++) v = i % 2 ? [v, i] : { v: v, i: i }; return JSON.stringify(v); });
show("getter", function () { var n = 0; var o = { a: 1, get b() { n++; return n; }, c: 3 }; return JSON.stringify(o) + n; });
show("ownToJSON", function () { var seen = []; var o = { a: { toJSON: function (k) { seen.push(typeof k + ":" + k); return { z: k }; } }, list: [{ toJSON: function (k) { seen.push(typeof k + ":" + k); return k; } }], toJSON: undefined }; return JSON.stringify(o) + seen.join(","); });
show("rootToJSON", function () { return JSON.stringify({ toJSON: function (k) { return "root[" + k + "]" + typeof k; } }); });
show("notCallableToJSON", function () { return JSON.stringify({ toJSON: 1, a: { toJSON: "x" } }); });
show("getterToJSON", function () { var n = 0; var o = { a: 1 }; Object.defineProperty(o, "toJSON", { get: function () { n++; return undefined; }, enumerable: false }); return JSON.stringify([o, o]) + n; });
show("protoToJSON", function () {
  var out = [];
  Object.prototype.toJSON = function (k) { return "P:" + k; };
  try { out.push(JSON.stringify({ a: 1 })); out.push(JSON.stringify([[1], { b: 2 }])); } finally { delete Object.prototype.toJSON; }
  out.push(JSON.stringify({ a: [1] }));
  Array.prototype.toJSON = function (k) { return "A:" + k + ":" + this.length; };
  try { out.push(JSON.stringify({ a: [1, 2], b: { c: [] } })); } finally { delete Array.prototype.toJSON; }
  out.push(JSON.stringify([[1]]));
  return out.join(";");
});
show("classToJSON", function () { function C(v) { this.v = v; } C.prototype.toJSON = function (k) { return k + "=" + this.v; }; return JSON.stringify({ x: new C(1), y: [new C(2)] }); });
show("date", function () { return JSON.stringify({ d: new Date(0) }); });
show("bigint", function () { return JSON.stringify({ a: 1n }); });
show("bigintToJSON", function () { BigInt.prototype.toJSON = function (k) { return k + ":" + this.toString(); }; try { return JSON.stringify({ a: 1n, b: [2n], c: Object(3n) }); } finally { delete BigInt.prototype.toJSON; } });
show("boxedBigint", function () { return JSON.stringify([Object(1n)]); });
show("boxed", function () { var calls = []; var n = new Number(3); n.valueOf = function () { calls.push("v"); return 4; }; n.toString = function () { calls.push("s"); return "5"; }; var s = new String("s"); s.toString = function () { calls.push("S"); return "t"; }; s.valueOf = function () { calls.push("V"); return "u"; }; return JSON.stringify({ n: n, s: s, b: new Boolean(false), l: [new Number(-0), new String("q\"")] }) + calls.join(""); });
show("mutateDelete", function () { var o = { a: { toJSON: function () { delete o.c; o.z = 1; return 1; } }, b: 2, c: 3, d: 4 }; return JSON.stringify(o) + Object.keys(o).join(""); });
show("mutateValue", function () { var o = { a: { toJSON: function () { o.b = 9; o.c = [o.b]; return 1; } }, b: 2, c: 3 }; return JSON.stringify(o); });
show("mutateAdd", function () { var o = { a: { toJSON: function () { o.e = 5; return 1; } }, b: 2, c: "three" }; return JSON.stringify(o) + JSON.stringify(o); });
show("mutateAccessor", function () { var o = { a: { toJSON: function () { Object.defineProperty(o, "b", { get: function () { return "g"; }, enumerable: true, configurable: true }); return 1; } }, b: 2, c: 3 }; return JSON.stringify(o); });
show("mutateHide", function () { var o = { a: { toJSON: function () { Object.defineProperty(o, "b", { enumerable: false }); return 1; } }, b: 2, c: 3 }; return JSON.stringify(o); });
show("mutateProto", function () { var o = { a: { toJSON: function () { delete o.b; Object.setPrototypeOf(o, { b: "inherited" }); return 1; } }, b: 2, c: 3 }; return JSON.stringify(o); });
show("mutateNestedGetter", function () { var o = { a: 1, b: { get g() { o.c = "changed"; delete o.d; return 0; } }, c: 3, d: 4 }; return JSON.stringify(o); });
show("arrayShrink", function () { var a = [{ toJSON: function () { a.length = 1; return 0; } }, 1, 2]; return JSON.stringify(a); });
show("arrayGrow", function () { var a = [{ toJSON: function () { a.push(9); return 0; } }, 1]; return JSON.stringify(a) + a.length; });
show("holes", function () { var a = [1, , 3]; a.extra = "x"; Array.prototype[1] = "proto"; try { return JSON.stringify(a); } finally { delete Array.prototype[1]; } });
show("cycleObject", function () { var o = { a: {} }; o.a.b = o; return JSON.stringify(o); });
show("cycleArray", function () { var a = [[]]; a[0].push(a); return JSON.stringify(a); });
show("shared", function () { var s = { x: 1 }; return JSON.stringify({ a: s, b: s, c: [s, s] }); });
show("proxyObject", function () { var order = []; var p = new Proxy({ a: 1, b: { c: 2 } }, { ownKeys: function (t) { order.push("keys"); return Reflect.ownKeys(t); }, getOwnPropertyDescriptor: function (t, k) { order.push("desc:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); }, get: function (t, k, r) { order.push("get:" + String(k)); return Reflect.get(t, k, r); } }); return JSON.stringify({ p: p }) + order.join(","); });
show("proxyArray", function () { var order = []; var p = new Proxy([1, [2]], { get: function (t, k, r) { order.push("get:" + String(k)); return Reflect.get(t, k, r); } }); return JSON.stringify(p) + order.join(","); });
show("revoked", function () { var r = Proxy.revocable({}, {}); r.revoke(); return JSON.stringify({ r: r.proxy }); });
show("replacerFn", function () { var seen = []; var root = { a: 1, b: [2, { c: 3 }], d: "s" }; var t = JSON.stringify(root, function (k, v) { seen.push((this === root ? "root" : Array.isArray(this) ? "array" : typeof this) + ":" + k); return typeof v === "number" ? v * 2 : v; }); return t + seen.join(","); });
show("replacerRootHolder", function () { var root = { a: 1 }; return JSON.stringify(root, function (k, v) { return k === "" ? (this !== root && this[""] === root && v === root ? { ok: true } : { ok: false }) : v; }); });
show("replacerList", function () { return JSON.stringify({ b: 1, a: 2, c: { a: 3, b: 4, 1: 5 }, 1: 6 }, ["a", new String("c"), 1, "a", new Number(1), {}, "missing"]); });
show("replacerListArray", function () { return JSON.stringify([{ a: 1, b: 2 }, { b: 3 }], ["b"]); });
show("collections", function () { var m = new Map([[1, 2]]); m.extra = 1; return JSON.stringify([m, new Set([1]), new Uint8Array([1, 2]), new Error("e"), /x/g, Object(Symbol("s"))]); });
show("fnWithToJSON", function () { var f = function () {}; f.toJSON = function () { return "fn"; }; return JSON.stringify({ f: f }); });
show("raw", function () { return typeof JSON.rawJSON === "function" ? JSON.stringify({ a: JSON.rawJSON("1e1000"), b: [JSON.rawJSON("\"s\"")] }) : "absent"; });
show("gapString", function () { return JSON.stringify({ a: [1, {}], b: {} , c: [] }, null, "--abcdefghijkl"); });
show("twice", function () { var o = { p: 1, q: "two", r: [3] }; var t = ""; for (var i = 0; i < 30; i++) { o.p = i; t = JSON.stringify(o); } return t; });
var result = log.join("\n");
"###,
        r###"nested={"a":1,"b":"x","c":[1,"two",null,true,false,{"d":0,"e":1.5,"f":1e+21,"g":null,"h":null,"i":2147483648,"j":-5,"k":123456789}],"m":{},"n":[]}
nested2={\u000a  "a": 1,\u000a  "b": "x",\u000a  "c": [\u000a    1,\u000a    "two",\u000a    null,\u000a    true,\u000a    false,\u000a    {\u000a      "d": 0,\u000a      "e": 1.5,\u000a      "f": 1e+21,\u000a      "g": null,\u000a      "h": null,\u000a      "i": 2147483648,\u000a      "j": -5,\u000a      "k": 123456789\u000a    }\u000a  ],\u000a  "m": {},\u000a  "n": []\u000a}
nestedTab={\u000a\u0009"a": 1,\u000a\u0009"b": "x",\u000a\u0009"c": [\u000a\u0009\u00091,\u000a\u0009\u0009"two",\u000a\u0009\u0009null,\u000a\u0009\u0009true,\u000a\u0009\u0009false,\u000a\u0009\u0009{\u000a\u0009\u0009\u0009"d": 0,\u000a\u0009\u0009\u0009"e": 1.5,\u000a\u0009\u0009\u0009"f": 1e+21,\u000a\u0009\u0009\u0009"g": null,\u000a\u0009\u0009\u0009"h": null,\u000a\u0009\u0009\u0009"i": 2147483648,\u000a\u0009\u0009\u0009"j": -5,\u000a\u0009\u0009\u0009"k": 123456789\u000a\u0009\u0009}\u000a\u0009],\u000a\u0009"m": {},\u000a\u0009"n": []\u000a}
nested20={\u000a          "a": [\u000a                    1,\u000a                    {\u000a                              "b": 2\u000a                    }\u000a          ]\u000a}
numbers=[0,0,1,9,10,99,100,2147483647,2147483648,4294967295,-1,-2147483648,0.1,1e-7,123456789012345680000,1e+21,-1e-7,5e-324,1.7976931348623157e+308]
strings=["","plain","\"q\"","back\\slash","\b\f\n\r\t","\u0000\u0001\u001f\u007f","\ud800","\udc00","a\ud800","\udc00b","\ud83d\ude00","\ude00\ud83d","\u2028\u2029","\ue000\uffff\uff0c","caf\u00e9"]
keys={"k\"\n":1,"\ud800":2,"":3,"caf\u00e9":4}
long=948:{"long":" !\"#$%&'()*+,-./0123456789:;<=:56789:;<="]}
skips={"k":1,"a":[null,null,null]}
onlySkips={}
rootUndefined=undefined/undefined/undefined
indexKeys={"1":"y","2":"x","b":1,"a":3}
symbolKeys={"a":1}
hidden={"a":1,"c":3}
frozen={"a":1,"b":[2]}
nullProto={"a":1,"b":"c"}
inherited={"own":2}
many=881:{"k0":0,"k1":1,"k2":2,"k3":3,":7,"k98":98,"k99":99}
deep=[{"v":[{"v":[{"v":[{"v":[{"v":[{"v":1,"i":0},1],"i":2},3],"i":4},5],"i":6},7],"i":8},9],"i":10},11]
getter={"a":1,"b":1,"c":3}1
ownToJSON={"a":{"z":"a"},"list":["0"]}string:a,string:0
rootToJSON="root[]string"
notCallableToJSON={"toJSON":1,"a":{"toJSON":"x"}}
getterToJSON=[{"a":1},{"a":1}]2
protoToJSON="P:";"P:";{"a":[1]};{"a":"A:a:2","b":{"c":"A:c:0"}};[[1]]
classToJSON={"x":"x=1","y":["0=2"]}
date={"d":"1970-01-01T00:00:00.000Z"}
bigint=TypeError
bigintToJSON={"a":"a:1","b":["0:2"],"c":"c:3"}
boxedBigint=TypeError
boxed={"n":4,"s":"t","b":false,"l":[0,"q\""]}vS
mutateDelete={"a":1,"b":2,"d":4}abdz
mutateValue={"a":1,"b":9,"c":[9]}
mutateAdd={"a":1,"b":2,"c":"three"}{"a":1,"b":2,"c":"three","e":5}
mutateAccessor={"a":1,"b":"g","c":3}
mutateHide={"a":1,"b":2,"c":3}
mutateProto={"a":1,"b":"inherited","c":3}
mutateNestedGetter={"a":1,"b":{"g":0},"c":"changed"}
arrayShrink=[0,null,null]
arrayGrow=[0,1]3
holes=[1,"proto",3]
cycleObject=TypeError
cycleArray=TypeError
shared={"a":{"x":1},"b":{"x":1},"c":[{"x":1},{"x":1}]}
proxyObject={"p":{"a":1,"b":{"c":2}}}get:toJSON,keys,desc:a,desc:b,get:a,get:b
proxyArray=[1,[2]]get:toJSON,get:length,get:0,get:1
revoked=TypeError
replacerFn={"a":2,"b":[4,{"c":6}],"d":"s"}object:,root:a,root:b,array:0,array:1,object:c,root:d
replacerRootHolder={"ok":true}
replacerList={"a":2,"c":{"a":3,"1":5},"1":6}
replacerListArray=[{"b":2},{"b":3}]
collections=[{"extra":1},{},{"0":1,"1":2},{},{},{}]
fnWithToJSON={"f":"fn"}
raw={"a":1e1000,"b":["s"]}
gapString={\u000a--abcdefgh"a": [\u000a--abcdefgh--abcdefgh1,\u000a--abcdefgh--abcdefgh{}\u000a--abcdefgh],\u000a--abcdefgh"b": {},\u000a--abcdefgh"c": []\u000a}
twice={"p":29,"q":"two","r":[3]}"###,
    );
}
