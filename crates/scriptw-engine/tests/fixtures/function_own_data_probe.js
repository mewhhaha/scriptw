// Reuse the canonical reflection harness; source functions stay sloppy.
function read(fn, key) { return fn[key]; }
function metadata(fn) { return [fn.name, fn.length, fn.prototype]; }
function equal(actual, expected) { assert.sameValue(actual, expected); }
function target(a, b) {}
var original = target.prototype, changed = {}, token = {}, trace = '';
equal(metadata(target)[0], 'target');
equal(metadata(target)[1], 2);
equal(metadata(target)[2], original);
Object.defineProperty(target, 'name', {value: 'renamed'});
Object.defineProperty(target, 'length', {value: 7});
target.prototype = changed;
equal(read(target, 'name'), 'renamed');
equal(metadata(target)[1], 7);
equal(metadata(target)[2], changed);
Object.defineProperty(target, 'name', {get() { trace += 'n'; equal(this, target); return 'getter'; }});
equal(metadata(target)[0], 'getter');
equal(trace, 'n');
var receiver = {};
Object.defineProperty(target, 'length', {get() { return this; }});
equal(Reflect.get(target, 'length', receiver), receiver);
equal(metadata(target)[1], target);
assert(delete target.name);
assert(delete target.length);
equal(metadata(target)[0], '');
equal(metadata(target)[1], 0);
equal(Reflect.deleteProperty(target, 'prototype'), false);
equal(read(target, 'prototype'), changed);
var custom = () => 1, inherited = {}, parent = Object.create(Function.prototype);
parent.prototype = inherited;
Object.setPrototypeOf(custom, parent);
Object.defineProperty(custom, 'prototype', {value: token, configurable: true});
equal(metadata(custom)[2], token);
Object.defineProperty(custom, 'prototype', {get() { equal(this, custom); return changed; }});
equal(metadata(custom)[2], changed);
assert(delete custom.prototype);
equal(metadata(custom)[2], inherited);
var coercions = 0;
equal(read(target, {toString() { coercions++; return 'prototype'; }}), changed);
equal(coercions, 1);

// Bind and Proxy records keep their own algorithms and original receiver.
function base(a, b) {}
var bound = base.bind(null, 1);
equal(metadata(bound)[0], 'bound base');
equal(metadata(bound)[1], 1);
Object.defineProperty(bound, 'name', {get() { equal(this, bound); return 'bound-get'; }});
equal(metadata(bound)[0], 'bound-get');
var gets = 0, proxy = new Proxy(base, {get(t, key, receiver) {
  gets++; equal(receiver, proxy); return Reflect.get(t, key, receiver);
}});
equal(metadata(proxy)[0], 'base');
equal(gets, 3);
Object.defineProperty(base, 'name', {value: 'fixed', configurable: false});
assert.throws(TypeError, function() { return new Proxy(base, {get() { return 'wrong'; }}).name; });
var revoked = Proxy.revocable(base, {});
revoked.revoke();
assert.throws(TypeError, function() { return read(revoked.proxy, 'name'); });

// Ordinary reads see the actual innermost active data, then its restored token.
var saved = [], marker = {};
function reflect(fn) { return [fn.caller, read(fn, 'arguments')]; }
function recursive(depth, expectedCaller) {
  var before = reflect(recursive);
  equal(before[0], expectedCaller);
  equal(before[1], arguments);
  saved.push(before[1]);
  if (depth) {
    var inner = recursive(depth - 1, recursive);
    equal(inner, depth - 1);
    equal(reflect(recursive)[1], before[1]);
  }
  gc();
  equal(read(recursive, 'arguments'), before[1]);
  return depth;
}
function outer() { var result = recursive(2, outer); equal(result, 2); }
outer();
equal(saved.length, 3);
assert(saved[0] !== saved[1] && saved[1] !== saved[2]);
equal(reflect(recursive)[0], null);
equal(reflect(recursive)[1], null);
function strictCaller() { 'use strict'; var result = recursive(0, null); return result; }
equal(strictCaller(), 0);
function abrupt() { equal(read(abrupt, 'arguments'), arguments); throw marker; }
try { abrupt(19); } catch (error) { equal(error, marker); }
equal(read(abrupt, 'arguments'), null);
var replaced = false;
function userOwned() {
  if (!replaced) {
    Object.defineProperty(userOwned, 'arguments', {value: token});
    replaced = true;
  }
  equal(read(userOwned, 'arguments'), token);
}
userOwned(1);
equal(read(userOwned, 'arguments'), token);
userOwned(2);
equal(read(userOwned, 'arguments'), token);
Object.defineProperty(recursive, 'arguments', {value: token});
equal(read(recursive, 'arguments'), token);
Object.defineProperty(recursive, 'caller', {get() { equal(this, recursive); return changed; }});
equal(read(recursive, 'caller'), changed);
assert(delete recursive.caller);
assert.throws(TypeError, function() { return read(recursive, 'caller'); });
print('function-own-data:ok');
