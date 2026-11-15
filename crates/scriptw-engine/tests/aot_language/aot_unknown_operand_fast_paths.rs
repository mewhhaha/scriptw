//! Operators on operands whose types the compiler does not know answer the
//! common live types before their general R kernel: an addition whose syntax
//! expects a String concatenates a String with a String or a Number, a
//! numeric operator keeps a Number operand as it is, `== null` reads only the
//! other operand's tag, `s[0] === "c"` compares one code unit of a String
//! primitive, and `o[k] = v` finds a child shape among its parent's
//! children. Each script warms its sites with the admitted types and then
//! runs the same sites with every excluded kind: conversions that call user
//! code (and their order), Symbols and BigInts, -0 and NaN, wrappers,
//! Proxies, accessors, indices outside a String and lone surrogates.
//! Expectations are Node 24's, taken by running each body as a Script, as
//! function code and as a module.
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
fn string_expected_additions_concatenate_only_strings_and_numbers() {
    // Sites with an empty literal, a String literal or a concatenation on
    // one side, warmed with Strings and Numbers and then given every other
    // kind; Number::toString through those sites; the compound form; and
    // the order of the two evaluations against the two conversions.
    run_as_script_function_and_module(
        r##"
var log = [];
var calls = [];
function pre(x) { return "" + x; }
function post(x) { return x + ""; }
function left(x) { return "a" + x; }
function right(x) { return x + "b"; }
function chain(a, b) { return a + ":" + b; }
function show(f, a, b) {
  var r;
  try { r = b === undefined && f.length === 1 ? f(a) : f(a, b); r = typeof r + "=" + r + "#" + r.length; }
  catch (e) { r = e.constructor.name; }
  if (calls.length) { r += "<" + calls.join("") + ">"; calls.length = 0; }
  return r;
}
function all(label, value) {
  log.push(label + "|" + show(pre, value) + "|" + show(post, value) + "|" + show(left, value) + "|" + show(right, value) + "|" + show(chain, value, value) + "|" + show(chain, "k", value) + "|" + show(chain, value, 5));
}
// Warm every site with Strings and Numbers.
for (var i = 0; i < 4; i++) { all("s" + i, "x" + i); all("n" + i, i * 7); }
all("empty", "");
all("neg0", -0); all("nan", NaN); all("ninf", -Infinity); all("frac", 0.1); all("big", 1e21); all("tiny", 1e-7); all("neg", -42); all("max", 9007199254740993);
all("true", true); all("null", null); all("undefined", undefined); all("bigint", 10n);
all("symbol", Symbol("s"));
all("valueOf", { valueOf: function () { calls.push("v"); return 3; }, toString: function () { calls.push("t"); return "T"; } });
all("toString", { valueOf: function () { calls.push("v"); return {}; }, toString: function () { calls.push("t"); return "T"; } });
all("toPrimitive", { [Symbol.toPrimitive]: function (hint) { calls.push(hint[0]); return 8; }, valueOf: function () { calls.push("v"); return 3; } });
all("symbolResult", { [Symbol.toPrimitive]: function () { calls.push("p"); return Symbol(); } });
all("objectResult", { [Symbol.toPrimitive]: function () { calls.push("p"); return {}; } });
all("throwing", { valueOf: function () { calls.push("v"); throw new RangeError("no"); } });
all("stringObject", new String("w")); all("numberObject", new Number(5)); all("booleanObject", new Boolean(false));
all("array", [1, [2, 3]]); all("function", function f() {}.name === "f" ? [] : 0);
all("proxy", new Proxy({}, { get: function (t, k) { calls.push(typeof k === "symbol" ? "S" : k[0]); return k === "toString" ? function () { return "px"; } : undefined; } }));
all("nullProto", Object.create(null));
// Warmed again after the excluded kinds.
all("again", "z"); all("again", 11);
// A surrogate pair formed at the boundary, and lengths.
var joined = left("\ud83d") + "" ; var pair = chain("\ud83d", "\ude00");
var lone = [pre("\ud83d"), post("\ude00"), left("\ud83d"), right("\ude00"), chain("\ud83d", "\ude00"), chain("\ude00", "\ud83d")].map(function (s) { var u = []; for (var j = 0; j < s.length; j++) u.push(s.charCodeAt(j).toString(16)); return u.join("."); });
log.push(lone.join("/"), pair.length, pair.codePointAt(2).toString(16), ("\ud83d" + pre("\ude00")).codePointAt(0).toString(16), joined.length);
// Number::toString through the empty-literal sites.
var numbers = [];
for (var n = 0; n < 125; n++) numbers.push(pre(n) === String(n) && post(n) === n.toString() && pre(n).length === (n < 10 ? 1 : n < 100 ? 2 : 3));
log.push(numbers.indexOf(false), pre(9), pre(10), pre(99), pre(100), pre(-1), pre(-9), pre(1.5), pre(99.5), pre(2 ** 53), pre(4294967296), pre(123456789012), pre(1e-6), pre(0.000001234), pre(-1e-7), pre(1e100), pre(0 / 0), pre(1 / 0), pre(-1 / 0), 1 / pre(-0), typeof pre(7));
// The compound form and a longer chain.
var acc = ""; for (var k = 0; k < 12; k++) acc += k; acc += null; acc += 1.5;
var text = "Row " + 7 + " of " + 12 + ": " + "done" + "!";
log.push(acc, text, text.length);
// Three operands: the inner addition converts before the third evaluates.
function lead(a, b) { return "<" + a + b; }
function tri(a, b, c) { return a + "-" + b + c; }
var seq = [];
function named(tag, text) { return { toString: function () { seq.push(tag); return text; } }; }
function three(f, a, b, c) { var r; try { r = f(a, b, c); r = typeof r + "=" + r; } catch (e) { r = e.constructor.name; } if (seq.length) { r += "<" + seq.join("") + ">"; seq.length = 0; } return r; }
var kinds = ["s", 4, -0, 2.5, null, undefined, true, 9n, named("o", "O"), [1, 2], new String("w")];
var triples = [];
for (var x = 0; x < kinds.length; x++) for (var y = 0; y < kinds.length; y++) triples.push(three(lead, kinds[x], kinds[y]) + "/" + three(tri, kinds[x], kinds[y], kinds[(x + y) % kinds.length]) + "/" + three(tri, "p", kinds[x], kinds[y]));
log.push(triples.join(";"));
log.push(three(lead, Symbol(), "s"), three(lead, "s", Symbol()), three(tri, "s", Symbol(), named("n", "N")), three(tri, named("a", "A"), named("b", "B"), named("c", "C")), three(tri, named("a", "A"), Symbol(), named("c", "C")));
log.push("<" + named("a", "A") + (seq.push("e"), named("b", "B")), seq.join(""), "<" + 1 + (seq.push("f"), 2), "" + (seq.push("g"), 3) + (seq.push("h"), "4"), seq.join(""));
// Operands evaluate left to right before either conversion.
var order = [];
function tick(tag, v) { order.push(tag); return v; }
var o1 = { toString: function () { order.push("L"); return "l"; } };
var o2 = { toString: function () { order.push("R"); return "r"; } };
log.push(tick(1, o1) + "" + tick(2, o2), "" + tick(3, o1) + tick(4, 5), tick(5, 1) + tick(6, 2) + "", order.join(""));
print(log.join());
"##,
        "s0|string=x0#2|string=x0#2|string=ax0#3|string=x0b#3|string=x0:x0#5|string=k:x0#4|string=x0:5#4,n0|string=0#1|string=0#1|string=a0#2|string=0b#2|string=0:0#3|string=k:0#3|string=0:5#3,s1|string=x1#2|string=x1#2|string=ax1#3|string=x1b#3|string=x1:x1#5|string=k:x1#4|string=x1:5#4,n1|string=7#1|string=7#1|string=a7#2|string=7b#2|string=7:7#3|string=k:7#3|string=7:5#3,s2|string=x2#2|string=x2#2|string=ax2#3|string=x2b#3|string=x2:x2#5|string=k:x2#4|string=x2:5#4,n2|string=14#2|string=14#2|string=a14#3|string=14b#3|string=14:14#5|string=k:14#4|string=14:5#4,s3|string=x3#2|string=x3#2|string=ax3#3|string=x3b#3|string=x3:x3#5|string=k:x3#4|string=x3:5#4,n3|string=21#2|string=21#2|string=a21#3|string=21b#3|string=21:21#5|string=k:21#4|string=21:5#4,empty|string=#0|string=#0|string=a#1|string=b#1|string=:#1|string=k:#2|string=:5#2,neg0|string=0#1|string=0#1|string=a0#2|string=0b#2|string=0:0#3|string=k:0#3|string=0:5#3,nan|string=NaN#3|string=NaN#3|string=aNaN#4|string=NaNb#4|string=NaN:NaN#7|string=k:NaN#5|string=NaN:5#5,ninf|string=-Infinity#9|string=-Infinity#9|string=a-Infinity#10|string=-Infinityb#10|string=-Infinity:-Infinity#19|string=k:-Infinity#11|string=-Infinity:5#11,frac|string=0.1#3|string=0.1#3|string=a0.1#4|string=0.1b#4|string=0.1:0.1#7|string=k:0.1#5|string=0.1:5#5,big|string=1e+21#5|string=1e+21#5|string=a1e+21#6|string=1e+21b#6|string=1e+21:1e+21#11|string=k:1e+21#7|string=1e+21:5#7,tiny|string=1e-7#4|string=1e-7#4|string=a1e-7#5|string=1e-7b#5|string=1e-7:1e-7#9|string=k:1e-7#6|string=1e-7:5#6,neg|string=-42#3|string=-42#3|string=a-42#4|string=-42b#4|string=-42:-42#7|string=k:-42#5|string=-42:5#5,max|string=9007199254740992#16|string=9007199254740992#16|string=a9007199254740992#17|string=9007199254740992b#17|string=9007199254740992:9007199254740992#33|string=k:9007199254740992#18|string=9007199254740992:5#18,true|string=true#4|string=true#4|string=atrue#5|string=trueb#5|string=true:true#9|string=k:true#6|string=true:5#6,null|string=null#4|string=null#4|string=anull#5|string=nullb#5|string=null:null#9|string=k:null#6|string=null:5#6,undefined|string=undefined#9|string=undefined#9|string=aundefined#10|string=undefinedb#10|string=undefined:undefined#19|string=k:undefined#11|string=undefined:5#11,bigint|string=10#2|string=10#2|string=a10#3|string=10b#3|string=10:10#5|string=k:10#4|string=10:5#4,symbol|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError,valueOf|string=3#1<v>|string=3#1<v>|string=a3#2<v>|string=3b#2<v>|string=3:3#3<vv>|string=k:3#3<v>|string=3:5#3<v>,toString|string=T#1<vt>|string=T#1<vt>|string=aT#2<vt>|string=Tb#2<vt>|string=T:T#3<vtvt>|string=k:T#3<vt>|string=T:5#3<vt>,toPrimitive|string=8#1<d>|string=8#1<d>|string=a8#2<d>|string=8b#2<d>|string=8:8#3<dd>|string=k:8#3<d>|string=8:5#3<d>,symbolResult|TypeError<p>|TypeError<p>|TypeError<p>|TypeError<p>|TypeError<p>|TypeError<p>|TypeError<p>,objectResult|TypeError<p>|TypeError<p>|TypeError<p>|TypeError<p>|TypeError<p>|TypeError<p>|TypeError<p>,throwing|RangeError<v>|RangeError<v>|RangeError<v>|RangeError<v>|RangeError<v>|RangeError<v>|RangeError<v>,stringObject|string=w#1|string=w#1|string=aw#2|string=wb#2|string=w:w#3|string=k:w#3|string=w:5#3,numberObject|string=5#1|string=5#1|string=a5#2|string=5b#2|string=5:5#3|string=k:5#3|string=5:5#3,booleanObject|string=false#5|string=false#5|string=afalse#6|string=falseb#6|string=false:false#11|string=k:false#7|string=false:5#7,array|string=1,2,3#5|string=1,2,3#5|string=a1,2,3#6|string=1,2,3b#6|string=1,2,3:1,2,3#11|string=k:1,2,3#7|string=1,2,3:5#7,function|string=#0|string=#0|string=a#1|string=b#1|string=:#1|string=k:#2|string=:5#2,proxy|string=px#2<Svt>|string=px#2<Svt>|string=apx#3<Svt>|string=pxb#3<Svt>|string=px:px#5<SvtSvt>|string=k:px#4<Svt>|string=px:5#4<Svt>,nullProto|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError,again|string=z#1|string=z#1|string=az#2|string=zb#2|string=z:z#3|string=k:z#3|string=z:5#3,again|string=11#2|string=11#2|string=a11#3|string=11b#3|string=11:11#5|string=k:11#4|string=11:5#4,d83d/de00/61.d83d/de00.62/d83d.3a.de00/de00.3a.d83d,3,de00,1f600,2,-1,9,10,99,100,-1,-9,1.5,99.5,9007199254740992,4294967296,123456789012,0.000001,0.000001234,-1e-7,1e+100,NaN,Infinity,-Infinity,Infinity,string,01234567891011null1.5,Row 7 of 12: done!,18,string=<ss/string=s-ss/string=p-ss;string=<s4/string=s-44/string=p-s4;string=<s0/string=s-00/string=p-s0;string=<s2.5/string=s-2.52.5/string=p-s2.5;string=<snull/string=s-nullnull/string=p-snull;string=<sundefined/string=s-undefinedundefined/string=p-sundefined;string=<strue/string=s-truetrue/string=p-strue;string=<s9/string=s-99/string=p-s9;string=<sO<o>/string=s-OO<oo>/string=p-sO<o>;string=<s1,2/string=s-1,21,2/string=p-s1,2;string=<sw/string=s-ww/string=p-sw;string=<4s/string=4-s4/string=p-4s;string=<44/string=4-40/string=p-44;string=<40/string=4-02.5/string=p-40;string=<42.5/string=4-2.5null/string=p-42.5;string=<4null/string=4-nullundefined/string=p-4null;string=<4undefined/string=4-undefinedtrue/string=p-4undefined;string=<4true/string=4-true9/string=p-4true;string=<49/string=4-9O<o>/string=p-49;string=<4O<o>/string=4-O1,2<o>/string=p-4O<o>;string=<41,2/string=4-1,2w/string=p-41,2;string=<4w/string=4-ws/string=p-4w;string=<0s/string=0-s0/string=p-0s;string=<04/string=0-42.5/string=p-04;string=<00/string=0-0null/string=p-00;string=<02.5/string=0-2.5undefined/string=p-02.5;string=<0null/string=0-nulltrue/string=p-0null;string=<0undefined/string=0-undefined9/string=p-0undefined;string=<0true/string=0-trueO<o>/string=p-0true;string=<09/string=0-91,2/string=p-09;string=<0O<o>/string=0-Ow<o>/string=p-0O<o>;string=<01,2/string=0-1,2s/string=p-01,2;string=<0w/string=0-w4/string=p-0w;string=<2.5s/string=2.5-s2.5/string=p-2.5s;string=<2.54/string=2.5-4null/string=p-2.54;string=<2.50/string=2.5-0undefined/string=p-2.50;string=<2.52.5/string=2.5-2.5true/string=p-2.52.5;string=<2.5null/string=2.5-null9/string=p-2.5null;string=<2.5undefined/string=2.5-undefinedO<o>/string=p-2.5undefined;string=<2.5true/string=2.5-true1,2/string=p-2.5true;string=<2.59/string=2.5-9w/string=p-2.59;string=<2.5O<o>/string=2.5-Os<o>/string=p-2.5O<o>;string=<2.51,2/string=2.5-1,24/string=p-2.51,2;string=<2.5w/string=2.5-w0/string=p-2.5w;string=<nulls/string=null-snull/string=p-nulls;string=<null4/string=null-4undefined/string=p-null4;string=<null0/string=null-0true/string=p-null0;string=<null2.5/string=null-2.59/string=p-null2.5;string=<nullnull/string=null-nullO<o>/string=p-nullnull;string=<nullundefined/string=null-undefined1,2/string=p-nullundefined;string=<nulltrue/string=null-truew/string=p-nulltrue;string=<null9/string=null-9s/string=p-null9;string=<nullO<o>/string=null-O4<o>/string=p-nullO<o>;string=<null1,2/string=null-1,20/string=p-null1,2;string=<nullw/string=null-w2.5/string=p-nullw;string=<undefineds/string=undefined-sundefined/string=p-undefineds;string=<undefined4/string=undefined-4true/string=p-undefined4;string=<undefined0/string=undefined-09/string=p-undefined0;string=<undefined2.5/string=undefined-2.5O<o>/string=p-undefined2.5;string=<undefinednull/string=undefined-null1,2/string=p-undefinednull;string=<undefinedundefined/string=undefined-undefinedw/string=p-undefinedundefined;string=<undefinedtrue/string=undefined-trues/string=p-undefinedtrue;string=<undefined9/string=undefined-94/string=p-undefined9;string=<undefinedO<o>/string=undefined-O0<o>/string=p-undefinedO<o>;string=<undefined1,2/string=undefined-1,22.5/string=p-undefined1,2;string=<undefinedw/string=undefined-wnull/string=p-undefinedw;string=<trues/string=true-strue/string=p-trues;string=<true4/string=true-49/string=p-true4;string=<true0/string=true-0O<o>/string=p-true0;string=<true2.5/string=true-2.51,2/string=p-true2.5;string=<truenull/string=true-nullw/string=p-truenull;string=<trueundefined/string=true-undefineds/string=p-trueundefined;string=<truetrue/string=true-true4/string=p-truetrue;string=<true9/string=true-90/string=p-true9;string=<trueO<o>/string=true-O2.5<o>/string=p-trueO<o>;string=<true1,2/string=true-1,2null/string=p-true1,2;string=<truew/string=true-wundefined/string=p-truew;string=<9s/string=9-s9/string=p-9s;string=<94/string=9-4O<o>/string=p-94;string=<90/string=9-01,2/string=p-90;string=<92.5/string=9-2.5w/string=p-92.5;string=<9null/string=9-nulls/string=p-9null;string=<9undefined/string=9-undefined4/string=p-9undefined;string=<9true/string=9-true0/string=p-9true;string=<99/string=9-92.5/string=p-99;string=<9O<o>/string=9-Onull<o>/string=p-9O<o>;string=<91,2/string=9-1,2undefined/string=p-91,2;string=<9w/string=9-wtrue/string=p-9w;string=<Os<o>/string=O-sO<oo>/string=p-Os<o>;string=<O4<o>/string=O-41,2<o>/string=p-O4<o>;string=<O0<o>/string=O-0w<o>/string=p-O0<o>;string=<O2.5<o>/string=O-2.5s<o>/string=p-O2.5<o>;string=<Onull<o>/string=O-null4<o>/string=p-Onull<o>;string=<Oundefined<o>/string=O-undefined0<o>/string=p-Oundefined<o>;string=<Otrue<o>/string=O-true2.5<o>/string=p-Otrue<o>;string=<O9<o>/string=O-9null<o>/string=p-O9<o>;string=<OO<oo>/string=O-Oundefined<oo>/string=p-OO<oo>;string=<O1,2<o>/string=O-1,2true<o>/string=p-O1,2<o>;string=<Ow<o>/string=O-w9<o>/string=p-Ow<o>;string=<1,2s/string=1,2-s1,2/string=p-1,2s;string=<1,24/string=1,2-4w/string=p-1,24;string=<1,20/string=1,2-0s/string=p-1,20;string=<1,22.5/string=1,2-2.54/string=p-1,22.5;string=<1,2null/string=1,2-null0/string=p-1,2null;string=<1,2undefined/string=1,2-undefined2.5/string=p-1,2undefined;string=<1,2true/string=1,2-truenull/string=p-1,2true;string=<1,29/string=1,2-9undefined/string=p-1,29;string=<1,2O<o>/string=1,2-Otrue<o>/string=p-1,2O<o>;string=<1,21,2/string=1,2-1,29/string=p-1,21,2;string=<1,2w/string=1,2-wO<o>/string=p-1,2w;string=<ws/string=w-sw/string=p-ws;string=<w4/string=w-4s/string=p-w4;string=<w0/string=w-04/string=p-w0;string=<w2.5/string=w-2.50/string=p-w2.5;string=<wnull/string=w-null2.5/string=p-wnull;string=<wundefined/string=w-undefinednull/string=p-wundefined;string=<wtrue/string=w-trueundefined/string=p-wtrue;string=<w9/string=w-9true/string=p-w9;string=<wO<o>/string=w-O9<o>/string=p-wO<o>;string=<w1,2/string=w-1,2O<o>/string=p-w1,2;string=<ww/string=w-w1,2/string=p-ww,TypeError,TypeError,TypeError,string=A-BC<abc>,TypeError<a>,<AB,aeb,<12,34,aebfgh,lr,l5,3,1L2R3L456",
    );
}

