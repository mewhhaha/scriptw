var rows, keys;
export function prepare(input) {
  rows = [];
  for (var i = 0; i < 64; i++) {
    rows.push({x: input + i, y: input * 3 + i * 2, mask: (input + i) & 3});
  }
  keys = ['x', 'y', 'prefix-x'.slice(7), String.fromCharCode(121)];

  for (var i = 0; i < rows.length; i++) {
    var row = rows[i];
    row.temporary = i;
    Object.defineProperty(row, 'x', {configurable: true, enumerable: true,
      get: (function (value) { return function () { return value; }; })(input + i)});
    var value = row.x;
    delete row.temporary;
    delete row.x;
    Object.defineProperty(row, 'x', {value: value, writable: true,
      configurable: true, enumerable: true});
  }

}
export function run() {
  var sum = 0;
  for (var i = 0; i < 8192; i++) sum += rows[i & 63][keys[i & 3]];
  print(sum);
}
