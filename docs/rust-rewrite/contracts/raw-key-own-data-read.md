# Raw-key own-data reads

Source `GetV` retains both evaluated operands before trying an own-data read.
A successful concrete representation guard skips `ValueToObject`,
`ValueToPropertyKey` and `ObjectRead`:

- A Number key reads a present packed Array slot. Finite integral keys in
  `[0, 4294967295)` qualify; numeric `-0` denotes slot zero. Holes miss.
- An actual String key probes existing simple-object own data through the
  shared compact shape/slot, intrinsic storage or ordinary data-descriptor
  path. String equality uses UTF-16 contents, so separately produced equal
  keys work. Accessors and absent properties miss.

These probes retain no new cache, receiver or stored value across reads. Each
read loads the object's current storage and value. They invoke no JavaScript
hooks; Wasm GC locals root references during shared String comparison.

A miss resumes the original operation with the original saved operands:
`ToObject`, then `ToPropertyKey`, then full lookup with the original receiver.
Nullish targets still reject before key coercion, while the raw key expression
has already run. Proxies, revoked proxies, primitive receivers, prototype
getters, accessor promotion and thrown completions retain that path. String
`"-0"`, fractional Number keys, NaN, infinities and non-index bounds do not enter
the packed Number arm. This changes no backend or conformance claim.

`raw_key_own_data` inspects emitted source bodies and their guarded early exits;
`aot_raw_key_own_data` exercises live mutation, key equality, GC, holes,
prototype/accessor/proxy receivers and operand/coercion/exception order through
Wasm AOT.
