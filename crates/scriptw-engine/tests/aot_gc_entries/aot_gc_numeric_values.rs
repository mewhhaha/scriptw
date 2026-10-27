use scriptw_engine::{CompileOptions, Engine, ExecutionBackend, RealmBuilder, RunOptions};

fn run_boolean(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let engine = Engine::new(RealmBuilder::new().build());
    let outcome = engine
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("GC numeric control executes through Wasm");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn gc_bigint_arithmetic_retains_exact_magnitude_and_arbitrary_exponents() {
    run_boolean(
        r#"
var wide = (1n << 129n) + 7n;
var huge = 1n << 128n;
var failed = false;
try { 1n ** -huge; } catch (error) { failed = error instanceof RangeError; }
((wide * 3n + 2n) / wide === 3n && (wide * 3n + 2n) % wide === 2n &&
 (-wide * 3n - 2n) / wide === -3n && (-wide * 3n - 2n) % wide === -2n &&
 ((wide ^ -1n) === ~wide) && ((wide << -5n) === (wide >> 5n)) &&
 (-wide >> 10000n) === -1n && (wide >> huge) === 0n &&
 1n ** huge === 1n && (-1n) ** huge === 1n && (-1n) ** (huge + 1n) === -1n &&
 0n ** huge === 0n && failed &&
 BigInt(Number.MAX_VALUE) === ((1n << 1024n) - (1n << 971n)) && BigInt(-0) === 0n);
"#,
    );
}

#[test]
fn gc_bigint_fixed_width_coerces_in_order_and_retains_small_large_width_results() {
    run_boolean(
        r#"
var log = [];
var width = { valueOf: function() { log.push("bits"); return 65; } };
var value = { valueOf: function() { log.push("value"); return (1n << 64n) + 3n; } };
var signed = BigInt.asIntN(width, value);
var zeroRejected = false;
try { BigInt.asUintN(0, 1); } catch (error) { zeroRejected = error instanceof TypeError; }
var marker = {};
var widthThrow = { valueOf: function() { throw marker; } };
var untouched = true;
var rejectedValue = { valueOf: function() { untouched = false; return 1n; } };
var retainedThrow = false;
try { BigInt.asIntN(widthThrow, rejectedValue); } catch (error) { retainedThrow = error === marker; }
(signed === -(1n << 64n) + 3n && log.join(",") === "bits,value" &&
 BigInt.asUintN(65, -1n) === (1n << 65n) - 1n && BigInt.asIntN(65, -1n) === -1n &&
 BigInt.asIntN(0, -7n) === 0n && BigInt.asUintN(0, -7n) === 0n &&
 BigInt.asIntN(2 ** 40, 1n) === 1n && BigInt.asIntN(2 ** 40, -1n) === -1n &&
 BigInt.asUintN(2 ** 40, 1n) === 1n && zeroRejected && retainedThrow && untouched);
"#,
    );
}

#[test]
fn gc_number_brand_and_constructor_hooks_preserve_whole_completions() {
    run_boolean(
        r#"
var log = [];
var marker = {};
var input = { valueOf: function() { log.push("value"); return 7n; } };
function Target() {}
var target = new Proxy(Target, { get: function(object, key) {
  if (key === "prototype") { log.push("prototype"); throw marker; }
  return object[key];
} });
var whole = false;
try { Reflect.construct(Number, [input], target); } catch (error) { whole = error === marker; }
var hooked = false;
var digits = { valueOf: function() { hooked = true; return 2; } };
var brand = false;
try { Number.prototype.toFixed.call({}, digits); } catch (error) { brand = error instanceof TypeError; }
var probe = { valueOf: function() { hooked = true; return 1; } };
var predicates = !Number.isFinite(probe) && !Number.isNaN(probe) &&
  !Number.isInteger(probe) && !Number.isSafeInteger(probe);
(whole && log.join(",") === "value,prototype" && brand && !hooked && predicates &&
 Number() === 0 && Number.isNaN(Number(undefined)) && Number(1n << 64n) === 2 ** 64 &&
 Number.prototype.valueOf.call(new Number(-0)) === 0 &&
 Object.is(Number.prototype.valueOf.call(new Number(-0)), -0));
"#,
    );
}

#[test]
fn gc_primitive_boxes_keep_brand_and_constructor_prototype_identity() {
    run_boolean(
        r#"
var called = false;
var input = { valueOf: function() { called = true; throw 1; } };
function Target() {}
var boxed = Reflect.construct(Boolean, [input], Target);
var wrongNumber = false;
var wrongBoolean = false;
try { Number.prototype.valueOf.call(boxed); } catch (error) { wrongNumber = error instanceof TypeError; }
try { Boolean.prototype.valueOf.call(new Number(1)); } catch (error) { wrongBoolean = error instanceof TypeError; }
(Object.getPrototypeOf(boxed) === Target.prototype &&
 Boolean.prototype.valueOf.call(boxed) === true && Boolean.prototype.toString.call(boxed) === "true" &&
 Boolean.prototype.valueOf.call(Boolean.prototype) === false &&
 Boolean.prototype.toString.call(Boolean.prototype) === "false" &&
 !Boolean(0n) && Boolean(1n << 100n) && !Boolean() && Boolean(input) &&
 wrongNumber && wrongBoolean && !called);
"#,
    );
}

#[test]
fn gc_math_argument_hooks_preserve_order_and_original_throw_values() {
    run_boolean(
        r#"
var marker = {};
var laterCalls = 0;
var first = { valueOf: function() { throw marker; } };
var later = { valueOf: function() { laterCalls++; return 2; } };
var retained = 0;
try { Math.pow(first, later); } catch (error) { if (error === marker) retained++; }
try { Math.atan2(first, later); } catch (error) { if (error === marker) retained++; }
try { Math.imul(first, later); } catch (error) { if (error === marker) retained++; }
try { Math.sqrt(first); } catch (error) { if (error === marker) retained++; }
try { Math.max(NaN, first, later); } catch (error) { if (error === marker) retained++; }
try { Math.hypot(Infinity, first, later); } catch (error) { if (error === marker) retained++; }
var sequence = [];
var a = { valueOf: function() { sequence.push("a"); return NaN; } };
var b = { valueOf: function() { sequence.push("b"); return Infinity; } };
(retained === 6 && laterCalls === 0 && Math.hypot(a, b) === Infinity &&
 sequence.join(",") === "a,b" && Object.is(Math.min(0, -0), -0) &&
 Object.is(Math.max(-0, 0), 0) && Object.is(Math.round(-0.1), -0) &&
 Object.is(Math.sign(-0), -0) && Math.imul(4294967295, 2) === -2);
"#,
    );
}

#[test]
fn gc_math_sum_precise_rounds_once_and_preserves_iterator_error_precedence() {
    run_boolean(
        r#"
var marker = {};
var closeCalls = 0;
var coercions = 0;
var index = 0;
var bad = { valueOf: function() { coercions++; return 2; } };
var source = {};
source[Symbol.iterator] = function() {
  return {
    next: function() { return { value: index++ === 0 ? 1 : bad, done: false }; },
    return: function() { closeCalls++; throw marker; }
  };
};
var rejected = false;
try { Math.sumPrecise(source); } catch (error) { rejected = error instanceof TypeError; }
var valueCloseCalls = 0;
var valueSource = {};
valueSource[Symbol.iterator] = function() {
  return {
    next: function() { return { get value() { throw marker; }, done: false }; },
    return: function() { valueCloseCalls++; return {}; }
  };
};
var retained = false;
try { Math.sumPrecise(valueSource); } catch (error) { retained = error === marker; }
var nextGets = 0;
var nextIndex = 0;
var cached = {};
cached[Symbol.iterator] = function() {
  return { get next() {
    nextGets++;
    return function() { return { value: ++nextIndex, done: nextIndex > 3 }; };
  } };
};
(rejected && closeCalls === 1 && coercions === 0 && retained && valueCloseCalls === 0 &&
 Math.sumPrecise(cached) === 6 && nextGets === 1 &&
 Math.sumPrecise([1e16, 1, -1e16]) === 1 &&
 Math.sumPrecise([Number.MAX_VALUE, Number.MAX_VALUE, -Number.MAX_VALUE]) === Number.MAX_VALUE &&
 Math.sumPrecise([Number.MIN_VALUE, Number.MIN_VALUE]) === Number.MIN_VALUE * 2 &&
 Math.sumPrecise([1, 2 ** -53]) === 1 &&
 Math.sumPrecise([1, 3 * (2 ** -53)]) === 1 + 2 ** -51 &&
 Object.is(Math.sumPrecise([]), -0) && Object.is(Math.sumPrecise([-0, -0]), -0) &&
 Object.is(Math.sumPrecise([-0, 0]), 0) && Number.isNaN(Math.sumPrecise([Infinity, -Infinity])));
"#,
    );
}

#[test]
fn gc_math_f16round_shares_exact_binary16_ties_and_signed_zero() {
    run_boolean(
        r#"
(Object.is(Math.f16round(-0), -0) && Math.f16round(2 ** -25) === 0 &&
 Math.f16round(2 ** -24) === 2 ** -24 && Math.f16round(1 + 2 ** -11) === 1 &&
 Math.f16round(1 + 3 * (2 ** -11)) === 1 + 2 ** -9 &&
 Math.f16round(65520) === Infinity && Math.f16round(-65520) === -Infinity &&
 Number.isNaN(Math.f16round(NaN)));
"#,
    );
}

#[test]
fn gc_coercing_global_numeric_calls_retain_arguments_and_throw_identity() {
    run_boolean(
        r#"
var marker = {};
var log = [];
var input = { valueOf: function() { log.push("coerce"); throw marker; } };
function ignored() { log.push("extra"); return {}; }
var retained = 0;
try { isFinite(input, ignored()); } catch (error) { if (error === marker) retained++; }
try { globalThis.isNaN(input, ignored()); } catch (error) { if (error === marker) retained++; }
var bigintRejected = false;
try { isFinite(0n); } catch (error) { bigintRejected = error instanceof TypeError; }
(retained === 2 && log.join(",") === "extra,coerce,extra,coerce" &&
 bigintRejected && isFinite("  ") && isFinite(null) && !isFinite(undefined) &&
 isNaN(undefined) && !isNaN("42") && !Number.isFinite("42") && !Number.isNaN(undefined));
"#,
    );
}

#[test]
fn gc_numeric_locale_format_retains_primitive_and_intrinsic_call_identity() {
    run_boolean(
        r#"
var original = Intl.NumberFormat;
var getter = Object.getOwnPropertyDescriptor(original.prototype, "format");
Intl.NumberFormat = function() { throw "public constructor read"; };
Object.defineProperty(original.prototype, "format", { get: function() { throw "public getter read"; }, configurable: true });
var wide = (1n << 100n).toLocaleString("en-US");
var zero = (-0).toLocaleString("en-US");
var marker = {};
var untouched = true;
var options = { get style() { throw marker; } };
var retained = false;
try { (12).toLocaleString("en-US", options); } catch (error) { retained = error === marker; }
var locales = { get length() { untouched = false; return 0; } };
var wrongBrand = false;
try { BigInt.prototype.toLocaleString.call(new Number(1), locales); } catch (error) { wrongBrand = error instanceof TypeError; }
Intl.NumberFormat = original;
Object.defineProperty(original.prototype, "format", getter);
(wide === "1,267,650,600,228,229,401,496,703,205,376" && zero === "-0" &&
 retained && wrongBrand && untouched);
"#,
    );
}

#[test]
fn gc_number_strings_keep_safe_integer_and_shortest_decimal_boundaries() {
    run_boolean(
        r#"
var cases = [
 [0,'0'],[-0,'0'],[1,'1'],[-1,'-1'],[9,'9'],[10,'10'],[-10,'-10'],
 [999999999999999,'999999999999999'],[1000000000000000,'1000000000000000'],
 [4503599627370495.5,'4503599627370495.5'],[-4503599627370495.5,'-4503599627370495.5'],
 [9007199254740990,'9007199254740990'],[-9007199254740990,'-9007199254740990'],
 [9007199254740991,'9007199254740991'],[-9007199254740991,'-9007199254740991'],
 [9007199254740992,'9007199254740992'],[-9007199254740992,'-9007199254740992'],
 [9007199254740994,'9007199254740994'],[-9007199254740994,'-9007199254740994'],
 [1000000000000000128,'1000000000000000100'],[-1000000000000000128,'-1000000000000000100'],
 [0.5,'0.5'],[-0.5,'-0.5'],[1.0000000000000002,'1.0000000000000002'],
 [0.9999999999999999,'0.9999999999999999'],[5e-324,'5e-324'],[-5e-324,'-5e-324'],
 [1e-323,'1e-323'],[1e21,'1e+21'],[1.7976931348623157e308,'1.7976931348623157e+308'],
 [Infinity,'Infinity'],[-Infinity,'-Infinity'],[NaN,'NaN']
];
var ok=true;
for(var i=0;i<cases.length;i++) {
 var n=cases[i][0], expected=cases[i][1];
 ok=ok && String(n)===expected && (''+n)===expected && [n].join('')===expected;
 var object={};object[n]='value';ok=ok && Object.keys(object)[0]===expected;
}
ok;
"#,
    );
}

#[test]
fn gc_runtime_safe_integer_decimal_matches_rust_integer_oracle() {
    let mut values = vec![
        0i64,
        1,
        -1,
        9,
        -9,
        10,
        -10,
        99,
        -99,
        100,
        -100,
        9_007_199_254_740_991,
        -9_007_199_254_740_991,
    ];
    for power in 1..=15 {
        let magnitude = 10i64.pow(power);
        for offset in [-1, 0, 1] {
            values.push(magnitude + offset);
            values.push(-magnitude - offset);
        }
    }
    let mut state = 0x1234_5678_9abc_def0u64;
    for _ in 0..256 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let value = (state % 9_007_199_254_740_992) as i64;
        values.push(value);
        values.push(-value);
    }
    let pairs = values
        .iter()
        .map(|value| format!("[{value},'{value}']"))
        .collect::<Vec<_>>()
        .join(",");
    run_boolean(&format!(
        "var pairs=[{pairs}],ok=true;function convert(n){{return String(n);}}for(var i=0;i<pairs.length;i++){{ok=ok&&convert(pairs[i][0])===pairs[i][1];}}ok;"
    ));
}
