var rows, seed;
export function prepare(input) {
  seed = input;
  var targets = [{length: 1, name: 'Array'}, {length: 0, name: 'Map'},
    {length: 0, name: 'Set'}, {length: 7, name: 'Date'},
    {length: 1, name: 'Number'}, {length: 1, name: 'String'},
    {length: 1, name: 'Boolean'}, {length: 3, name: 'Uint8Array'}];
  rows = [];
  for (var i = 0; i < 64; i++) rows.push(targets[(input + i) & 7]);
}
export function run() {
  var sum = seed;
  for (var i = 0; i < 8192; i++) {
    var row = rows[i & 63];
    sum += row.length + row.name.length;
  }
  print(sum);
}
