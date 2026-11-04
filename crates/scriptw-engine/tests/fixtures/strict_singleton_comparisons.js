function check(value, message) { if (!value) throw message; }
function verify(value, isNull, isUndefined) {
  check((value === null) === isNull, 'right null');
  check((null === value) === isNull, 'left null');
  check((value !== null) === !isNull, 'right not null');
  check((null !== value) === !isNull, 'left not null');
  check((value === void 0) === isUndefined, 'right undefined');
  check((void 0 === value) === isUndefined, 'left undefined');
  check((value !== void 0) === !isUndefined, 'right not undefined');
  check((void 0 !== value) === !isUndefined, 'left not undefined');
}
verify(null, true, false);
verify(undefined, false, true);
for (var value of [false, true, 0, -0, NaN, Infinity, '', 'value', 0n, 17n,
                   Symbol('value'), {}, [], function () {}, Object(1), Object('x')]) {
  verify(value, false, false);
}
var coercions = 0;
var hostile = {
  get valueOf() { coercions++; throw 'valueOf must not be read'; },
  get toString() { coercions++; throw 'toString must not be read'; },
  get [Symbol.toPrimitive]() { coercions++; throw 'toPrimitive must not be read'; }
};
verify(hostile, false, false);
var proxy = new Proxy({}, {get: function () { coercions++; throw 'proxy must not get'; }});
verify(proxy, false, false);
var revoked = Proxy.revocable(function () {}, {});
revoked.revoke();
verify(revoked.proxy, false, false);
check(coercions === 0, 'strict singleton comparison invokes no user code');

function dynamic(left, right) { return left === right; }
check(dynamic('ab', 'a' + 'b') && dynamic(3n, 1n + 2n), 'dynamic content equality');
check(!dynamic(NaN, NaN) && dynamic(-0, 0), 'dynamic numeric equality');
function shadowed(value, undefined) { return value === undefined; }
check(shadowed(7, 7) && shadowed(null, null), 'shadowed undefined remains dynamic');
check(!shadowed(void 0, 7), 'identifier name is not a singleton proof');

var effects = [];
function mark(name, value) { effects.push(name); gc(); return value; }
function reset() { effects = []; }
check(mark('left', undefined) === void mark('right', 9), 'right effectful void');
check(effects.join(',') === 'left,right', 'right void evaluates after the left');
reset();
check(void mark('left', 9) === mark('right', undefined), 'left effectful void');
check(effects.join(',') === 'left,right', 'left void evaluates before the right');
reset();
check(void mark('left', 1) === void mark('right', 2), 'both void operands');
check(effects.join(',') === 'left,right', 'both singleton operands still execute once');
reset();
check(void mark('left', 1) !== null, 'distinct singleton kinds');
check(effects.join(',') === 'left', 'false comparison still evaluates void');
reset();
check(null !== void mark('right', 1), 'reversed distinct singleton kinds');
check(effects.join(',') === 'right', 'reversed false comparison still evaluates void');

function fail(reason) { effects.push('throw'); gc(); throw reason; }
for (var reason of [undefined, null, -0, NaN, Symbol('throw'), {reason: 'identity'}]) {
  reset();
  var caught = false;
  try { void fail(reason) === mark('unreachable', undefined); }
  catch (error) { caught = true; check(Object.is(error, reason), 'left void whole Throw'); }
  check(caught && effects.join(',') === 'throw', 'left Throw skips the right');
  reset();
  caught = false;
  try { mark('left', undefined) !== void fail(reason); }
  catch (error) { caught = true; check(Object.is(error, reason), 'right void whole Throw'); }
  check(caught && effects.join(',') === 'left,throw', 'right Throw follows the left');
}
reset();
var getter = {get value() { effects.push('getter'); gc(); return undefined; }};
check(getter.value === void mark('right', 1), 'getter before void');
check(effects.join(',') === 'getter,right', 'getter evaluated once and before the right');
reset();
check(void mark('left', 1) === getter.value, 'void before getter');
check(effects.join(',') === 'left,getter', 'getter evaluated once and after the left');

var tdz = false;
try { void pending === null; let pending; }
catch (error) { tdz = error instanceof ReferenceError; }
check(tdz, 'void retains TDZ even when comparison cannot be equal');
reset();
tdz = false;
try { mark('left', undefined) === void later; let later; }
catch (error) { tdz = error instanceof ReferenceError; }
check(tdz && effects.join(',') === 'left', 'right TDZ follows left effects');
tdz = false;
try { null === void __scriptw_singleton_missing; }
catch (error) { tdz = error instanceof ReferenceError; }
check(tdz, 'void preserves unresolved reference Throw');
print('ok');
