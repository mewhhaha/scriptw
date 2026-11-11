# Intl.Locale heap-slot identity authority

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

## Closed layout identities

The passive Intl.Locale record contains exactly five capability-free
`IntlLocaleHeapSlot` identities in tag-payload, language-payload,
script-payload, region-payload and base-name-payload order.

One private exhaustive `metadata()` projection is the sole authority for all
five identities' record names, slot names, offsets, widths and pointer
classifications. Every slot remains eight bytes wide. Tag, language, script,
region and base-name payloads occupy offsets 0, 8, 16, 24 and 32 respectively.
All five payloads remain pointer-classified.

This zero-scalar/five-pointer census is a retention invariant. An initialized
Locale record must keep every materialized string payload visible to tracing,
including optional script and region payloads when present. An arbitrary row
can no longer omit one of those edges, classify it as scalar or reorder one
payload independently of the closed identity registry.

The focused recursive structure regression pins the exact capability-free
domain, rejects derived and manual incidental capabilities, requires one
no-wildcard metadata projection, preserves typed registry order and verifies
that no second Rust source constructs free-form Intl.Locale rows. The bounded
heap owner witness asserts every projected field. The private
`IntlLocaleStringSlot` remains the independent runtime authority for accessor
offsets and optional-result policy. Its fixed catalog boundary is recorded in
[`intl-locale-string-slot-dispatch.md`](intl-locale-string-slot-dispatch.md).

## Passive boundary

This invariant reorganizes passive Rust layout metadata only. It does not
change Locale allocation, initialization, accessors, optional script or region
semantics, canonicalization, emitted Wasm, root scanning or collector
execution. All Intl runtime offset consumers remain unchanged.

```sh
cargo test -p scriptw-aot-wasm --test intl_locale_heap_slot_structure
cargo test -p scriptw-aot-wasm --lib heap::tests::intl_locale_heap_slot_identities_own_layout_metadata -- --exact --test-threads=1
cargo test -p scriptw-aot-wasm --lib heap::tests::heap_layout_registry_ -- --test-threads=1
rustfmt --check crates/scriptw-aot-wasm/src/heap_intl_locale_layout.rs crates/scriptw-aot-wasm/src/heap.rs crates/scriptw-aot-wasm/tests/intl_locale_heap_slot_structure.rs
git diff --check
```

Dry source review pins the exact five rows, offsets 0, 8, 16, 24 and 32, the
zero-scalar/five-pointer census, typed registry order and unchanged Intl
runtime offset consumers. At the Batch AL checkpoint, `cargo xc` is green, the
new structure target passes `4/4`, its string-slot neighbor passed `3/3` at
that checkpoint, the bounded heap owner passes `1/1`, and the registry checks
pass `2/2`.
No runtime CLI, Test262 leaf or semantic golden was required or run for this
passive metadata change.
