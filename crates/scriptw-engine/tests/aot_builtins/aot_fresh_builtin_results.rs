//! Builtin results that are fresh until returned (split's Array, iterator
//! result objects, Array.from's %Array% result, NumberToString's digits) keep
//! every observable ECMAScript property, and every exotic case keeps its
//! general path: Symbol.split dispatch, holes, accessors and Proxies in
//! Array iteration, and other constructors in Array.from.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, RealmBuilder, RunOptions,
};

fn check(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{HELPERS}\n{source}\ntrue;");
        let outcome = Engine::new(RealmBuilder::new().build())
            .run_script(
                &source,
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
            .expect("fresh builtin result controls compile and execute through Wasm");
        assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
        assert!(
            outcome.note.contains("boolean(true)"),
            "{}\n{source}",
            outcome.note
        );
    }
}

const HELPERS: &str = r#"
function check(value, label) { if (!value) throw new Error(label); }
function same(actual, expected) {
  if (actual.length !== expected.length) return false;
  for (var i = 0; i < expected.length; i++) {
    if (!(i in actual) || actual[i] !== expected[i]) return false;
  }
  return true;
}
function dynamic(text) { return text; }
"#;

#[test]
fn split_results_are_complete_ordinary_arrays_and_keep_symbol_split() {
    check(
        r#"
check(same('a,b,,c'.split(','), ['a', 'b', '', 'c']), 'one-unit separator');
check(same(',a,'.split(','), ['', 'a', '']), 'separators at both ends');
check(same('abc'.split(','), ['abc']), 'absent separator');
check(same(''.split(','), ['']), 'empty input');
check(''.split('').length === 0, 'empty input and separator');
check(same('abc'.split(''), ['a', 'b', 'c']), 'empty separator');
check(same('abc'.split(''.padEnd(0), 2), ['a', 'b']), 'empty separator with limit');
check(same('abc'.split(undefined), ['abc']), 'undefined separator');
check(same('a,b,c'.split(',', 2), ['a', 'b']), 'limit');
check('a,b'.split(',', 0).length === 0, 'zero limit');
check(same('a,b'.split(',', -1), ['a', 'b']), 'ToUint32 limit');
check(same('a--b--c'.split('--'), ['a', 'b', 'c']), 'two-unit separator');
check(same('aaa'.split('aa'), ['', 'a']), 'no overlapping matches');
check(same('a-b'.split('-b'), ['a', '']), 'separator at the end');
check(same('a\ud800b\ud800'.split('\ud800'), ['a', 'b', '']), 'lone surrogate unit');
var text = '';
for (var i = 0; i < 300; i++) text += i + ',';
var parts = text.split(dynamic(','));
check(parts.length === 301 && parts[0] === '0' && parts[64] === '64' &&
  parts[299] === '299' && parts[300] === '', 'many pieces');
var result = 'x,y'.split(',');
check(Array.isArray(result) && Object.getPrototypeOf(result) === Array.prototype, 'Array');
check(Object.isExtensible(result), 'extensible');
var length = Object.getOwnPropertyDescriptor(result, 'length');
check(length.value === 2 && length.writable && !length.enumerable && !length.configurable,
  'length descriptor');
var element = Object.getOwnPropertyDescriptor(result, 1);
check(element.value === 'y' && element.writable && element.enumerable && element.configurable,
  'element descriptor');
check(Object.keys(result).join() === '0,1' && !(2 in result), 'own keys');
result.push('z');
result[6] = 'w';
delete result[0];
check(result.length === 7 && result[2] === 'z' && result[6] === 'w' && !(0 in result) &&
  !(4 in result), 'mutation');
result.length = 2;
check(result.length === 2 && result[1] === 'y' && !(2 in result), 'truncation');
check('p,q'.split(',').map(function (x) { return x + '!'; }).join() === 'p!,q!', 'consumer');
Object.defineProperty(Array.prototype, 0, {
  set: function () { throw new Error('inherited setter'); }, configurable: true
});
check('p,q'.split(',')[0] === 'p', 'CreateDataProperty ignores inherited setters');
delete Array.prototype[0];
check(same('a1b2c'.split(/\d/), ['a', 'b', 'c']), 'RegExp separator');
var custom = {};
custom[Symbol.split] = function (input, limit) { return 'custom:' + input + ':' + limit; };
check('abc'.split(custom, 3) === 'custom:abc:3', 'custom Symbol.split');
var order = '';
var separator = { toString: function () { order += 's'; return 'b'; } };
var limit = { valueOf: function () { order += 'l'; return 5; } };
check(same('abc'.split(separator, limit), ['a', 'c']) && order === 'ls', 'conversion order');
check('a.b.c'.replace('.', '-') === 'a-b.c' && 'a.b.c'.replaceAll('.', '-') === 'a-b-c',
  'one-unit replace');
check('abcabc'.replace('ca', 'X') === 'abXbc' && 'abcab'.replaceAll('ab', '') === 'c',
  'longer replace');
check('abc'.replace('', '-') === '-abc' && 'ab'.replaceAll('', '-') === '-a-b-', 'empty search');
check('aab'.replace('ab', 'X') === 'aX', 'mismatch after first unit');
"#,
    );
}

