# For-in enumeration

`ForInArray`, `ForInString`, and `ForInObject` retain their lowering-time value
classification and share one AOT emitter in `control_flow/for_in.rs`. The head
is evaluated once in its required temporal dead zone. Nullish values finish
without an iteration; other primitives are boxed in the source execution realm.
The shared native cursor owns the same primitive boxing and internal-method
observations for eager, generator, async and async-generator consumers.

The emitter implements a lazy EnumerateObjectProperties traversal. Each object
level obtains its own property keys once through the typed internal OwnKeys
helper. Symbols and already visited names are skipped. Immediately before
considering an unvisited string key, typed internal GetOwnProperty checks
that the property still exists and whether it is enumerable. Existing
non-enumerable properties mark names visited and shadow inherited properties;
missing descriptors do not. Enumeration does not read the property value.
ForIn reads presence and enumerability immediately from the existing descriptor
lookup. Ordinary objects and Arrays do not clone the descriptor for this scalar
observation; other consumers retain completed snapshots. Exotics keep the same
lookup, callback sequence and whole Throw propagation.

Only after exhausting a level does internal GetPrototypeOf obtain the next
prototype. These operations use compiler-owned internal-method owners, so
reassigning the public `Reflect` or `Object` methods cannot change enumeration.
Their ordinary, array, string, proxy, and namespace internal-method dispatch
remains owned by those shared implementations. In particular, the loop does
not scan raw object entries or substitute a snapshot of the full prototype
chain. Prototype trap failures and descriptor failures propagate normally.

The private visited-name Set persists across levels. Keys added after a level's key
snapshot are not appended to that snapshot. A key removed before its
descriptor check is skipped, and a subsequent prototype can still expose it.
Prototype keys are acquired when that level is reached. These rules describe
this implementation's mutation behavior; ECMAScript permits implementation
variation for several mutations during for-in enumeration.

Loop completion values are saved across internal method calls and binding
publication. A published global binding can invoke user code and throw before
the body runs. Property and destructuring assignment heads likewise carry an
empty normal completion, keeping their assigned key out of the body's value.
Per-iteration lexical environments surround publication and
body execution; labelled continue, break, return, throw, and finally use the
existing control-target unwinding rules. Planner accounting retains the full
enumerator state across nested heads, bodies, and publication.

The obsolete primitive refusal and three compiler shortcuts were removed:
`for_in_known_empty_target`, `for_in_global_non_enumerable_guard_only`, and
`for_in_builtin_non_enumerable_assert_only`. Constructor names, known builtin
property attributes, and assertion-shaped bodies cannot suppress execution.
These were IR-lowering shortcuts, outside the canonical Test262 runner audit
whose source input is `crates/scriptw-test262/src/lib.rs`. Their removal changes
zero canonical shortcut ledger rows or counts.

Regression coverage is in `aot_for_in_enumeration.rs`, the corresponding IR
tests, and deep planner and intrinsic-root tests. The confirmed baseline
witness is `language/statements/for-in/order-property-on-prototype.js`.
The [resumable family](resumable-for-in-family.md) retains the same cursor
through its original invocation-cell edge.

Visited names use the existing canonical `SetObject`, `SetEntryTable`,
`SetEntry`, hash buckets and geometric-growth helper. A private String-only
constructor/has/add facade allocates its own null-prototype ordinary header;
it never publishes the Set as a JavaScript value or consults Set constructors,
prototypes, iterators, species or public methods. Content hashing and
SameValueZero preserve String equality, including distinct references and lone
UTF-16 surrogates. Symbols are excluded before membership lookup.

The cursor's immutable non-null `VISITED_SET` edge keeps the same Set identity.
Membership is checked before the live descriptor; insertion follows descriptor
existence and precedes its enumerable test. Each operation reacquires current
collection storage, so no bucket, slot or table crosses a descriptor callback.
The selected key and cursor remain rooted across traps, nested enumeration,
GC and suspension. NEXT_KEY still advances before callback-capable work.

This replaces the visited list's linear scan and exact prefix copying. It adds
fixed header/collection/ordinal storage per cursor and a SetEntry per new key;
contains and insert perform separate hashed lookups. Small-width cost and wider
lookup/copy savings require matched measurements; no timing follows from this
representation alone. OwnKeys, descriptors, binding publication and GC still
contribute to loop cost.

Layout ABI 52 changes only cursor field 3 from mutable PropertyKeyTable to
immutable SetObject. There are no new GC types, helper IDs or value/completion
lanes; the complete recursive R/P type graph nevertheless changes. ABI 51
runtime caches and build packages must be rejected before admission, and all
Portable bundles and native images must be rebuilt together. The layout,
emission, resumable topology and Engine tests cover this boundary; historical
benchmark and Test262 publications retain their original build identities.
