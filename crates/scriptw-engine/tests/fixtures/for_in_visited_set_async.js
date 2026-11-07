function check(condition, message) { if (!condition) throw new Error(message); }
function name(index) { return ('discard:key-' + index).slice(8); }
var parent = Object.create(null), child = Object.create(parent), expected = [];
for (var index = 0; index < 72; index++) {
  parent[name(index)] = index;
  Object.defineProperty(child, name(index), {
    value: index, enumerable: index % 9 !== 0, configurable: true
  });
  if (index % 9 !== 0) expected.push(name(index));
}
parent.tail = 1;
expected.push('tail');
function forbidden() { throw new Error('public Set hook'); }
Set.prototype.has = forbidden;
Set.prototype.add = forbidden;
globalThis.Set = forbidden;
async function collect(value) {
  var result = [];
  for (const key in await Promise.resolve(value)) {
    await Promise.resolve(0); gc(); result.push(key);
    for (const nested in {inside: 1}) check(nested === 'inside', 'nested async cursor');
  }
  return result;
}
async function* keys(value) {
  for (const key in await Promise.resolve(value)) {
    yield key; await Promise.resolve(0); gc();
  }
}
async function run() {
  check((await collect(child)).join('|') === expected.join('|'), 'Await retains wide Set and hidden shadows');
  var iterator = keys(child), result = [], step = await iterator.next();
  while (!step.done) {
    result.push(step.value); gc();
    step = await iterator.next();
  }
  check(result.join('|') === expected.join('|'), 'Yield and Await share one retained Set');
  // Deletion before the live descriptor does not mark a name visited. A later
  // prototype exposes it; deletion/reinsertion before inspection remains visible.
  var base = {deleted: 1, recreated: 2}, value = Object.create(base);
  value.first = 1; value.deleted = 3; value.recreated = 4;
  iterator = keys(value);
  check((await iterator.next()).value === 'first', 'mutation starts after first body');
  delete value.deleted;
  delete value.recreated;
  value.recreated = 5;
  gc(); result = [];
  step = await iterator.next();
  while (!step.done) { result.push(step.value); step = await iterator.next(); }
  check(result.slice().sort().join('|') === 'deleted|recreated', 'live deletion and re-add after suspension');
  iterator = keys(child);
  await iterator.next();
  var marker = {retired: 1};
  var done = await iterator.return(marker);
  check(done.done && done.value === marker && (await iterator.next()).done, 'return retires the retained Set');
}
run().then(function () { print('for-in-visited-set-async:ok'); }, function (error) { print(error); throw error; });
