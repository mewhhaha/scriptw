function check(value, label) {
  if (!value) throw new Error(label);
}

function exercise(method, source, expected) {
  var saved = [];
  var nested = false;
  var result = source[method](function(a, b) {
    var first = a, second = b;
    var call = saved.length;
    var marker = typeof first === "bigint" ? BigInt(1000 + call) : 1000 + call;
    arguments[0] = marker;
    arguments[1] = marker;
    saved.push({ args: arguments, marker: marker });
    if (!nested) {
      nested = true;
      var inner = new Uint8Array([3, 1, 2]).sort(function(x, y) { return x - y; });
      check(inner[0] === 1 && inner[2] === 3, "reentrant comparator");
      gc();
    }
    return {
      valueOf: function() {
        gc();
        return first < second ? -1 : first > second ? 1 : 0;
      }
    };
  });
  check(saved.length > 1, "multiple comparisons");
  for (var i = 0; i < expected.length; ++i) {
    check(Object.is(result[i], expected[i]), "snapshot result " + method + " " + i);
  }
  for (var i = 0; i < saved.length; ++i) {
    check(saved[i].args[0] === saved[i].marker, "retained first argument");
    check(saved[i].args[1] === saved[i].marker, "retained second argument");
    if (i > 0) check(saved[i - 1].args !== saved[i].args, "fresh arguments identity");
  }
  if (method === "toSorted") check(result !== source, "fresh sorted copy");
  else check(result === source, "in-place identity");
}

exercise("sort", new Int32Array([4, 1, 3, 2]), [1, 2, 3, 4]);
exercise("toSorted", new Int32Array([4, 1, 3, 2]), [1, 2, 3, 4]);
exercise("sort", new BigInt64Array([4n, -1n, 3n, 2n]), [-1n, 2n, 3n, 4n]);
exercise("toSorted", new BigInt64Array([4n, -1n, 3n, 2n]), [-1n, 2n, 3n, 4n]);
// Equal comparator results retain the exact source bits and their stable order.
exercise("sort", new Float64Array([-0, 0, -0, 0]), [-0, 0, -0, 0]);
exercise("toSorted", new Float64Array([-0, 0, -0, 0]), [-0, 0, -0, 0]);
true;
