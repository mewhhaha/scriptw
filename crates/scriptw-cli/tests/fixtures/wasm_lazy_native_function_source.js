function check(value, label) { if (!value) throw new Error(label); }
var stringify = Function.prototype.toString;
var invoke = Function.prototype.call.bind(stringify);
var array = Array;
var abs = Math.abs;
var getter = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get;
var other = __scriptwCreateRealm().global;
var foreignArray = other.Array;
var bound = array.bind(null);
var proxy = new Proxy(array, {});
function user /* preserve source */ (x) { return x + 1; }

// Alter public metadata before the first source observation. Internal native
// source must neither read these properties nor execute user code.
Object.defineProperty(array, 'name', { get: function () { throw new Error('name read'); } });
Object.defineProperty(getter, 'name', { value: 'changed getter', configurable: true });
array.toString = function () { throw new Error('public toString read'); };
getter.toString = array.toString;
gc();
check(invoke(array) === 'function Array() { [native code] }', 'constructor source');
check(invoke(abs) === 'function abs() { [native code] }', 'deferred method source');
check(invoke(stringify) === 'function toString() { [native code] }', 'canonical method source');
check(invoke(getter) === 'function get byteLength() { [native code] }', 'accessor source');
check(invoke(foreignArray) === 'function Array() { [native code] }', 'foreign constructor source');
check(other.Function.prototype.toString.call(array) === invoke(array), 'borrowed foreign toString');
check(invoke(bound) === 'function () { [native code] }', 'bound source');
check(invoke(proxy) === 'function () { [native code] }', 'proxy source');
check(invoke(user) === 'function user /* preserve source */ (x) { return x + 1; }', 'original user source');
var saved = invoke(getter);
gc();
check(invoke(getter) === saved && invoke(foreignArray) === invoke(array), 'cached source survives gc');
check(abs(-2) === 2 && getter.call(new ArrayBuffer(7)) === 7, 'callable behavior');
var caught = false;
try { invoke({}); } catch (error) { caught = error instanceof TypeError; }
check(caught, 'non-callable rejection');
print('lazy-native-function-source-ok');
