(function () {
  function check(value) { if (!value) throw new Error('runtime String key mismatch'); }
  const owners = [Object.prototype, Function.prototype, Array.prototype,
                  String.prototype, Number.prototype, Map.prototype, Set.prototype,
                  Object, Array, String, Math, JSON, Reflect];
  const keys = [];
  for (const owner of owners) {
    for (const name of Object.getOwnPropertyNames(owner)) keys.push(name);
    if (typeof owner === 'function') keys.push(owner.name);
  }
  keys.push(Function.prototype.name);
  const object = Object.create(null), map = new Map(), set = new Set();
  for (let i = 0; i < keys.length; ++i) {
    // Builtin reflection returns R Strings. This construction produces equal,
    // separately allocated content through the ordinary String path.
    const fresh = ('!' + keys[i]).slice(1);
    check(fresh === keys[i]);
    Object.defineProperty(object, keys[i], {value: i, configurable: true});
    map.set(fresh, i); set.add(keys[i]);
    check(object[fresh] === i && map.get(keys[i]) === i && set.has(fresh));
  }
  if (typeof gc === 'function') gc();
  for (const key of map.keys()) {
    const fresh = ('!' + key).slice(1);
    check(object[fresh] === map.get(fresh) && set.has(fresh));
    check(delete object[fresh] && map.delete(key) && set.delete(fresh));
    check(!Object.hasOwn(object, key) && !map.has(fresh) && !set.has(key));
  }
  check(map.size === 0 && set.size === 0);
})();
true;
