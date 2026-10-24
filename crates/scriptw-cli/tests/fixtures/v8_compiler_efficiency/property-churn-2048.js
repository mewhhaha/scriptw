(function workload() {
var object = Object.create(null); var sum = 0;
for (var i = 0; i < 2048; ++i) { object.value = i; sum += object.value; delete object.value; sum += Reflect.ownKeys(object).length; }
print(sum);
})();
