function check(ok, message) { if (!ok) throw message; }

const events = [];
const backing = { length: 5, 1: 'old', 3: null, 4: undefined };
const receiver = new Proxy(backing, {
  get(target, key, actual) { events.push('get:' + key); return Reflect.get(target, key, actual); },
  has() { throw 'join must Get through holes'; }
});
const separator = { toString() { events.push('separator'); return '\0\ud800'; } };
backing[0] = { toString() {
  events.push('first');
  check(['nested', 'join'].join(':') === 'nested:join', 'reentrant join');
  backing.length = 0;
  backing[1] = { toString() { events.push('second'); gc(); return '\udfff\ud83d\ude00'; } };
  gc();
  return 'A';
} };
const result = Array.prototype.join.call(receiver, separator);
check(result === 'A\0\ud800\udfff\ud83d\ude00\0\ud800\0\ud800\0\ud800', 'exact UTF16 and holes');
check(events.join('|') === 'get:length|separator|get:0|first|get:1|second|get:2|get:3|get:4', 'captured length and live read/conversion order');
let converted = 0;
check([].join({ toString() { converted++; return '|'; } }) === '' && converted === 1, 'empty join still converts separator');
check([''].join() === '' && ['x'].join('unused') === 'x', 'empty and singleton text');

const marker = {};
let later = 0;
const abrupt = { length: 3, 0: 'prefix', get 1() { return { toString() { gc(); throw marker; } }; }, get 2() { later++; return 'bad'; } };
try { Array.prototype.join.call(abrupt, '|'); throw 'join did not throw'; }
catch (error) { check(error === marker, 'join abrupt identity'); }
check(later === 0, 'no later Get after failed conversion');

const resizable = new ArrayBuffer(3, { maxByteLength: 6 });
const typed = new Uint8Array(resizable);
typed.set([7, 8, 9]);
check(typed.join({ toString() { resizable.resize(1); gc(); return ':'; } }) === '7::', 'typed join retains captured length and live resized reads');
const parts = [];
for (let i = 0; i < 1025; i++) parts.push(i % 2 ? '\ud800\0' : '\ud83d\ude00');
const text = parts.join('|');
check(text.length === 3074 && text.charCodeAt(3) === 0xd800 && text.charCodeAt(4) === 0 &&
  text.charCodeAt(text.length - 2) === 0xd83d && text.charCodeAt(text.length - 1) === 0xde00, 'growth boundaries preserve all units');
const exactParts = ['\ud800', '123456789012345'];
const exactText = exactParts.join('');
const trimmedText = ['\udfff', 'xyz'].join('');
exactParts[0] = 'changed';
for (let i = 0; i < 25; i++) { ['replacement', 'more units'].join(''); gc(); }
check(exactText.length === 16 && exactText.charCodeAt(0) === 0xd800 && exactText.slice(1) === '123456789012345', 'published exact backing remains immutable');
check(trimmedText.length === 4 && trimmedText.charCodeAt(0) === 0xdfff && trimmedText.slice(1) === 'xyz', 'spare backing is trimmed without exposing capacity');
// Concrete-array own reads must recheck every index after observable coercion.
const live = ['first', undefined, 'stale', , 'last'];
const liveProto = Object.create(Array.prototype);
let inheritedGets = 0, ownGets = 0;
Object.defineProperty(liveProto, '1', { get() { inheritedGets++; return 'wrong'; }, configurable: true });
Object.defineProperty(liveProto, '3', { get() { inheritedGets++; gc(); return 'inherited'; }, configurable: true });
Object.setPrototypeOf(live, liveProto);
live[0] = { toString() {
  Object.defineProperty(live, '2', { get() { ownGets++; gc(); return 'updated'; }, configurable: true });
  check(['nested', 'own'].join(':') === 'nested:own', 'concrete read reentrancy');
  return 'first';
} };
check(live.join('|') === 'first||updated|inherited|last' && inheritedGets === 1 && ownGets === 1,
  'own undefined shadows inherited getter; later own accessor and hole stay live');
const separatorMutation = ['old', 'tail'];
check(separatorMutation.join({ toString() { separatorMutation[0] = 'new'; gc(); return ':'; } }) === 'new:tail',
  'separator coercion precedes own-data probe');
const proxyEvents = [];
const arrayProxy = new Proxy(['a', undefined, , 'd'], {
  get(target, key, receiver) { proxyEvents.push(String(key)); return Reflect.get(target, key, receiver); },
  has() { throw 'concrete-array shortcut cannot add HasProperty'; }
});
check(Array.prototype.join.call(arrayProxy, ':') === 'a:::d' && proxyEvents.join(',') === 'length,0,1,2,3',
  'Proxy array cannot bypass ordered Get traps');
check(Array.prototype.join.call(new Uint8Array([7, 8]), ':') === '7:8', 'borrowed typed-array read fallback');
check(Array.prototype.join.call({ length: 2, 0: 'generic', 1: undefined }, ':') === 'generic:', 'generic read fallback');
print('array-private-join:ok');
262;
