//! TextEncoder.prototype.encode and TextDecoder.prototype.decode copy each
//! ASCII run through a leaf kernel and handle every other scalar themselves.
//! encode writes an input of at most 2^20 code units once into a growing
//! scratch array and counts a longer one first. The script covers run
//! lengths around the kernel's edges, every UTF-8 width, lone and paired
//! surrogates at both ends, inputs that grow the scratch array, both sides
//! of the 2^20 limit, argument coercion, malformed and truncated sequences,
//! the byte order mark, `fatal`, `ignoreBOM`, every split of a streamed
//! input, every kind of buffer source and a resized length-tracking view.
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
fn text_codecs_match_node_across_ascii_runs_and_other_scalars() {
    assert_node_report(
        r###"
var log = [];
function hash(bytes) { var h = 2166136261; for (var i = 0; i < bytes.length; i++) { h ^= bytes[i]; h = Math.imul(h, 16777619) >>> 0; } return bytes.length + "#" + h.toString(16); }
function units(s) { var h = 2166136261; for (var i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619) >>> 0; } return s.length + "#" + h.toString(16); }
function hex(s) { var out = ""; for (var i = 0; i < s.length; i++) out += (i ? " " : "") + s.charCodeAt(i).toString(16); return out; }
function list(bytes) { return Array.prototype.join.call(bytes, ","); }
function rep(s, n) { var out = ""; for (var i = 0; i < n; i++) out += s; return out; }
var enc = new TextEncoder();
function attempt(label, f) { try { log.push(label + "=" + f()); } catch (e) { log.push(label + "=" + e.constructor.name); } }
attempt("shape", function () { var b = enc.encode("abc"); return [b instanceof Uint8Array, b.byteOffset, b.byteLength, b.buffer.byteLength, b.buffer.resizable, Object.getPrototypeOf(b) === Uint8Array.prototype, b.buffer instanceof ArrayBuffer].join(","); });
var ascii = [0, 1, 7, 8, 9, 15, 16, 17, 100, 5000];
attempt("ascii", function () { return ascii.map(function (n) { var s = ""; for (var i = 0; i < n; i++) s += String.fromCharCode(i % 128); return hash(enc.encode(s)); }).join(" "); });
attempt("small", function () { return ["", "a", "\u007f", "\u0080", "߿", "ࠀ", "￿", "퟿", "\ud800", "\udfff", "𐀀", "􏿿", "\udc00\ud800", "a\ud800", "\ud800a", "\ud800𐀀", "café", "écaf", "x€y😀z"].map(function (s) { return list(enc.encode(s)); }).join(" | "); });
attempt("growth", function () { return [rep("€", 1000), rep("😀", 1000), rep("a€", 700), rep("abcdefghé", 500), rep("\ud800", 333), "a" + rep("€", 40), rep("a", 4000) + "€", "€" + rep("a", 4000)].map(function (s) { return hash(enc.encode(s)); }).join(" "); });
attempt("rows", function () { var s = ""; for (var i = 0; i < 32; i++) s += "[\"$\",\"li\",\"" + i + "\",{\"data-id\":" + i + ",\"children\":\"Item <" + i + "> & café\"}]"; var b = enc.encode(s); return hash(b) + ":" + units(new TextDecoder().decode(b)); });
attempt("coerce", function () { return [list(enc.encode()), list(enc.encode(undefined)), list(enc.encode(null)), list(enc.encode(12)), list(enc.encode({ toString: function () { return "oé"; } }))].join(" | "); });
attempt("symbol", function () { return enc.encode(Symbol("s")).length; });
attempt("huge", function () { var s = "0123456789abcdef"; while (s.length < 1048576) s += s; var exact = enc.encode(s); var t = s + "tail\u00e9\ud83d\ude00\ud800"; var b = enc.encode(t); var d = new TextDecoder().decode(b); return [s.length, exact.length, new TextDecoder().decode(exact) === s, b.length, hash(b.subarray(0, 40)), hash(b.subarray(b.length - 40)), d.length === t.length, hex(d.slice(-5)), d.slice(0, -1) === t.slice(0, -1)].join(":"); });
attempt("hugeMixed", function () { var s = "abcdefg\u00e9"; while (s.length < 1048576) s += s; var at = enc.encode(s); var over = enc.encode(s + "!"); return [at.length, hash(at.subarray(at.length - 30)), over.length, hash(over.subarray(over.length - 30)), new TextDecoder().decode(over) === s + "!"].join(":"); });
var dec = new TextDecoder();
function bytes() { return new Uint8Array(Array.prototype.slice.call(arguments)); }
attempt("decodeAscii", function () { return ascii.map(function (n) { var b = new Uint8Array(n); for (var i = 0; i < n; i++) b[i] = i % 128; return units(dec.decode(b)); }).join(" "); });
attempt("decodeMixed", function () { return [bytes(), bytes(0x61), bytes(0xc3, 0xa9), bytes(0x61, 0xc3, 0xa9, 0x62), bytes(0xe2, 0x82, 0xac), bytes(0xf0, 0x9f, 0x98, 0x80), bytes(0x80), bytes(0xc0, 0x80), bytes(0xc3), bytes(0xe2, 0x82), bytes(0xe0, 0x80, 0x80), bytes(0xed, 0xa0, 0x80), bytes(0xf4, 0x90, 0x80, 0x80), bytes(0xff, 0x61, 0xfe), bytes(0x61, 0xf0, 0x9f, 0x61), bytes(0xef, 0xbb, 0xbf, 0x61), bytes(0x61, 0xef, 0xbb, 0xbf), bytes(0xef, 0xbb, 0xbf), bytes(0xef, 0xbb, 0xbf, 0xef, 0xbb, 0xbf)].map(function (b) { return "[" + hex(dec.decode(b)) + "]"; }).join(""); });
attempt("ignoreBOM", function () { var d = new TextDecoder("utf-8", { ignoreBOM: true }); return hex(d.decode(bytes(0xef, 0xbb, 0xbf, 0x61))) + "/" + hex(d.decode(bytes(0x62))) + "/" + d.ignoreBOM + d.fatal + d.encoding; });
attempt("fatal", function () { var d = new TextDecoder("utf8", { fatal: true }); var out = []; [bytes(0x61, 0x62), bytes(0x61, 0x80), bytes(0xc3), bytes(0x61, 0xc3, 0xa9), bytes(0xef, 0xbb, 0xbf, 0x7a)].forEach(function (b) { try { out.push(hex(d.decode(b))); } catch (e) { out.push(e.constructor.name); } }); return out.join("/"); });
attempt("stream", function () { var s = "﻿abéc€d😀efghijklmnopqrstuvwxyzé"; var all = enc.encode(s); var whole = hex(dec.decode(all)); var bad = []; for (var split = 0; split <= all.length; split++) { var d = new TextDecoder(); var got = d.decode(all.subarray(0, split), { stream: true }) + d.decode(all.subarray(split), { stream: true }) + d.decode(); if (hex(got) !== whole) bad.push(split); } return whole + "|" + bad.join(","); });
attempt("streamTriple", function () { var all = enc.encode("x😀y€z"); var bad = 0; for (var a = 0; a <= all.length; a++) for (var b = a; b <= all.length; b++) { var d = new TextDecoder(); var got = d.decode(all.subarray(0, a), { stream: true }) + d.decode(all.subarray(a, b), { stream: true }) + d.decode(all.subarray(b)); if (got !== "x😀y€z") bad++; } return bad; });
attempt("streamFlush", function () { var d = new TextDecoder(); return hex(d.decode(bytes(0x61, 0xe2, 0x82), { stream: true })) + "/" + hex(d.decode()) + "/" + hex(d.decode(bytes(0xef, 0xbb, 0xbf, 0x62))); });
attempt("sources", function () { var b = new Uint8Array(40); for (var i = 0; i < 40; i++) b[i] = 65 + i % 26; var view = new DataView(b.buffer, 3, 20); var sub = b.subarray(5, 30); var words = new Uint16Array(b.buffer, 2, 9); return [dec.decode(b.buffer), dec.decode(view), dec.decode(sub), dec.decode(words), dec.decode(b.buffer.slice(1, 18))].join("/"); });
attempt("resizable", function () { var buffer = new ArrayBuffer(24, { maxByteLength: 64 }); var tracking = new Uint8Array(buffer, 4); for (var i = 0; i < tracking.length; i++) tracking[i] = 97 + i; var before = dec.decode(tracking); buffer.resize(40); var grown = dec.decode(tracking).length; buffer.resize(10); return before + "/" + grown + "/" + dec.decode(tracking); });
attempt("badInput", function () { return dec.decode("text"); });
attempt("badThis", function () { return TextEncoder.prototype.encode.call({}, "x"); });
var result = log.join("\n");
"###,
        r###"shape=true,0,3,3,false,true,true
ascii=0#811c9dc5 1#50c5d1f 7#e4991188 8#6bf6a41d 9#a444d0f 15#8d1126b8 16#c8fff215 17#6fe9fddf 100#de239011 5000#65baed9d
small= | 97 | 127 | 194,128 | 223,191 | 224,160,128 | 239,191,191 | 237,159,191,238,128,128 | 239,191,189 | 239,191,189 | 240,144,128,128 | 244,143,191,191 | 239,191,189,239,191,189 | 97,239,191,189 | 239,191,189,97 | 239,191,189,240,144,128,128 | 99,97,102,195,169 | 195,169,99,97,102 | 120,226,130,172,121,240,159,152,128,122
growth=3000#3b2c2a45 4000#99891b05 2800#7ab109ed 5000#f5cf5735 999#555154a6 121#df09392c 4003#ce5c1c4b 4003#60fdf50b
rows=1922#77d5a389:1890#848a2abb
coerce= |  | 110,117,108,108 | 49,50 | 111,195,169
symbol=TypeError
huge=1048576:1048576:true:1048589:40#c660d80d:40#9f35152d:true:6c e9 d83d de00 fffd:true
hugeMixed=1179648:30#d8ac9e1a:1179649:30#2f923aa:true
decodeAscii=0#811c9dc5 1#50c5d1f 7#e4991188 8#6bf6a41d 9#a444d0f 15#8d1126b8 16#c8fff215 17#6fe9fddf 100#de239011 5000#65baed9d
decodeMixed=[][61][e9][61 e9 62][20ac][d83d de00][fffd][fffd fffd][fffd][fffd][fffd fffd fffd][fffd fffd fffd][fffd fffd fffd fffd][fffd 61 fffd][61 fffd 61][61][61 feff][][feff]
ignoreBOM=feff 61/62/truefalseutf-8
fatal=61 62/TypeError/TypeError/61 e9/7a
stream=61 62 e9 63 20ac 64 d83d de00 65 66 67 68 69 6a 6b 6c 6d 6e 6f 70 71 72 73 74 75 76 77 78 79 7a e9|
streamTriple=0
streamFlush=61/fffd/62
sources=ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMN/DEFGHIJKLMNOPQRSTUVW/FGHIJKLMNOPQRSTUVWXYZABCD/CDEFGHIJKLMNOPQRST/BCDEFGHIJKLMNOPQR
resizable=abcdefghijklmnopqrst/36/abcdef
badInput=TypeError
badThis=TypeError"###,
    );
}
