function check(ok, message) { if (!ok) throw message; }

// Occupancy must track deletions, refills and length changes, not just capacity.
const refilled = ['first', 'discarded', 'last'];
delete refilled[1];
refilled[1] = undefined;
let inheritedGets = 0;
const prototype = Object.create(Array.prototype);
Object.defineProperty(prototype, '1', { configurable: true, get() {
  inheritedGets++; gc(); return 'inherited';
} });
Object.setPrototypeOf(refilled, prototype);
gc();
check(refilled.join('|') === 'first||last' && inheritedGets === 0,
  'refilled own undefined remains present and shadows the prototype');
delete refilled[1];
check(refilled.join('|') === 'first|inherited|last' && inheritedGets === 1,
  'deletion invalidates the dense proof even while capacity is unchanged');
Object.defineProperty(refilled, '1', {
  value: null, writable: true, enumerable: true, configurable: true
});
check(refilled.join('|') === 'first||last' && inheritedGets === 1,
  'refilling a deleted slot restores dense own data');

const regrown = ['start', 'stale middle', 'stale end'];
regrown.length = 1;
regrown.length = 3;
regrown[2] = 'end';
Object.setPrototypeOf(regrown, prototype);
check(regrown.join(':') === 'start:inherited:end' && inheritedGets === 2,
  'shrink and regrow cannot expose stale values from the retained capacity');
Object.defineProperty(regrown, '1', {
  value: undefined, writable: true, enumerable: true, configurable: true
});
gc();
check(regrown.join(':') === 'start::end' && inheritedGets === 2,
  'all regrown indices are dense only after the last own slot is refilled');

const reshaped = ['before', 'stale middle', 'stale end'];
Object.setPrototypeOf(reshaped, prototype);
let separatorConversions = 0;
check(reshaped.join({ toString() {
  separatorConversions++;
  reshaped.length = 1;
  reshaped.length = 3;
  reshaped[2] = 'after';
  gc();
  return '|';
} }) === 'before|inherited|after' && separatorConversions === 1 && inheritedGets === 3,
  'separator shrink and regrow preserve captured length but invalidate dense occupancy');

// A late unsupported value must restart ordinary joining at index zero. Its
// conversion can change both an already-read prefix and a later indexed slot.
let trace = '';
const late = ['prefix', null, undefined, null, 'stale'];
late[3] = { toString() {
  trace += 'element;';
  late[0] = 'changed too late';
  Object.defineProperty(late, '4', { configurable: true, get() {
    trace += 'later;'; gc(); return 'fresh';
  } });
  check(['nested', 'join'].join(':') === 'nested:join', 'fallback can reenter packed join');
  gc();
  return 'object';
} };
const text = late.join({ toString() { trace += 'separator;'; gc(); return '|'; } });
check(text === 'prefix|||object|fresh' && trace === 'separator;element;later;',
  'fallback retains the original prefix, converts once and observes the later accessor');
gc();
check(text === 'prefix|||object|fresh', 'published fallback result survives collection');
print('array-packed-join-dense-prefix:ok');
262;
