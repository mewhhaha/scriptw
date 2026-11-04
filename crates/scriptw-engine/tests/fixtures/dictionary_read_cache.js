function same(actual, expected) {
  if (!Object.is(actual, expected)) throw new Error('dictionary cache value');
}
function throwsValue(expected, callback) {
  try { callback(); } catch (error) { same(error, expected); return; }
  throw new Error('dictionary cache missing Throw');
}
function field(value, unused, extra) { return value.field; }
function length(value, unused, extra) { return value.length; }
function zero(value, unused, extra) { return value['0']; }
function name(value, unused, extra) { return value.name; }
function activeArguments(value, unused, extra) { return value.arguments; }
function activeCaller(value, unused, extra) { return value.caller; }
var strictMode = (function() { return this; })() === undefined;
var compact = {field: 'compact', other: 1};
same(field(compact), 'compact');
var record = Object.create(null);
record.first = 1; record.second = 2; record.third = 3; record.field = 'dictionary';
same(field(record), 'dictionary');
same(field(record), 'dictionary');
// A dictionary POSITION must not remain authorized by the prior compact SHAPE.
same(field(compact), 'compact');
same(field(record), 'dictionary');
var token = {}, symbol = Symbol('live'), values = [undefined, null, false, -0, NaN, 13n, symbol, token];
for (var i = 0; i < values.length; i++) {
  record.field = values[i];
  same(field(record), values[i]);
  if (typeof gc === 'function') gc();
  same(field(record), values[i]);
}
// Other dictionary receivers need their own live key/descriptor, even at a
// coincident slot. A different String or Symbol key cannot authorize a read.
var other = Object.create(null);
other.first = 1; other.second = 2; other.third = 3; other.wrong = 4;
other[Symbol('field')] = 'symbol'; other.field = 'other';
same(field(other), 'other'); same(field(record), token);
delete record.first; delete record.second; delete record.third;
for (var round = 0; round < 48; round++) {
  var key = 'temporary' + round;
  record[key] = round;
  same(field(record), token);
  delete record[key];
}
// Delete/reinsert and compaction may move the target entry or leave a hole.
delete record.field;
same(field(record), undefined);
record.field = 'reinserted';
same(field(record), 'reinserted');
var trace = '';
Object.defineProperty(record, 'field', {configurable: true, get: function() {
  same(this, record); trace += 'g';
  if (typeof gc === 'function') gc();
  Object.defineProperty(record, 'field', {value: token, writable: true, configurable: true});
  return 'getter';
}});
same(field(record), 'getter'); same(trace, 'g'); same(field(record), token);
Object.defineProperty(record, 'field', {configurable: true, get: function() { throw token; }});
throwsValue(token, function() { field(record); });
delete record.field;
var prototype = {get field() { same(this, record); trace += 'p'; return 'inherited'; }};
Object.setPrototypeOf(record, prototype);
same(field(record), 'inherited'); same(trace, 'gp');
Object.setPrototypeOf(record, {field: 'new prototype'});
same(field(record), 'new prototype');
Object.defineProperty(record, 'field', {value: symbol, configurable: true});
same(field(record), symbol);
// A noncanonical but equal computed String may miss the hint safely.
var computed = Object.create(null);
var dynamicKey = ['fi', 'eld'].join('');
computed[dynamicKey] = 91;
same(field(computed), 91); same(field(computed), 91);
// Primitive boxes retain virtual String indices and immutable own length.
var box = new String('abc'); box.field = token;
same(field(box), token); same(zero(box), 'a'); same(length(box), 3);
same(Reflect.defineProperty(box, '0', {value: 'wrong'}), false);
same(Reflect.defineProperty(box, 'length', {value: 7}), false);
same(zero(box), 'a'); same(length(box), 3);
// Arguments indexed mappings are separate concrete records, not ordinary data.
function mapped(value) {
  var object = arguments;
  same(zero(object), 1);
  value = 2;
  if (strictMode) same(zero(object), 1); else same(zero(object), 2);
  object[0] = 3; same(zero(object), 3);
  if (strictMode) same(value, 2); else same(value, 3);
  delete object[0]; same(zero(object), undefined);
}
mapped(1);
function legacy(depth, caller) {
  same(activeCaller(legacy), caller);
  same(activeArguments(legacy)[0], depth);
  if (depth) legacy(depth - 1, legacy);
  if (typeof gc === 'function') gc();
  same(activeArguments(legacy)[0], depth);
}
function outer() { legacy(2, outer); }
if (!strictMode) {
  outer();
  same(activeCaller(legacy), null);
  same(activeArguments(legacy), null);
}
function callable(a, b) {}
same(name(callable), 'callable');
Object.defineProperty(callable, 'name', {value: 'renamed'});
same(name(callable), 'renamed');
Object.defineProperty(callable, 'name', {get: function() { same(this, callable); return token; }});
same(name(callable), token);
var bound = callable.bind(null);
Object.defineProperty(bound, 'name', {get: function() { same(this, bound); return symbol; }});
same(name(bound), symbol);
var hits = 0, proxy = new Proxy(record, {get: function(target, key, receiver) {
  same(receiver, proxy); same(key, 'field'); hits++; return token;
}});
same(field(proxy), token); same(field(proxy), token); same(hits, 2);
var revoked = Proxy.revocable(record, {}); revoked.revoke();
var failed = false;
try { field(revoked.proxy); } catch (error) { failed = error.constructor === TypeError; }
same(failed, true);
print('dictionary-read-cache:ok');
