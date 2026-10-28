# Created-Realm Array Prototype Lifecycle

## Current atomic GC source — 2026-10-05

The private `functions/created_realm_array_prototype.rs` owner still separates
`ReservedRealmArrayPrototypeLocal(GcLocalSlot<ArrayObject>)` from the completed
`RealmArrayPrototypeLocal(GcLocal<ArrayObject>)`. Both have private fields and
no Clone/Copy capability. Shared GC bootstrap consumes reservation,
initialization, intrinsic publication and constructor linking before NativeHost
exposes the created global. The historical host-only consumer and raw
payload/tag/call-count census below have retired.

The maintained `created_realm_array_prototype_structure` guard protects these
real private reservation/completion types and their typed method boundary.
Existing `aot_created_realm_array_iterator` semantic sources remain. See
[NativeHost GC values](native-host-gc-values.md).

All source, types and controls for the atomic batch remain uncompiled and
unexecuted. Final representation/helper/guard composition also remains pending.
Earlier verification commands and results below retain their original source
scope; they are historical records, not instructions to run during the full-task
dry-source pass. Later verification follows the [batch workflow](../../../CONTRIBUTING.md#verification)
with a confirmed aggregate 4096 MiB cap, swap zero and serial execution.

## Historical predecessor record

## Private owner

A created realm's `%Array.prototype%` progresses through two states owned by
the private `functions/created_realm_array_prototype.rs` module:

- `ReservedRealmArrayPrototypeLocal` is reserved temporary storage that has
  not yet been initialized as an Array exotic object.
- `RealmArrayPrototypeLocal` proves that the storage contains the initialized
  Array prototype for publication and constructor linking.

Both states are non-`Copy`, have private fields, and are absent from the parent
module's imports and re-exports. Callers infer the states from inherent method
results, so only the child can construct a witness or project its raw local.

## Lifecycle

The created-realm host bootstrap is the only complete external consumer. It
reserves storage, consumes that storage while initializing the Array exotic
object, borrows the initialized witness while publishing and defining
properties, then consumes the witness when releasing its temporary local.

Initialization emits a zero `length`, installs the created realm's
`%Object.prototype%` payload as `[[Prototype]]`, and records the Object tag for
that prototype relation. Publication writes the initialized Array object into
the created realm's Array-prototype intrinsic slot.

Constructor linking records the Array payload and tag on `%Array%`, defines
`%Array%.prototype` with `{ writable: false, enumerable: false, configurable:
false }`, and defines `%Array.prototype%.constructor` with `{ writable: true,
enumerable: false, configurable: true }`.

The bootstrap defines `Symbol.iterator` as the same function object as `values`,
with writable and configurable set and enumerable unset. It uses the initialized
Array witness and the well-known symbol key, so literals created in the realm
inherit a working default iterator.

The recursive call census is one reserve, one initialize, one intrinsic store,
four property-definition calls, one constructor bind and one release. Three of
the property-definition calls remain in the host bootstrap; the fourth is the
child-owned `constructor` definition used by the bind method.

## Original extraction evidence

The exact 16-line state block retains SHA-256
`d557dc697bfaf3c5b9ac81521126963a18f1c5fbb7cd11ab7afbad94d76d0b0a`.
The exact 168-line method block retains SHA-256
`1784e1c9ebf445d237c1da5ad952e250a3a5d1cf8dd15c773bae6bf2a19aea17`.
Together the 184 selected source lines retain SHA-256
`10e8fae5d82a0ae8440df773b12241da77102853725bbb35bbd7e99d8e279fa1`.
The 189-line private child has SHA-256
`22b55e6995b096b43ee910a8fd158b73d6d0f455e37dde1ffb78277a65e988f2`,
and the extraction reduces `functions.rs` from 12,452 to 12,267 lines.

The focused structure witness pins sole type, construction, projection and
method ownership; the exact recursive call census; the initializer's Array
layout; both property-attribute rows; and the host lifecycle order. The
module-boundary audit separately enforces the private module, absence of
imports and re-exports, child and parent line budgets, and the same ownership
census.

The original extraction changed ownership without changing observable behavior.
The later iterator-alias repair adds bootstrap publication and is covered by
`aot_created_realm_array_iterator`, including descriptors, function identity,
and iteration within and across realms.
