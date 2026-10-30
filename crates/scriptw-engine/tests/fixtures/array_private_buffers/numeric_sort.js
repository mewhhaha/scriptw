function check(ok, message) { if (!ok) throw message; }
function ascending(a, b) { return a - b; }
function descending(a, b) { return b - a; }
function same(actual, expected, message) {
  check(actual.length === expected.length, message + ': length');
  for (let i = 0; i < actual.length; i++) check(Object.is(actual[i], expected[i]), message + ':' + i);
}
function reference(values) {
  const result = values.slice();
  for (let i = 1; i < result.length; i++) {
    const value = result[i];
    let j = i;
    while (j && result[j - 1] > value) { result[j] = result[j - 1]; j--; }
    result[j] = value;
  }
  return result;
}
for (const size of [0, 1, 2, 3, 5, 16, 65, 257]) {
  const values = [];
  for (let i = 0; i < size; i++) values.push(((i * 173 + size * 7) % 31) - 15);
  const expected = reference(values), original = values.slice();
  same(values.toSorted(ascending), expected, 'copying numeric merge');
  same(values, original, 'source snapshot remains independent');
  check(values.sort(ascending) === values, 'sort returns original receiver');
  same(values, expected, 'stable numeric merge');
  const backward = [];
  for (let i = size; i > 0; i--) backward.push(i);
  const forward = reference(backward);
  same(backward.toSorted(ascending), forward, 'strict descending natural run');
  same(forward.toSorted(descending), backward, 'descending callback');
}
const zeros = [3, -0, 0, -0, 0, -2, 1];
same(zeros.toSorted(ascending), reference(zeros), 'stable signed-zero equality');
const special = [NaN, 3, -0, 0, Infinity, -Infinity, NaN];
same(special.toSorted(function(a, b) { return a - a; }), special, 'NaN comparison is stable equality');
for (const value of [Object.preventExtensions([3, 1, 2]), [3, 1, 2]]) {
  Object.defineProperty(value, 'length', {writable: false});
  same(value.sort(ascending), [1, 2, 3], 'writable own values need no length change');
}
const frozen = Object.freeze([3, 1, 2]);
let failed = false;
try { frozen.sort(ascending); } catch (error) { failed = error instanceof TypeError; }
check(failed && frozen[0] === 3 && frozen[1] === 1, 'readonly values retain throwing writeback');
same([10, 2, 1].toSorted(), [1, 10, 2], 'undefined comparator stays lexicographic');
const sparse = [3, , 1];
same(sparse.toSorted(ascending), [1, 3, undefined], 'holes keep read-through semantics');
const effects = [3, 2, 1];
let touched = false;
effects.sort(function(a, b) { if (!touched) { touched = true; effects.length = 0; gc(); } return a - b; });
same(effects, [1, 2, 3], 'effectful callback uses independent generic snapshots');
class SubArray extends Array {}
const sub = new SubArray(3, 1, 2);
check(sub.sort(ascending) === sub, 'subclass receiver identity');
same(sub, [1, 2, 3], 'subclass numeric storage');
const foreign = __scriptwCreateRealm().global;
const foreignCopy = foreign.Array.prototype.toSorted.call([3, 1, 2], ascending);
check(Object.getPrototypeOf(foreignCopy) === foreign.Array.prototype, 'copy defining Realm');
same(foreignCopy, [1, 2, 3], 'cross-Realm scalar callback');
gc();
same(foreignCopy, [1, 2, 3], 'private roots survive collection');
print('array-unboxed-numeric-sort:ok');
262;
