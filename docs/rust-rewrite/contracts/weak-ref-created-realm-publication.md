# Created-Realm `WeakRef` publication

## Current atomic GC source — 2026-10-05

The common completed GC bootstrap publishes WeakRef intrinsics with the
actual typed Realm and FunctionContext. NativeHost's separate
`created_realm_weak_ref_intrinsics.rs` installer and
`created_realm_weak_ref_publication_structure` raw publication mirror are
retired. Existing CLI fixture bytes and every earlier focused receipt remain.

The selected [weak facility](weak-unavailable-runtime-boundary.md) is still
unavailable. Current valid preflight rejects before keeping a target; it does
not retain targets strongly as a substitute. The earlier record/token/hash
census and successful-instance observations below describe their predecessor.

All source, types and controls for the atomic batch remain uncompiled and
unexecuted. Final representation/helper/guard composition also remains pending.
Earlier verification commands and results below retain their original source
scope; they are historical records, not instructions to run during the full-task
dry-source pass. Later verification follows the [batch workflow](../../../CONTRIBUTING.md#verification)
with a confirmed aggregate 4096 MiB cap, swap zero and serial execution.

## Historical predecessor record

Current source — 2026-10-03: actual weak operations follow the
[unavailable facility boundary](weak-unavailable-runtime-boundary.md). Active
strong-retaining weak producers are retired. Installed intrinsic publication
and earlier JavaScript validation remain observable; passive layout/edge rows
are inventory only. Producer, valid-instance and verification descriptions
below retain their earlier source scope and do not verify the current batch.
Real weak reachability and semantic GC remain open; current controls are unexecuted.

## Scope

This boundary publishes the implemented `WeakRef` constructor and prototype in
realms created by the Test262 host capability. It closes the missing intrinsic
surface needed to exercise the existing constructor prototype fallback and
current-function-Realm TypeErrors. It does not implement weak reachability.

The entry-Realm algorithm owners remain:

- `crates/scriptw-aot-wasm/src/builtins/weak_ref.rs` for construction, `deref`,
  branding and errors; and
- `crates/scriptw-aot-wasm/src/intrinsics/collections.rs` for the entry-Realm
  `deref` and `@@toStringTag` properties.

Created-Realm materialization and publication are owned by the private
`crates/scriptw-aot-wasm/src/builtins/host/created_realm_weak_ref_intrinsics.rs`
child. The parent host bootstrap retains only the inferred producer/consumer
call pair.

## Publication lifecycle

`CreatedRealmWeakRefIntrinsics` is `pub(super)` only because Rust requires the
return and parameter type to be visible with its sibling-visible methods. It is
non-`Copy` and `must_use`, and both fields remain child-private. The parent
cannot construct or project its raw prototype and constructor locals; a
recursive source policy also forbids parent import, re-export or explicit
naming. Before returning that token, the materializer must:

1. allocate `%WeakRef.prototype%` under the created Realm's
   `%Object.prototype%`;
2. store that identity in the closed
   `NonArrayRealmIntrinsicSlot::WeakRefPrototype` slot;
3. materialize the fresh constructor and install the reciprocal
   constructor/prototype links with their exact descriptor attributes;
4. install the fresh `deref` function; and
5. append the exact `@@toStringTag` data property.

Only the consuming publisher accepts this token. It exposes `WeakRef` on the
created global and releases both retained locals. The ordinary created-Realm
body cannot publish a raw constructor/prototype pair or omit the Realm slot
write while still calling that publisher.

The `WeakRef.prototype` property on the constructor is non-writable,
non-enumerable and non-configurable. This differs from the common constructor
prototype link used by Map and Set, so created-Realm bootstrap uses the
explicit-flags operation. The prototype's `constructor` and `deref` properties
remain writable, non-enumerable and configurable. `@@toStringTag` is the
well-known Symbol key, not the ordinary String `"Symbol.toStringTag"`, and has
attributes `{ writable: false, enumerable: false, configurable: true }`.
Constructor-first linking matches entry-Realm bootstrap and preserves the exact
`Reflect.ownKeys(WeakRef.prototype)` order: `constructor`, `deref`,
`Symbol.toStringTag`.

## Realm ownership

Both created callables are materialized through the existing
`RealmFunctionMaterializationContext`. Before either is exposed, its
environment handle points to itself and its TypeError prototype snapshot points
to the created Realm's `%TypeError.prototype%`.

Consequently:

- calling the created constructor without `new`;
- passing a target that cannot be held weakly; and
- borrowing the created Realm's `deref` with an incompatible receiver

all create the error in the executing builtin's Realm. The constructor's
existing `NewTargetPrototypeFallback::RealmIntrinsic` path now finds the
created Realm's populated WeakRef slot for primitive foreign
`NewTarget.prototype` values.

## Evidence

The bounded source test is
`crates/scriptw-aot-wasm/tests/created_realm_weak_ref_publication_structure.rs`.
It pins the private token lifecycle, the required Realm slot, both self-backed
callables, constructor-first own-key order, the exact descriptors, the existing
constructor fallback and the current-function-Realm error routes.

The source-free fixture
`crates/scriptw-cli/tests/fixtures/wasm_weak_ref_created_realm.js` covers fresh
identities, prototype parents, exact prototype own keys, construction and
`deref`, all seven primitive fallback classes after overwriting the created
Realm's mutable global binding, and the three created-Realm TypeError branches.

On 2026-08-27, `cargo xc`, formatting, diff hygiene, task-plan,
module-boundary, shortcut-inventory and fixture-syntax gates passed. The
bounded structure target passed `4/4`, and the exact CLI fixture passed `1/1`.
The six selected non-GC pinned files below passed all `12/12` sloppy/strict
Wasm-AOT executions with every failure bucket at zero. The shared semantic
golden passed `2/2` in 685.75 seconds with 682 dumps. Relative to the preceding
680-dump Number checkpoint it added only this fixture and the independent
for-await identifier-assignment witness, removed none and left all 680 retained
dumps byte-identical.

Batch AF moved the complete carrier, materializer and publisher together. The
exact pre-move five-line carrier and 153-line method selection retain SHA-256
`50b98c378f3a260b73ab69e22538e856b957c5203c470489baab4e0677568244`
and
`10a82a763c5b87ef5e28dbd72e26d62d1873675864462e7622b3dd06cbff7a68`;
their combined 158-line hash is
`24d820fe3c2b14085b1f3aa3373537fb1def4ef211ef410025aea1f68300f119`.
After the required `pub(super)` visibility changes, the same selected bodies
have SHA-256
`9d17048c6ff4a8f4cacfd97b8c6ac0edc40c5039fe18f563843f931fe403e479`.
The resulting 8,941-line parent and 163-line child have SHA-256
`6ffaf8361a886420f7ee766a66154f6fc42bf9c5704cac6a2fc7e9e64e218b3a`
and
`5d460ca0be7e9eef7f81cee28ca258ac1fc9b4b6b655523651f4b64d4caea049`.
The unchanged inferred 12-line caller pair retains SHA-256
`41419c46072b0e4ae037b6217d5312768a77c9ff4f6c780b55381be0253a96eb`.
Recursive structure and module policies pin five child carrier mentions, sole
construction and destructure, one producer/consumer definition and parent call
each, private fields, and consuming release of both locals. The retargeted
source target passes `4/4`. At the Batch AF shared checkpoint, `cargo xc` is
green and the exact created-Realm WeakRef CLI fixture passes `1/1`. The six
exact pinned leaves below pass all `12/12` sloppy/strict Wasm-AOT executions
with every failure bucket at zero. Batch AF did not rerun the semantic golden;
the earlier golden result above remains historical evidence.

On 2026-08-31, a correctness follow-up moved the reciprocal constructor link
ahead of `deref` and `@@toStringTag`. The strengthened structure target passes
`5/5`, fixture syntax is valid, and the exact live Wasm-AOT fixture passes
`1/1`, including the exact own-key order and BigInt fallback. On 2026-09-01,
the focused constructor, `deref`, tag, prototype and foreign-NewTarget cohort
passed all `12/12` sloppy/strict Wasm-AOT executions with every failure and
non-success bucket at zero:

- `built-ins/WeakRef/prototype/constructor.js`;
- `built-ins/WeakRef/prototype/deref/prop-desc.js`;
- `built-ins/WeakRef/prototype/Symbol.toStringTag.js`;
- `built-ins/WeakRef/prototype/prop-desc.js`;
- `built-ins/WeakRef/proto-from-ctor-realm.js`; and
- `built-ins/WeakRef/newtarget-prototype-is-not-object.js`.

At that shared checkpoint, `cargo xc`, formatting, diff hygiene, task-plan,
README-status, host-ABI and fixture-syntax gates are green. The broad
`scriptw-aot-wasm` library target remains at its unchanged `367/374` baseline.
The module-boundary gate is red only on the pre-existing raw line caps in
`scriptw-ir/src/lowering/builtin_call_info.rs` (`2291 > 2250`) and
`scriptw-ir/src/builtins.rs` (`1769 > 1760`).

The authoritative pinned cases are:

- `built-ins/WeakRef/proto-from-ctor-realm.js`;
- `built-ins/WeakRef/newtarget-prototype-is-not-object.js`;
- `built-ins/WeakRef/prototype/constructor.js`;
- `built-ins/WeakRef/prototype/prop-desc.js`;
- `built-ins/WeakRef/prototype/deref/this-not-object-throws.js`; and
- `built-ins/WeakRef/prototype/deref/custom-this.js`.

The cross-Realm file constructs `new other.Function()` and therefore remains
subject to the Wasm-AOT dynamic-source policy. The source-free fixture is the
direct product-path witness; it does not make the policy-classified pinned file
green by substitution.

The focused verification commands are:

```sh
cargo test -p scriptw-aot-wasm --test created_realm_weak_ref_publication_structure
cargo test -p scriptw-cli --test cli \
  iterator::run_wasm_backend_succeeds_for_created_realm_weak_ref_publication \
  -- --exact
```

## Nonclaims

The other weak families are owned by their separate created-Realm publication
contracts. This boundary does not add weak cleanup jobs. The current record
still retains its target strongly because the collector's weak/ephemeron
capability remains unavailable. No GC, liveness, cleanup ordering, full WeakRef
tree, broad Realm matrix or T21 completion claim follows from this publication
seam.
