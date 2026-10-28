# Realm-record heap-slot identity authority

Current dry source — 2026-10-05: the raw-offset source-mirror target below
is retired from the atomic semantic GC draft. It checked passive layout
spelling, offsets and source occurrence counts. The current GC authoring uses
typed registry fields and complete semantic value owners; these earlier
mirrors cannot verify that representation or its behavior. Historical source
descriptions, commands and results below retain their original scope.

The full GC source cutover, remaining Temporal and AsyncDisposableStack
authoring, compiler checks and runtime verification remain pending. See the
[current value/heap architecture](../value-heap-gc.md). Weak reachability retains
its explicit [unavailable facility boundary](weak-unavailable-runtime-boundary.md).
No verification or conformance result is inferred from this retirement.

## Current atomic GC source — 2026-10-05

The semantic Realm record is the actual typed GC `RealmRecord` declared by
`gc_types/layouts.rs`, with schema-owned typed accessors. Its intrinsic table,
global environment, module registry and other retained ownership edges are
collector references. NativeHost's created Realm uses that same record and
completed bootstrap. The historical eight-byte manual-heap offset/pointer
inventory below is not the current field or tracing authority.

No compatibility record or manual object model is introduced. Final retirement
of raw heap offset/helper consumers and composed guard reconciliation remain
part of the source pass. See [NativeHost GC values](native-host-gc-values.md).

All source, types and controls for the atomic batch remain uncompiled and
unexecuted. Final representation/helper/guard composition also remains pending.
Earlier verification commands and results below retain their original source
scope; they are historical records, not instructions to run during the full-task
dry-source pass. Later verification follows the [batch workflow](../../../CONTRIBUTING.md#verification)
with a confirmed aggregate 4096 MiB cap, swap zero and serial execution.

## Historical predecessor record

## Closed layout identities

The passive Realm record contains exactly eight capability-free
`RealmRecordHeapSlot` identities in Realm-id, Agent-id, global-object,
global-this, global-environment, intrinsics, host-hooks and module-registry
order.

One private exhaustive `metadata()` projection is the sole authority for all
eight identities' record names, slot names, offsets, widths and pointer
classifications. Every slot remains eight bytes wide. Realm and Agent ids
occupy offsets 0 and 8. Global object, global this, global environment,
intrinsics, host hooks and module registry occupy offsets 16, 24, 32, 40, 48
and 56. The two ids remain scalar, while all six Realm ownership edges remain
pointer-classified. The record size is 64 bytes.

This two-scalar/six-pointer census is a retention invariant. A Realm must
keep its global state, intrinsic table, host state and module registry visible
to tracing, while neither identity word may be scanned as an address. An arbitrary row can no longer reverse either side of that
relation or reorder one field independently of the closed identity registry.

The focused recursive structure regression pins the exact capability-free
domain, rejects derived and manual incidental capabilities, requires one
no-wildcard metadata projection, preserves typed registry order and verifies
that no second Rust source constructs free-form Realm rows. The bounded heap
owner witness asserts every projected field. `RealmRecordLocal`, Realm-id
allocation and created-Realm publication policies remain independent lifetime
and semantic authorities.

## Private-element ownership correction

The 2026-09-12 correction removes the obsolete Realm private-element head and its
initialization. Each Private Name now owns its rows through a pointer-bearing
slot in its declaring private environment. This prevents foreign-eval class
definitions from being separated from later instance brands by the caller's
execution realm. All eight remaining Realm offsets are unchanged. The
structure target passes 4/4, the backend library passes 431/431 and the
finite-eval Wasmtime target passes 12/12; see the
[batch verification](../expression-semantics-followup-20260912.md).

## Historical passive boundary

The original nine-slot identity migration reorganized passive Rust layout metadata only. It does not
change Realm allocation, initialization, lookup, intrinsic publication,
global-environment behavior, host hooks, module loading, private elements,
emitted Wasm, root scanning or collector execution. All Realm runtime offset
and size consumers remain unchanged.

```sh
cargo test -p scriptw-aot-wasm --test realm_record_heap_slot_structure
cargo test -p scriptw-aot-wasm --test modules_realms -- created_realm_array_prototype_structure::
cargo test -p scriptw-aot-wasm --test modules_realms -- created_realm_promise_publication_structure::
cargo test -p scriptw-aot-wasm --lib heap::tests::realm_record_heap_slot_identities_own_layout_metadata -- --exact --test-threads=1
cargo test -p scriptw-aot-wasm --lib heap::tests::heap_layout_registry_ -- --test-threads=1
rustfmt --check crates/scriptw-aot-wasm/src/heap_realm_record_layout.rs crates/scriptw-aot-wasm/src/heap.rs crates/scriptw-aot-wasm/tests/realm_record_heap_slot_structure.rs
git diff --check
```

At that earlier checkpoint, dry source review pinned the exact nine rows, offsets 0, 8, 16, 24, 32, 40, 48,
56 and 64, the two-scalar/seven-pointer census, typed registry order and
unchanged runtime offset consumers. At the Batch AN checkpoint, `cargo xc` is
green, the new structure target passes `4/4`, the Array and Promise Realm
neighbors pass `3/3` and `5/5`, the bounded heap owner passes `1/1`, and the
registry checks pass `2/2`. No runtime CLI, Test262 leaf or semantic golden was
required or run for this passive metadata change.