#[test]
fn numeric_operators_convert_every_operand_that_is_not_a_number() {
    // Arithmetic, bitwise, relational and update sites with a Number
    // literal or two unknown operands, warmed with Numbers and then given
    // Strings, null, undefined, Booleans, BigInts (alone and mixed), Symbols
    // and objects whose conversions are counted.
    run_as_script_function_and_module(
        r##"
var log = [];
var calls = [];
function sub1(x) { return x - 1; }
function rsub(x) { return 10 - x; }
function mul2(x) { return x * 2; }
function div(x, y) { return x / y; }
function rem3(x) { return x % 3; }
function pow(x, y) { return x ** y; }
function sub(x, y) { return x - y; }
function neg(x) { return -x; }
function inc(x) { x++; return x; }
function post(x) { var y = x--; return y; }
function lt(x) { return 3200 < x; }
function ge(x, y) { return x >= y; }
function shl(x) { return x << 1; }
function show(f, a, b) {
  var r;
  try { r = f(a, b); r = typeof r === "number" && r === 0 && 1 / r < 0 ? "-0" : typeof r === "bigint" ? r + "n" : String(r); }
  catch (e) { r = e.constructor.name; }
  if (calls.length) { r += "<" + calls.join("") + ">"; calls.length = 0; }
  return r;
}
function all(label, value, other) {
  log.push(label + "|" + [sub1, rsub, mul2, rem3, neg, inc, post, lt, shl].map(function (f) { return show(f, value); }).join("|") + "|" + [div, pow, sub, ge].map(function (f) { return show(f, value, other); }).join("|") + "|" + [div, pow, sub, ge].map(function (f) { return show(f, other, value); }).join("|"));
}
for (var i = 0; i < 4; i++) all("n" + i, i * 3 + 1, 2);
all("neg0", -0, 1); all("zero", 0, -1); all("nan", NaN, 1); all("inf", Infinity, -Infinity); all("frac", 0.5, 4);
all("numeric", "5", 2); all("text", "abc", 2); all("empty", "", 2); all("hex", "0x10", "  7  ");
all("true", true, false); all("null", null, 1); all("undefined", undefined, 1);
all("bigint", 7n, 2n); all("mixed", 7n, 2); all("mixed2", 7, 2n);
all("symbol", Symbol(), 1);
all("valueOf", { valueOf: function () { calls.push("a"); return 6; } }, { valueOf: function () { calls.push("b"); return 3; } });
all("toString", { toString: function () { calls.push("a"); return "9"; } }, { valueOf: function () { calls.push("b"); return 2n; } });
all("toPrimitive", { [Symbol.toPrimitive]: function (hint) { calls.push(hint[0]); return 4; } }, 2);
all("throwing", { valueOf: function () { calls.push("a"); throw new RangeError("no"); } }, { valueOf: function () { calls.push("b"); return 1; } });
all("symbolic", { valueOf: function () { calls.push("a"); return Symbol(); } }, { valueOf: function () { calls.push("b"); return 1; } });
all("stringObject", new String("12"), new Number(4)); all("array", [8], [2]); all("date", new Date(86400000), 1000);
all("again", 9, 3);
// Both operands evaluate before either converts; the left converts first.
var order = [];
function tick(tag, v) { order.push(tag); return v; }
var l = { valueOf: function () { order.push("L"); return 20; } };
var r = { valueOf: function () { order.push("R"); return 4; } };
log.push(tick(1, l) - tick(2, r), tick(3, l) * 2, 100 / tick(4, r), tick(5, l) % tick(6, r), order.join(""));
// Updates keep the converted old value.
var u = "5"; var before = u++; var w = { valueOf: function () { return 2; } }; var kept = w--; var b = 5n; b++; var z = -0; var zold = z++;
log.push(typeof before, before, u, typeof kept, kept, w, b + "n", 1 / zold, z);
print(log.join());
"##,
        "n0|0|9|2|1|-1|2|1|false|2|0.5|1|-1|false|2|2|1|true,n1|3|6|8|1|-4|5|4|false|8|2|16|2|true|0.5|16|-2|false,n2|6|3|14|1|-7|8|7|false|14|3.5|49|5|true|0.2857142857142857|128|-5|false,n3|9|0|20|1|-10|11|10|false|20|5|100|8|true|0.2|1024|-8|false,neg0|-1|10|-0|-0|0|1|-0|false|0|-0|-0|-1|false|-Infinity|1|1|true,zero|-1|10|0|0|-0|1|0|false|0|-0|Infinity|1|true|-Infinity|1|-1|false,nan|NaN|NaN|NaN|NaN|NaN|NaN|NaN|false|0|NaN|NaN|NaN|false|NaN|NaN|NaN|false,inf|Infinity|-Infinity|Infinity|NaN|-Infinity|Infinity|Infinity|true|0|NaN|0|Infinity|true|NaN|Infinity|-Infinity|false,frac|-0.5|9.5|1|0.5|-0.5|1.5|0.5|false|0|0.125|0.0625|-3.5|false|8|2|3.5|true,numeric|4|5|10|2|-5|6|5|false|10|2.5|25|3|true|0.4|32|-3|false,text|NaN|NaN|NaN|NaN|NaN|NaN|NaN|false|0|NaN|NaN|NaN|false|NaN|NaN|NaN|false,empty|-1|10|0|0|-0|1|0|false|0|0|0|-2|false|Infinity|1|2|true,hex|15|-6|32|1|-16|17|16|false|32|2.2857142857142856|268435456|9|true|0.4375|33232930569601|-9|false,true|0|9|2|1|-1|2|1|false|2|Infinity|1|1|true|0|0|-1|false,null|-1|10|0|0|-0|1|0|false|0|0|0|-1|false|Infinity|1|1|true,undefined|NaN|NaN|NaN|NaN|NaN|NaN|NaN|false|0|NaN|NaN|NaN|false|NaN|NaN|NaN|false,bigint|TypeError|TypeError|TypeError|TypeError|-7n|8n|7n|false|TypeError|3n|49n|5n|true|0n|128n|-5n|false,mixed|TypeError|TypeError|TypeError|TypeError|-7n|8n|7n|false|TypeError|TypeError|TypeError|TypeError|true|TypeError|TypeError|TypeError|false,mixed2|6|3|14|1|-7|8|7|false|14|TypeError|TypeError|TypeError|true|TypeError|TypeError|TypeError|false,symbol|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError|TypeError,valueOf|5<a>|4<a>|12<a>|0<a>|-6<a>|7<a>|6<a>|false<a>|12<a>|2<ab>|216<ab>|3<ab>|true<ab>|0.5<ba>|729<ba>|-3<ba>|false<ba>,toString|8<a>|1<a>|18<a>|0<a>|-9<a>|10<a>|9<a>|false<a>|18<a>|TypeError<ab>|TypeError<ab>|TypeError<ab>|true<ab>|TypeError<ba>|TypeError<ba>|TypeError<ba>|false<ba>,toPrimitive|3<n>|6<n>|8<n>|1<n>|-4<n>|5<n>|4<n>|false<n>|8<n>|2<n>|16<n>|2<n>|true<n>|0.5<n>|16<n>|-2<n>|false<n>,throwing|RangeError<a>|RangeError<a>|RangeError<a>|RangeError<a>|RangeError<a>|RangeError<a>|RangeError<a>|RangeError<a>|RangeError<a>|RangeError<a>|RangeError<a>|RangeError<a>|RangeError<a>|RangeError<ba>|RangeError<ba>|RangeError<ba>|RangeError<ba>,symbolic|TypeError<a>|TypeError<a>|TypeError<a>|TypeError<a>|TypeError<a>|TypeError<a>|TypeError<a>|TypeError<a>|TypeError<a>|TypeError<a>|TypeError<a>|TypeError<a>|TypeError<ab>|TypeError<ba>|TypeError<ba>|TypeError<ba>|TypeError<ba>,stringObject|11|-2|24|0|-12|13|12|false|24|3|20736|8|true|0.3333333333333333|16777216|-8|false,array|7|2|16|2|-8|9|8|false|16|4|64|6|true|0.25|256|-6|false,date|86399999|-86399990|172800000|0|-86400000|86400001|86400000|true|172800000|86400|Infinity|86399000|true|0.000011574074074074073|Infinity|-86399000|false,again|8|1|18|0|-9|10|9|false|18|3|729|6|true|0.3333333333333333|19683|-6|false,16,40,25,0,12LR3L4R56LR,number,5,6,number,2,1,6n,-Infinity,1",
    );
}

