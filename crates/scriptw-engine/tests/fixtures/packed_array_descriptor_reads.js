function same(actual, expected, message) {
  if (!Object.is(actual, expected)) throw new Error(message);
}
function check(value, message) { if (!value) throw new Error(message); }
function collect() { if (typeof gc === 'function') gc(); }
function data(array, key, expected) {
  var result = Object.getOwnPropertyDescriptor(array, key);
  check(result && result.writable && result.enumerable && result.configurable, 'data attributes');
  check(!('get' in result) && !('set' in result), 'data fields');
  same(result.value, expected, 'descriptor payload');
  return result;
}

var numbers = [-0, NaN, Infinity, -Infinity, 7];
var negativeZero = data(numbers, '0', -0), notANumber = data(numbers, '1', NaN);
data(numbers, '2', Infinity); data(numbers, '3', -Infinity);
numbers[0] = 9; numbers[1] = 11; collect();
same(negativeZero.value, -0, 'Number snapshot survives overwrite');
same(notANumber.value, NaN, 'NaN snapshot survives overwrite');
negativeZero.value = 100; same(numbers[0], 9, 'public descriptor is independent');

var marker = {}, symbol = Symbol('value');
var values = [undefined, null, true, 'text', 13n, symbol, marker];
for (var i = 0; i < values.length; i++) data(values, String(i), values[i]);
var held = data(values, '6', marker);
values[6] = {}; delete values[5]; collect();
same(held.value, marker, 'shared immutable edge survives slot replacement and GC');
same(Object.getOwnPropertyDescriptor(values, '5'), undefined, 'deleted packed value is absent');
check('0' in values && Object.hasOwn(values, '0'), 'present undefined is not a hole');

var hole = [17, , 23], inheritedReads = 0, token = {};
var prototype = Object.create(Array.prototype);
Object.defineProperty(prototype, '1', { configurable: true, get: function () {
  inheritedReads++; collect(); throw token;
}});
Object.setPrototypeOf(hole, prototype);
same(Object.getOwnPropertyDescriptor(hole, '1'), undefined, 'own query does not visit prototype');
check('1' in hole && !Object.hasOwn(hole, '1'), 'presence follows prototype without Get');
same(inheritedReads, 0, 'presence invokes no getter');
var caught = false; try { hole[1]; } catch (error) { caught = error === token; }
check(caught && inheritedReads === 1, 'ordinary Get preserves inherited Throw');
var listed = ''; for (var key in hole) listed += key + ',';
same(listed, '0,2,', 'nonenumerable prototype key remains shadowed');
hole.length = 4294967295;
same(Object.getOwnPropertyDescriptor(hole, '4294967294'), undefined, 'logical length is not occupancy');
same(Object.getOwnPropertyDescriptor(hole, '-0'), undefined, 'non-index spelling remains distinct');
var length = Object.getOwnPropertyDescriptor(hole, 'length');
check(length.value === 4294967295 && length.writable && !length.enumerable && !length.configurable, 'virtual length attributes');

var changed = [31, 37, 41], getterCalls = 0;
data(changed, '1', 37);
Object.defineProperty(changed, '1', { configurable: true, enumerable: false, get: function () {
  getterCalls++; collect(); return 43;
}});
var accessor = Object.getOwnPropertyDescriptor(changed, '1');
check(typeof accessor.get === 'function' && !accessor.enumerable && accessor.configurable, 'real accessor transition');
same(getterCalls, 0, 'descriptor acquisition does not call getter');
same(changed[1], 43, 'transitioned accessor Get');
same(getterCalls, 1, 'one getter');
Object.defineProperty(changed, '2', { value: 41, configurable: false, writable: false });
check(!Reflect.defineProperty(changed, 'length', { value: 1 }), 'length shrink keeps fixed indexed descriptor');
same(changed.length, 3, 'failed length shrink commits correct boundary');
check(!Reflect.deleteProperty(changed, '2') && !Reflect.set(changed, '2', 47), 'fixed descriptor remains enforced');
var sparse = [53]; data(sparse, '0', 53); sparse[1000000] = 59;
data(sparse, '1000000', 59); data(sparse, '0', 53);

// Conversion invokes source code only after the completed target snapshot.
var target = [61], events = '';
var proxy = new Proxy(target, { getOwnPropertyDescriptor: function (actual, key) {
  check(actual === target && key === '0', 'Proxy target/key'); events += 'trap,';
  return { configurable: true, enumerable: true, writable: true, get value() {
    events += 'value,'; Object.defineProperty(target, '0', { value: 67, writable: false, configurable: false });
    collect(); return 61;
  }};
}});
same(Object.getOwnPropertyDescriptor(proxy, '0').value, 61, 'pre-reentry target snapshot');
same(target[0], 67, 'reentry changes actual storage');
same(events, 'trap,value,', 'trap conversion order');
var revoked = Proxy.revocable([71], {}); revoked.revoke();
var rejected = false; try { Object.getOwnPropertyDescriptor(revoked.proxy, '0'); } catch (error) { rejected = error instanceof TypeError; }
check(rejected, 'revoked Proxy stays exotic');
print('packed-descriptor-reads');
