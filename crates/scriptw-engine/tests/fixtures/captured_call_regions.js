var rows, steps, readTick, writeTick;
function prepare(input) {
  rows = [];
  for (var i = 0; i < 4; i++) rows.push({x: input + i, y: input * 3 + i * 2, mask: (input + i) & 3});
  var tick = input;
  steps = [function (row) {
    tick = (tick + 1) & 255;
    var value = row.x + tick;
    if (row.mask & 1) value = value ^ row.y;
    return value & 1023;
  }, function (row) {
    tick = (tick + 3) & 255;
    var value = row.y + tick;
    if (row.mask & 2) value = value ^ row.x;
    return value & 1023;
  }];
  readTick = function () { return tick; };
  writeTick = function (value) { tick = value; };
}
function frame(index, row) { return steps[index & 1](row); }
function run() {
  var sum = 0;
  for (var i = 0; i < 32; i++) sum += frame(i, rows[i & 3]);
  return sum;
}
function expected(a, b, shared, same) {
  var sum = 0;
  for (var i = 0; i < 32; i++) {
    var row = rows[i & 3], value;
    if ((i & 1) === 0 || same) {
      a = (a + 1) & 255;
      value = row.x + a;
      if (row.mask & 1) value = value ^ row.y;
      if (shared) b = a;
    } else {
      b = (b + 3) & 255;
      value = row.y + b;
      if (row.mask & 2) value = value ^ row.x;
      if (shared) a = b;
    }
    sum += value & 1023;
  }
  return [sum, a, b];
}
function check(ok, message) { if (!ok) throw new Error(message); }
for (var input of [7, -3, -0, 1.5, 2147483647, NaN, Infinity]) {
  prepare(input);
  var want = expected(input, input, true, false);
  check(run() === want[0] && readTick() === want[1], 'shared cells and numeric values');
  want = expected(want[1], want[1], true, false);
  check(run() === want[0] && readTick() === want[1], 'published retained state');
}
prepare(3);
var old = steps[0], oldRead = readTick;
prepare(11);
steps[0] = old;
var want = expected(3, 11, false, false);
check(run() === want[0] && oldRead() === want[1] && readTick() === want[2], 'distinct environments of identical code');
prepare(3);
steps[1] = steps[0];
want = expected(3, 3, true, true);
check(run() === want[0] && readTick() === want[1], 'same callable in both slots');
prepare(5);
var trace = [];
Object.defineProperty(rows[0], 'x', {get: function () { trace.push(readTick()); writeTick(100); return 5; }});
run();
check(trace.length === 8 && trace[0] === 6 && readTick() === 107, 'getter decline precedes every source write');
prepare(5);
trace = [];
rows[0] = new Proxy(rows[0], {get: function (target, key, receiver) {trace.push(key);return Reflect.get(target,key,receiver);}});
run();
check(trace.length === 24 && trace.slice(0,3).join(',') === 'x,mask,y', 'Proxy evaluation order and no speculative traps');
prepare(5);
delete rows[1];
var caught = false;
try { run(); } catch (error) { caught = error instanceof TypeError; }
check(caught && readTick() === 9, 'hole retains completed prefix');
prepare(5);
rows[0].x = Symbol();
caught = false;
try { run(); } catch (error) { caught = error instanceof TypeError; }
check(caught && readTick() === 6, 'non-Number coercion and partial write');
prepare(5);
var calls = 0;
steps[0] = function (row) { calls++; return 5; };
run();
check(calls === 16 && readTick() === 53, 'replacement retains actual callable');
gc();
check(readTick() === 53, 'published cell survives collection');
print('captured call regions ok');
