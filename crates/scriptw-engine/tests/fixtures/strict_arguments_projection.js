function same(actual, expected, label) {
  if (!Object.is(actual, expected)) throw new Error(label);
}
function fail(value, expected, label) {
  var caught = false;
  try { value(); } catch (error) { caught = true; same(error, expected, label); }
  same(caught, true, label + ' thrown');
}
var marker = {}, symbol = Symbol('argument'), big = 123456789012345678901n;
var revoke = Proxy.revocable({}, {}), revoked = revoke.proxy;
revoke.revoke();
function snapshot(value, callback) {
  'use strict';
  value = 123;
  var first = arguments[0];
  callback();
  return [first, arguments[0], arguments.length, value];
}
function quiet() { gc(); }
var inputs = [undefined, null, false, -0, NaN, Infinity, 1.25, 4294967296, 'text', big, symbol, marker, revoked];
for (var i = 0; i < inputs.length; i++) {
  var output = snapshot(inputs[i], quiet, 'extra', i);
  same(output[0], inputs[i], 'first snapshot ' + i);
  same(output[1], inputs[i], 'after callback ' + i);
  same(output[2], 4, 'extra count ' + i);
  same(output[3], 123, 'reassigned formal ' + i);
}
function recurse(value, depth) {
  'use strict';
  if (depth) recurse(marker, depth - 1);
  gc();
  return arguments[0];
}
same(recurse(symbol, 5), symbol, 'recursive snapshot');
function identity(value) { 'use strict'; return arguments[0]; }
function forward(value) { 'use strict'; return identity(arguments[0]); }
same(forward(symbol), symbol, 'proper tail operand snapshot');
same(new identity(marker), marker, 'construct return object');
same(new identity(1) instanceof identity, true, 'construct return primitive');
var conversions = 0;
var numeric = {valueOf: function () { conversions++; gc(); return 5; }};
function convert(value) { 'use strict'; value=0; return arguments[0]+arguments.length; }
same(convert(numeric, 2, 3), 8, 'conversion and actual count');
same(conversions, 1, 'one source conversion');
var abrupt = Symbol('abrupt');
function throwsNow() { gc(); throw abrupt; }
fail(function () { snapshot(marker, throwsNow); }, abrupt, 'callback whole throw');
var box = {get x() { gc(); throw abrupt; }};
function read(value) { 'use strict'; return arguments[0].x; }
fail(function () { read(box); }, abrupt, 'getter whole throw');
var tick = 9;
function step(value, index) {
  'use strict';
  var original = value;
  value = index + tick;
  tick = (tick + 1) & 255;
  var row = arguments[0];
  return ((row.x ^ value) + row.y + arguments.length + tick) & 1023;
}
same(step({x:7,y:13}, 2), ((7 ^ 11) + 13 + 2 + 10) & 1023, 'captured tick');
same(tick, 10, 'captured write published');

// No own index zero: the getter receives the actual Arguments object, changes
// it, escapes it, and reenters another invocation before later reads execute.
var inherited = 0, escaped, nested;
function empty(value) {
  'use strict';
  var before = arguments.length;
  var first = arguments[0];
  var after = arguments.length;
  var second = arguments[0];
  return [before, first, after, second];
}
Object.defineProperty(Object.prototype, '0', {configurable:true, get:function () {
  inherited++;
  escaped = this;
  Object.defineProperty(this, '0', {value:symbol,writable:true,enumerable:true,configurable:true});
  this.length = 9;
  gc();
  nested = identity(marker);
  return big;
}});
var missing;
try {
  same(identity(undefined), undefined, 'present undefined shadows getter');
  same(inherited, 0, 'no inherited access for own undefined');
  missing = empty();
} finally {
  delete Object.prototype[0];
}
same(inherited, 1, 'single inherited read');
same(missing[0], 0, 'initial empty length');
same(missing[1], big, 'getter result');
same(missing[2], 9, 'later length stays generic');
same(missing[3], symbol, 'later index stays generic');
same(escaped.length, 9, 'escaped real object');
same(nested, marker, 'getter reentry');
gc();
same(escaped[0], symbol, 'escaped object rooted');
Object.defineProperty(Object.prototype, '0', {configurable:true, get:function () { gc(); throw abrupt; }});
try { fail(function () { empty(); }, abrupt, 'absent getter throw'); }
finally { delete Object.prototype[0]; }

// These functions must retain generic semantics: calls expose the Arguments
// receiver, writes alter its data, and child closures observe the real object.
function receiver(value) { 'use strict'; return arguments[0](); }
function getThis() { return this; }
var received = receiver(getThis);
same(received[0], getThis, 'reference callee receiver');
same(received.length, 1, 'reference callee arguments');
function written(value) { 'use strict'; arguments[0]=symbol; return arguments[0]; }
same(written(marker), symbol, 'write refusal');
// A lexical arrow owns the parent reference.
function lexical(value) { 'use strict'; return () => arguments[0]; }
same(lexical(marker)(), marker, 'captured arguments refusal');
function defaulted(value=marker) { return arguments[0]; }
same(defaulted(), undefined, 'default parameter refusal');
print('strict arguments projection ok');
