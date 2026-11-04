function read(fn, key) { return fn[key]; }
function equal(actual, expected) { assert.sameValue(actual, expected); }
var realm = __scriptwCreateRealm(), foreign = realm.global;
var A = foreign.Array, foreignType = foreign.TypeError;
// Promote before observing Array.from: its retained recipe must still resolve
// in the defining Realm through the existing PropertyValue decoder.
Object.defineProperty(A, 'extra', {value: 1});
var from = read(A, 'from');
equal(from, read(A, 'from'));
equal(Object.getPrototypeOf(from), foreign.Function.prototype);
equal(Object.getPrototypeOf(from.call(A, {0: 8, length: 1})), A.prototype);
assert.throws(foreignType, function() { from.call(null, null); });
var fn = realm.evalScript('(function foreignFunction(a,b){})');
equal(read(fn, 'name'), 'foreignFunction');
equal(read(fn, 'length'), 2);
var bound = fn.bind(null, 1);
var getter = realm.evalScript('(function(){throw new TypeError("foreign getter");})');
Object.defineProperty(fn, 'name', {get: getter});
assert.throws(foreignType, function() { read(fn, 'name'); });
var strictFn = realm.evalScript('(function(){"use strict";})');
assert.throws(foreignType, function() { read(strictFn, 'caller'); });
equal(read(bound, 'length'), 1);
assert.throws(foreignType, function() { read(bound, 'caller'); });
var record = realm.evalScript('Proxy.revocable(function(){}, {})');
record.revoke();
// Revocation is an error of this local Get; the Proxy has no callable Realm.
assert.throws(TypeError, function() { read(record.proxy, 'name'); });
gc();
equal(read(A, 'from'), from);
equal(Object.getPrototypeOf(from), foreign.Function.prototype);
print('function-own-data:ok');
