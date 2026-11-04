function check(ok, label) { if (!ok) throw new Error(label); }
function read(value, unused, extra) { return value.field; }
function attempt(value) { var result = read(value); trace += 'after'; return result; }
var object = {}, symbol = Symbol('outcome'), fn = function value() {};
var values = [undefined, null, false, true, 0, -0, NaN, Infinity, -Infinity,
  17.25, 'text', 12345678901234567890n, symbol, object, [], fn];
var trace = '', caught, actual;
for (var i = 0; i < values.length; i++) {
  var expected = values[i], compact = {field: expected};
  check(Object.is(read(compact), expected), 'compact Normal value');
  compact.field = object;
  check(read(compact) === object, 'live compact replacement');
  var accessor = {get field() { gc(); return expected; }};
  check(Object.is(read(accessor), expected), 'accessor Normal value');
  var throwing = {get field() { gc(); throw expected; }};
  caught = false; trace = '';
  try { attempt(throwing); } catch (error) { caught = true; actual = error; }
  check(caught && Object.is(actual, expected) && trace === '', 'getter whole Throw');
  var proxy = new Proxy({}, {get(target, key, receiver) {
    check(key === 'field' && receiver === proxy, 'Proxy key/receiver');
    gc(); throw expected;
  }});
  caught = false;
  try { read(proxy); } catch (error) { caught = true; actual = error; }
  check(caught && Object.is(actual, expected), 'Proxy whole Throw');
  try { try { read(throwing); } finally { gc(); trace += 'finally'; } }
  catch (error) { check(Object.is(error, expected), 'finally preserves payload'); }
  check(trace === 'finally', 'finally runs once');
}
var replacement = {}, replacementSeen;
try { try { read({get field() { throw symbol; }}); } finally { throw replacement; } }
catch (error) { replacementSeen = error; }
check(replacementSeen === replacement, 'finally replacement');
var prototype = {field: 9}, inherited = Object.create(prototype);
check(read(inherited) === 9, 'prototype miss');
prototype.field = 11;
check(read(inherited) === 11, 'prototype live value');
check(read({}) === undefined, 'absent Normal undefined');
for (var missing of [null, undefined]) {
  caught = false;
  try { read(missing); } catch (error) { caught = error instanceof TypeError; }
  check(caught, 'nullish execution Realm');
}
var realm = __scriptwCreateRealm(), foreign = realm.global;
var foreignGetter = realm.evalScript('(function(){throw new TypeError("foreign getter");})');
var foreignThrower = {};
Object.defineProperty(foreignThrower, 'field', {get: foreignGetter});
caught = false;
try { read(foreignThrower); } catch (error) {
  caught = error instanceof foreign.TypeError && !(error instanceof TypeError);
}
check(caught, 'actual getter defining Realm');
var foreignToken = realm.evalScript('({token: 1})');
check(read({field: foreignToken}) === foreignToken, 'cross-Realm Normal identity');
caught = false;
try { read({get field() { gc(); throw foreignToken; }}); }
catch (error) { caught = error === foreignToken; }
check(caught, 'cross-Realm Throw identity');
print('cached-read-outcome:ok');
