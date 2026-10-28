use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, RealmBuilder, RunOptions,
};

fn check_length(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
            source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("primitive length controls compile and execute through Wasm");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        outcome.note.contains("boolean(true)"),
        "{}\n{source}",
        outcome.note
    );
}

#[test]
fn primitive_lengths_count_utf16_and_keep_boxed_proxy_and_other_key_lookup() {
    check_length(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
function lengthOf(value) { return value.length; }
var text = 'a\u0000\ud800\udc00\udfff';
check(text.length === 5 && lengthOf(text) === 5, 'UTF16 units');
check(lengthOf('') === 0 && lengthOf('\ud800') === 1 && lengthOf('\udc00') === 1 &&
  lengthOf('\ud83d\ude00') === 2, 'empty, isolated and paired surrogates');
check(lengthOf(new String('abc')) === 3, 'boxed String own length');
var events = '';
var proxy = new Proxy(new String('abc'), {
  get: function(target, key, receiver) {
    events += String(key) + ',';
    return Reflect.get(target, key, receiver);
  }
});
check(lengthOf(proxy) === 3 && events === 'length,', 'Proxy length interception');
var violated = false;
try { lengthOf(new Proxy(new String('abc'), { get: function() { return 91; } })); }
catch (error) { violated = error instanceof TypeError; }
check(violated, 'boxed length Proxy invariant');
var object = { get length() { events += 'object,'; return 17; } };
check(lengthOf(object) === 17 && events === 'length,object,', 'object getter');
Object.defineProperty(Number.prototype, 'length', {
  get: function() { 'use strict'; check(this === 7, 'primitive getter receiver'); return 23; },
  configurable: true
});
check(lengthOf(7) === 23, 'nonstring prototype lookup');
check('abc'[Symbol.iterator] === String.prototype[Symbol.iterator] &&
  'abc'.missing === undefined, 'other keys');
var pair = Proxy.revocable(new String('abc'), {});
pair.revoke();
var threw = false;
try { lengthOf(pair.proxy); } catch (error) { threw = error instanceof TypeError; }
check(threw, 'revoked Proxy');
check(!Reflect.defineProperty(String.prototype, 'length', { value: 99 }) &&
  lengthOf('abcd') === 4, 'immutable String length');
true;
"#,
    );
}

#[test]
fn computed_length_keys_preserve_receiver_key_and_conversion_order() {
    check_length(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var events = '';
var text = 'abcd';
function receiver() { events += 'receiver,'; return text; }
function key() {
  events += 'key,';
  return { [Symbol.toPrimitive]: function(hint) {
    events += hint + ','; text = 'x'; return 'length';
  } };
}
check(receiver()[key()] === 4 && events === 'receiver,key,string,' && text === 'x',
  'saved receiver and computed conversion');
events = '';
var threw = false;
try { null[key()]; } catch (error) { threw = error instanceof TypeError; }
check(threw && events === 'key,', 'key evaluation precedes null check; conversion follows it');
events = '';
var marker = {}, caught = false;
try { 'abc'[{ [Symbol.toPrimitive]: function() { events += 'convert,'; throw marker; } }]; }
catch (error) { caught = error === marker; }
check(caught && events === 'convert,', 'key throw identity and conversion once');
var boxed = new String('abc');
check(boxed[{ toString: function() { events += 'boxed,'; return 'length'; } }] === 3 &&
  events === 'convert,boxed,', 'boxed computed key');
true;
"#,
    );
}

#[test]
fn concat_length_uses_actual_strings_and_preserves_nonstring_addition_results() {
    check_length(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
function addLength(left, right) { return (left + right).length; }
var leftCalls = 0, rightCalls = 0;
function left() { leftCalls++; return 'a\u0000\ud800'; }
function right() { rightCalls++; return '\udc00b'; }
check((left() + right()).length === 5 && leftCalls === 1 && rightCalls === 1,
  'once-evaluated UTF16 operands');
check(addLength('', '') === 0 && addLength('\ud800', '\udc00') === 2 &&
  addLength('x\u0000', '\udfff') === 3, 'UTF16 sum');
var primitiveCalls = 0;
String.prototype[Symbol.toPrimitive] = function() { primitiveCalls++; return 'changed'; };
check(addLength('a', 'bc') === 3 && primitiveCalls === 0, 'primitive strings ignore prototype conversion');
check(addLength(new String('a'), 'bc') === 9 && primitiveCalls === 1, 'boxed conversion remains observable');
var events = '';
Object.defineProperty(Number.prototype, 'length', {
  get: function() { 'use strict'; events += 'number,'; check(this === 5, 'Number result'); return 41; },
  configurable: true
});
Object.defineProperty(BigInt.prototype, 'length', {
  get: function() { 'use strict'; events += 'bigint,'; check(this === 5n, 'BigInt result'); return 43; },
  configurable: true
});
check(addLength(2, 3) === 41 && addLength(2n, 3n) === 43 &&
  events === 'number,bigint,', 'fallback reads actual nonstring result');
check(addLength('x', 2n) === 2 && addLength(2n, 'x') === 2, 'BigInt string conversion');
var object = { [Symbol.toPrimitive]: function(hint) { events += hint + ','; return 'old'; } };
function mutate() {
  events += 'rhs,';
  object[Symbol.toPrimitive] = function(hint) { events += 'new:' + hint + ','; return 'new'; };
  return 'R';
}
events = '';
check((object + mutate()).length === 4 && events === 'rhs,new:default,',
  'both operands precede ToPrimitive');
var size = 65536;
var a = 'a'.repeat(size), b = '\udfff'.repeat(size + 1);
check(addLength(a, b) === 2 * size + 1, 'large UTF16 sum');
true;
"#,
    );
}

#[test]
fn concat_length_fallback_preserves_throws_and_computed_outer_consumers() {
    check_length(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var marker = {}, events = '', caught = false;
function left() { events += 'left,'; throw marker; }
function right() { events += 'right,'; return 'r'; }
try { (left() + right()).length; } catch (error) { caught = error === marker; }
check(caught && events === 'left,', 'lhs throw and rhs not evaluated');
var object = { [Symbol.toPrimitive]: function(hint) { events += hint + ','; throw marker; } };
function failRight() { events += 'rhs,'; throw marker; }
events = '';
caught = false;
try { (object + failRight()).length; } catch (error) { caught = error === marker; }
check(caught && events === 'rhs,', 'rhs throw without earlier coercion');
events = '';
caught = false;
try { (object + right()).length; } catch (error) { caught = error === marker; }
check(caught && events === 'right,default,', 'ToPrimitive throw after rhs exactly once');
events = '';
var other = { [Symbol.toPrimitive]: function(hint) { events += hint + ','; return 's'; } };
var threw = false;
try { (Symbol('x') + other).length; } catch (error) { threw = error instanceof TypeError; }
check(threw && events === 'default,', 'both ToPrimitive calls precede Symbol ToString failure');
threw = false;
try { (1n + 2).length; } catch (error) { threw = error instanceof TypeError; }
check(threw, 'mixed numeric types');
events = '';
function computed() { events += 'key,'; return 'length'; }
check(('a' + other)[computed()] === 2 && events === 'default,key,',
  'computed outer key runs after complete addition');
Object.defineProperty(Number.prototype, 'length', {
  get: function() { throw marker; }, configurable: true
});
caught = false;
try { (other.valueOf = function() { return 2; }, 1 + Number(1)).length; }
catch (error) { caught = error === marker; }
check(caught, 'result getter throw');
true;
"#,
    );
}
