(function workload() {
function add(left, right) { return left + right; }
var sum = 0; for (var i = 0; i < 32768; ++i) sum += add(i, 1);
print(sum);
})();
