function check(ok, message) { if (!ok) throw message; }

const bits = [-0, NaN, Infinity, -Infinity, 0];
bits[4] = -0;
gc();
check(Object.is(bits[0], -0) && Number.isNaN(bits[1]) && bits[2] === Infinity &&
  bits[3] === -Infinity && Object.is(bits[4], -0), 'dense reads and overwrites retain Number bits');

// The first hole must retain every occupied bit, including complete words.
for (const count of [63, 64, 65, 127, 128, 129]) {
  const array = [];
  for (let i = 0; i < count; i++) array.push(i);
  const missing = count - 2;
  delete array[missing];
  gc();
  check(array.length === count && !(missing in array), 'interior deletion across presence words');
  for (let i = 0; i < count; i++) {
    if (i !== missing) check(array[i] === i, 'first hole preserves every other Number');
  }
  array[missing] = -0;
  check(Object.is(array[missing], -0) && Reflect.ownKeys(array).length === count + 1,
    'hole refill restores exact occupancy without losing negative zero');
}

const grown = [];
for (let i = 0; i < 130; i++) grown.push(i);
grown.length = 65;
delete grown[64];
grown.length = 130;
delete grown[129];
grown[64] = 640;
grown[127] = 1270;
gc();
check(grown.length === 130 && grown[63] === 63 && grown[64] === 640 && grown[127] === 1270 &&
  !(65 in grown) && !(128 in grown) && !(129 in grown) && Reflect.ownKeys(grown).length === 67,
  'tail deletion and logical regrowth never resurrect spare-capacity values');

const huge = [1, 2, 3, 4];
huge.length = 4294967295;
delete huge[3];
delete huge[4294967294];
huge[3] = 30;
gc();
check(huge.length === 4294967295 && huge[3] === 30 && !(4 in huge) &&
  Reflect.ownKeys(huge).join(',') === '0,1,2,3,length',
  'prefix extent stays independent of maximal logical length');
const firstZero = [], firstGap = [], firstFar = [];
firstZero.length = firstGap.length = firstFar.length = 1000000;
firstZero[0] = 10;
firstGap[7] = 70;
firstFar[1048576] = 99;
gc();
check(firstZero[0] === 10 && !(1 in firstZero) && firstGap[7] === 70 && !(0 in firstGap) &&
  firstFar[1048576] === 99 && !(1048575 in firstFar),
  'empty logical arrays admit first contiguous, gapped and distant writes separately');

let setterTrace = '';
const prototype = Object.create(Array.prototype);
const intercepted = [1, 2, 3, 4];
intercepted.length = 1;
intercepted.length = 4;
Object.defineProperty(prototype, '1', { configurable: true,
  get() { return 77; },
  set(value) { setterTrace += value + ';'; intercepted[0] = 9; gc(); }
});
Object.defineProperty(prototype, '2', { configurable: true, get() { return 88; } });
Object.setPrototypeOf(intercepted, prototype);
intercepted[1] = 7;
Object.defineProperty(intercepted, '2', {
  value: 6, writable: true, enumerable: true, configurable: true
});
intercepted[3] = 5;
check(setterTrace === '7;' && intercepted[0] === 9 && intercepted[1] === 77 &&
  intercepted[2] === 6 && intercepted[3] === 5 && !Object.hasOwn(intercepted, '1') &&
  Reflect.ownKeys(intercepted).join(',') === '0,2,3,length',
  'regrown gaps retain inherited Set while default own definitions bypass it');

const promoted = [NaN, -0, Infinity, -Infinity];
delete promoted[1];
promoted[2] = undefined;
promoted[3] = 'tail';
const marker = { value: 42 };
promoted[0] = marker;
gc();
check(promoted[0] === marker && !(1 in promoted) && Object.hasOwn(promoted, '2') &&
  promoted[2] === undefined && promoted[3] === 'tail' &&
  Reflect.ownKeys(promoted).join(',') === '0,2,3,length',
  'Number promotion preserves a hole, own undefined and rooted references');
const directPromotion = [-0, NaN, Infinity, -Infinity];
directPromotion[2] = marker;
gc();
check(Object.is(directPromotion[0], -0) && Number.isNaN(directPromotion[1]) &&
  directPromotion[2] === marker && directPromotion[3] === -Infinity,
  'promotion directly from a dense prefix preserves all occupied values');

const attributes = [10, -0, NaN, 40];
Object.defineProperty(attributes, '1', { writable: false, enumerable: false });
gc();
const descriptor = Object.getOwnPropertyDescriptor(attributes, '1');
check(Object.is(descriptor.value, -0) && !descriptor.writable && !descriptor.enumerable &&
  descriptor.configurable && !Reflect.set(attributes, '1', 2) && Number.isNaN(attributes[2]),
  'descriptor materialization preserves Number bits and requested attributes');
const frozen = [-0, NaN, Infinity, -Infinity];
Object.freeze(frozen);
gc();
check(Object.is(frozen[0], -0) && Number.isNaN(frozen[1]) && frozen[2] === Infinity &&
  frozen[3] === -Infinity && !Reflect.set(frozen, '0', 1) && !Reflect.deleteProperty(frozen, '3'),
  'freezing a dense prefix retires its writable storage');

const sorting = [];
for (let i = 65; i > 0; i--) sorting.push(i);
let first = true;
sorting.sort(function(a, b) {
  if (first) {
    first = false;
    sorting.length = 63;
    sorting.length = 65;
    sorting[64] = 'stale';
    gc();
  }
  return a - b;
});
check(sorting.length === 65 && sorting[0] === 1 && sorting[62] === 63 &&
  sorting[63] === 64 && sorting[64] === 65,
  'sort revalidates shrink, gap and promotion before complete ordered writeback');
print('array-dense-number-prefix:ok');
262;
