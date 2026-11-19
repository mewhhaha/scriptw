// Strict sources whose own arguments object only literal-index and length
// reads observe enter captured-call regions with positional snapshots.
var rows = [], step, tick = 3;
for (var r = 0; r < 8; r++) rows.push({x: r * 3 + 1, y: r * 5 + 2});
function make(start) {
  var count = start;
  return function (row, index) {
    'use strict';
    var kept = row;
    row = index * 2;
    count = (count + 1) & 63;
    var record = arguments[0];
    return record.x + kept.y + arguments[1] * 3 + arguments.length + row + count;
  };
}
function numbers(value, index) {
  'use strict';
  value = value * 2;
  index = 0;
  return arguments[0] + arguments[1] * 3 + arguments.length + value + index;
}
function computed(row, index) { 'use strict'; return arguments[index & 1] === row ? row.x : index; }
function run() { var sum = 0; for (var i = 0; i < 64; i++) sum += step(rows[i & 7], i); return sum; }
function runNumbers() { var sum = 0; for (var i = 0; i < 64; i++) sum += step(i & 7, i); return sum; }
var results = [];
step = make(5); results.push(run(), run());
step = numbers; results.push(runNumbers());
step = computed; results.push(run());
step = make(1); results.push(run());
step = function (row, index) { return row.x + index; }; results.push(run());
if (results.join() !== "14208,14208,6848,1344,14208,2752") throw new Error(results.join());
print("captured call arguments ok");
