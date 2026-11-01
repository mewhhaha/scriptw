use super::*;

#[test]
fn wasm_backend_returns_rooted_string_code_units_without_a_scalar_address() {
    let (scalar, tag, completion, units) = run_wasm_raw("\",\";");
    assert_eq!(tag.value_kind(), ValueKind::String);
    assert_eq!(completion, 0);
    assert_eq!(scalar, 0);
    assert_eq!(
        &*units.expect("rooted String code units"),
        &[u16::from(b',')]
    );
}

#[test]
fn wasm_backend_standalone_primitives_preserve_utf16_and_number_completions() {
    for (source, expected) in [
        ("'';", &[][..]),
        ("'a\\0b';", &[0x0061, 0x0000, 0x0062][..]),
        ("'\\u{1f600}';", &[0xd83d, 0xde00][..]),
        ("'\\ud800x\\udfff';", &[0xd800, 0x0078, 0xdfff][..]),
        (
            "'\\u{f0000}D800';",
            &[0xdb80, 0xdc00, 0x0044, 0x0038, 0x0030, 0x0030][..],
        ),
        ("'' ? 'wrong' : '\\ud800';", &[0xd800][..]),
        ("'x' && '\\udfff';", &[0xdfff][..]),
        ("('ignored', '\\0');", &[0x0000][..]),
    ] {
        let (scalar, tag, completion, units) = run_wasm_raw(source);
        assert_eq!(tag.value_kind(), ValueKind::String, "{source}");
        assert_eq!(completion, 0, "{source}");
        assert_eq!(scalar, 0, "{source}");
        assert_eq!(
            &*units.expect("rooted String code units"),
            expected,
            "{source}"
        );
    }
    for (source, expected) in [
        ("-0;", -0.0f64),
        ("(-0, 0, -0);", -0.0),
        ("'' ? 1 : -0;", -0.0),
        ("1 / 0;", f64::INFINITY),
        ("1 / -0;", f64::NEG_INFINITY),
        ("(1 + 2) * 3;", 9.0),
        ("0 / 0;", f64::NAN),
    ] {
        let (scalar, tag, completion, units) = run_wasm_raw(source);
        assert_eq!(tag.value_kind(), ValueKind::Number, "{source}");
        assert_eq!(completion, 0, "{source}");
        assert!(units.is_none(), "{source}");
        if expected.is_nan() {
            assert!(f64::from_bits(scalar as u64).is_nan(), "{source}");
        } else {
            assert_eq!(scalar as u64, expected.to_bits(), "{source}");
        }
    }
    for (source, expected) in [("!'';", 1), ("!'x';", 0), ("!!'\\0';", 1)] {
        let (scalar, tag, completion, units) = run_wasm_raw(source);
        assert_eq!(tag.value_kind(), ValueKind::Boolean, "{source}");
        assert_eq!(completion, 0, "{source}");
        assert_eq!(scalar, expected, "{source}");
        assert!(units.is_none(), "{source}");
    }
}

