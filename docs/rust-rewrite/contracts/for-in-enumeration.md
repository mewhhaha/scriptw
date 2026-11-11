# For-in enumeration

`ForInArray`, `ForInString`, and `ForInObject` retain their lowering-time value
classification and share one AOT emitter in `control_flow/for_in.rs`. The head
is evaluated once in its required temporal dead zone. Nullish values finish
without an iteration; other primitives are boxed in the source execution realm.
The shared native cursor owns the same primitive boxing and internal-method
observations for eager, generator, async and async-generator consumers.

The emitter implements a lazy EnumerateObjectProperties traversal. Each object
level obtains its own property keys once. An actual ordinary compact data
object may borrow its immutable shape keys when their order already matches
OwnKeys; other cases use the typed internal OwnKeys helper. Symbols and already visited names are skipped. Immediately before
considering an unvisited string key, typed internal GetOwnProperty checks
that the property still exists and whether it is enumerable. Existing
non-enumerable properties mark names visited and shadow inherited properties;
missing descriptors do not. Enumeration does not read the property value.
ForIn reads presence and enumerability immediately from the existing descriptor
lookup. Ordinary objects and Arrays do not clone the descriptor for this scalar
observation; other consumers retain completed snapshots. Exotics keep the same
lookup, callback sequence and whole Throw propagation.
Compact storage reads slot attributes directly, intrinsic storage reads row
flags, and dictionary storage reads its existing descriptor flags. These scalar
ordinary observations allocate no descriptor and do not resolve property values
or deferred recipes. Array storage retains its indexed/length descriptor rules.

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

The cursor retains the current object, key snapshot and next index. A compact
phase also roots its original storage. Each advance checks that the object's
live storage still matches; structural mutation resumes the original snapshot
through the general descriptor path, with the already processed prefix marked.
Mutable values are never cached by enumeration.

On the first general level, snapshots of at most 64 keys use a scalar bitmap
for keys whose live descriptor existed. OwnKeys uniqueness makes a duplicate
membership lookup unnecessary on this level. Missing descriptors leave their
bits clear; existing non-enumerable properties set their bits before the
enumerable test. Larger snapshots use the canonical Set immediately.

The nullable mutable `VISITED_SET` edge becomes non-null only when needed.
Before traversing a prototype, the cursor seeds that same canonical Set from
its processed compact prefix or its first-level found bitmap. Later levels
check membership before observing the live descriptor and insert only after
existence is established. No bucket, slot or table crosses a descriptor callback.
Keys and the cursor remain rooted across traps, nested enumeration, GC and
suspension; NEXT_KEY still advances before callback-capable work.

After exhausting own keys, the current prototype is observed. A guarded null
or sealed, wholly non-enumerable Object.prototype tail can finish immediately.
The guard authenticates the actual ordinary layout, intrinsic rows and null
ancestor; mutating the prototype or its properties restores general traversal.
It never suppresses Proxy observations or assumes a pristine prototype across
body execution.

The cursor retains compact key-order evidence, captured storage and the found
bitmap, with a lazy visited Set. The current layout stores these cursor and immutable
shape fields. All older Portable bundles and native images must be rebuilt together. Layout, emission, mutation, resumable
and Engine tests cover this boundary; historical benchmark and Test262
publications retain their original build identities.
