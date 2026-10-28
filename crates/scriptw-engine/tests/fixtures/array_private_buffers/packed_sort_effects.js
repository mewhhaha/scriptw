function check(ok, message) { if (!ok) throw message; }
function ascending(a, b) { return a - b; }
function typeError(action) {
  let caught = false;
  try { action(); } catch (error) { caught = error instanceof TypeError; }
  check(caught, 'expected TypeError');
}

const numeric = [3, -0, 0, 2, 1, undefined];
check(numeric.sort(ascending) === numeric && Object.is(numeric[0], -0) && Object.is(numeric[1], 0) &&
  numeric[2] === 1 && numeric[3] === 2 && numeric[4] === 3 && numeric[5] === undefined,
  'ordinary binary numeric comparator keeps stable zeros and undefined ordering');
const arrow = [3, 1, 2].toSorted((a, b) => a - b);
check(arrow[0] === 1 && arrow[1] === 2 && arrow[2] === 3, 'arrow binary numeric comparator');
const nan = [3, 2, 1].toSorted(function(a, b) { return 0 / 0; });
check(nan[0] === 3 && nan[1] === 2 && nan[2] === 1, 'typed NaN comparator remains stable equality');
for (const array of [Object.preventExtensions([3, 2, 1]), [3, 2, 1]]) {
  Object.defineProperty(array, 'length', { writable: false });
  check(array.sort(ascending) === array && array[0] === 1 && array[1] === 2 && array[2] === 3,
    'own writable replacements need neither extensibility nor writable length');
}

let first = true;
const shrunk = [3, 2, 1];
typeError(function() { shrunk.sort(function(a, b) {
  if (first) {
    first = false; shrunk.length = 1;
    Object.defineProperty(shrunk, 'length', { writable: false });
    gc();
  }
  return a - b;
}); });
check(shrunk.length === 1 && shrunk[0] === 1 && !Object.hasOwn(shrunk, '1') && !Object.hasOwn(shrunk, '2'),
  'shrink then readonly length permits only the first ordered write');
first = true;
const readonlyIndex = [3, 2, 1];
typeError(function() { readonlyIndex.sort(function(a, b) {
  if (first) { first = false; Object.defineProperty(readonlyIndex, '1', { value: 22, writable: false }); gc(); }
  return a - b;
}); });
check(readonlyIndex.length === 3 && readonlyIndex[0] === 1 && readonlyIndex[1] === 22 && readonlyIndex[2] === 1,
  'readonly second index preserves the first write and untouched final index');

first = true;
let setterTrace = '';
const accessor = [3, 2, 1];
typeError(function() { accessor.sort(function(a, b) {
  if (first) {
    first = false;
    Object.defineProperty(accessor, '0', { configurable: true, get() { return 90; }, set(value) { setterTrace += value + ';'; gc(); } });
    Object.defineProperty(accessor, '1', { value: 88, writable: false });
  }
  return a - b;
}); });
check(setterTrace === '1;' && accessor[0] === 90 && accessor[1] === 88 && accessor[2] === 1,
  'accessor transition falls back from index zero without any speculative write');

first = true;
const frozen = [3, 2, 1];
typeError(function() { frozen.sort(function(a, b) {
  if (first) { first = false; Object.freeze(frozen); gc(); }
  return a - b;
}); });
check(frozen[0] === 3 && frozen[1] === 2 && frozen[2] === 1, 'freeze rejects before first writeback');
first = true;
const sealed = [3, 2, 1];
sealed.sort(function(a, b) { if (first) { first = false; Object.seal(sealed); gc(); } return a - b; });
const sealedDescriptor = Object.getOwnPropertyDescriptor(sealed, '0');
check(sealed[0] === 1 && sealed[1] === 2 && sealed[2] === 3 && sealedDescriptor.writable && !sealedDescriptor.configurable,
  'sealed own writable data sorts without changing attributes');
first = true;
const attributes = [3, 2, 1];
attributes.sort(function(a, b) {
  if (first) { first = false; Object.defineProperty(attributes, '0', { enumerable: false }); }
  return a - b;
});
check(attributes[0] === 1 && attributes[1] === 2 && attributes[2] === 3 &&
  !Object.getOwnPropertyDescriptor(attributes, '0').enumerable, 'writeback preserves callback attribute changes');

