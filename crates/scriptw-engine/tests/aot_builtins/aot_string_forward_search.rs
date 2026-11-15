//! String.prototype.indexOf and includes find the next position holding the
//! needle's first code unit in a leaf kernel before comparing the needle.
//! The script covers the empty needle, needles at both ends and longer than
//! the string, repeated first units and overlapping candidates, every
//! position coercion, surrogates and NUL, argument coercion and its order,
//! the RegExp rejection of includes, and the backward and anchored searches
//! that keep the original loop.
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
fn forward_string_searches_match_node() {
    assert_node_report(
        r###"
var log = [];
function attempt(label, f) { try { log.push(label + "=" + f()); } catch (e) { log.push(label + "=" + e.constructor.name); } }
var text = ""; for (var i = 0; i < 32; i++) text += "[\"$\",\"li\",\"" + i + "\",{\"data-id\":" + i + ",\"children\":\"Item <" + i + "> & café\"}]\n";
attempt("rows", function () { return [text.includes("Item <31> & café"), text.includes("Item <32>"), text.indexOf("Item <31>"), text.indexOf("Item <3"), text.indexOf("Item <3", 300), text.indexOf("\n"), text.indexOf("\n", 60), text.indexOf("]\n", text.length - 2), text.indexOf("]\n", text.length - 1), text.length].join(","); });
attempt("empty", function () { return ["abc".indexOf(""), "abc".indexOf("", 2), "abc".indexOf("", 3), "abc".indexOf("", 9), "".indexOf(""), "".indexOf("a"), "abc".includes(""), "".includes(""), "abc".includes("", 7)].join(","); });
attempt("edges", function () { return ["abc".indexOf("abc"), "abc".indexOf("abcd"), "abc".indexOf("c"), "abc".indexOf("a"), "abc".indexOf("bc"), "abc".indexOf("d"), "abc".indexOf("c", 2), "abc".indexOf("c", 3), "abc".indexOf("a", 1), "a".indexOf("a"), "a".includes("a", 1)].join(","); });
attempt("repeats", function () { var s = ""; for (var i = 0; i < 200; i++) s += "a"; var t = s + "b" + s; return [t.indexOf("aaaab"), t.indexOf("ab"), t.indexOf("ba"), t.indexOf("baaa", 100), t.indexOf("aab", 199), t.indexOf("aab", 200), t.includes("bb"), t.indexOf("ababa"), "ababababc".indexOf("ababc"), "ababa".indexOf("aba", 1), "aaa".indexOf("aa", 1), "aaa".indexOf("aa", 2)].join(","); });
attempt("positions", function () { var s = "hello world hello"; return [s.indexOf("hello", -5), s.indexOf("hello", 0.9), s.indexOf("hello", 1), s.indexOf("hello", "1"), s.indexOf("hello", NaN), s.indexOf("hello", Infinity), s.indexOf("hello", -Infinity), s.indexOf("hello", 12), s.indexOf("hello", 13), s.indexOf("o", 4.99), s.indexOf("o", { valueOf: function () { return 5; } }), s.includes("world", 6), s.includes("world", 7), s.includes("hello", undefined), s.includes("hello", null)].join(","); });
attempt("surrogates", function () { var s = "a😀b😁c\ud83d"; return [s.indexOf("\ud83d"), s.indexOf("\ud83d", 2), s.indexOf("\ude01"), s.indexOf("😁"), s.indexOf("\ud83dc"), s.indexOf("\ud83d", 7), s.includes("\ude00b"), s.indexOf("\u0000"), "x\u0000y".indexOf("\u0000")].join(","); });
attempt("coerce", function () { return ["undefined".indexOf(), "xundefined".indexOf(undefined), "null".includes(null), "a12b".indexOf(12), "a1,2b".indexOf([1, 2]), "[object Object]".includes({}), "true".indexOf(true), "abc".indexOf({ toString: function () { return "c"; } })].join(","); });
attempt("regexp", function () { return "abc".includes(/b/); });
attempt("regexpOff", function () { var r = /b/; r[Symbol.match] = false; return "a/b/c".includes(r); });
attempt("order", function () { var order = []; var r = String.prototype.indexOf.call({ toString: function () { order.push("this"); return "needle in hay"; } }, { toString: function () { order.push("search"); return "in"; } }, { valueOf: function () { order.push("pos"); return 1; } }); return r + order.join(">"); });
attempt("nullThis", function () { return String.prototype.includes.call(null, "a"); });
attempt("symbol", function () { return "abc".indexOf(Symbol("a")); });
attempt("others", function () { var s = "abcabcabc"; return [s.lastIndexOf("abc"), s.lastIndexOf("abc", 5), s.lastIndexOf("c", 0), s.lastIndexOf(""), s.startsWith("abc"), s.startsWith("bc", 1), s.startsWith("abcd"), s.endsWith("abc"), s.endsWith("ab", 8), s.endsWith(""), "".startsWith("")].join(","); });
attempt("long", function () { var s = ""; for (var i = 0; i < 5000; i++) s += String.fromCharCode(97 + i % 23); var needle = s.slice(4980) + "!"; return [(s + "!").indexOf(needle), s.indexOf(needle), s.indexOf(s.slice(100, 4000)), s.indexOf("w" + "a"), s.indexOf("wb")].join(","); });
var result = log.join("\n");
"###,
        r###"rows=true,false,1902,213,1841,57,115,1920,-1,1922
empty=0,2,3,3,0,-1,true,true,true
edges=0,-1,2,0,1,-1,2,-1,-1,0,false
repeats=196,199,200,200,-1,-1,false,-1,4,2,1,-1
positions=0,0,12,12,0,-1,0,12,-1,4,7,true,false,true,true
surrogates=1,4,5,4,-1,7,true,-1,1
coerce=0,1,true,1,1,true,0,2
regexp=TypeError
regexpOff=true
order=7this>search>pos
nullThis=TypeError
symbol=TypeError
others=6,3,-1,9,true,true,false,true,true,true,true
long=4980,-1,8,22,-1"###,
    );
}
