var left, right, seed;
export function prepare(input) {
  seed = input;
  left = [input, input, -0, -0, NaN, Infinity, -Infinity, 2.5,
    input + 0.25, input + 1, 0, -1, NaN, input * 2, Infinity, -Infinity];
  right = [input, input + 1, 0, -0, NaN, Infinity, Infinity, 2.5,
    input + 0.5, input + 1, NaN, -1, 0, input * 2, -Infinity, -Infinity];
}
export function run() {
  var sum = seed * 17;
  for (var i = 0; i < 8192; i++) {
    var slot = (i + seed) & 15;
    if (left[slot] === right[slot]) sum += (slot & 3) + 1;
    else sum -= (slot & 1) + 1;
  }
  print(sum);
}
