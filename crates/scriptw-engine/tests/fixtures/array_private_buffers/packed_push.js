function check(ok, message) { if (!ok) throw message; }
function rejects(action, kind) {
  let caught = false;
  try { action(); } catch (error) { caught = error instanceof kind; }
  check(caught, 'expected ' + kind.name);
}

const marker = {}, symbol = Symbol('packed');
const dense = [];
for (let i = 0; i < 130; i++) check(dense.push(i, 'v' + i, null) === (i + 1) * 3, 'geometric multi-argument push');
check(dense.push(marker, symbol, undefined, -0, NaN) === 395, 'mixed packed publication');
gc();
check(dense[390] === marker && dense[391] === symbol && dense[392] === undefined &&
  Object.is(dense[393], -0) && Number.isNaN(dense[394]), 'rooted whole values');
const descriptor = Object.getOwnPropertyDescriptor(dense, '390');
check(descriptor.value === marker && descriptor.writable && descriptor.enumerable && descriptor.configurable,
  'default own data attributes');
const evaluated = [];
evaluated.push((evaluated.push('inner'), 'outer'));
check(evaluated.join() === 'inner,outer', 'argument effects finish before receiver length snapshot');

const intercepted = ['old', 'tail'];
let setterTrace = '';
Object.defineProperty(Array.prototype, '2', { configurable: true, set(value) {
  setterTrace += value + ';';
  intercepted.length = 0;
  gc();
} });
check(intercepted.push('setter', 'own') === 4, 'inherited setter still runs');
delete Array.prototype[2];
check(setterTrace === 'setter;' && intercepted.length === 4 && !(0 in intercepted) &&
  !(2 in intercepted) && intercepted[3] === 'own', 'setter mutation and subsequent Set order');

Object.defineProperty(Object.prototype, '0', { configurable: true, value: 'blocked', writable: false });
const blocked = [];
rejects(function() { blocked.push('value'); }, TypeError);
delete Object.prototype[0];
check(blocked.length === 0 && !Object.hasOwn(blocked, '0'), 'live Object prototype nonwritable property');
Object.defineProperty(Array.prototype, '0', { configurable: true, get() { return 'readonly'; } });
rejects(function() { [].push('value'); }, TypeError);
delete Array.prototype[0];

let proxyTrace = '';
const proxy = new Proxy(Object.create(Array.prototype), { set(target, key, value, receiver) {
  proxyTrace += key + ':' + value + ';';
  return Reflect.set(target, key, value, receiver);
} });
const proxiedPrototype = [];
Object.setPrototypeOf(proxiedPrototype, proxy);
check(proxiedPrototype.push('proxy') === 1 && proxiedPrototype[0] === 'proxy' && proxyTrace === '0:proxy;',
  'prototype proxy traps cannot be bypassed');
let receiverTrace = '';
const receiverProxy = new Proxy([], {
  get(target, key, receiver) { receiverTrace += 'get:' + key + ';'; return Reflect.get(target, key, receiver); },
  set(target, key, value, receiver) { receiverTrace += 'set:' + key + ';'; return Reflect.set(target, key, value, receiver); }
});
check(Array.prototype.push.call(receiverProxy, 'a', 'b') === 2 &&
  receiverTrace === 'get:length;set:0;set:1;set:length;', 'receiver proxy uses ordinary operations');

const sparse = new Array(2);
const sparsePrototype = Object.create(Array.prototype);
let sparseValue;
Object.defineProperty(sparsePrototype, '2', { set(value) { sparseValue = value; }, configurable: true });
Object.setPrototypeOf(sparse, sparsePrototype);
check(sparse.push('sparse') === 3 && sparseValue === 'sparse' && !Object.hasOwn(sparse, '2'), 'sparse fallback');
for (const array of [Object.freeze([1]), Object.seal([1]), Object.preventExtensions([1])]) {
  rejects(function() { array.push(2); }, TypeError);
  check(array.length === 1 && array[0] === 1 && !Object.hasOwn(array, '1'), 'failed push leaves prior data');
}
const readonly = [1];
Object.defineProperty(readonly, 'length', { writable: false });
rejects(function() { readonly.push(); }, TypeError);
rejects(function() { readonly.push(2); }, TypeError);
check(readonly.length === 1 && !Object.hasOwn(readonly, '1'), 'readonly length including zero arguments');
check(Object.preventExtensions([]).push() === 0 && Object.seal([1]).push() === 1,
  'zero arguments still Set writable length');
const overflow = [];
overflow.length = 4294967294;
rejects(function() { overflow.push('last-index', 'named'); }, RangeError);
check(overflow.length === 4294967295 && overflow[4294967294] === 'last-index' &&
  overflow[4294967295] === 'named', 'uint32 overflow retains both preceding writes');

const other = $262.createRealm().global;
const borrowed = [];
check(other.Array.prototype.push.call(borrowed, marker) === 1 && borrowed[0] === marker,
  'borrowed cross-Realm push uses receiver storage');
let realmSet;
Object.defineProperty(Array.prototype, '1', { configurable: true, set(value) { realmSet = value; } });
check(other.Array.prototype.push.call(borrowed, 'cross-realm') === 2, 'borrowed push checks receiver prototype');
delete Array.prototype[1];
check(realmSet === 'cross-realm' && !Object.hasOwn(borrowed, '1'), 'borrowed live prototype setter');
print('array-packed-push:ok');
262;
