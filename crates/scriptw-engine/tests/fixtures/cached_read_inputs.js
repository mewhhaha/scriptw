function same(actual, expected, message) {
  if (!Object.is(actual, expected)) throw new Error(message);
}
function read(value, unused, extra) { return value.field; }
function shaped(value, unused, extra) { var record = {field: value}; return record.field; }
var marker = {}, symbol = Symbol('field');
same(shaped(marker), marker, 'shaped PropertyRead');
var record = {field: marker};
record[symbol] = 29;
for (var warm = 0; warm < 4; warm++) same(read(record), marker, 'warm literal key');
var getterCalls = 0;
var primitives = [7, 'text', true, 9n, symbol];
var prototypes = [Number.prototype, String.prototype, Boolean.prototype, BigInt.prototype, Symbol.prototype];
for (var index = 0; index < primitives.length; index++) {
  var prototype = prototypes[index], previous = Object.getOwnPropertyDescriptor(prototype, 'field');
  Object.defineProperty(prototype, 'field', {configurable: true, get: function() {
    'use strict';
    getterCalls++;
    gc();
    return this;
  }});
  try { same(read(primitives[index]), primitives[index], 'getter keeps raw primitive receiver'); }
  finally {
    if (previous) Object.defineProperty(prototype, 'field', previous);
    else delete prototype.field;
  }
}
same(getterCalls, primitives.length, 'one getter per primitive');
var receiver;
Object.defineProperty(record, 'field', {configurable: true, get: function() {
  receiver = this;
  gc();
  return marker;
}});
same(read(record), marker, 'promoted accessor fallback');
same(receiver, record, 'ordinary getter receiver');
var proxy = new Proxy(record, {get: function(target, key, actualReceiver) {
  same(key, 'field', 'literal String stays distinct from Symbol');
  same(actualReceiver, proxy, 'proxy get receiver');
  gc();
  return Reflect.get(target, key, actualReceiver);
}});
same(read(proxy), marker, 'proxy result');
same(receiver, proxy, 'inherited getter receives proxy');
var replacement = {get field() { same(this, record, 'prototype receiver'); return 41; }};
delete record.field;
Object.setPrototypeOf(record, replacement);
same(read(record), 41, 'deleted own key uses live prototype');
Object.defineProperty(record, 'field', {value: 43, configurable: true});
same(read(record), 43, 'replacement data property');
var keyEffects = 0;
var coercible = {[Symbol.toPrimitive]: function() { keyEffects++; gc(); return symbol; }};
same(record[coercible], 29, 'generic Symbol key conversion');
same(keyEffects, 1, 'key converts once');
var log = '';
function base() { log += 'base,'; return null; }
function key() { log += 'key,'; return coercible; }
try { base()[key()]; throw marker; }
catch (error) { same(error instanceof TypeError, true, 'nullish fails before coercion'); }
same(log, 'base,key,', 'both source operands evaluate in order');
same(keyEffects, 1, 'nullish base prevents key coercion');
for (var nullish of [null, undefined]) {
  try { read(nullish); throw marker; }
  catch (error) { same(error instanceof TypeError, true, 'literal nullish error'); }
}
var revoked = Proxy.revocable({}, {});
revoked.revoke();
try { read(revoked.proxy); throw marker; }
catch (error) { same(error instanceof TypeError, true, 'revoked proxy still throws'); }
var throws = [undefined, -0, NaN, symbol, 7n, marker];
for (var reason of throws) {
  Object.defineProperty(record, 'field', {configurable: true, get: function() { gc(); throw reason; }});
  try { read(record); throw new Error('missing getter Throw'); }
  catch (error) { same(error, reason, 'getter Throw payload'); }
}
var parent = {get field() { return this.payload; }};
var child = {payload: marker, read() { return super.field; }};
Object.setPrototypeOf(child, parent);
same(child.read(), marker, 'super retains distinct target and receiver');
same(Reflect.get(parent, 'field', {payload: symbol}), symbol, 'explicit distinct receiver');
print('cached-inputs:ok');
