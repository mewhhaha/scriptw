function check(ok, message) { if (!ok) throw message; }

const options = {}, locales = {};
const events = [];
const backing = { length: 3, 1: 'old', 2: null };
const first = { get toLocaleString() {
  events.push('method');
  check(this === first, 'method getter receiver');
  return function(actualLocales, actualOptions) {
    events.push('call');
    check(this === first && actualLocales === locales && actualOptions === options && arguments.length === 2, 'locale call operands');
    check([{ toLocaleString() { return 'nested'; } }].toLocaleString() === 'nested', 'reentrant locale');
    backing.length = 0;
    backing[1] = { toLocaleString() { events.push('second'); gc(); return '\udfff'; } };
    gc();
    return { toString() { events.push('string'); return '\0\ud800'; } };
  };
} };
backing[0] = first;
const receiver = new Proxy(backing, {
  get(target, key, actual) { events.push('get:' + key); return Reflect.get(target, key, actual); },
  has() { throw 'locale must Get through holes'; }
});
check(Array.prototype.toLocaleString.call(receiver, locales, options) === '\0\ud800,\udfff,', 'locale exact UTF16');
check(events.join('|') === 'get:length|get:0|method|call|string|get:1|second|get:2', 'locale ordered effects');

const marker = {};
let later = 0;
try { Array.prototype.toLocaleString.call({ length: 3, 0: null, 1: { toLocaleString() { return { toString() { throw marker; } }; } }, get 2() { later++; } }); throw 'locale did not throw'; }
catch (error) { check(error === marker, 'locale abrupt identity'); }
check(later === 0, 'locale stops after throw');

const original = Number.prototype.toLocaleString;
const buffer = new ArrayBuffer(3, { maxByteLength: 6 });
const typed = new Uint8Array(buffer);
typed.set([7, 8, 9]);
let calls = 0;
try {
  Number.prototype.toLocaleString = function(actualLocales, actualOptions) {
    check(actualLocales === locales && actualOptions === options, 'typed locale arguments');
    if (++calls === 1) buffer.resize(1);
    gc();
    return 'seven';
  };
  check(typed.toLocaleString(locales, options) === 'seven,,', 'typed locale captured length and fresh view reads');
  check(calls === 1, 'missing typed elements do not call locale');
} finally { Number.prototype.toLocaleString = original; }
print('array-private-locale:ok');
262;
