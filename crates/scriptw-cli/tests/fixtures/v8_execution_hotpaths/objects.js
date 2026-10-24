(function workload() {
  var rows = [];
  for (var i = 0; i < 500; i++) rows.push({x: i, y: (i * 7) % 101});
  var sum = 0;
  for (var i = 0; i < rows.length; i++) sum += rows[i].x + rows[i].y;
  print(String(sum));
})();
