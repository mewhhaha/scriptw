var rows, steps;
export function prepare(input) {
  rows = [];
  for (var i = 0; i < 64; i++) {
    rows.push({x: input + i, y: input * 3 + i * 2, mask: (input + i) & 3});
  }
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
}
function frame(index, row) {
  return steps[index & 1](row);
}
export function run() {
  var sum = 0;
  for (var call = 0; call < 16; call++) {
    for (var i = 0; i < 512; i++) sum += frame(i, rows[i & 63]);
  }
  print(sum);
}
