(function workload() {
var map = new Map(); var sum = 0;
for (var i = 0; i < 10000; ++i) { map.set(i, i); sum += map.size; map.clear(); }
if (map.size !== 0 || !map.keys().next().done) throw new Error("map clear");
print(sum);
})();
