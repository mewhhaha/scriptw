function check(value, message) { if (!value) throw message; }
function verify(value, spelling) {
  check((typeof value === 'undefined') === (spelling === 'undefined'), 'undefined');
  check((typeof value === 'boolean') === (spelling === 'boolean'), 'boolean');
  check((typeof value === 'number') === (spelling === 'number'), 'number');
  check((typeof value === 'string') === (spelling === 'string'), 'string');
  check((typeof value === 'symbol') === (spelling === 'symbol'), 'symbol');
  check((typeof value === 'bigint') === (spelling === 'bigint'), 'bigint');
  check((typeof value === 'object') === (spelling === 'object'), 'object');
  check((typeof value === 'function') === (spelling === 'function'), 'function');
  check((typeof value == 'undefined') === (spelling === 'undefined'), 'loose undefined');
  check((typeof value == 'boolean') === (spelling === 'boolean'), 'loose boolean');
  check((typeof value == 'number') === (spelling === 'number'), 'loose number');
  check((typeof value == 'string') === (spelling === 'string'), 'loose string');
  check((typeof value == 'symbol') === (spelling === 'symbol'), 'loose symbol');
  check((typeof value == 'bigint') === (spelling === 'bigint'), 'loose bigint');
  check((typeof value == 'object') === (spelling === 'object'), 'loose object');
  check((typeof value == 'function') === (spelling === 'function'), 'loose function');
  check((typeof value !== 'undefined') === (spelling !== 'undefined'), 'inverse undefined');
  check((typeof value !== 'boolean') === (spelling !== 'boolean'), 'inverse boolean');
  check((typeof value !== 'string') === (spelling !== 'string'), 'inverse string');
  check((typeof value !== 'symbol') === (spelling !== 'symbol'), 'inverse symbol');
  check((typeof value !== 'bigint') === (spelling !== 'bigint'), 'inverse bigint');
  check((typeof value !== 'object') === (spelling !== 'object'), 'inverse object');
  check((typeof value !== 'function') === (spelling !== 'function'), 'inverse function');
  check((typeof value != 'undefined') === (spelling !== 'undefined'), 'loose inverse undefined');
  check((typeof value != 'boolean') === (spelling !== 'boolean'), 'loose inverse boolean');
  check((typeof value != 'number') === (spelling !== 'number'), 'loose inverse number');
  check((typeof value != 'string') === (spelling !== 'string'), 'loose inverse string');
  check((typeof value != 'symbol') === (spelling !== 'symbol'), 'loose inverse symbol');
  check((typeof value != 'bigint') === (spelling !== 'bigint'), 'loose inverse bigint');
  check((typeof value != 'function') === (spelling !== 'function'), 'loose inverse function');
  check((typeof value !== 'number') === (spelling !== 'number'), 'strict inverse');
  check(('number' == typeof value) === (spelling === 'number'), 'reversed loose');
  check((typeof value != 'object') === (spelling !== 'object'), 'loose inverse');
  check(!(typeof value === 'null') && typeof value !== 'not-a-type', 'invalid spellings');
}
for (var number of [0, -0, NaN, Infinity, -Infinity, 3.25]) verify(number, 'number');
verify(undefined, 'undefined');
verify(null, 'object');
verify(false, 'boolean');
verify(true, 'boolean');
verify('', 'string');
verify('value', 'string');
verify(Symbol('kind'), 'symbol');
verify(17n, 'bigint');
verify({}, 'object');
verify([], 'object');
verify(Object.create(null), 'object');
verify(Object(1), 'object');
verify(Object('a'), 'object');
verify(Object(1n), 'object');
verify(function ordinary() {}, 'function');
verify(function () {}.bind(null), 'function');
verify(() => 1, 'function');
verify(class Example {}, 'function');
var traps = 0;
var callable = new Proxy(function () {}, {
  get: function () { traps++; throw 'typeof must not get'; }
});
verify(callable, 'function');
var revoked = Proxy.revocable(function () {}, {});
revoked.revoke();
verify(revoked.proxy, 'function');
var objectProxy = Proxy.revocable({}, {});
objectProxy.revoke();
verify(objectProxy.proxy, 'object');
check(traps === 0, 'callability uses internal slots');

var effects = 0, reason = {reason: 'stop'};
function acquire() { effects++; gc(); return 12; }
check(typeof acquire() === 'number' && effects === 1, 'operand once');
check('number' === typeof acquire() && effects === 2, 'reversed operand once');
var invalid = typeof acquire() === 'invalid';
check(!invalid && effects === 3, 'invalid spelling still evaluates');
var getter = {get value() { effects++; gc(); throw reason; }};
try { typeof getter.value === 'invalid'; throw 'missing getter throw'; }
catch (error) { check(error === reason && effects === 4, 'invalid spelling preserves abrupt'); }
var caught = false;
try { typeof pending === 'undefined'; let pending; }
catch (error) { caught = error instanceof ReferenceError; }
check(caught, 'resolved lexical TDZ');
check(typeof __scriptw_typeof_comparison_missing === 'undefined', 'unresolved identifier');
var coercions = 0;
var spelling = {toString: function () { coercions++; return 'number'; }};
check(typeof acquire() == spelling && coercions === 1 && effects === 5, 'dynamic comparison coercion');
check(!(typeof acquire() === spelling) && coercions === 1 && effects === 6, 'strict dynamic comparison');
try { 'invalid' == typeof getter.value; throw 'missing reversed getter throw'; }
catch (error) { check(error === reason && effects === 7, 'reversed invalid spelling preserves abrupt'); }
function failUndefined() { throw undefined; }
var caughtUndefined = false;
try { typeof failUndefined() !== 'invalid'; }
catch (error) { caughtUndefined = error === undefined; }
check(caughtUndefined, 'invalid spelling preserves undefined throw');
print('ok');