#[test]
fn loose_equality_with_null_or_undefined_syntax_converts_nothing() {
    // `== null`, `null ==`, `!= null`, `== undefined` and `void` forms,
    // warmed with null, undefined, objects and Numbers and then given every
    // other kind; no conversion of an object or a Proxy is ever called.
    run_as_script_function_and_module(
        r##"
var log = [];
var calls = [];
function eqNull(x) { return x == null; }
function nullEq(x) { return null == x; }
function neNull(x) { return x != null; }
function eqUndefined(x) { return x == undefined; }
function eqVoid(x) { return x == void 0; }
function voidNe(x) { return void 0 != x; }
function guard(x) { return x != null && typeof x == "object" ? "o" : x == null ? "n" : "p"; }
function bit(f, x) { var r; try { r = f(x); r = r === true ? "1" : r === false ? "0" : String(r); } catch (e) { r = e.constructor.name; } return r; }
function all(label, value) {
  var r = label + ":" + [eqNull, nullEq, neNull, eqUndefined, eqVoid, voidNe, guard].map(function (f) { return bit(f, value); }).join("");
  if (calls.length) { r += "<" + calls.join("") + ">"; calls.length = 0; }
  log.push(r);
}
for (var i = 0; i < 3; i++) { all("null", null); all("undefined", undefined); all("object", {}); all("number", i); }
all("zero", 0); all("neg0", -0); all("nan", NaN); all("empty", ""); all("text", "null"); all("false", false); all("true", true);
all("bigint", 0n); all("symbol", Symbol()); all("array", []); all("function", function () {}); all("arrow", function () { return null; });
all("valueOf", { valueOf: function () { calls.push("v"); return null; }, toString: function () { calls.push("t"); return "undefined"; } });
all("toPrimitive", { [Symbol.toPrimitive]: function () { calls.push("p"); return undefined; } });
all("proxy", new Proxy({}, { get: function () { calls.push("g"); return undefined; }, has: function () { calls.push("h"); return false; } }));
all("callableProxy", new Proxy(function () {}, { get: function () { calls.push("g"); return undefined; } }));
all("bound", function () {}.bind(null)); all("nullProto", Object.create(null)); all("stringObject", new String("")); all("class", class {});
all("again", null); all("again", 1);
// Both operands evaluate, in order, when one of them is `void` syntax.
var order = [];
function tick(tag, v) { order.push(tag); return v; }
log.push(tick(1, null) == void tick(2, 0), void tick(3, 0) == tick(4, 0), void tick(5, 0) != void tick(6, 0), null == undefined, undefined != null, order.join(""));
var thrown; try { log.push(void (function () { throw new RangeError("x"); })() == null); } catch (e) { thrown = e.constructor.name; }
log.push(thrown);
print(log.join());
"##,
        "null:110110n,undefined:110110n,object:001001o,number:001001p,null:110110n,undefined:110110n,object:001001o,number:001001p,null:110110n,undefined:110110n,object:001001o,number:001001p,zero:001001p,neg0:001001p,nan:001001p,empty:001001p,text:001001p,false:001001p,true:001001p,bigint:001001p,symbol:001001p,array:001001o,function:001001p,arrow:001001p,valueOf:001001o,toPrimitive:001001o,proxy:001001o,callableProxy:001001p,bound:001001p,nullProto:001001o,stringObject:001001o,class:001001p,again:110110n,again:001001p,true,false,false,true,false,123456,RangeError",
    );
}

