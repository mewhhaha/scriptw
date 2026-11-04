# GetOwnProperty runtime owner

`objects/own_property.rs` owns one typed `ObjectGetOwnProperty` runtime helper.
It takes an object Value, an already converted PropertyKey, and the executing
Realm, Environment and Program. It returns the whole Completion followed by a
nullable `PropertyDescriptor`. Null means absence on normal completion; every
abrupt exit returns null and retains the original Throw. The caller roots both
results before checking completion, including callers that must close acquired
iterators before propagating failure.

The helper reuses the non-Proxy storage dispatcher for ordinary objects, Arrays,
Arguments, String boxes, TypedArrays, functions and module namespaces. Mutable
storage descriptors are copied before publication. Mapped argument values and
namespace TDZ are observed at acquisition; no descriptor cache is introduced.

The private `own_property/proxy.rs` child retains the Proxy algorithm. GetMethod
and the trap call precede recursive target acquisition, IsExtensible and
observable ToPropertyDescriptor reads. Missing traps recurse through the same
runtime helper. An undefined trap result with an absent target descriptor skips
IsExtensible. Present descriptors are completed once, checked by the existing
compatibility kernel and nonconfigurable/nonwritable constraints, then stored
as private records. Recursive target snapshots survive later conversion effects.
Generated errors use the supplied execution Realm; the helper has no callable
context. Nested user calls retain their actual callee Realm and whole Throws.

`builtins/object/get_own_property_descriptor.rs` owns public materialization.
Object.getOwnPropertyDescriptor completes ToObject before ToPropertyKey;
Reflect.getOwnPropertyDescriptor retains its object requirement before key
conversion. Both then acquire the private snapshot and expose a fresh ordinary
descriptor with the native method's executing-Realm Object prototype. The same
adapter supplies observable entries of Object.getOwnPropertyDescriptors.
Internal Get, Proxy invariants, ForIn, bind metadata, Iterator.zipKeyed and
ShadowRealm import checks never construct that JavaScript transport object or
a temporary public descriptor callable.

Runtime layout 51 records the helper function/type ABI change. The GC object
model is unchanged; canonical R source identity remains part of its cache key.
Previous layout-50 cache/package entries are rejected and rebuilt. Encoded R/P
acceptance lives in `tests/emission/own_property_helper.rs`; Engine descriptor,
Realm, Proxy, iterator, namespace and exotic controls remain required. This
contract describes the implementation, not a full Test262 acceptance claim.
