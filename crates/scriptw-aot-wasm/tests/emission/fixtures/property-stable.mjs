var rows, keys;
export function prepare(input) {
  rows = [];
  for (var i = 0; i < 64; i++) {
    rows.push({x: input + i, y: input * 3 + i * 2, mask: (input + i) & 3});
  }
  keys = ['x', 'y', 'prefix-x'.slice(7), String.fromCharCode(121)];

}
export function run() {
  var sum = 0;
  for (var i = 0; i < 8192; i++) sum += rows[i & 63][keys[i & 3]];
  print(sum);
}
