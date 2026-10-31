function check(ok, message) { if (!ok) throw message; }

class OwnedArray extends Array {
  #state = { owner: this, value: 17 };
  readState() { return this.#state; }
  replaceState() { this.#state = { owner: this, value: 23 }; }
}
const readState = OwnedArray.prototype.readState;
const replaceState = OwnedArray.prototype.replaceState;
const array = new OwnedArray(-0, NaN, Infinity, 4);
const symbol = Symbol('owner');
array.note = { owner: array };
array[symbol] = array;
Object.defineProperty(array, 'hidden', { value: 19, configurable: true });
const prototype = Object.create(Array.prototype);
prototype.inherited = 29;
Object.setPrototypeOf(array, prototype);
gc();
check(Array.isArray(array) && Object.getPrototypeOf(array) === prototype && array.inherited === 29 &&
  Object.is(array[0], -0) && Number.isNaN(array[1]) && array[2] === Infinity && array[3] === 4,
  'ordinary properties and prototype changes preserve numeric indexed storage');
check(array.note.owner === array && array[symbol] === array && array.hidden === 19 &&
  readState.call(array).owner === array && readState.call(array).value === 17,
  'ordinary, symbol and private roots retain the same array identity');

array[2] = array.note;
array[3] = array;
Object.defineProperty(array, '1', {
  value: readState.call(array), writable: false, enumerable: false, configurable: true
});
delete array.hidden;
array.note = null;
array.length = 4294967295;
gc();
const indexedState = Object.getOwnPropertyDescriptor(array, '1');
check(indexedState.value === readState.call(array) && !indexedState.writable &&
  !indexedState.enumerable && indexedState.configurable && !Reflect.set(array, '1', null),
  'descriptor promotion retains the private object and its attributes');
check(Object.is(array[0], -0) && array[1].owner === array && array[2].owner === array &&
  array[3] === array && array[symbol] === array && array.length === 4294967295 && !(4 in array) &&
  Object.getOwnPropertyNames(array).join(',') === '0,1,2,3,length,note',
  'promoted cyclic values and occupied indices survive independently of logical length');

Object.freeze(array);
replaceState.call(array);
gc();
check(Object.isFrozen(array) && !Reflect.set(array, '0', 1) && !Reflect.deleteProperty(array, '3') &&
  readState.call(array).owner === array && readState.call(array).value === 23 &&
  array[1].value === 17 && array[3] === array && Object.is(array[0], -0),
  'private replacement on a frozen array preserves its distinct indexed roots');

function capture(strings) { gc(); return strings; }
function template() { return capture`head\n${0}tail`; }
const strings = template();
const raw = strings.raw;
gc();
check(template() === strings && strings.raw === raw && Array.isArray(strings) && Array.isArray(raw) &&
  Object.isFrozen(strings) && Object.isFrozen(raw) && strings.length === 2 && raw.length === 2 &&
  strings[0] === 'head\n' && raw[0] === 'head\\n' && strings[1] === 'tail' && raw[1] === 'tail',
  'cached cooked and raw template arrays retain identity and frozen indexed values');
const rawDescriptor = Object.getOwnPropertyDescriptor(strings, 'raw');
const elementDescriptor = Object.getOwnPropertyDescriptor(raw, '0');
check(rawDescriptor.value === raw && !rawDescriptor.writable && !rawDescriptor.enumerable &&
  !rawDescriptor.configurable && !elementDescriptor.writable && elementDescriptor.enumerable &&
  !elementDescriptor.configurable && !Reflect.set(strings, '0', 'changed') &&
  !Reflect.defineProperty(raw, '0', { value: 'changed' }) && !Reflect.deleteProperty(strings, 'raw'),
  'template array descriptors reject mutation through the ordinary object API');
gc();
check(template() === strings && strings.raw === raw && strings[0] === 'head\n' && raw[0] === 'head\\n',
  'rejected mutations and collection preserve both template arrays');

print('array-fused-owner-lifecycle:ok');
262;
