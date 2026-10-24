(function workload() {
var set = new Set(); var sum = 0;
for (var i = 0; i < 10000; ++i) { set.add(i); sum += set.size; set.clear(); }
if (set.size !== 0 || !set.keys().next().done) throw new Error("set clear");
print(sum);
})();
