//! JSON.stringify in a plain context (no replacer function, no property
//! list, no gap) serializes actual Arrays and ordinary objects in a hot
//! runtime entry once the call-free walk proves their chains lack "toJSON",
//! and hands every other value to the general helper, which hands its
//! members back. The script checks what that split must keep exact: every
//! root kind; "toJSON" appearing on an intrinsic prototype, an own object or
//! an Array.prototype parent while the walk is under way; inherited toJSON
//! getters; class instances; toJSON results nested inside the hot walk;
//! cycles detected across the hot and general entries; dictionary and
//! intrinsic objects; exotic, Symbol, function and wrapper members with and
//! without prototype toJSON methods; getter order and exceptions; holes,
//! deep nesting, and the gap, property-list and replacer contexts, which stay
//! on the general helper.
//! Expectations are Node's for the same script.
use scriptw_engine::{CompileOptions, Engine, ExecutionBackend, RealmBuilder, RunOptions};

/// Runs `source`, which leaves its report in `result`, and compares the
/// report line by line with Node's for the same source.
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
fn json_stringify_plain_entry_matches_node_for_every_route() {
    assert_node_report(
        r###"
var log = [];
function show(label, make) {
  var text;
  try { text = make(); text = text === undefined ? "undefined" : String(text); } catch (e) { text = e.constructor.name; }
  log.push(label + "=" + text);
}
show("roots", function () { return [JSON.stringify("s"), JSON.stringify(5), JSON.stringify(null), JSON.stringify(true), JSON.stringify(new Date(0)), JSON.stringify(new String("w")), JSON.stringify([]), JSON.stringify({})].join(" "); });
show("protoAddedByGetter", function () {
  var o = { a: { get g() { Object.prototype.toJSON = function (k) { return "P" + k; }; return 1; } }, b: { c: 1 }, d: [2] };
  try { return JSON.stringify(o); } finally { delete Object.prototype.toJSON; }
});
show("arrayProtoAddedByGetter", function () {
  var a = [{ get x() { Array.prototype.toJSON = function (k) { return "A" + k; }; return 0; } }, [1], { y: [2] }];
  try { return JSON.stringify(a); } finally { delete Array.prototype.toJSON; }
});
show("siblingOwnToJSONAdded", function () {
  var later = { v: 1 };
  var o = { a: { get g() { later.toJSON = function (k) { return "L" + k; }; return 1; } }, b: later, c: [later] };
  return JSON.stringify(o);
});
show("arrayProtoChain", function () {
  var saved = Object.getPrototypeOf(Array.prototype);
  var p = Object.create(Object.prototype, { toJSON: { value: function (k) { return "X" + k + ":" + this.length; }, configurable: true } });
  Object.setPrototypeOf(Array.prototype, p);
  try { return JSON.stringify({ a: [1, 2], b: {}, c: [[3]] }); } finally { Object.setPrototypeOf(Array.prototype, saved); }
});
show("inheritedGetterToJSON", function () {
  var n = 0; var proto = {}; Object.defineProperty(proto, "toJSON", { get: function () { n++; return undefined; } });
  var o = Object.create(proto); o.a = Object.create(proto); o.a.b = [Object.create(proto)];
  return JSON.stringify(o) + n;
});
show("classInstances", function () {
  function P(x) { this.x = x; this.kids = []; }
  var root = new P(1); root.kids.push(new P(2), new P(3)); root.kids[0].kids.push({ plain: true });
  return JSON.stringify(root);
});
show("toJSONResultNested", function () {
  var inner = { deep: [1, { e: "f" }] };
  return JSON.stringify({ a: { toJSON: function () { return { r: [inner, inner], s: "t" }; } }, b: [{ toJSON: function (k) { return [k, { k: k }]; } }] });
});
show("cycleThroughToJSON", function () {
  var c = { x: 1 }; c.self = { toJSON: function () { return { inner: [c] }; } };
  return JSON.stringify(c);
});
show("cycleAfterToJSONSibling", function () {
  var c = { a: { toJSON: function () { return 1; } }, b: [] }; c.b.push({ back: c });
  return JSON.stringify(c);
});
show("noCycleAcrossSiblings", function () {
  var s = [1]; var t = { s: s };
  return JSON.stringify([t, { toJSON: function () { return [t, s]; } }, t]);
});
show("dictionary", function () {
  var o = {}; for (var i = 0; i < 40; i++) o["k" + i] = i; for (var j = 0; j < 40; j += 3) delete o["k" + j];
  o.n = { m: [o.k1, o.k2] }; var t = JSON.stringify(o); return t.length + ":" + t.slice(0, 24) + ":" + t.slice(-26);
});
show("dictionaryToJSON", function () {
  var o = {}; for (var i = 0; i < 20; i++) o["k" + i] = i; delete o.k3; o.toJSON = function (k) { return "D" + k; };
  return JSON.stringify({ o: o, p: [o] });
});
show("intrinsicObjects", function () { return JSON.stringify([Math, JSON, Reflect]); });
show("membersExotic", function () {
  return JSON.stringify({ s: Symbol("q"), f: function () {}, u: undefined, a: [Symbol("r"), function () {}, undefined], m: new Map([[1, 2]]), args: (function () { return arguments; })(1, 2) });
});
show("functionProtoToJSON", function () {
  Function.prototype.toJSON = function (k) { return "F" + k; };
  try { return JSON.stringify({ f: function () {}, a: [function () {}] }); } finally { delete Function.prototype.toJSON; }
});
show("symbolProtoToJSON", function () {
  Symbol.prototype.toJSON = function (k) { return "S" + k; };
  try { return JSON.stringify({ s: Symbol(), a: [Symbol()] }); } finally { delete Symbol.prototype.toJSON; }
});
show("stringProtoToJSON", function () {
  String.prototype.toJSON = function (k) { return "never"; };
  try { return JSON.stringify({ s: "x", a: ["y"], w: new String("z") }); } finally { delete String.prototype.toJSON; }
});
show("getterThrows", function () { return JSON.stringify({ a: 1, b: [{ get c() { throw new RangeError("c"); } }] }); });
show("getterOrder", function () {
  var order = []; function g(name, v) { return { get: function () { order.push(name); return v; }, enumerable: true }; }
  var inner = Object.defineProperties({}, { p: g("p", 1), q: g("q", [2]) });
  var o = Object.defineProperties({}, { x: g("x", inner), y: g("y", 3) });
  return JSON.stringify([o, o]) + order.join("");
});
show("holeyNested", function () { var a = [, [, {}], ,]; return JSON.stringify(a); });
show("deepMixed", function () { var v = "leaf"; for (var i = 0; i < 200; i++) v = i % 3 ? [v] : { n: v }; var t = JSON.stringify(v); return t.length + ":" + t.slice(0, 20) + ":" + t.slice(-20); });
show("gapAndListStillGeneral", function () { var o = { a: [1, { b: 2 }], c: { toJSON: function () { return { d: 4 }; } } }; return JSON.stringify(o, null, 1) + "|" + JSON.stringify(o, ["a", "b", "c", "d"]) + "|" + JSON.stringify(o, function (k, v) { return v; }); });
show("rootArrayToJSON", function () { var a = [1]; a.toJSON = function (k) { return "own" + typeof k + k; }; return JSON.stringify(a) + JSON.stringify([a]); });
var result = log.join("\n");
"###,
        r###"roots="s" 5 null true "1970-01-01T00:00:00.000Z" "w" [] {}
protoAddedByGetter={"a":{"g":1},"b":"Pb","d":"Pd"}
arrayProtoAddedByGetter=[{"x":0},"A1",{"y":"Ay"}]
siblingOwnToJSONAdded={"a":{"g":1},"b":"Lb","c":["L0"]}
arrayProtoChain={"a":"Xa:2","b":{},"c":"Xc:1"}
inheritedGetterToJSON={"a":{"b":[{}]}}3
classInstances={"x":1,"kids":[{"x":2,"kids":[{"plain":true}]},{"x":3,"kids":[]}]}
toJSONResultNested={"a":{"r":[{"deep":[1,{"e":"f"}]},{"deep":[1,{"e":"f"}]}],"s":"t"},"b":[["0",{"k":"0"}]]}
cycleThroughToJSON=TypeError
cycleAfterToJSONSibling=TypeError
noCycleAcrossSiblings=[{"s":[1]},[{"s":[1]},[1]],{"s":[1]}]
dictionary=239:{"k1":1,"k2":2,"k4":4,"k:,"k38":38,"n":{"m":[1,2]}}
dictionaryToJSON={"o":"Do","p":["D0"]}
intrinsicObjects=[{},{},{}]
membersExotic={"a":[null,null,null],"m":{},"args":{"0":1,"1":2}}
functionProtoToJSON={"f":"Ff","a":["F0"]}
symbolProtoToJSON={"a":[null]}
stringProtoToJSON={"s":"x","a":["y"],"w":"never"}
getterThrows=RangeError
getterOrder=[{"x":{"p":1,"q":[2]},"y":3},{"x":{"p":1,"q":[2]},"y":3}]xpqyxpqy
holeyNested=[null,[null,{}],null]
deepMixed=674:[{"n":[[{"n":[[{"n"::}]]}]]}]]}]]}]]}]]}]
gapAndListStillGeneral={
 "a": [
  1,
  {
   "b": 2
  }
 ],
 "c": {
  "d": 4
 }
}|{"a":[1,{"b":2}],"c":{"d":4}}|{"a":[1,{"b":2}],"c":{"d":4}}
rootArrayToJSON="ownstring"["ownstring0"]"###,
    );
}
