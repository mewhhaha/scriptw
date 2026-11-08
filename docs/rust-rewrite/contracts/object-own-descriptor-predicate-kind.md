# Object own-descriptor predicates

`builtins/object/own_descriptor_predicate.rs` owns the closed ObjectHasOwn,
PrototypeHasOwnProperty and PrototypePropertyIsEnumerable policies. Object.hasOwn
completes ToObject before ToPropertyKey; both prototype forms complete the key
first, including when ToObject(this) throws. The executing builtin retains its
Realm and whole abrupt completion.

All three consume the existing scalar own-property observation immediately.
Actual OrdinaryObject references read compact attributes, intrinsic rows or
stored dictionary descriptor flags without materializing property values or
copying descriptors. Presence remains independent of enumerability. Accessors
are never invoked; own Undefined remains present. Concrete Arrays retain their
indexed, named, Symbol and virtual-length lookup. Other exotics, including
Proxies, retain the complete ObjectGetOwnProperty helper and its original traps,
validation and throws.

No user code can run between obtaining the scalar facts and consuming them.
Consumers that retain a descriptor across effects, including Proxy invariant
checks, still require the independent completed snapshot described in
[Proxy target descriptor completion](proxy-target-descriptor-completion.md).

The emitted-module witness checks all three predicate bodies, ordinary storage
without descriptor/value construction, canonical Array/exotic fallbacks, and a
snapshot-taking control. The CLI predicate fixture covers storage transitions,
key-coercion mutations, symbols, accessors, Arrays, typed arrays, boxed strings,
Proxy trap counts, original throws, conversion order and foreign error realms.
