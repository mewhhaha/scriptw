//! The general Get walks a chain of objects whose own lookup for the key is
//! the ordinary one in their header without a helper entry per hop, and
//! calls a getter it finds with the original receiver. Besides ordinary
//! objects and functions that now admits Maps, Sets, WeakMaps, WeakSets,
//! Promises, TextEncoders, TextDecoders, ReadableStreams, their readers and
//! byte controllers, and typed arrays for keys that cannot be canonical
//! numeric Strings. The script reads accessors and methods through warmed
//! sites on each kind and then every case that must stay exotic or observe
//! a change: canonical numeric keys that a typed array answers itself,
//! expandos and shadowing own properties, getters that see the receiver,
//! redefined and deleted prototype accessors, replaced prototypes
//! (ordinary, Proxy and null), detached buffers, receivers of the wrong
//! brand, throwing getters and setter-only accessors.
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
fn composite_receivers_read_accessors_and_methods_as_node_does() {
    assert_node_report(
        r###"
var log = [];
function attempt(label, f) { try { log.push(label + "=" + f()); } catch (e) { log.push(label + "=" + e.constructor.name); } }
function show(v) { return typeof v === "function" ? "fn:" + v.name : typeof v === "object" && v !== null ? Object.prototype.toString.call(v) : String(v); }
function read(o, k) { return o[k]; }
function byteLength(o) { return o.byteLength; }
function length(o) { return o.length; }
function buffer(o) { return o.buffer; }
function set(o) { return o.set; }
function size(o) { return o.size; }
function encode(o) { return o.encode; }
function who(o) { return o.who; }
var ta = new Uint8Array(new ArrayBuffer(16), 4, 8);
attempt("typedAccessors", function () { var out = []; for (var i = 0; i < 20; i++) out = [byteLength(ta), length(ta), buffer(ta) === ta.buffer, set(ta) === Uint8Array.prototype.set, ta.byteOffset, ta[Symbol.toStringTag], ta.BYTES_PER_ELEMENT, ta.constructor === Uint8Array, ta.missing]; return out.join(","); });
attempt("typedKinds", function () { return [new Float64Array(3), new Uint16Array(new ArrayBuffer(8), 2), new BigInt64Array(2), new Uint8ClampedArray(5)].map(function (t) { return byteLength(t) + ":" + length(t) + ":" + t.byteOffset + ":" + t[Symbol.toStringTag]; }).join(" "); });
attempt("typedNumericKeys", function () {
  var keys = ["0", "7", "8", "-0", "1.5", "NaN", "Infinity", "-Infinity", "1e+21", "1e3", "01", "+1", "Infinityx", "-", "", " 1", "1 ", "0x1", "N", "I", "-1", "1e21", "1e-7", "0.0000001", "4294967296"];
  keys.forEach(function (k) { Object.prototype[k] = "proto:" + k; });
  try { return keys.map(function (k) { var v; for (var i = 0; i < 3; i++) v = read(ta, k); return k + ">" + v; }).join("|"); } finally { keys.forEach(function (k) { delete Object.prototype[k]; }); }
});
attempt("typedExpando", function () { var t = new Uint8Array(4); var out = [t.own]; t.own = 5; out.push(t.own, read(t, "own")); Object.defineProperty(t, "byteLength", { value: "shadow", configurable: true }); out.push(byteLength(t)); delete t.byteLength; out.push(byteLength(t)); Object.defineProperty(t, "acc", { get: function () { return this === t ? "own-getter" : "wrong"; } }); out.push(t.acc); return out.join(","); });
attempt("typedReceiver", function () {
  Object.defineProperty(Object.prototype, "who", { get: function () { return this === ta ? "ta" : typeof this; }, configurable: true });
  try { var out = []; for (var i = 0; i < 12; i++) out[0] = who(ta); out.push(who(new Uint8Array(1)), who(new Set()), who(new TextEncoder()), who(5), who("s")); return out.join(","); } finally { delete Object.prototype.who; }
});
attempt("typedProtoChanges", function () {
  var t = new Uint8Array(6); var out = [];
  for (var i = 0; i < 12; i++) out[0] = byteLength(t);
  var original = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(Uint8Array.prototype), "byteLength");
  Object.defineProperty(Uint8Array.prototype, "byteLength", { get: function () { return "sub:" + original.get.call(this); }, configurable: true });
  out.push(byteLength(t)); delete Uint8Array.prototype.byteLength; out.push(byteLength(t));
  Object.setPrototypeOf(t, { get byteLength() { return "custom:" + (this === t); } }); out.push(byteLength(t), length(t));
  var traps = []; Object.setPrototypeOf(t, new Proxy({}, { get: function (target, key, receiver) { traps.push(String(key) + ":" + (receiver === t)); return "trap"; } }));
  out.push(byteLength(t), t[0], read(t, "1.5"), read(t, "x")); out.push(traps.join("+"));
  Object.setPrototypeOf(t, null); out.push(byteLength(t), t[1]);
  return out.join(",");
});
attempt("detached", function () { var t = new Uint8Array(8); t.buffer.transfer(); return [byteLength(t), length(t), t.byteOffset, buffer(t).byteLength, t[0]].join(","); });
attempt("wrongReceiver", function () { var o = Object.create(new Uint8Array(4)); return byteLength(o); });
attempt("setSize", function () { var s = new Set([1, 2, 3]); var out = []; for (var i = 0; i < 20; i++) out[0] = size(s); s.add(4); out.push(size(s), size(new Map([[1, 1]])), size({ size: "plain" }), size([]), size("str")); return out.join(","); });
attempt("setSizeInherited", function () { return size(Object.create(new Set())); });
attempt("setSubclass", function () { class S extends Set { get size() { return "sub:" + super.size; } } var s = new S([1]); var out = []; for (var i = 0; i < 12; i++) out[0] = size(s); s.size2 = 1; Object.defineProperty(s, "size", { value: "own", configurable: true }); out.push(size(s)); delete s.size; out.push(size(s)); return out.join(","); });
attempt("setSizeRedefined", function () {
  var s = new Set([1]); var original = Object.getOwnPropertyDescriptor(Set.prototype, "size"); var out = [];
  for (var i = 0; i < 12; i++) out[0] = size(s);
  try { Object.defineProperty(Set.prototype, "size", { value: "data", configurable: true }); out.push(size(s)); delete Set.prototype.size; out.push(size(s)); Object.prototype.size = "object"; out.push(size(s)); delete Object.prototype.size; }
  finally { Object.defineProperty(Set.prototype, "size", original); }
  out.push(size(s)); return out.join(",");
});
attempt("mapWeak", function () { var m = new Map(); var w = new WeakMap(); var ws = new WeakSet(); var p = Promise.resolve(1); return [show(m.get), show(m.size), show(w.has), show(ws.add), show(p.then), show(p.constructor), m[Symbol.toStringTag], p[Symbol.toStringTag], show(read(m, "0")), show(read(w, "length"))].join(","); });
attempt("codecs", function () { var e = new TextEncoder(); var d = new TextDecoder("utf-8", { fatal: true }); var out = []; for (var i = 0; i < 12; i++) out = [show(encode(e)), e.encoding, show(e.encodeInto), d.fatal, d.ignoreBOM, d.encoding, show(d.decode), e.nothing, show(e.toString)]; e.encode = "own"; out.push(encode(e)); return out.join(","); });
attempt("stream", function () {
  var out = []; var controller;
  var stream = new ReadableStream({ type: "bytes", start: function (c) { controller = c; for (var i = 0; i < 12; i++) out = [show(c.enqueue), show(c.close), show(c.error), c.desiredSize, c.missing]; } }, { highWaterMark: 0 });
  var reader = stream.getReader();
  out.push(show(stream.getReader), stream.locked, show(reader.read), show(reader.releaseLock), show(reader.closed), stream.missing, reader.missing);
  controller.custom = 1; out.push(controller.custom);
  return out.join(",");
});
attempt("mixedSite", function () { var receivers = [new Uint8Array(3), new Set([1]), new Map(), new TextEncoder(), { byteLength: "o" }, [1, 2], "abc", new ArrayBuffer(5), new DataView(new ArrayBuffer(9), 2), function f(a, b) {}]; var out = []; for (var round = 0; round < 4; round++) out = receivers.map(function (r) { return byteLength(r) + "/" + length(r) + "/" + size(r); }); return out.join(" "); });
attempt("getterThrows", function () { var t = new Uint8Array(2); Object.defineProperty(Object.getPrototypeOf(t), "boom", { get: function () { throw new RangeError("x"); }, configurable: true }); try { return read(t, "boom"); } finally { delete Uint8Array.prototype.boom; } });
attempt("undefinedGetter", function () { var t = new Uint8Array(2); Object.defineProperty(Uint8Array.prototype, "setter", { set: function (v) {}, configurable: true }); try { return read(t, "setter") + "," + read(new Set(), "setter"); } finally { delete Uint8Array.prototype.setter; } });
attempt("symbols", function () { var t = new Uint8Array(2); return [show(t[Symbol.iterator]), show(new Set()[Symbol.iterator]), show(t[Symbol("none")]), show(new TextEncoder()[Symbol.iterator])].join(","); });
var result = log.join("\n");
"###,
        r###"typedAccessors=8,8,true,true,4,Uint8Array,1,true,
typedKinds=24:3:0:Float64Array 6:3:2:Uint16Array 16:2:0:BigInt64Array 5:5:0:Uint8ClampedArray
typedNumericKeys=0>0|7>0|8>undefined|-0>undefined|1.5>undefined|NaN>undefined|Infinity>undefined|-Infinity>undefined|1e+21>undefined|1e3>proto:1e3|01>proto:01|+1>proto:+1|Infinityx>proto:Infinityx|->proto:-|>proto:| 1>proto: 1|1 >proto:1 |0x1>proto:0x1|N>proto:N|I>proto:I|-1>undefined|1e21>proto:1e21|1e-7>undefined|0.0000001>proto:0.0000001|4294967296>undefined
typedExpando=,5,5,shadow,4,own-getter
typedReceiver=ta,object,object,object,object,object
typedProtoChanges=6,sub:6,6,custom:true,,trap,0,,trap,byteLength:true+x:true,,0
detached=0,0,0,0,
wrongReceiver=TypeError
setSize=3,4,1,plain,,
setSizeInherited=TypeError
setSubclass=sub:1,own,sub:1
setSizeRedefined=1,data,,object,1
mapWeak=fn:get,0,fn:has,fn:add,fn:then,fn:Promise,Map,Promise,undefined,undefined
codecs=fn:encode,utf-8,fn:encodeInto,true,false,utf-8,fn:decode,,fn:toString,own
stream=fn:enqueue,fn:close,fn:error,0,,fn:getReader,true,fn:read,fn:releaseLock,[object Promise],,,1
mixedSite=3/3/undefined undefined/undefined/1 undefined/undefined/0 undefined/undefined/undefined o/undefined/undefined undefined/2/undefined undefined/3/undefined 5/undefined/undefined 7/undefined/undefined undefined/2/undefined
getterThrows=RangeError
undefinedGetter=undefined,undefined
symbols=fn:values,fn:values,undefined,undefined"###,
    );
}
