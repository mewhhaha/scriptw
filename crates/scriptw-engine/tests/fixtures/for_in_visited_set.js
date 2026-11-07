function check(condition, message) { if (!condition) throw new Error(message); }
function keyName(index) {
  return ('discard:wide-' + String.fromCharCode(0xd800) + '-' + index).slice(8);
}
function names(object) {
  var result = [];
  for (var key in object) { result.push(key); if (result.length % 8 === 0) gc(); }
  return result;
}
function* suspendedNames(object) { for (const key in object) { yield key; gc(); } }
var parent = Object.create(null), child = Object.create(parent), expected = [];
for (var index = 0; index < 72; index++) {
  parent[keyName(index)] = index;
  Object.defineProperty(child, keyName(index), {
    get() { throw new Error('enumeration read a property value'); },
    enumerable: index % 7 !== 0, configurable: true
  });
  if (index % 7 !== 0) expected.push(keyName(index));
}
parent.tail = 1;
child[Symbol('ignored')] = 2;
expected.push('tail');
// Internal membership must never touch a public constructor, prototype hook,
// iterator or reflective method, including after yielding to source code.
var OriginalSet = Set;
function forbidden() { throw new Error('public hook entered private enumeration'); }
OriginalSet.prototype.has = forbidden;
OriginalSet.prototype.add = forbidden;
OriginalSet.prototype[Symbol.iterator] = forbidden;
Object.defineProperty(OriginalSet.prototype, 'size', { get: forbidden });
globalThis.Set = forbidden;
Reflect.ownKeys = forbidden;
Reflect.getOwnPropertyDescriptor = forbidden;
Reflect.getPrototypeOf = forbidden;
check(names(child).join('|') === expected.join('|'), 'ordinary content keys and hidden shadows');
var iterator = suspendedNames(child), received = [], step;
while (!(step = iterator.next()).done) {
  received.push(step.value);
  check(names({nested: 1}).join('|') === 'nested', 'independent reentrant cursor');
  gc();
}
check(received.join('|') === expected.join('|'), 'generator retains its Set across GC');

var width = 72, events = [], ownDescriptors = [], inheritedDescriptors = [];
function keyList() {
  var result = [];
  for (var index = 0; index < width; index++) result.push(keyName(index));
  return result;
}
var prototype = new Proxy(Object.create(null), {
  ownKeys() { events.push('parent keys'); return keyList().concat(['tail']); },
  getOwnPropertyDescriptor(target, key) {
    inheritedDescriptors.push(key); gc();
    return {value: 1, enumerable: true, configurable: true};
  },
  getPrototypeOf() { return null; }
});
var source = new Proxy(Object.create(null), {
  ownKeys() { events.push('own keys'); return keyList().concat([Symbol('excluded')]); },
  getOwnPropertyDescriptor(target, key) {
    ownDescriptors.push(key);
    check(names({reentry: 1}).join('|') === 'reentry', 'descriptor reentry'); gc();
    if (key === keyName(11)) return undefined;
    return {value: 1, enumerable: key !== keyName(7), configurable: true};
  },
  getPrototypeOf() { return prototype; }
});
expected = [];
for (var index = 0; index < width; index++) {
  if (index !== 7 && index !== 11) expected.push(keyName(index));
}
expected.push(keyName(11), 'tail');
received = names(source);
check(received.length === expected.length &&
  received.slice().sort().join('|') === expected.slice().sort().join('|'),
  'content equality across distinct String references and prototype levels');
check(events.join('|') === 'own keys|parent keys', 'each level acquires one snapshot');

var marker = {whole: 'descriptor'}, caught;
try {
  names(new Proxy(Object.create(null), {
    ownKeys() { return keyList(); },
    getOwnPropertyDescriptor(target, key) {
      if (key === keyName(65)) throw marker;
      return {value: 1, enumerable: true, configurable: true};
    }
  }));
} catch (error) { caught = error; }
check(caught === marker, 'whole descriptor Throw after multiple storage growths');
marker = {whole: 'prototype'}; caught = undefined;
try { names(new Proxy({own: 1}, { getPrototypeOf() { throw marker; } })); }
catch (error) { caught = error; }
check(caught === marker, 'whole prototype Throw');
caught = undefined;
try { names(new Proxy({}, { ownKeys() { return ['same', ('x' + 'same').slice(1)]; } })); }
catch (error) { caught = error; }
check(caught instanceof TypeError, 'OwnKeys duplicate rejection precedes visited filtering');
check(names(null).length === 0 && names(undefined).length === 0, 'completed nullish cursors');
check(names('ab').join('|') === '0|1', 'boxed primitive index Strings');
print('for-in-visited-set:ok');
