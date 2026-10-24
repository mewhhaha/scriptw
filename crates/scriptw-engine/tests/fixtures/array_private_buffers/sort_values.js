function check(ok, message) { if (!ok) throw message; }

const original = [];
for (let i = 0; i < 257; i++) original.push({ key: (256 - i) % 11, ordinal: i });
let calls = 0;
const sorted = original.toSorted(function(left, right) {
  if (++calls % 31 === 0) gc();
  return left.key - right.key;
});
// This is an implementation complexity guard, not an ECMAScript requirement.
check(calls > 0 && calls < 4096, 'bounded sort comparisons');
for (let i = 0; i < sorted.length; i++) {
  const value = sorted[i];
  check(value === original[value.ordinal], 'retained object identity');
  if (i) check(sorted[i - 1].key < value.key ||
    (sorted[i - 1].key === value.key && sorted[i - 1].ordinal < value.ordinal), 'stable order');
  check(original[i].ordinal === i, 'toSorted leaves source order');
}
const receiver = [];
for (let i = 256; i >= 0; i--) receiver.push(i);
calls = 0;
check(receiver.sort(function(a, b) { calls++; return a - b; }) === receiver, 'sort identity');
check(calls < 4096, 'reverse sort bounded comparisons');
for (let i = 0; i < receiver.length; i++) check(receiver[i] === i, 'numeric order');
calls = 0;
receiver.sort(function(a, b) { calls++; return a - b; });
check(calls === receiver.length - 1, 'already ordered input retains linear comparisons');

const nan = [3, -0, 1, 0].toSorted(function() { return NaN; });
check(nan[0] === 3 && Object.is(nan[1], -0) && nan[2] === 1 && Object.is(nan[3], 0), 'NaN is stable equality');
const empty = [];
check(empty.sort() === empty && empty.toSorted().length === 0, 'empty lists');
const one = { name: 'one' };
check([one].toSorted(function() { throw 'singleton compared'; })[0] === one, 'singleton');
const unicode = ['\udfff', '\ud83d\ude00', 'a', '\ud800', '\0', undefined, , ];
const lexical = unicode.toSorted();
check(lexical[0] === '\0' && lexical[1] === 'a' && lexical[2] === '\ud800' &&
  lexical[3] === '\ud83d\ude00' && lexical[4] === '\udfff' && lexical[5] === undefined &&
  lexical[6] === undefined && Object.hasOwn(lexical, 6), 'UTF16 default and read-through holes');
unicode.sort();
check(unicode[5] === undefined && Object.hasOwn(unicode, 5) && !Object.hasOwn(unicode, 6), 'sort skips holes');
print('array-private-sort-values:ok');
262;
