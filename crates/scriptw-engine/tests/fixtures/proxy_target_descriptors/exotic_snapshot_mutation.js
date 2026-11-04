function check(condition, label) {
  if (!condition) throw label;
}
function checkPublicData(target, key, value, writable, enumerable, configurable) {
  const first = Object.getOwnPropertyDescriptor(target, key);
  const second = Reflect.getOwnPropertyDescriptor(target, key);
  check(first !== second && Object.getPrototypeOf(first) === Object.prototype,
    'fresh public exotic descriptor');
  check(Object.keys(first).join(',') === 'value,writable,enumerable,configurable',
    'public exotic descriptor fields');
  check(first.value === value && first.writable === writable &&
    first.enumerable === enumerable && first.configurable === configurable,
    'public exotic descriptor values');
  first.value = 'changed result';
  check(second.value === value && Reflect.get(target, key) === value,
    'public descriptor independence');
}
let cases = 0;
// Callable storage differs from ordinary object storage, including the native
// metadata recipe for Math.abs. ToPropertyDescriptor runs after target capture.
for (const target of [function named(a) {}, Math.abs]) {
  checkPublicData(target, 'length', 1, false, false, true);
  Object.defineProperty(target, 'length', {
    value: 1, writable: true, enumerable: false, configurable: false
  });
  let conversions = 0;
  const fields = {
    get value() {
      conversions++;
      Object.defineProperty(target, 'length', {value: 2, writable: false});
      return 1;
    },
    writable: true, enumerable: false, configurable: false
  };
  const proxy = new Proxy(target, {getOwnPropertyDescriptor() {return fields;}});
  const snapshot = Reflect.getOwnPropertyDescriptor(proxy, 'length');
  check(snapshot.value === 1 && snapshot.writable && conversions === 1,
    'callable descriptor captured before result conversion');
  checkPublicData(target, 'length', 2, false, false, false);
  let threw = false;
  try {Object.getOwnPropertyDescriptor(proxy, 'length');}
  catch (error) {threw = Object.getPrototypeOf(error) === TypeError.prototype;}
  check(threw && conversions === 2, 'callable descriptor refresh rejects stale report');
  cases++;
}
const bytes = new Uint8Array([4]);
const byteProxy = new Proxy(bytes, {getOwnPropertyDescriptor() {
  return {
    get value() {bytes[0] = 7; return 4;},
    writable: true, enumerable: true, configurable: true
  };
}});
check(Reflect.getOwnPropertyDescriptor(byteProxy, '0').value === 4 && bytes[0] === 7,
  'integer-indexed descriptor conversion effects');
checkPublicData(bytes, '0', 7, true, true, true);
check(Reflect.getOwnPropertyDescriptor(bytes, '-0') === undefined &&
  Reflect.getOwnPropertyDescriptor(bytes, '1') === undefined,
  'integer-indexed absent descriptor');
cases++;
const text = new String('ab');
checkPublicData(text, '0', 'a', false, true, false);
checkPublicData(text, 'length', 2, false, false, false);
check(Reflect.getOwnPropertyDescriptor(text, '2') === undefined,
  'string exotic absent descriptor');
cases++;
(function (value) {
  const args = arguments;
  const proxy = new Proxy(args, {getOwnPropertyDescriptor() {
    return {
      get value() {args[0] = 9; return 5;},
      writable: true, enumerable: true, configurable: true
    };
  }});
  check(Object.getOwnPropertyDescriptor(proxy, '0').value === 5 && args[0] === 9,
    'arguments descriptor conversion effects');
  checkPublicData(args, '0', 9, true, true, true);
})(5);
cases++;
check(cases === 5, 'exotic descriptor census');
print('exotic-descriptor-snapshot-mutation:ok');
262;
