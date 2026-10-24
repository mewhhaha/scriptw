// Retain objects, arrays and strings together until the final checksum.
(function workload() {
  var rows = [];
  for (var i = 0; i < 8192; i++) {
    rows.push({name: 'row-' + i, values: [i, i + 1, i + 2, i + 3]});
  }
  var sum = 0;
  for (var column = 0; column < 4; column++) {
    for (var i = 0; i < rows.length; i++) sum += rows[i].values[column];
  }
  print(String(sum));
})();
