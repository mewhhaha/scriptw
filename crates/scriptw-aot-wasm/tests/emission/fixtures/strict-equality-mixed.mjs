var left, right, seed, coercions;
export function prepare(input) {
  seed = input;
  coercions = 0;
  var text = 'text-' + input;
  var copy = ('prefix-' + text).slice(7);
  var big = 9007199254740993n + BigInt(input);
  var bigCopy = BigInt('9007199254740993') + BigInt(input);
  var symbol = Symbol('same'), otherSymbol = Symbol('same');
  var object = {}, otherObject = {};
  Object.defineProperty(object, Symbol.toPrimitive, {
    get: function () { coercions++; throw 'strict equality coerced an object'; }
  });
  var fn = function value() {}, otherFn = function value() {};
  var first = Proxy.revocable(fn, {}), second = Proxy.revocable(otherFn, {});
  var proxy = first.proxy, otherProxy = second.proxy;
  first.revoke(); second.revoke();
  left = [NaN, -0, 0, text, text, big, big, symbol,
    symbol, object, object, fn, fn, proxy, proxy, undefined,
    null, false, true, true, null, 1, 1n, Infinity,
    -Infinity, Infinity, text, big, symbol, object, input + 0.5, input + 0.5];
  right = [NaN, 0, -0, copy, text + 'x', bigCopy, big + 1n, symbol,
    otherSymbol, object, otherObject, fn, otherFn, proxy, otherProxy, undefined,
    null, false, true, false, undefined, '1', 1, Infinity,
    -Infinity, -Infinity, new String(text), Object(big), Object(symbol), otherObject,
    input + 0.5, input + 1.5];
}
export function run() {
  var sum = seed * 17;
  for (var i = 0; i < 8192; i++) {
    var slot = (i + seed) & 31;
    if (left[slot] === right[slot]) sum += (slot & 3) + 1;
    else sum -= (slot & 1) + 1;
  }
  if (coercions) throw 'strict equality invoked coercion';
  print(sum);
}
