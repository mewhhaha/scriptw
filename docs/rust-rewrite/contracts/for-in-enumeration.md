# For-in enumeration

`ForInArray`, `ForInString`, and `ForInObject` retain their lowering-time value
classification and share one AOT emitter in `control_flow/for_in.rs`. The head
is evaluated once in its required temporal dead zone. Nullish values finish
without an iteration; other primitives are boxed in the source execution realm.
The intrinsic plan roots the applicable primitive prototype installers as well
as the three reflective internal-method implementations.

The emitter implements a lazy EnumerateObjectProperties traversal. Each object
level obtains its own property keys once through intrinsic `Reflect.ownKeys`.
Symbols and already visited names are skipped. Immediately before considering
an unvisited string key, intrinsic `Reflect.getOwnPropertyDescriptor` checks
that the property still exists and whether it is enumerable. Existing
non-enumerable properties mark names visited and shadow inherited properties;
missing descriptors do not. Enumeration does not read the property value.

Only after exhausting a level does intrinsic `Reflect.getPrototypeOf` obtain
the next prototype. These calls use compiler-owned builtin identities, so
reassigning the public `Reflect` or `Object` methods cannot change enumeration.
Their ordinary, array, string, proxy, and namespace internal-method dispatch
remains owned by those shared implementations. In particular, the loop does
not scan raw object entries or substitute a snapshot of the full prototype
chain. Prototype trap failures and descriptor failures propagate normally.

The visited-name list persists across levels. Keys added after a level's key
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
Await inside a for-in body still requires a resumable enumeration state and
retains its explicit unsupported diagnostic. This change does not claim that
separate suspension feature.

Visited-key tables remain private and read-only after publication. Appending a
key creates one new StoredValue and fills fresh construction storage with it,
then copies the old immutable prefix with typed array.copy. Existing snapshots
are shared without reboxing. This removes redundant boxes; reference copying
and visited-key comparisons still have quadratic worst cases. Live descriptor,
deletion, shadowing and prototype observations retain their existing owners.
