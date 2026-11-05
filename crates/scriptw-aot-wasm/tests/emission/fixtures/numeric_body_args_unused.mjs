var rows, step;
export function prepare(input) {
  rows = [];
  for (var i = 0; i < 64; i++) {
    rows.push({x: input + i, y: input * 3 + i * 2, mask: (input + i) & 3});
  }
  var tick = input;
  step = function (value, index) {
    'use strict';
    var original = value;
    value = index + tick;
    tick = (tick + 1) & 255;
    var record = original;
    return ((record.x ^ value) + record.y + 2 + tick) & 1023;
  };
}
export function run() {
  var sum = 0;
  for (var i = 0; i < 1024; i++) sum += step(rows[i & 63], i);
  print(sum);
}
