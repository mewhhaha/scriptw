(function () {
  function check(actual, expected, label) {
    if (actual !== expected) throw new Error(label);
  }
  var a = [];
  for (var i = 0; i < 8; i++) {
    check(a.push(i), i * 6 + 1, 'push one length');
    check(a.push(i, i + 1), i * 6 + 3, 'push two length');
    check(a.push(i, i + 1, i + 2), i * 6 + 6, 'push three length');
  }
  check(a[47], 9, 'last packed value');
  var marker = {x: 91};
  a.push(marker);
  for (var j = 0; j < 256; j++) { var garbage = {a: [j, j + 1]}; }
  check(a[48], marker, 'reference push stays rooted');

  var originalPush = Array.prototype.push;
  var patchedCalls = 0;
  var savedTarget = [];
  function replacePush() {
    Array.prototype.push = function (value) {
      patchedCalls++;
      check(this, savedTarget, 'patched receiver');
      return value + 100;
    };
    return 17;
  }
  check(savedTarget.push(replacePush()), 1, 'callee saved before argument');
  check(savedTarget[0], 17, 'saved native writes argument once');
  check(patchedCalls, 0, 'late replacement does not replace saved callee');
  check(savedTarget.push(23), 123, 'next call sees patched method');
  check(patchedCalls, 1, 'patched method called once');
  Array.prototype.push = originalPush;

  var getterCalls = 0;
  var argumentCalls = 0;
  var calleeTarget = [];
  Object.defineProperty(calleeTarget, 'push', {get: function () {
    getterCalls++;
    check(this, calleeTarget, 'callee getter receiver');
    return originalPush;
  }});
  function argument(value) { argumentCalls++; return value; }
  check(calleeTarget.push(argument(31), argument(37)), 2, 'getter call push');
  check(getterCalls, 1, 'callee getter once');
  check(argumentCalls, 2, 'arguments once');

  var setterTarget = [];
  var setterValue = 0;
  var proto = Object.create(Array.prototype);
  Object.defineProperty(proto, '0', {set: function (value) {
    check(this, setterTarget, 'prototype setter receiver'); setterValue = value;
  }});
  Object.setPrototypeOf(setterTarget, proto);
  check(setterTarget.push(41), 1, 'setter fallback length');
  check(setterValue, 41, 'prototype indexed setter is observed');
  check(Object.prototype.hasOwnProperty.call(setterTarget, '0'), false, 'setter makes no own index');

  function pushSloppy(value) { return value.push(1); }
  function pushStrict(value) { 'use strict'; return value.push(1); }
  var locked = [];
  Object.defineProperty(locked, 'length', {writable: false});
  var caught = 0;
  try { pushSloppy(locked); } catch (error) { if (error instanceof TypeError) caught++; }
  try { pushStrict(locked); } catch (error) { if (error instanceof TypeError) caught++; }
  try { locked.push(); } catch (error) { if (error instanceof TypeError) caught++; }
  check(caught, 3, 'push length Set always throws in strict and sloppy source');
  check(locked.length, 0, 'declined push does not mutate');

  var proxyCalls = 0;
  var proxy = new Proxy([], {get: function (target, key, receiver) {
    proxyCalls++;
    return Reflect.get(target, key, receiver);
  }});
  check(proxy.push(43), 1, 'proxy receiver falls back');
  check(proxy[0], 43, 'proxy push writes');
  check(proxyCalls, 3, 'proxy method length and element reads occur once');

  var record = {x: 1, y: 2};
  var getterReads = 0;
  var inherited = Object.create(Object.prototype);
  Object.defineProperty(inherited, 'x', {get: function () {
    check(this, record, 'inherited getter receiver'); getterReads++; return 13;
  }});
  function mutate(value, index) {
    if (index === 1) value.x = 3;
    if (index === 2) Object.defineProperty(value, 'x', {configurable: true, get: function () {
      check(this, value, 'own getter receiver'); getterReads++; return 7;
    }});
    if (index === 3) Object.defineProperty(value, 'x', {configurable: true, writable: true, value: marker});
    if (index === 4) { delete value.x; Object.setPrototypeOf(value, inherited); }
  }
  function readAcrossCalls(value) {
    var result = [];
    for (var k = 0; k < 5; k++) {
      mutate(value, k);
      result.push(value.x);
    }
    return result;
  }
  var reads = readAcrossCalls(record);
  check(reads[0], 1, 'first compact own read');
  check(reads[1], 3, 'live compact value after callback');
  check(reads[2], 7, 'accessor after promotion');
  check(reads[3], marker, 'new reference value after descriptor change');
  check(reads[4], 13, 'prototype after deletion');
  check(getterReads, 2, 'own and inherited getters each run once');
  function ownRead(value) { return value.x; }
  check(ownRead({y: 2, x: 19}), 19, 'different slot shape');
  check(ownRead({x: 23, y: 2}), 23, 'different shape same site');
  var readProxyCalls = 0;
  var readProxy = new Proxy({x: 29}, {get: function (target, key, receiver) {
    readProxyCalls++; check(receiver, readProxy, 'read proxy receiver'); return target[key];
  }});
  check(ownRead(readProxy), 29, 'proxy read fallback');
  check(readProxyCalls, 1, 'proxy trap once');
  function checkPrimitiveReceiver(value, prototype, label) {
    var primitiveReceiver;
    var primitiveGetterCalls = 0;
    Object.defineProperty(prototype, 'x', {configurable: true, get: function () {
      'use strict'; primitiveReceiver = this; primitiveGetterCalls++; return 47;
    }});
    check(ownRead(value), 47, label + ' GetV boxes lookup');
    check(primitiveReceiver, value, label + ' strict getter retains primitive receiver');
    check(primitiveGetterCalls, 1, label + ' getter runs once');
    delete prototype.x;
  }
  checkPrimitiveReceiver(5, Number.prototype, 'Number');
  checkPrimitiveReceiver('primitive', String.prototype, 'String');
  checkPrimitiveReceiver(Symbol('primitive'), Symbol.prototype, 'Symbol');
  checkPrimitiveReceiver(53n, BigInt.prototype, 'BigInt');
  caught = 0;
  try { ownRead(null); } catch (error) { if (error instanceof TypeError) caught++; }
  try { ownRead(undefined); } catch (error) { if (error instanceof TypeError) caught++; }
  check(caught, 2, 'nullish static read errors');
  print('guarded-object-paths:ok');
})();