#[test]
fn iterator_results_are_fresh_ordered_data_objects_and_array_reads_keep_get() {
    check(
        r#"
var iterator = [10, 20][Symbol.iterator]();
var first = iterator.next();
check(Object.getPrototypeOf(first) === Object.prototype, 'prototype');
check(Object.keys(first).join() === 'value,done', 'key order');
var value = Object.getOwnPropertyDescriptor(first, 'value');
check(value.value === 10 && value.writable && value.enumerable && value.configurable, 'value');
var done = Object.getOwnPropertyDescriptor(first, 'done');
check(done.value === false && done.writable && done.enumerable && done.configurable, 'done');
first.extra = 1;
delete first.value;
first.done = 'changed';
check(Object.keys(first).join() === 'done,extra' && first.value === undefined &&
  first.done === 'changed', 'mutation');
var second = iterator.next();
check(second !== first && second.value === 20 && second.done === false, 'fresh result');
var end = iterator.next();
check(end.value === undefined && end.done === true && 'value' in end, 'completion');
check(JSON.stringify(end) === '{"done":true}' && JSON.stringify(second) === '{"value":20,"done":false}',
  'serialization');
var symbol = Symbol('s');
var object = {};
var values = [1.5, 'x', null, undefined, object, true, symbol, -0];
var seen = [];
for (var item of values) seen.push(item);
check(seen.length === 8 && seen[0] === 1.5 && seen[1] === 'x' && seen[2] === null &&
  seen[3] === undefined && seen[4] === object && seen[5] === true && seen[6] === symbol &&
  Object.is(seen[7], -0), 'every value kind');
var entry = ['a', 'b'].entries().next().value;
check(entry[0] === 0 && entry[1] === 'a', 'entries');
var key = ['a'].keys().next();
check(key.value === 0 && key.done === false, 'keys');
Array.prototype[1] = 'inherited';
seen = [];
for (var hole of [0, , 2]) seen.push(hole);
check(seen.join() === '0,inherited,2', 'a hole reads the prototype');
delete Array.prototype[1];
var accessor = [0];
Object.defineProperty(accessor, 1, {
  get: function () { return 'getter:' + (this === accessor); }, enumerable: true, configurable: true
});
seen = [];
for (var read of accessor) seen.push(read);
check(seen.join() === '0,getter:true', 'an own accessor is called with the Array');
var mutated = [1, 2, 3];
seen = [];
for (var m of mutated) { seen.push(m); if (m === 1) { mutated[2] = 'late'; mutated.length = 3; } }
check(seen.join() === '1,2,late', 'later elements are read when reached');
var log = [];
var proxy = new Proxy([7, 8], {
  get: function (target, property, receiver) {
    log.push(String(property));
    return Reflect.get(target, property, receiver);
  }
});
seen = [];
for (var proxied of Array.prototype.values.call(proxy)) seen.push(proxied);
check(seen.join() === '7,8' && log.join() === 'length,0,length,1,length', 'Proxy Get trace');
var arrayLike = { length: 2, 0: 'p', get 1() { return 'q'; } };
seen = [];
for (var like of Array.prototype.values.call(arrayLike)) seen.push(like);
check(seen.join() === 'p,q', 'array-like');
function* generator() { yield 1; }
var generated = generator().next();
check(Object.keys(generated).join() === 'value,done' && generated.value === 1, 'generator result');
var stringResult = 'ab'[Symbol.iterator]().next();
check(Object.keys(stringResult).join() === 'value,done' && stringResult.value === 'a', 'string iterator');
var mapResult = new Map([[1, 2]]).entries().next();
check(Object.keys(mapResult).join() === 'value,done' && mapResult.value[1] === 2, 'map iterator');
"#,
    );
}

