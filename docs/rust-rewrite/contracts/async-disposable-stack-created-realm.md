# AsyncDisposableStack created-Realm ownership

## Current atomic GC source — 2026-10-05

The common completed GC bootstrap publishes AsyncDisposableStack intrinsics
for created Realms. The host's separate
`created_realm_async_disposable_stack_intrinsics.rs` duplicate and
`created_realm_async_disposable_stack_structure` raw-slot mirror are retired.
Typed Realm intrinsic records and immutable FunctionContext capture replace
the former 424-byte offset and environment-zero protocol. Constructor fallback
consumes the shared completed GetPrototypeFromConstructor operation; whole
completions and captured disposal/reaction state retain errors and callbacks.

The existing `aot_created_realm_async_disposable_stack` and
`aot_async_disposable_stack_realm` semantic targets remain. Their historical
observations do not certify the current GC source.

All source, types and controls for the atomic batch remain uncompiled and
unexecuted. Final representation/helper/guard composition also remains pending.
Earlier verification commands and results below retain their original source
scope; they are historical records, not instructions to run during the full-task
dry-source pass. Later verification follows the [batch workflow](../../../CONTRIBUTING.md#verification)
with a confirmed aggregate 4096 MiB cap, swap zero and serial execution.

## Historical predecessor record

The source-free AsyncDisposableStack surface is published in every created
Realm. Empty Function construction makes a foreign NewTarget directly reachable,
so constructor prototype fallback is ordinary supported behavior.

The canonical intrinsic record contains a traced eight-byte
`%AsyncDisposableStack.prototype%` entry at offset 424; its total size is 432
bytes. Entry bootstrap and created-Realm bootstrap both populate that slot.
`OrdinaryCreateFromConstructor` selects the slot from NewTarget's function Realm
after the one observable prototype Get returns a non-object. A custom object
prototype is retained, and an abrupt Get propagates before allocation.

Created bootstrap uses `RealmFunctionMaterializationContext` for the constructor,
adopt, defer, move, use, disposeAsync and the disposed getter. It publishes one
disposeAsync function object under both its string name and `Symbol.asyncDispose`.
Function prototypes, defining Realm metadata and error prototype snapshots all
belong to that Realm. Prototype roots remain in a must-use bootstrap result until
global publication consumes them; no entry globals are temporarily replaced.

The move allocator selects the executing method's canonical prototype and
allocates its ordinary result in one operation. It accepts no caller-selected
prototype. The builtin ABI's zero environment denotes the entry Realm; foreign
methods always carry their own function object, whose defining Realm and
intrinsic record must exist. Missing records or canonical prototypes fail as
internal invariants rather than falling back to an unrelated Realm.

Disposal Promise capabilities use the existing typed current-function intrinsic
Promise constructor boundary. Internal disposal reactions retain their opaque
state independently of the function environment, allowing their real defining
Realm and Function prototype to survive. The reaction restores the disposal
method environment before continuing the resource walk and constructing errors.

The synchronous `Symbol.dispose` fallback uses an internal callable wrapper
created in the `use` method's Realm, independently of the later `disposeAsync`
method's Realm. It captures the selected method, allocates its canonical Promise
capability before calling that method, discards a normal return, and converts
a synchronous throw into rejection before the resource walk awaits. This
preserves `GetDisposeMethod` semantics even when the discarded return is a
Promise or has an observable `then` getter.

The engine targets `aot_created_realm_async_disposable_stack` and
`aot_async_disposable_stack_realm` cover publication, descriptor and prototype
identity, constructor versus NewTarget ownership, mutation of JavaScript global
properties, completion Promises, errors, move results and disposal callbacks.
The structural target `created_realm_async_disposable_stack_structure` guards
the new traced slot and its allocation/publication boundaries. These are
focused regression targets; they do not establish full conformance counts.
