function check(condition, message) {
  if (!condition) throw new Error(message);
}
function name(index) { return 'wide-\u03bb-' + index; }
function collect(object) {
  var result = [];
  for (var key in object) {
    result.push(key);
    if (result.length % 8 === 0) gc();
  }
  return result.join('|');
}

var prototype = Object.create(null);
var object = Object.create(prototype);
var expected = [];
for (var index = 0; index < 48; index++) {
  prototype[name(index)] = index;
  Object.defineProperty(object, name(index), {
    get() { throw new Error('enumeration read a property value'); },
    enumerable: index % 7 !== 0,
    configurable: true
  });
  if (index % 7 !== 0) expected.push(name(index));
}
prototype.tail = 1;
object[Symbol('ignored')] = 2;
expected.push('tail');
check(collect(object) === expected.join('|'), 'wide ordinary shadowing');
check(collect(object) === expected.join('|'), 'fresh cursor after collection');

var names = [], descriptors = [];
for (var index = 0; index < 48; index++) names.push(name(index));
var missing = false, fail = false, marker = {reason: 'descriptor'};
var inherited = new Proxy(Object.create(null), {
  ownKeys() { return names.concat(['tail']); },
  getOwnPropertyDescriptor(target, key) {
    gc();
    return {value: 1, enumerable: true, configurable: true};
  },
  getPrototypeOf() { return null; }
});
var proxy = new Proxy(Object.create(null), {
  ownKeys() { return names.concat([Symbol('ignored')]); },
  getOwnPropertyDescriptor(target, key) {
    descriptors.push(key);
    gc();
    if (fail && key === name(37)) throw marker;
    if (key === 'tail' || (missing && key === name(11))) return undefined;
    return {value: 1, enumerable: key !== name(7), configurable: true};
  },
  getPrototypeOf() { return inherited; }
});
function* keys(object) {
  for (var key in object) yield key;
}
var iterator = keys(proxy), actual = [], step;
while (!(step = iterator.next()).done) {
  actual.push(step.value);
  missing = true;
  gc();
}
expected = [];
for (var index = 0; index < 48; index++) {
  if (index !== 7 && index !== 11) expected.push(name(index));
}
expected.push(name(11), 'tail');
// Proxy enumeration need not follow CreateForInIterator's exact trap/order
// sequence. Preserve the required key set and at-most-once behavior instead.
check(actual.length === expected.length, 'no duplicate or missing keys after suspension');
check(actual.slice().sort().join('|') === expected.slice().sort().join('|'),
      'missing own property exposes its inherited replacement; hidden own property shadows');
fail = true;
descriptors = [];
var caught;
try { collect(proxy); } catch (error) { caught = error; }
check(caught === marker, 'whole abrupt completion after a wide copied prefix');
check(descriptors[descriptors.length - 1] === name(37), 'abrupt completion stops subsequent descriptor observations');
print('for-in-prefix-copy:ok');