#[test]
fn computed_additions_find_their_child_shape_in_any_order_of_asking() {
    // `o[k] = v` with an unknown String key adds through the parent
    // shape's set of children, which a found child reorders. Objects built
    // with many first keys, in changing orders and frequencies, keep their
    // own key order and values; accessors, read-only and non-extensible
    // receivers still refuse.
    run_as_script_function_and_module(
        r##"
var log = [];
var refused = [];
function build(keys, base) { "use strict"; var o = base || {}; for (var i = 0; i < keys.length; i++) { try { o[keys[i]] = keys[i] + i; } catch (e) { refused.push(keys[i]); } } return o; }
function describe(o) { var parts = []; for (var k in o) parts.push(k + "=" + o[k]); return parts.join("&"); }
var names = [];
for (var n = 0; n < 14; n++) names.push("k" + n);
// One parent shape gets a child for each first key; the same additions are
// then asked for again in other orders and frequencies.
var seen = [];
for (var round = 0; round < 7; round++) {
  for (var start = 0; start < names.length; start++) {
    var keys = [names[(start * 5 + round) % 14], names[(start + round * 3) % 14], names[(start * 3 + round + 1) % 14], names[(start + 7) % 14]];
    var o = build(keys);
    var unique = keys.filter(function (k, i) { return keys.indexOf(k) === i; });
    var expected = unique.map(function (k) { return k + "=" + k + keys.lastIndexOf(k); }).join("&");
    seen.push(describe(o) === expected && Object.keys(o).join() === unique.join() && JSON.stringify(o) === JSON.stringify(build(keys)) ? 1 : 0);
  }
}
log.push(seen.join(""));
// A hot first key among cold ones, on plain objects and on objects with a
// prototype, an accessor or a frozen ancestor in the way.
var hot = [];
for (var h = 0; h < 40; h++) hot.push(describe(build([h % 4 === 3 ? names[h % 14] : "hot", "tail" + (h % 3)])));
log.push(hot.filter(function (text, i) { return hot.indexOf(text) === i; }).join("|"));
var calls = [];
var proto = { set k3(v) { calls.push("set" + v); }, get k3() { return "getter"; } };
Object.defineProperty(proto, "k5", { value: "fixed", writable: false, enumerable: true });
var inherited = build(["k1", "k3", "k5", "k7"], Object.create(proto));
log.push(describe(inherited), Object.keys(inherited).join(), calls.join());
var sealed = Object.preventExtensions(build(["k2"]));
build(["k2", "k4"], sealed);
log.push(describe(sealed), describe(build(["k4", "k2"], Object.freeze(build(["k4"])))));
build(["k9"], Object.freeze({}));
log.push(refused.join(), describe(build(["__proto__", "k1"])), describe(build(["1", "k1", "0"])), describe(build(["k1", "k1", "k1"])));
print(log.join());
"##,
        "11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111,hot=hot0&tail0=tail01|hot=hot0&tail1=tail11|hot=hot0&tail2=tail21|k3=k30&tail0=tail01|k7=k70&tail1=tail11|k11=k110&tail2=tail21|k1=k10&tail0=tail01|k5=k50&tail1=tail11|k9=k90&tail2=tail21|k13=k130&tail0=tail01|k3=k30&tail1=tail11|k7=k70&tail2=tail21|k11=k110&tail0=tail01,k1=k10&k7=k73&k3=getter&k5=fixed,k1,k7,setk31,k2=k20,k4=k40,k5,k4,k4,k2,k9,k1=k11,0=02&1=10&k1=k11,k1=k12",
    );
}

