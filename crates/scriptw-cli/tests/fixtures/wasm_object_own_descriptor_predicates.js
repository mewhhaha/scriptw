var hasOwnProperty = Object.prototype.hasOwnProperty;
var propertyIsEnumerable = Object.prototype.propertyIsEnumerable;

function assertSame(actual, expected, label) {
  if (actual !== expected) throw label;
}

function assertPredicates(value, key, present, enumerable, label) {
  assertSame(Object.hasOwn(value, key), present, label + " Object.hasOwn");
  assertSame(
    hasOwnProperty.call(value, key),
    present,
    label + " hasOwnProperty"
  );
  assertSame(
    propertyIsEnumerable.call(value, key),
    enumerable,
    label + " propertyIsEnumerable"
  );
}

var compact = { first: undefined, second: null };
assertPredicates(compact, "first", true, true, "inline own undefined");
assertPredicates(compact, "missing", false, false, "inline absent");
var wider = { first: undefined, second: 2, third: 3 };
assertPredicates(wider, "third", true, true, "wider compact property");
Object.freeze(wider);
assertPredicates(wider, "first", true, true, "frozen own undefined");
Object.defineProperty(compact, "first", { enumerable: false });
assertPredicates(compact, "first", true, false, "changed enumerability");
delete compact.second;
assertPredicates(compact, "second", false, false, "deleted property");
compact.second = undefined;
assertPredicates(compact, "second", true, true, "recreated own undefined");
assertPredicates(Object.create(compact), "first", false, false, "inherited hidden property");
assertPredicates(Object.prototype, "toString", true, false, "intrinsic ordinary flags");

var coercionTarget = { changing: 1 };
var coercionEffects = 0;
var coercionGetterCalls = 0;
var changingKey = {
  [Symbol.toPrimitive]: function () {
    coercionEffects += 1;
    Object.defineProperty(coercionTarget, "changing", {
      get: function () { coercionGetterCalls += 1; return 9; },
      enumerable: false,
      configurable: true,
    });
    return "changing";
  },
};
assertPredicates(coercionTarget, changingKey, true, false, "coercion changes descriptor");
assertSame(coercionEffects, 3, "one coercion per predicate");
assertSame(coercionGetterCalls, 0, "coercion-installed getter stays unobserved");
var deletingKey = {
  [Symbol.toPrimitive]: function () {
    delete coercionTarget.changing;
    return "changing";
  },
};
assertPredicates(coercionTarget, deletingKey, false, false, "coercion deletes property");

var packed = [1, 2, 3];
assertPredicates(packed, "1", true, true, "packed Array element");
assertPredicates(packed, "length", true, false, "Array virtual length");
delete packed[1];
assertPredicates(packed, "1", false, false, "Array hole");
packed[1] = undefined;
assertPredicates(packed, "1", true, true, "Array own undefined");
packed.named = 4;
assertPredicates(packed, "named", true, true, "Array ordinary header");

function argumentTail() { return arguments; }
var emptyTail = argumentTail.call(null);
var firstTail = argumentTail.call(null, undefined, 7);
var secondTail = argumentTail.call(null, undefined);
firstTail[0] = 19;
assertSame(emptyTail.length, 0, "empty native argument tail");
assertSame(firstTail[1], 7, "argument tail retains order");
assertSame(secondTail[0], undefined, "argument tails retain independent entries");
assertSame(Object.hasOwn(secondTail, "0"), true, "tail own undefined remains present");

var numeric = new Uint8Array([7]);
assertPredicates(numeric, "0", true, true, "typed array element");
assertPredicates(numeric, "-0", false, false, "typed array negative zero");
assertPredicates(numeric, "1", false, false, "typed array out of bounds");

var bigint = new BigInt64Array([11n]);
assertPredicates(bigint, "0", true, true, "bigint typed array element");

var visibleSymbol = Symbol("visible typed array property");
var hiddenSymbol = Symbol("hidden typed array property");
Object.defineProperty(numeric, visibleSymbol, {
  value: "visible",
  enumerable: true,
  configurable: true,
});
Object.defineProperty(numeric, hiddenSymbol, {
  value: "hidden",
  enumerable: false,
  configurable: true,
});
assertPredicates(numeric, visibleSymbol, true, true, "enumerable symbol");
assertPredicates(numeric, hiddenSymbol, true, false, "non-enumerable symbol");

var detached = new Uint8Array([9]);
detached.visible = 1;
Object.defineProperty(detached, "hidden", {
  value: 2,
  enumerable: false,
  configurable: true,
});
__scriptwDetachArrayBuffer(detached.buffer);
assertPredicates(detached, "0", false, false, "detached typed array element");
assertPredicates(detached, "visible", true, true, "detached ordinary property");
assertPredicates(detached, "hidden", true, false, "detached hidden property");

var boxedString = new String("A\ud83d\ude00Z");
assertPredicates(boxedString, "0", true, true, "boxed string first unit");
assertPredicates(boxedString, "1", true, true, "boxed string high surrogate");
assertPredicates(boxedString, "2", true, true, "boxed string low surrogate");
assertPredicates(boxedString, "3", true, true, "boxed string last unit");
assertPredicates(boxedString, "4", false, false, "boxed string out of bounds");
assertPredicates(boxedString, "length", true, false, "boxed string length");
assertPredicates("xy", "1", true, true, "primitive string element");

