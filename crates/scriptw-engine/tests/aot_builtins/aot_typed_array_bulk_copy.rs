//! A byte copy between two distinct collector-owned backings is one bulk
//! move from 16 bytes up; a shared block, one backing copied onto itself and
//! a short copy keep the ascending byte loop. The script drives every caller
//! of that copy: %TypedArray%.prototype.set across lengths around the limit,
//! offset views, overlapping ranges of one buffer in both directions,
//! same-width and converting element types, BigInt mismatches, array-likes,
//! offset coercion and range errors, detached, shrunk and length-tracking
//! buffers, ArrayBuffer.prototype.slice, transfer and resize, and
//! %TypedArray%.prototype.slice, including a species view of the source's
//! own buffer, where the ascending order is observable.
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
fn typed_array_byte_copies_match_node() {
    assert_node_report(
        r###"
var log = [];
function attempt(label, f) { try { log.push(label + "=" + f()); } catch (e) { log.push(label + "=" + e.constructor.name); } }
function fill(n, base) { var a = new Uint8Array(n); for (var i = 0; i < n; i++) a[i] = (base + i * 7) & 255; return a; }
function hash(bytes) { var h = 2166136261; for (var i = 0; i < bytes.length; i++) { h ^= bytes[i]; h = Math.imul(h, 16777619) >>> 0; } return bytes.length + "#" + h.toString(16); }
function list(a) { return Array.prototype.join.call(a, ","); }
attempt("distinct", function () { return [0, 1, 15, 16, 17, 31, 1000].map(function (n) { var t = fill(n + 9, 3); t.set(fill(n, 100), 5); return hash(t); }).join(" "); });
attempt("views", function () { var sb = fill(64, 1).buffer; var tb = fill(64, 200).buffer; var s = new Uint8Array(sb, 7, 40); var t = new Uint8Array(tb, 3, 50); t.set(s, 6); t.set(s.subarray(2, 20)); return hash(new Uint8Array(tb)) + ":" + hash(new Uint8Array(sb)); });
attempt("overlapForward", function () { var a = fill(64, 9); a.subarray(8).set(a.subarray(0, 40)); return hash(a); });
attempt("overlapBackward", function () { var a = fill(64, 9); a.set(a.subarray(8, 60), 0); return hash(a); });
attempt("overlapSame", function () { var a = fill(40, 9); a.set(a); a.set(a.subarray(0, 20), 20); return hash(a); });
attempt("overlapShort", function () { var a = fill(12, 4); a.set(a.subarray(0, 8), 2); return list(a); });
attempt("sameTypeWide", function () { var s = new Uint16Array(40); for (var i = 0; i < 40; i++) s[i] = i * 1027; var t = new Uint16Array(50); t.set(s, 7); return hash(new Uint8Array(t.buffer)); });
attempt("floatBits", function () { var raw = new Uint8Array(64); for (var i = 0; i < 64; i++) raw[i] = 255 - i; var s = new Float64Array(raw.buffer); var t = new Float64Array(10); t.set(s, 1); return hash(new Uint8Array(t.buffer)); });
attempt("signedToUnsigned", function () { var s = new Int8Array(32); for (var i = 0; i < 32; i++) s[i] = i * 9 - 100; var t = new Uint8Array(40); t.set(s, 4); return list(t); });
attempt("clamped", function () { var s = new Int16Array([-5, 0, 100, 255, 256, 3000, -32768, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]); var t = new Uint8ClampedArray(20); t.set(s, 1); return list(t); });
attempt("floatToInt", function () { var t = new Int16Array(6); t.set(new Float32Array([1.9, -1.9, NaN, Infinity, 65537.5, -0]), 0); return list(t); });
attempt("bigint", function () { var t = new BigUint64Array(3); t.set(new BigInt64Array([-1n, 2n]), 1); return t.join(","); });
attempt("mixedBigint", function () { new Uint8Array(4).set(new BigInt64Array(1)); return "no"; });
attempt("mixedBigint2", function () { new BigInt64Array(4).set(new Uint8Array(1)); return "no"; });
attempt("arrayLike", function () { var order = []; var t = new Uint8Array(20); t.set({ length: 3, get 0() { order.push(0); return 300; }, get 1() { order.push(1); return "7"; }, get 2() { order.push(2); return { valueOf: function () { order.push("v"); return 9.9; } }; } }, 16); t.set([1, 2, 3], 0); t.set("45", 5); return list(t) + ":" + order.join(""); });
attempt("offsets", function () { var out = []; [-1, 5, 6, 4.9, "2", NaN, Infinity, undefined, -0.5].forEach(function (o) { var t = new Uint8Array(20); try { t.set(fill(16, 1), o); out.push(hash(t)); } catch (e) { out.push(e.constructor.name); } }); return out.join(" "); });
attempt("tooLong", function () { new Uint8Array(16).set(fill(17, 0)); return "no"; });
attempt("notTyped", function () { Uint8Array.prototype.set.call([], [1]); return "no"; });
attempt("detachedTarget", function () { var t = new Uint8Array(32); t.buffer.transfer(); t.set(fill(16, 1)); return "no"; });
attempt("detachedSource", function () { var s = fill(32, 1); s.buffer.transfer(); var t = new Uint8Array(40); t.set(s); return hash(t); });
attempt("detachByOffset", function () { var t = new Uint8Array(32); t.set(fill(16, 1), { valueOf: function () { t.buffer.transfer(); return 0; } }); return "no"; });
attempt("shrinkByOffset", function () { var buffer = new ArrayBuffer(64, { maxByteLength: 128 }); var t = new Uint8Array(buffer); var out = []; try { t.set(fill(40, 1), { valueOf: function () { buffer.resize(32); return 0; } }); out.push("no"); } catch (e) { out.push(e.constructor.name); } out.push(t.length); t.set(fill(20, 5), 12); out.push(hash(t)); return out.join("/"); });
attempt("lengthTracking", function () { var buffer = new ArrayBuffer(48, { maxByteLength: 96 }); var s = new Uint8Array(buffer, 8); for (var i = 0; i < s.length; i++) s[i] = i + 1; buffer.resize(72); var t = new Uint8Array(80); t.set(s, 3); buffer.resize(4); var again; try { t.set(s); again = "ok"; } catch (e) { again = e.constructor.name; } return hash(t) + "/" + again; });
attempt("bufferSlice", function () { var a = fill(100, 17); return [hash(new Uint8Array(a.buffer.slice(0))), hash(new Uint8Array(a.buffer.slice(3, 90))), hash(new Uint8Array(a.buffer.slice(-20))), hash(new Uint8Array(a.buffer.slice(5, 9)))].join(" "); });
attempt("typedSlice", function () { var a = fill(100, 23); var w = new Uint32Array(fill(96, 5).buffer); return [hash(a.slice(1, 99)), hash(a.slice(40)), hash(new Uint8Array(w.slice(2, 20).buffer)), hash(a.subarray(10, 90).slice(5, 70))].join(" "); });
attempt("speciesSameBuffer", function () { var a = fill(48, 1); a.constructor = { [Symbol.species]: function (n) { return new Uint8Array(a.buffer, 1, n); } }; var b = a.slice(0, 40); return list(new Uint8Array(a.buffer)) + "|" + b.byteOffset; });
attempt("transfer", function () { var a = fill(64, 31); var b1 = a.buffer.transfer(40); var b2 = b1.transfer(80); return hash(new Uint8Array(b2)) + ":" + a.length + ":" + b1.byteLength; });
attempt("resizeKeeps", function () { var buffer = new ArrayBuffer(40, { maxByteLength: 200 }); var v = new Uint8Array(buffer); for (var i = 0; i < 40; i++) v[i] = i * 3; buffer.resize(160); buffer.resize(24); buffer.resize(64); return hash(v); });
attempt("fromTyped", function () { var a = fill(70, 2); return hash(new Uint8Array(a)) + " " + hash(new Uint8Array(new Uint8Array(a.buffer, 5, 33))) + " " + hash(Uint8Array.from(a)); });
var result = log.join("\n");
"###,
        r###"distinct=9#89399aaa 10#4d632316 24#60c8f667 25#6ccf501a 26#412b1d56 40#b17c4f07 1009#ef0253da
views=64#973a8d9d:64#f4a29bc5
overlapForward=64#653b6b85
overlapBackward=64#6bbe95a5
overlapSame=40#78b64d75
overlapShort=4,11,4,11,18,25,32,39,46,53,74,81
sameTypeWide=100#89377d1d
floatBits=80#6a670b05
signedToUnsigned=0,0,0,0,156,165,174,183,192,201,210,219,228,237,246,255,8,17,26,35,44,53,62,71,80,89,98,107,116,125,134,143,152,161,170,179,0,0,0,0
clamped=0,0,0,100,255,255,255,0,7,8,9,10,11,12,13,14,15,16,17,0
floatToInt=1,-1,0,0,1,0
bigint=0,18446744073709551615,2
mixedBigint=TypeError
mixedBigint2=TypeError
arrayLike=1,2,3,0,0,4,5,0,0,0,0,0,0,0,0,0,44,7,9,0:012v
offsets=RangeError RangeError RangeError 20#1716a995 20#f57f85b5 20#4a8d4755 RangeError 20#4a8d4755 20#4a8d4755
tooLong=RangeError
notTyped=TypeError
detachedTarget=TypeError
detachedSource=TypeError
detachByOffset=TypeError
shrinkByOffset=RangeError/32/32#a0126d2d
lengthTracking=80#a5b55c45/TypeError
bufferSlice=100#6a24f57d 87#a8ad11b8 20#a435c03d 4#e4ea3715
typedSlice=98#c2f46a4c 60#9151431 72#76a7e355 65#2e93e95f
speciesSameBuffer=1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,32,39,46,53,60,67,74|1
transfer=80#dba7c265:0:0
resizeKeeps=64#69c00925
fromTyped=70#d773bf68 33#90005700 70#d773bf68"###,
    );
}