#[test]
fn a_string_index_compares_one_code_unit_only_inside_a_string_primitive() {
    // `s[0] === "$"` and its relatives, warmed with Strings and then given
    // short Strings, wrappers, Arrays, accessors, Proxies and typed arrays;
    // an index past the end reads String.prototype and Object.prototype.
    run_as_script_function_and_module(
        r##"
var log = [];
var calls = [];
function first(s) { return s[0] === "$"; }
function swapped(s) { return "$" === s[0]; }
function notFirst(s) { return s[0] !== "$"; }
function at(s, i) { return s[i] === "b"; }
function last(s) { return s[s.length - 1] === "Z"; }
function two(s) { return s[0] === "$$"; }
function none(s) { return s[0] === ""; }
function high(s) { return s[1] === "\ude00"; }
function data(t) { return typeof t != "string" || t[4] !== ":" || t[0] !== "d" && t[0] !== "D" ? "no" : "data"; }
function bit(f, a, b) { var r; try { r = f(a, b); r = r === true ? "1" : r === false ? "0" : String(r); } catch (e) { r = e.constructor.name; } return r; }
function all(label, value, index) {
  var r = label + ":" + [first, swapped, notFirst, last, two, none, high].map(function (f) { return bit(f, value); }).join("") + bit(at, value, index) + "/" + bit(data, value);
  if (calls.length) { r += "<" + calls.join("") + ">"; calls.length = 0; }
  log.push(r);
}
for (var i = 0; i < 3; i++) { all("hit", "$b" + i, 1); all("miss", "ab" + i, 1); }
all("empty", "", 0); all("dollar", "$", 0); all("endZ", "abZ", 1); all("data", "data:x", 3); all("Data", "Data:b", 5); all("pair", "\ud83d\ude00", 0);
all("minusZero", "b$", -0); all("fraction", "bb", 0.5); all("negative", "bb", -1); all("nan", "bb", NaN); all("past", "bb", 2); all("huge", "bb", 4294967296); all("infinity", "bb", Infinity);
all("stringIndex", "ab", "1"); all("nonCanonical", "ab", "01"); all("booleanIndex", "ab", true); all("nullIndex", "ab", null);
all("objectIndex", "ab", { toString: function () { calls.push("k"); return "1"; } });
all("symbolIndex", "ab", Symbol.iterator); all("lengthIndex", "b", "length");
all("number", 5, 0); all("true", true, 0); all("bigint", 5n, 0); all("symbol", Symbol("$"), 0);
all("null", null, 0); all("undefined", undefined, 0);
all("wrapper", new String("$b"), 1); all("array", ["$", "b"], 1); all("arrayOfNumbers", [36, 98], 1); all("holey", [, "b"], 1);
all("object", { 0: "$", 1: "b", length: 2 }, 1);
all("getter", { get 0() { calls.push("g"); return "$"; }, get 1() { calls.push("h"); return "b"; }, get length() { calls.push("l"); return 1; } }, 1);
all("proxy", new Proxy({}, { get: function (t, k) { calls.push(typeof k === "symbol" ? "S" : k); return "$"; } }), 1);
all("typed", new Uint8Array([36, 98]), 1); all("arguments", (function () { return arguments; })("$", "b"), 1);
// An index past the end reads the prototype chain.
String.prototype[5] = "b"; Object.prototype[7] = "b"; String.prototype[0] = "$";
log.push(bit(at, "ab", 5), bit(at, "ab", 7), bit(at, "abcdefgh", 5), bit(at, "abcdefgh", 7), bit(first, ""), bit(first, "x"), bit(at, 12, 5));
delete String.prototype[5]; delete Object.prototype[7]; delete String.prototype[0];
log.push(bit(at, "ab", 5), bit(first, ""));
all("again", "$b", 1); all("again", "xy", 1);
// The base and the key evaluate once, in order, before the read.
var order = [];
function tick(tag, v) { order.push(tag); return v; }
log.push(tick(1, "$a")[tick(2, 0)] === "$", "a" === tick(3, "$a")[tick(4, 1)], tick(5, null) === null ? "ok" : 0, order.join(""));
var thrown; try { tick(6, null)[tick(7, 0)] === "$"; } catch (e) { thrown = e.constructor.name + order.join(""); }
log.push(thrown);
print(log.join());
"##,
        "hit:11000001/no,miss:00100001/no,hit:11000001/no,miss:00100001/no,hit:11000001/no,miss:00100001/no,empty:00100000/no,dollar:11000000/no,endZ:00110001/no,data:00100000/data,Data:00100001/data,pair:00100010/no,minusZero:00100001/no,fraction:00100000/no,negative:00100000/no,nan:00100000/no,past:00100000/no,huge:00100000/no,infinity:00100000/no,stringIndex:00100001/no,nonCanonical:00100000/no,booleanIndex:00100000/no,nullIndex:00100000/no,objectIndex:00100001/no<k>,symbolIndex:00100000/no,lengthIndex:00100000/no,number:00100000/no,true:00100000/no,bigint:00100000/no,symbol:00100000/no,null:TypeErrorTypeErrorTypeErrorTypeErrorTypeErrorTypeErrorTypeErrorTypeError/no,undefined:TypeErrorTypeErrorTypeErrorTypeErrorTypeErrorTypeErrorTypeErrorTypeError/no,wrapper:11000001/no,array:11000001/no,arrayOfNumbers:00100000/no,holey:00100001/no,object:11000001/no,getter:11000001/no<ggglggghh>,proxy:11000000/no<000lengthNaN0011>,typed:00100000/no,arguments:11000001/no,1,1,0,0,1,0,0,0,0,again:11000001/no,again:00100000/no,true,true,ok,12345,TypeError1234567",
    );
}