var getterCalls = 0;
var accessorTarget = {};
Object.defineProperty(accessorTarget, "visible", {
  get: function () {
    getterCalls = getterCalls + 1;
    return 1;
  },
  enumerable: true,
  configurable: true,
});
Object.defineProperty(accessorTarget, "hidden", {
  get: function () {
    getterCalls = getterCalls + 1;
    return 2;
  },
  enumerable: false,
  configurable: true,
});
assertPredicates(accessorTarget, "visible", true, true, "visible accessor");
assertPredicates(accessorTarget, "hidden", true, false, "hidden accessor");
assertSame(getterCalls, 0, "predicate invoked target getter");

function Constructor() {}
assertPredicates(Constructor, "prototype", true, false, "function prototype");

var proxyTrapCalls = 0;
var proxyHandlerPrototype = {
  getOwnPropertyDescriptor: function (target, key) {
    proxyTrapCalls = proxyTrapCalls + 1;
    return Reflect.getOwnPropertyDescriptor(target, key);
  },
};
var proxyTarget = { visible: 1 };
var proxy = new Proxy(proxyTarget, Object.create(proxyHandlerPrototype));
assertPredicates(proxy, "visible", true, true, "inherited proxy trap");
assertPredicates(proxy, "missing", false, false, "inherited proxy trap miss");
assertSame(proxyTrapCalls, 6, "proxy trap must run once per predicate");

var trapMarker = {};
var abruptProxy = new Proxy({}, {
  getOwnPropertyDescriptor: function () {
    throw trapMarker;
  },
});
var abruptCalls = [
  function () { Object.hasOwn(abruptProxy, "x"); },
  function () { hasOwnProperty.call(abruptProxy, "x"); },
  function () { propertyIsEnumerable.call(abruptProxy, "x"); },
];
for (var abruptIndex = 0; abruptIndex < abruptCalls.length; abruptIndex += 1) {
  var caughtTrapMarker = false;
  try {
    abruptCalls[abruptIndex]();
  } catch (error) {
    caughtTrapMarker = error === trapMarker;
  }
  assertSame(caughtTrapMarker, true, "proxy abrupt identity " + abruptIndex);
}

var staticKeyCalls = 0;
var staticKeyMarker = {};
var staticKey = {
  [Symbol.toPrimitive]: function () {
    staticKeyCalls = staticKeyCalls + 1;
    throw staticKeyMarker;
  },
};
var staticNullishTypeError = false;
try {
  Object.hasOwn(null, staticKey);
} catch (error) {
  staticNullishTypeError = error instanceof TypeError && error !== staticKeyMarker;
}
assertSame(staticNullishTypeError, true, "Object.hasOwn conversion order");
assertSame(staticKeyCalls, 0, "Object.hasOwn coerced key before receiver");

var prototypeKeyCalls = 0;
var prototypeKeyMarker = {};
var prototypeKey = {
  [Symbol.toPrimitive]: function () {
    prototypeKeyCalls = prototypeKeyCalls + 1;
    throw prototypeKeyMarker;
  },
};
var hasOwnPropertyKeyAbrupt = false;
try {
  hasOwnProperty.call(null, prototypeKey);
} catch (error) {
  hasOwnPropertyKeyAbrupt = error === prototypeKeyMarker;
}
assertSame(hasOwnPropertyKeyAbrupt, true, "hasOwnProperty conversion order");
assertSame(prototypeKeyCalls, 1, "hasOwnProperty key coercion count");

var enumerableKeyCalls = 0;
var enumerableKeyMarker = {};
var enumerableKey = {
  [Symbol.toPrimitive]: function () {
    enumerableKeyCalls = enumerableKeyCalls + 1;
    throw enumerableKeyMarker;
  },
};
var propertyIsEnumerableKeyAbrupt = false;
try {
  propertyIsEnumerable.call(null, enumerableKey);
} catch (error) {
  propertyIsEnumerableKeyAbrupt = error === enumerableKeyMarker;
}
assertSame(
  propertyIsEnumerableKeyAbrupt,
  true,
  "propertyIsEnumerable conversion order"
);
assertSame(enumerableKeyCalls, 1, "propertyIsEnumerable key coercion count");

var other = __scriptwCreateRealm().global;
function assertOtherRealmTypeError(action, label) {
  var correctRealm = false;
  try {
    action();
  } catch (error) {
    correctRealm = error instanceof other.TypeError && !(error instanceof TypeError);
  }
  assertSame(correctRealm, true, label);
}
assertOtherRealmTypeError(
  function () { other.Object.hasOwn(null, "x"); },
  "Object.hasOwn error realm"
);
assertOtherRealmTypeError(
  function () { other.Object.prototype.hasOwnProperty.call(null, "x"); },
  "hasOwnProperty error realm"
);
assertOtherRealmTypeError(
  function () { other.Object.prototype.propertyIsEnumerable.call(null, "x"); },
  "propertyIsEnumerable error realm"
);

true;