first = true;
let inheritedWrite = '';
const deleted = [3, 2, 1];
const livePrototype = Object.create(Array.prototype);
deleted.sort(function(a, b) {
  if (first) {
    first = false; delete deleted[1];
    Object.defineProperty(livePrototype, '1', { configurable: true, set(value) { inheritedWrite += value + ';'; } });
    Object.setPrototypeOf(deleted, livePrototype);
    gc();
  }
  return a - b;
});
check(deleted[0] === 1 && !Object.hasOwn(deleted, '1') && deleted[2] === 3 && inheritedWrite === '2;',
  'callback deletion preserves inherited setter interception');
first = true;
const grown = [3, 2, 1];
grown.sort(function(a, b) { if (first) { first = false; grown[3] = 'tail'; gc(); } return a - b; });
check(grown.length === 4 && grown[0] === 1 && grown[1] === 2 && grown[2] === 3 && grown[3] === 'tail',
  'writeback retains length and tail added by comparator');

let sparseTrace = '', comparing = false;
const sparse = [3, , 1];
const sparsePrototype = Object.create(Array.prototype);
Object.defineProperty(sparsePrototype, '1', { configurable: true,
  get() { check(!comparing, 'indexed reads finish before comparison'); sparseTrace += 'get;'; gc(); return 2; },
  set(value) { sparseTrace += 'set:' + value + ';'; }
});
Object.setPrototypeOf(sparse, sparsePrototype);
sparse.sort(function(a, b) { comparing = true; return a - b; });
check(sparse[0] === 1 && !Object.hasOwn(sparse, '1') && sparse[2] === 3 && sparseTrace === 'get;set:2;',
  'sparse inherited gather and ordered writeback');
first = true;
const sparseDelete = [3, 1];
sparseDelete.length = 5;
typeError(function() { sparseDelete.sort(function(a, b) {
  if (first) {
    first = false;
    sparseDelete[2] = 88;
    Object.defineProperty(sparseDelete, '3', { value: 99, writable: true, enumerable: true, configurable: false });
    sparseDelete[4] = 77;
    gc();
  }
  return a - b;
}); });
check(sparseDelete.length === 5 && sparseDelete[0] === 1 && sparseDelete[1] === 3 &&
  !Object.hasOwn(sparseDelete, '2') && sparseDelete[3] === 99 && sparseDelete[4] === 77,
  'trailing delete failure preserves preceding writes/delete and untouched later index');

first = true;
let prototypeWrites = 0;
const copiedSource = [3, 2, 1];
const copied = copiedSource.toSorted(function(a, b) {
  if (first) {
    first = false;
    Object.defineProperty(Array.prototype, '0', { configurable: true, set() { prototypeWrites++; } });
    gc();
  }
  return a - b;
});
delete Array.prototype[0];
check(prototypeWrites === 0 && copied[0] === 1 && copied[1] === 2 && copied[2] === 3 &&
  copiedSource[0] === 3 && copiedSource[1] === 2 && copiedSource[2] === 1,
  'fresh toSorted publication bypasses inherited setters without changing source');

first = true;
let coercions = 0;
const objects = [];
function entry(number) { return { [Symbol.toPrimitive](hint) {
  check(hint === 'number', 'numeric comparator retains ordinary ToPrimitive hint');
  coercions++;
  if (first) {
    first = false; objects.length = 0; gc();
    check([2, 1].sort(ascending)[0] === 1, 'numeric coercion reenters sort');
  }
  return number;
} }; }
const high = entry(3), low = entry(1), middle = entry(2);
objects.push(high, low, middle);
objects.sort(ascending);
gc();
check(coercions > 0 && objects.length === 3 && objects[0] === low && objects[1] === middle && objects[2] === high,
  'unsupported numeric operands keep coercion, reentry and rooted original snapshots');
first = true;
const lexical = [];
const z = { toString() {
  if (first) { first = false; lexical.length = 0; gc(); check([2, 1].sort(ascending)[0] === 1, 'default comparison reentry'); }
  return 'z';
} };
const a = { toString() { return 'a'; } };
lexical.push(z, a);
lexical.sort();
check(lexical.length === 2 && lexical[0] === a && lexical[1] === z, 'default String conversion preserves original objects after storage retirement');

const symbol = Symbol('operand'), symbols = [2, symbol];
typeError(function() { symbols.sort(ascending); });
check(symbols[0] === 2 && symbols[1] === symbol, 'Symbol callback failure precedes every write');
const bigints = [2n, 1n];
typeError(function() { bigints.sort(ascending); });
check(bigints[0] === 2n && bigints[1] === 1n, 'BigInt callback result still goes through throwing ToNumber');
print('array-packed-sort-effects:ok');
262;
