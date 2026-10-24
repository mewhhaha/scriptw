function check(ok, message) { if (!ok) throw message; }

const events = [];
const backing = { length: 4, 0: 3, 2: 1, 3: undefined };
let comparing = false;
const receiver = new Proxy(backing, {
  get(target, key, actual) { if (!comparing) events.push('get:' + key); return Reflect.get(target, key, actual); },
  has(target, key) { check(!comparing, 'has before comparisons'); events.push('has:' + key); return Reflect.has(target, key); },
  set(target, key, value) { check(comparing, 'write after comparisons'); events.push('set:' + key); return Reflect.set(target, key, value); },
  deleteProperty(target, key) { check(comparing, 'delete after comparisons'); events.push('delete:' + key); return Reflect.deleteProperty(target, key); }
});
const compare = new Proxy(function() { throw 'proxy target called'; }, {
  apply(target, that, args) {
    check(that === undefined && args.length === 2, 'comparator receiver and arity');
    check(args[0] !== undefined && args[1] !== undefined, 'undefined bypasses comparator');
    if (!comparing) {
      check(events.join('|') === 'get:length|has:0|get:0|has:1|has:2|get:2|has:3|get:3', 'complete ordered source snapshot');
      comparing = true;
      backing[0] = 99;
      gc();
      check([2, 1].sort(function(a, b) { return a - b; })[0] === 1, 'reentrant sort');
    }
    return { valueOf() { gc(); return args[0] - args[1]; } };
  }
});
check(Array.prototype.sort.call(receiver, compare) === receiver, 'proxy identity');
check(backing[0] === 1 && backing[1] === 3 && backing[2] === undefined && !Object.hasOwn(backing, 3), 'snapshot writes and trailing deletion');
check(events.slice(-4).join('|') === 'set:0|set:1|set:2|delete:3', 'writeback order');

const marker = {};
const abrupt = [4, 3, 2, 1];
let assigned = 'before', conversions = 0;
try {
  assigned = abrupt.sort(function() { return { valueOf() { conversions++; gc(); throw marker; } }; });
  throw 'sort did not throw';
} catch (error) { check(error === marker, 'comparison abrupt identity'); }
check(assigned === 'before' && conversions === 1 && abrupt.join(',') === '4,3,2,1', 'no writeback after comparison throw');
let gets = 0;
try { Array.prototype.toSorted.call({ length: 2, get 0() { gets++; throw marker; }, get 1() { throw 'later read'; } }); }
catch (error) { check(error === marker, 'read abrupt identity'); }
check(gets === 1, 'read stops at first throw');
print('array-private-sort-effects:ok');
262;
