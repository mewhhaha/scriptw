use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, ObservedCompletion, RealmBuilder,
    RunOptions,
};

fn assert_trace(source: &str, expected: &[&str]) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("remainder must compile and execute through Wasm AOT");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}\noutput: {:?}\nsource:\n{source}",
        outcome.completion,
        outcome.output_events,
    );
    assert_eq!(
        outcome.output_events,
        expected
            .iter()
            .map(|line| HostOutputEvent::PrintLine((*line).to_string()))
            .collect::<Vec<_>>(),
        "source:\n{source}",
    );
}

fn number_source(number: f64) -> String {
    if number.is_nan() {
        "NaN".to_string()
    } else if number == f64::INFINITY {
        "Infinity".to_string()
    } else if number == f64::NEG_INFINITY {
        "-Infinity".to_string()
    } else {
        format!("{number:?}")
    }
}

#[test]
fn runtime_remainders_match_exact_finite_and_exceptional_number_results() {
    let values = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        2.0,
        -2.0,
        7.0,
        -7.0,
        9_007_199_254_740_991.0,
        -9_007_199_254_740_991.0,
        9_007_199_254_740_992.0,
        -9_007_199_254_740_992.0,
        9_007_199_254_740_994.0,
        -9_007_199_254_740_994.0,
        0.1,
        -0.1,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::from_bits(3),
        -f64::from_bits(3),
        f64::MIN_POSITIVE,
        f64::from_bits(0x0010_0000_0000_0001),
        f64::MAX,
        -f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];
    let mut pairs = values
        .into_iter()
        .flat_map(|left| values.into_iter().map(move |right| (left, right)))
        .collect::<Vec<_>>();
    // Fixed bit patterns exercise unequal exponent gaps and nontrivial low
    // significand bits. Rust's floating remainder supplies the expected value;
    // the JavaScript operands are loaded from a runtime typed-array buffer.
    let mut seed = 0x632b_e59b_d9b4_e019_u64;
    for _ in 0..64 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let left = f64::from_bits(seed);
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let right = f64::from_bits(seed);
        pairs.push((left, right));
    }
    let numbers = pairs
        .iter()
        .flat_map(|&(left, right)| [left, right, left % right])
        .map(number_source)
        .collect::<Vec<_>>()
        .join(",");
    assert_trace(
        &format!(
            r#"
var values = new Float64Array([{numbers}]);
function remainder(left, right) {{ return left % right; }}
function compound(left, right) {{ let value = +left; return value %= +right; }}
for (var index = 0; index < values.length; index += 3) {{
  var left = values[index], right = values[index + 1], expected = values[index + 2];
  if (!Object.is(remainder(left, right), expected)) throw new Error("remainder " + index);
  if (!Object.is(compound(left, right), expected)) throw new Error("compound " + index);
}}
print("exact");
"#
        ),
        &["exact"],
    );
}

#[test]
fn signed_zero_is_published_through_each_compound_reference_path() {
    assert_trace(
        r#"
var globalValue = -1;
print(Object.is(globalValue %= -1, -0) + ":" + Object.is(globalValue, -0));
function local() { let value = -1; print(Object.is(value %= -1, -0) + ":" + Object.is(value, -0)); }
local();
var receiver = {value: -1};
print(Object.is(receiver.value %= -1, -0) + ":" + Object.is(receiver.value, -0));
var withReceiver = {value: -1};
with (withReceiver) { print(Object.is(value %= -1, -0)); }
print(Object.is(withReceiver.value, -0));
function dynamic() {
  var value = -1;
  print(Object.is(eval("value %= -1"), -0) + ":" + Object.is(value, -0));
}
dynamic();
var stored;
var accessor = {
  get value() { print("get"); return -1; },
  set value(value) { print("set:" + Object.is(value, -0) + ":" + (this === accessor)); stored = value; }
};
print("result:" + Object.is(accessor.value %= -1, -0) + ":" + Object.is(stored, -0));
"#,
        &[
            "true:true",
            "true:true",
            "true:true",
            "true",
            "true",
            "true:true",
            "get",
            "set:true:true",
            "result:true:true",
        ],
    );
}