#[test]
fn wasm_backend_lowers_template_expression_legacy_octal_string_literal() {
    let outcome = engine()
        .run_script(
            "`${'\\07'}` === '\\u0007';",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("template expression legacy octal string literal should run");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_stringify_space_coerces_boxed_values_once_after_replacer_setup() {
    let outcome = engine()
        .run_script(
            r#"
var order = [];
var replacer = new Proxy(["outer"], {
  get: function(target, key) {
    if (key === "length" || key === "0") order.push("replacer:" + key);
    return target[key];
  }
});
var numberCalls = 0;
var numberReceiver = false;
var numberArgs = "";
var numberSpace = new Number(1);
var numberHook = function(hint) {
  numberCalls++;
  order.push("space:" + hint);
  return "2.9";
};
numberSpace[Symbol.toPrimitive] = new Proxy(numberHook, {
  apply: function(target, receiver, args) {
    numberReceiver = receiver === numberSpace;
    numberArgs = args.length + ":" + args[0];
    return target.apply(receiver, args);
  }
});
numberSpace.valueOf = function() { throw new Error("valueOf should not run"); };
var numberResult = JSON.stringify(
  { outer: { inner: 1 }, dropped: 2 },
  replacer,
  numberSpace
);

var stringCalls = 0;
var stringSpace = new String("ignored");
stringSpace.toString = function() {
  stringCalls++;
  return 3;
};
stringSpace.valueOf = function() { throw new Error("valueOf should not run"); };
var stringResult = JSON.stringify({ a: 1 }, null, stringSpace);

[
  numberResult === '{\n  "outer": {}\n}',
  order.join(","),
  numberCalls,
  numberReceiver,
  numberArgs,
  stringResult === '{\n3"a": 1\n}',
  stringCalls
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("JSON stringify should coerce boxed space once after replacer setup");
    assert!(
        outcome.note.contains(
            "string(true|replacer:length,replacer:0,space:number|1|true|1:number|true|1)"
        ),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_stringify_space_truncates_utf16_code_units() {
    let outcome = engine()
        .run_script(
            r#"
var bmpGap = "éééééé";
var bmpExpected = "{\n" + bmpGap + '"a": {\n' + bmpGap + bmpGap +
  '"b": 1\n' + bmpGap + "}\n}";
var astralGap = "12345678😀";
var astralExpected = "{\n" + astralGap + '"a": 1\n}';
var highSurrogateGap = "123456789\uD83D";
var highSurrogateExpected = "{\n" + highSurrogateGap + '"a": 1\n}';
[
  JSON.stringify({ a: { b: 1 } }, null, bmpGap) === bmpExpected,
  JSON.stringify({ a: 1 }, null, "12345678😀X") === astralExpected,
  JSON.stringify({ a: 1 }, null, "123456789😀") === highSurrogateExpected
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("JSON stringify should truncate space to ten UTF-16 code units");
    assert!(
        outcome.note.contains("string(true|true|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_stringify_space_preserves_abrupt_identity_and_error_realm() {
    let outcome = engine()
        .run_script(
            r#"
var getterSentinel = {};
var getterIdentity = false;
var getterSpace = new String("gap");
Object.defineProperty(getterSpace, "toString", {
  get: function() { throw getterSentinel; }
});
try {
  JSON.stringify({}, null, getterSpace);
} catch (error) {
  getterIdentity = error === getterSentinel;
}

var applySentinel = {};
var applyIdentity = false;
var applySpace = new Number(1);
applySpace.valueOf = new Proxy(function() { return 1; }, {
  apply: function() { throw applySentinel; }
});
try {
  JSON.stringify({}, null, applySpace);
} catch (error) {
  applyIdentity = error === applySentinel;
}

var revoked = Proxy.revocable(function() { return 1; }, {});
var revokedSpace = new Number(1);
revokedSpace.valueOf = revoked.proxy;
revoked.revoke();
var revokedTypeError = false;
try {
  JSON.stringify({}, null, revokedSpace);
} catch (error) {
  revokedTypeError = error instanceof TypeError;
}

var other = __scriptwCreateRealm().global;
var exhaustedSpace = new Number(1);
exhaustedSpace.valueOf = function() { return {}; };
exhaustedSpace.toString = function() { return {}; };
var exhaustedRealm = false;
try {
  other.JSON.stringify({}, null, exhaustedSpace);
} catch (error) {
  exhaustedRealm = Object.getPrototypeOf(error) === other.TypeError.prototype;
}

var symbolSpace = new String("ignored");
symbolSpace.toString = function() { return Symbol("gap"); };
var symbolRealm = false;
try {
  other.JSON.stringify({}, null, symbolSpace);
} catch (error) {
  symbolRealm = Object.getPrototypeOf(error) === other.TypeError.prototype;
}

var nonCallableSpace = new Number(1);
nonCallableSpace[Symbol.toPrimitive] = 0;
var nonCallableRealm = false;
try {
  other.JSON.stringify({}, null, nonCallableSpace);
} catch (error) {
  nonCallableRealm = Object.getPrototypeOf(error) === other.TypeError.prototype;
}

var proxyGets = 0;
var ignoredProxy = new Proxy(new Number(2), {
  get: function() {
    proxyGets++;
    throw new Error("Proxy space should not be coerced");
  }
});
var ignoredResult = JSON.stringify({ a: 1 }, null, ignoredProxy);

[
  getterIdentity,
  applyIdentity,
  revokedTypeError,
  exhaustedRealm,
  symbolRealm,
  nonCallableRealm,
  ignoredResult === '{"a":1}',
  proxyGets
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("JSON stringify space coercion should preserve abrupt identity and realm");
    assert!(
        outcome
            .note
            .contains("string(true|true|true|true|true|true|true|0)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_stringify_escapes_only_unpaired_utf16_surrogates() {
    let outcome = engine()
        .run_script(
            r#"
var high = "\uD800";
var low = "\uDC00";
var object = {};
object[high] = low;
var checks = [
  JSON.stringify(high) === '"\\ud800"',
  JSON.stringify("\uDBFF") === '"\\udbff"',
  JSON.stringify(low) === '"\\udc00"',
  JSON.stringify("\uDFFF") === '"\\udfff"',
  JSON.stringify("\uD834\uDF06") === '"𝌆"',
  JSON.stringify(high + low) === '"𐀀"',
  JSON.stringify(low + high) === '"\\udc00\\ud800"',
  JSON.stringify(high + "x") === '"\\ud800x"',
  JSON.stringify(object) === '{"\\ud800":"\\udc00"}',
  JSON.stringify({ toJSON: function() { return "\uD801"; } }) === '"\\ud801"',
  JSON.stringify(0, function(key, value) {
    return key === "" ? "\uDFFE" : value;
  }) === '"\\udffe"',
  JSON.stringify({
    toJSON: function() { return new String(high + low); }
  }) === '"𐀀"',
  JSON.stringify(JSON.rawJSON('"\\ud800"')) === '"\\ud800"'
];
var allPassed = true;
for (var i = 0; i < checks.length; i++) {
  if (!checks[i]) allPassed = false;
}
allPassed;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("JSON stringify should escape only unpaired UTF-16 surrogates");
    assert!(
        outcome.note.contains("boolean(true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_json_stringify_handles_heap_bigints_and_uses_its_defining_realm() {
    let outcome = engine()
            .run_script(
                r#"
var huge = 340282366920938463463374607431768211456n;
var negativeHuge = -340282366920938463463374607431768211456n;
var decimal = "340282366920938463463374607431768211456";
var other = __scriptwCreateRealm().global;
var crossRealmWrapped = other.Object(other.BigInt(100));
var crossRealmWrappedPrototype = Object.getPrototypeOf(crossRealmWrapped) === other.BigInt.prototype;
var mainRealmThrow = false;
var otherRealmThrow = false;
var crossRealmWrappedThrow = false;
var crossRealmWrappedConstructor = false;
try { JSON.stringify(huge); } catch (error) {
  mainRealmThrow = error instanceof TypeError;
}
try { other.JSON.stringify(huge); } catch (error) {
  otherRealmThrow = error instanceof other.TypeError && !(error instanceof TypeError);
}
try { JSON.stringify(crossRealmWrapped); } catch (error) {
  crossRealmWrappedThrow = error instanceof TypeError;
  crossRealmWrappedConstructor = error.constructor === TypeError;
}
BigInt.prototype.toJSON = function() { return this.toString(); };
var mainPrimitive = JSON.stringify(huge);
var mainBoxed = JSON.stringify(Object(huge));
var otherStillThrows = false;
try { other.JSON.stringify(huge); } catch (error) {
  otherStillThrows = error instanceof other.TypeError;
}
other.BigInt.prototype.toJSON = function() { return this.toString(); };
[
  mainRealmThrow,
  otherRealmThrow,
  mainPrimitive === '"' + decimal + '"',
  mainBoxed === '"' + decimal + '"',
  otherStillThrows,
  other.JSON.stringify(huge) === '"' + decimal + '"',
  crossRealmWrappedPrototype,
  crossRealmWrappedThrow,
  crossRealmWrappedConstructor,
  JSON.stringify(crossRealmWrapped) === '"100"',
  JSON.stringify(huge, function(key, value) {
    return typeof value === "bigint" ? value.toString() : value;
  }) === '"' + decimal + '"',
  Object(huge).valueOf() === huge,
  huge.toString(16) === "100000000000000000000000000000000",
  JSON.stringify(negativeHuge) === '"-' + decimal + '"',
  negativeHuge.toString(16) === "-100000000000000000000000000000000"
].join("|");
"#,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("JSON stringify should handle heap BigInts in its defining realm");
    assert!(
        outcome.note.contains(
            "string(true|true|true|true|true|true|true|true|true|true|true|true|true|true|true)"
        ),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_formats_local_date_strings_in_utc() {
    let outcome = engine()
        .run_script(
            r#"
var epoch = new Date(0);
if (epoch.toDateString() !== "Thu Jan 01 1970") throw "epoch date";
if (epoch.toTimeString() !== "00:00:00 GMT+0000 (Coordinated Universal Time)") {
  throw "epoch time";
}
if (epoch.toString() !== "Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)") {
  throw "epoch string";
}
var leapDay = new Date("2000-02-29T23:04:05.006Z");
if (leapDay.toDateString() !== "Tue Feb 29 2000") throw "leap date";
if (leapDay.toTimeString() !== "23:04:05 GMT+0000 (Coordinated Universal Time)") {
  throw "leap time";
}
var negativeYear = new Date("-000001-07-01T00:00:00.000Z");
if (negativeYear.toDateString().split(" ")[3] !== "-0001") throw "negative date year";
if (negativeYear.toString().split(" ")[3] !== "-0001") throw "negative string year";
var invalid = new Date(NaN);
if (invalid.toDateString() !== "Invalid Date") throw "invalid date";
if (invalid.toTimeString() !== "Invalid Date") throw "invalid time";
if (invalid.toString() !== "Invalid Date") throw "invalid string";
var methods = [
  ["toDateString", Date.prototype.toDateString],
  ["toTimeString", Date.prototype.toTimeString],
  ["toString", Date.prototype.toString]
];
var brandErrors = 0;
var constructErrors = 0;
for (var entry of methods) {
  var name = entry[0];
  var method = entry[1];
  if (method.name !== name || method.length !== 0) throw name + " metadata";
  var descriptor = Object.getOwnPropertyDescriptor(Date.prototype, name);
  if (!descriptor.writable || descriptor.enumerable || !descriptor.configurable) {
    throw name + " descriptor";
  }
  try { method.call({}); } catch (error) {
    if (error.name === "TypeError") brandErrors += 1;
  }
  try { new method(); } catch (error) {
    if (error.name === "TypeError") constructErrors += 1;
  }
}
if (brandErrors !== 3) throw "brand errors";
if (constructErrors !== 3) throw "construct errors";
262;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("Wasm Date string methods should use deterministic UTC formatting");
    assert!(outcome.note.contains("number(262"));
}

#[test]
fn wasm_backend_regexp_dot_all_matches_line_terminators() {
    let outcome = engine()
        .run_script(
            r#"[
  /^.$/s.test("\n"),
  /^.$/.test("\n"),
  /^.$/s.test("\u{10300}"),
  /^.$/su.test("\u{10300}")
].join("|");"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("dotAll matching should retain Unicode code-point behavior");
    assert!(
        outcome.note.contains("string(true|false|false|true)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_regexp_search_uses_compiled_named_groups() {
    let outcome = engine()
        .run_script(
            r#"[
  "xab".search(/(?<x>a)|(?<x>b)/),
  "xba".search(/(?<x>a)|(?<x>b)/)
].join("|");"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("RegExp search should execute compiled named-group programs");
    assert!(
        outcome.note.contains("string(1|1)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_regexp_open_capture_backreferences_match_empty() {
    let outcome = engine()
        .run_script(
            r#"
let named = /(?<a>\k<a>\w)../.exec("bab");
let numbered = /(\1\w)../.exec("bab");
[named[0], named.groups.a, numbered[0], numbered[1]].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("backreferences to open captures should match the empty string");
    assert!(
        outcome.note.contains("string(bab|b|bab|b)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_regexp_lookbehind_matches_in_reverse() {
    let outcome = engine()
        .run_script(
            r#"
let fixed = "abcdef".match(/(?<=\w{3})f/);
let greedy = "abcdef".match(/(?<=\w+)f/);
let negative = "abc123".match(/(?<!\d{3})c/);
let alternative = "xy".match(/(?<=(?<first>.)|(?<second>.))y/);
[fixed[0], greedy[0], negative[0], alternative.groups.first, alternative.groups.second].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("lookbehind should match its body in reverse without consuming input");
    assert!(
        outcome.note.contains("string(f|f|c|x|)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_regexp_identity_k_with_lookbehind_syntax() {
    for source in [
        r#"/\k<a>(?<=>)a/.test("k<a>a");"#,
        r#"/(?<=>)\k<a>/.test(">k<a>");"#,
        r#"/\k<a>(?<!a)a/.test("k<a>a");"#,
        r#"/(?<!a>)\k<a>/.test("k<a>");"#,
    ] {
        let outcome = engine()
            .run_script(
                source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .unwrap_or_else(|error| panic!("{source} should execute: {error:?}"));
        assert!(
            outcome.note.contains("boolean(true)"),
            "source: {source}; note: {}",
            outcome.note
        );
    }
}

#[test]
fn wasm_backend_regexp_exec_resets_global_last_index_after_failure() {
    let source = r#"
let regexp = /a/g;
let reads = 0;
regexp.lastIndex = {
  valueOf() {
    reads += 1;
    return 42;
  }
};
let beyondEnd;
let beyondEndError = "none";
try {
  beyondEnd = regexp.exec("abc");
} catch (error) {
  beyondEndError = error.name;
}
let beyondEndState = [beyondEndError, beyondEnd === null, regexp.lastIndex, reads].join(":");

regexp.lastIndex = {
  valueOf() {
    reads += 1;
    return -1;
  }
};
let noMatch;
let noMatchError = "none";
try {
  noMatch = regexp.exec("nbc");
} catch (error) {
  noMatchError = error.name;
}
let noMatchState = [noMatchError, noMatch === null, regexp.lastIndex, reads].join(":");

let results = [];
for (let candidate of [Infinity, 2 ** 32, 5]) {
  for (let expression of [/./g, /./y, /./gy]) {
    expression.lastIndex = candidate;
    try {
      results.push((expression.exec("test") === null) + ":" + expression.lastIndex);
    } catch (error) {
      results.push(error.name + ":" + expression.lastIndex);
    }
  }
}
[beyondEndState, noMatchState, results.join(",")].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("failed global and sticky RegExp exec calls should reset lastIndex");
    assert!(
            outcome.note.contains(
                "string(none:true:0:1|none:true:0:2|true:0,true:0,true:0,true:0,true:0,true:0,true:0,true:0,true:0)"
            ),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_regexp_exec_observes_last_index_once_and_writes_conditionally() {
    let source = r#"
function counter(reads) {
  return {
    valueOf() {
      reads.count += 1;
      return 0;
    }
  };
}

let failureReads = { count: 0 };
let failureCounter = counter(failureReads);
let nonglobalFailure = /a/;
nonglobalFailure.lastIndex = failureCounter;
let failed = nonglobalFailure.exec("nbc");

let successReads = { count: 0 };
let successCounter = counter(successReads);
let nonglobalSuccess = /./;
nonglobalSuccess.lastIndex = successCounter;
let succeeded = nonglobalSuccess.exec("abc");

let globalReads = { count: 0 };
let global = /./g;
global.lastIndex = counter(globalReads);
let globalResult = global.exec("abc");

let unicode = /./ug;
unicode.exec("𝌆");

[
  failed === null,
  nonglobalFailure.lastIndex === failureCounter,
  failureReads.count,
  succeeded[0],
  nonglobalSuccess.lastIndex === successCounter,
  successReads.count,
  globalResult[0],
  global.lastIndex,
  globalReads.count,
  unicode.lastIndex
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("RegExp exec should observe lastIndex with spec ordering");
    assert!(
        outcome
            .note
            .contains("string(true|true|1|a|true|1|a|1|1|2)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_adds_string_or_number_bindings_by_runtime_tag() {
    let outcome = engine()
        .run_script(
            r#"
function add(flag) {
  var value;
  if (flag) {
    value = 1;
  } else {
    value = "a";
  }
  return value + 1;
}
[add(true), add(false)].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("string-or-number bindings should use tagged addition");
    assert!(
        outcome.note.contains("string(2|a1)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_resolves_string_raw_as_a_live_tagged_template_method() {
    let outcome = engine()
        .run_script(
            r#"
let calls = 0;
let String = {
  raw(template, value) {
    calls++;
    return [this === String, template[0], template.raw[0], value, template[1]].join("|");
  }
};
[String.raw`cooked\n${7}tail`, calls].join(",");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("String.raw tagged calls should resolve the live method and preserve its receiver");
    assert!(
        outcome
            .note
            .contains("string(true|cooked\n|cooked\\n|7|tail,1)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_runtime_regexp_uses_lexical_source_with_finite_flags() {
    let outcome = engine()
        .run_script(
            r#"
let source = "(?<fst>.)(?<snd>.)|(?<thd>x)";
let result = "";
for (let flags of ["g", "gu"]) {
  let regexp = new RegExp(source, flags);
  result += "abcd".replace(regexp, "$<snd>$<fst>");
}
result;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("runtime RegExp should select lexical source and finite flags");
    assert!(
        outcome.note.contains("string(badcbadc)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_regexp_match_all_uses_compiled_named_groups() {
    let outcome = engine()
        .run_script(
            r#"
let iterator = "ba".matchAll(/(?<x>a)|(?<x>b)/g);
let first = iterator.next().value;
let second = iterator.next().value;
[
  first[0], first[1], first[2], first.groups.x,
  second[0], second[1], second[2], second.groups.x
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("RegExp matchAll should iterate compiled named-group programs");
    assert!(
        outcome.note.contains("string(b||b|b|a|a||a)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_string_concat_is_generic_and_orders_string_conversion() {
    let source = r#"
let order = [];
let receiver = {
  toString() {
    order.push("receiver");
    return "base";
  }
};
receiver.concat = String.prototype.concat;
let first = {
  toString() {
    order.push("first");
    return ":first";
  }
};
let second = {
  toString() {
    order.push("second");
    return ":second";
  }
};
receiver.concat(first, second) + "|" + order.join(",");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("String.prototype.concat should be generic and preserve conversion order");
    assert!(
        outcome
            .note
            .contains("string(base:first:second|receiver,first,second)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_regexp_substitution_checks_wide_length_before_allocating_result() {
    let source = r#"
let text = 'a'.repeat(1 << 20);
let replacement = '$1'.repeat(1 << 15);
let caught = false;
try {
  text.replace(/(.+)/g, replacement);
} catch (error) {
  caught = error instanceof RangeError;
}
let normal = 'aba'.replace(/(a)/g, '$1$1');
caught && normal === 'aabaa';
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("oversized substitution is a catchable JavaScript completion");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_regexp_substitution_keeps_named_capture_effects_and_utf16_units() {
    let source = r#"
let trace = [];
let count = 0;
let groups = {
  get name() {
    trace.push('get');
    return {
      toString() {
        trace.push('convert');
        if ('z'.replace(/(z)/, '$1') !== 'z') throw 'reentrant replacement failed';
        return ++count === 1 ? '\uD800' : '\uDC00';
      }
    };
  }
};
let result = ['a', 'x'];
result.index = 0;
result.groups = groups;
let matcher = { global: false, exec() { return result; } };
let value = RegExp.prototype[Symbol.replace].call(matcher, 'a', '$<name>$<name>$1$$');
let sentinel = {};
let reads = 0;
let converts = 0;
result.groups = {
  get name() {
    if (++reads === 2) throw sentinel;
    return { toString() { converts++; return 'first'; } };
  }
};
let abrupt = false;
try { RegExp.prototype[Symbol.replace].call(matcher, 'a', '$<name>$<name>'); }
catch (error) { abrupt = error === sentinel; }
value === '\uD800\uDC00x$' && trace.join(',') === 'get,convert,get,convert'
  && abrupt && reads === 2 && converts === 1;
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("substitution publishes only completed pieces without repeating user effects");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn wasm_backend_string_substring_treats_explicit_undefined_as_string_end() {
    let source = r#"
let caught = "none";
try {
  "abc".substring({ valueOf() { throw "start"; } }, undefined);
} catch (error) {
  caught = error;
}
["undefined".substring("e", undefined), caught].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("String.prototype.substring should handle undefined end and coercion throws");
    assert!(
        outcome.note.contains("string(undefined|start)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_string_from_char_code_handles_variadic_uint16_and_detached_calls() {
    let source = r#"
let fromCharCode = String.fromCharCode;
delete String.fromCharCode;
[
  fromCharCode(),
  fromCharCode(65, 66),
  fromCharCode(Infinity).charCodeAt(0),
  fromCharCode(-1.2).charCodeAt(0),
  fromCharCode.name,
  fromCharCode.length
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("String.fromCharCode should apply ToUint16 in detached variadic calls");
    assert!(
        outcome.note.contains("string(|AB|0|65535|fromCharCode|1)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_string_from_code_point_handles_variadic_unicode_and_abrupt_values() {
    let source = r#"
let failures = [];
try {
  String.fromCodePoint(3.5);
} catch (error) {
  failures.push(error.name);
}
try {
  String.fromCodePoint(65, { valueOf() { throw "coercion"; } });
} catch (error) {
  failures.push(error);
}
[
  String.fromCodePoint(65, 0x1D306),
  String.fromCodePoint(0x2F804),
  String.fromCodePoint.length,
  String.fromCodePoint.name,
  failures.join(",")
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("String.fromCodePoint should encode scalars and preserve abrupt conversions");
    assert!(
        outcome
            .note
            .contains("string(A𝌆|你|1|fromCodePoint|RangeError,coercion)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_string_raw_handles_templates_substitutions_and_abrupt_getters() {
    let source = r#"
let steps = [];
let template = {
  raw: {
    get length() { steps.push("length"); return 2; },
    get 0() { steps.push("first"); return "a"; },
    get 1() { steps.push("second"); throw "segment"; }
  }
};
let substitution = {
  toString() { steps.push("substitution"); return "b"; }
};
let caught = "none";
try {
  String.raw(template, substitution);
} catch (error) {
  caught = error;
}
[
  String.raw({ raw: ["a", "c"] }, substitution),
  String.raw`line\\n${"value"}`,
  String.raw.length,
  String.raw.name,
  steps.join(","),
  caught
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("String.raw should preserve raw segments and abrupt getter ordering");
    assert!(
        outcome.note.contains(
            "string(abc|line\\\\nvalue|1|raw|length,first,substitution,second,substitution|segment)"
        ),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_string_normalize_handles_all_forms_hangul_and_coercion() {
    let source = r#"
let decomposedHangul = "\u1100\u1161\u11A8";
[
  "\u1E9B\u0323".normalize("NFD") === "\u017F\u0323\u0307",
  "\uFB01".normalize("NFKC"),
  "\u00E9".normalize("NFKD") === "e\u0301",
  decomposedHangul.normalize("NFC"),
  "\uAC01".normalize("NFD") === decomposedHangul,
  "\uD800".normalize() === "\uD800",
  "\u00E9".normalize({ toString() { return "NFD"; } })
].join("|");
"#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("String.prototype.normalize should implement all Unicode normalization forms");
    assert!(
        outcome.note.contains("string(true|fi|true|각|true|true|é)"),
        "note: {}",
        outcome.note
    );
}

#[test]
fn wasm_backend_supports_dynamic_primitive_string_concat_and_equality() {
    for (source, expected) in [
        ("\"a\" + \"b\";", "string(ab)"),
        ("\"x\" + 1;", "string(x1)"),
        ("function f(x) { return \"v=\" + x; } f(3);", "string(v=3)"),
        ("(\"a\" + \"b\") === \"ab\";", "boolean(true)"),
    ] {
        let outcome = engine()
            .run_script(
                source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    ..RunOptions::default()
                },
            )
            .expect("string concat or equality should run");
        assert!(outcome.note.contains(expected));
    }
}

#[test]
fn wasm_backend_yields_regexp_literal_and_assigns_resumed_value() {
    let source = r#"
            let received;
            const sent = {};
            function* sequence() {
                received = yield/abc/i;
            }
            const iterator = sequence();
            const yielded = iterator.next();
            const completed = iterator.next(sent);
            yielded.value.test("ABC") === true && yielded.done === false
                && completed.value === undefined && completed.done === true
                && received === sent;
        "#;
    let outcome = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("regexp generator yield should resume: {err:?}"));
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}