#[test]
fn array_from_defines_fresh_array_results_and_keeps_other_constructors_generic() {
    check(
        r#"
var mapped = Array.from([1, 2, 3], function (value, index) { return value * 10 + index; });
check(Array.isArray(mapped) && mapped.length === 3 && mapped.join() === '10,21,32', 'iterable map');
var sparse = Array.from({ length: 3, 0: 'a', 2: 'c' });
check(sparse.length === 3 && sparse[0] === 'a' && (1 in sparse) && sparse[1] === undefined &&
  sparse[2] === 'c', 'array-like reads every index');
check(Array.from('abc').join() === 'a,b,c' && Array.from(new Set([5, 6])).join() === '5,6',
  'string and Set');
var many = Array.from({ length: 100 }, function (_, index) { return index; });
var sum = 0;
for (var i = 0; i < many.length; i++) sum += many[i];
check(many.length === 100 && sum === 4950, 'many array-like elements');
var iterated = Array.from(many.values(), function (value) { return value * 2; });
check(iterated.length === 100 && iterated[99] === 198, 'many iterated elements');
var element = Object.getOwnPropertyDescriptor(Array.from([4]), 0);
check(element.value === 4 && element.writable && element.enumerable && element.configurable,
  'default data element');
var length = Object.getOwnPropertyDescriptor(Array.from([4, 5]), 'length');
check(length.value === 2 && length.writable && !length.enumerable && !length.configurable,
  'length');
Object.defineProperty(Array.prototype, 0, {
  set: function () { throw new Error('inherited setter'); }, configurable: true
});
check(Array.from([9])[0] === 9 && Array.from({ length: 1, 0: 9 })[0] === 9,
  'CreateDataProperty ignores inherited setters');
delete Array.prototype[0];
var detached = Array.from.call(undefined, { length: 2, 0: 'x', 1: 'y' });
check(Array.isArray(detached) && detached.join() === 'x,y', 'non-constructor array-like');
var plain = Array.from.call({}, [1, 2]);
check(Array.isArray(plain) && plain.length === 2 && plain[1] === 2, 'non-constructor iterable');
class MyArray extends Array {}
var subclass = MyArray.from([1, 2]);
check(subclass instanceof MyArray && subclass.length === 2 && subclass[1] === 2, 'subclass');
var leaked;
var observed = [];
function Leak() { leaked = this; }
var fromIterable = Array.from.call(Leak, [1, 2], function (value) {
  observed.push(Object.keys(leaked).join('|'));
  return value;
});
check(fromIterable === leaked && fromIterable instanceof Leak && fromIterable.length === 2 &&
  observed.join(';') === ';0', 'leaked iterable target observes each definition');
observed = [];
var fromLike = Array.from.call(Leak, { length: 2, 0: 'p', 1: 'q' }, function (value) {
  observed.push(Object.keys(leaked).join('|') + ':' + leaked.length);
  return value;
});
check(fromLike === leaked && fromLike[1] === 'q' && fromLike.length === 2 &&
  observed.join(';') === ':undefined;0:undefined',
  'leaked array-like target observes each definition and the final length Set');
function Frozen() { return Object.freeze([]); }
var threw = false;
try { Array.from.call(Frozen, [1]); } catch (error) { threw = error instanceof TypeError; }
check(threw, 'a non-extensible constructed target still throws');
threw = false;
try { Array.from({ length: 4294967296 }); } catch (error) { threw = error instanceof RangeError; }
check(threw, 'an invalid array length still throws');
var trace = [];
var source = { length: 2 };
Object.defineProperty(source, 0, { get: function () { trace.push('get0'); return 'a'; } });
Object.defineProperty(source, 1, { get: function () { trace.push('get1'); return 'b'; } });
var traced = Array.from(source, function (value, index) { trace.push('map' + index); return value; });
check(traced.join() === 'a,b' && trace.join() === 'get0,map0,get1,map1', 'read/map order');
"#,
    );
}

#[test]
fn integral_number_strings_are_exact_decimal_digits() {
    check(
        r#"
function text(value) { return '' + value; }
var cases = [
  ['100', '100'], ['999', '999'], ['1000', '1000'], ['9999', '9999'], ['10000', '10000'],
  ['65536', '65536'], ['123456789', '123456789'], ['999999999', '999999999'],
  ['1000000000', '1000000000'], ['4294967294', '4294967294'], ['4294967295', '4294967295'],
  ['4294967296', '4294967296'], ['9007199254740991', '9007199254740991'],
  ['-100', '-100'], ['100.5', '100.5'], ['1e21', '1e+21'], ['-0', '0'], ['0', '0'],
  ['7', '7'], ['99', '99'], ['1e-7', '1e-7'], ['NaN', 'NaN'], ['-Infinity', '-Infinity']
];
for (var i = 0; i < cases.length; i++) {
  var number = Number(dynamic(cases[i][0]));
  check(text(number) === cases[i][1], 'String(' + cases[i][0] + ')');
  check(String(number) === cases[i][1] && number.toString() === cases[i][1],
    'toString ' + cases[i][0]);
}
check((Number(dynamic('255'))).toString(16) === 'ff', 'radix keeps its own path');
check([100, 1000, 65536].join() === '100,1000,65536', 'join');
var object = {};
object[Number(dynamic('300'))] = 1;
check(Object.keys(object)[0] === '300' && object['300'] === 1, 'property key');
"#,
    );
}