#[test]
fn nested_rhs_remainders_preserve_the_original_left_operand() {
    assert_trace(
        r#"
function nested(left, middle, right) { return left % (middle % right); }
print(nested(17, 8, 3));
print(nested(-17, 8, 3));
let value = 17;
print(value %= (value = 8, value % 3));
print(value);
var receiver = {value: 17};
print(receiver.value %= (receiver.value = 8, receiver.value % 3));
print(receiver.value);
function lhs() { print("lhs"); return 17; }
function rhs() { print("rhs"); return 8 % 3; }
print(lhs() % rhs());
"#,
        &["1", "-1", "1", "1", "1", "1", "lhs", "rhs", "1"],
    );
}

#[test]
fn coercions_and_abrupt_completions_precede_compound_publication() {
    assert_trace(
        r#"
var marker = {};
var left = { [Symbol.toPrimitive](hint) { print("left:" + hint); return 7; } };
var right = { [Symbol.toPrimitive](hint) { print("right:" + hint); return 3; } };
function leftExpression() { print("left expression"); return left; }
function rightExpression() { print("right expression"); return right; }
print(leftExpression() % rightExpression());
var stored = left;
var receiver = {
  get value() { print("get"); return stored; },
  set value(value) { print("set"); stored = value; }
};
right[Symbol.toPrimitive] = function() { print("right throw"); throw marker; };
try { receiver.value %= right; } catch (error) { print(error === marker); } finally { print("finally"); }
print(stored === left);
left[Symbol.toPrimitive] = function() { print("left throw"); throw marker; };
try { leftExpression() % rightExpression(); } catch (error) { print(error === marker); }
var symbol = Symbol();
try { symbol % rightExpression(); } catch (error) { print(error instanceof TypeError); }
var readonly = Object.defineProperty({}, "value", {value: -1});
(function() { "use strict";
  try { readonly.value %= -1; } catch (error) { print(error instanceof TypeError); }
})();
print(readonly.value);
"#,
        &[
            "left expression",
            "right expression",
            "left:number",
            "right:number",
            "1",
            "get",
            "left:number",
            "right throw",
            "true",
            "finally",
            "true",
            "left expression",
            "right expression",
            "left throw",
            "true",
            "right expression",
            "true",
            "true",
            "-1",
        ],
    );
}

#[test]
fn integer_and_exact_remainders_convert_each_operand_once_in_spec_order() {
    assert_trace(
        r#"
function check(left, right, expected) {
  var log = [];
  var lhs = { [Symbol.toPrimitive](hint) { log.push('left:' + hint); return left; } };
  var rhs = { [Symbol.toPrimitive](hint) { log.push('right:' + hint); return right; } };
  function leftExpression() { log.push('left expression'); return lhs; }
  function rightExpression() { log.push('right expression'); return rhs; }
  var actual = leftExpression() % rightExpression();
  if (!Object.is(actual, expected) || log.join(',') !== 'left expression,right expression,left:number,right:number') throw 'binary order';
  log = [];
  var stored = lhs;
  var target = {
    get value() { log.push('get'); return stored; },
    set value(value) { log.push('set'); stored = value; }
  };
  actual = target.value %= rightExpression();
  if (!Object.is(actual, expected) || !Object.is(stored, expected) || log.join(',') !== 'get,right expression,left:number,right:number,set') throw 'compound order';
}
check(-0, 2, -0);
check(-7, -3, -1);
check(9007199254740991, -2, 1);
check(-9007199254740991, 9007199254740991, -0);
check(9007199254740992, 3, 2);
check(7.5, -3, 1.5);
check(7, -0, NaN);
check(-5e-324, 2, -5e-324);
check(7, Infinity, 7);
check(Infinity, 2, NaN);
check(NaN, 2, NaN);
print('ordered');
"#,
        &["ordered"],
    );
}

#[test]
fn bigint_remainders_keep_the_numeric_kind_and_mixed_type_rules() {
    assert_trace(
        r#"
var left = { [Symbol.toPrimitive]() { print("big left"); return 9223372036854775809n; } };
var right = { [Symbol.toPrimitive]() { print("big right"); return 3n; } };
print(left % right);
var receiver = {value: -9223372036854775809n};
print(receiver.value %= 5n);
print(receiver.value);
var number = { [Symbol.toPrimitive]() { print("number right"); return 3; } };
try { left % number; } catch (error) { print(error instanceof TypeError); }
try { left % 0n; } catch (error) { print(error instanceof RangeError); }
"#,
        &[
            "big left",
            "big right",
            "0",
            "-4",
            "-4",
            "big left",
            "number right",
            "true",
            "big left",
            "true",
        ],
    );
}
