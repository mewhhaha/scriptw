# Heap collector-policy authority

## Current atomic GC source — 2026-10-05

Native `gc()` now consumes the registered `GcHostImport::CollectGc` token in
`builtins/host.rs`; its actual Engine callback invokes the selected Wasmtime
collector. Live JavaScript references are rooted GC values and typed strong
record edges. The old `NonMovingMetadataChecked` manual-heap policy and its
nonexecutable host route below describe the predecessor, rather than the
current product representation. The obsolete `heap_collector_policy_structure`
transport mirror is retired; `aot_native_host_values` authors a live-root
collector witness.

The source-only retirement removes the unused passive policy and required-phase
modules, their module declarations and their exact source mirrors. The final cleanup also retires the unused passive
weak-edge inventory; actual runtime capability remains explicit.
Promise state readers and settlement now consume GC fields; the old raw-offset
Promise lifecycle mirror is retired. Existing whole-value Promise and native
collector semantic controls remain authored and unrun.

Strong collector support does not supply the selected runtime's missing weak
reference/ephemeron facility. That remains an explicit
[unavailable capability](weak-unavailable-runtime-boundary.md), with no
strong-retaining weak substitute or parallel manual object model. The final
raw heap/helper retirement is written, and the bounded production caller
census has zero remaining retired-provider or passive-metadata references.
Final compiler-control and source-guard composition have finished independent
source review.

The complete source checkpoint includes types, meaningful controls and
documentation. The complete source and integration repairs pass the
whole-workspace, all-feature, all-target Rust type check on 2026-10-05 under the
confirmed 4 GiB aggregate cap. Source guards, emitted Wasm validation and runtime
proof remain pending; authored Rust controls have not been executed.
Earlier verification commands and results below retain their original source
scope; they are historical records, not instructions to run during the full-task
dry-source pass. Later verification follows the [batch workflow](../../../CONTRIBUTING.md#verification)
with a confirmed aggregate 4096 MiB cap, swap zero and serial execution.

## Historical predecessor record

## Closed passive policy

The passive heap inventory selects exactly one
`HeapCollectorPolicy::NonMovingMetadataChecked` identity. Six exhaustive
no-wildcard projections own its diagnostic name, movement behavior, root
sources, weak edges, required phases and executable state.

The policy remains named `non-moving-tracing-collector`, does not move objects,
uses the exact `HEAP_ROOT_SOURCES`, `HEAP_WEAK_EDGES` and
`REQUIRED_HEAP_COLLECTOR_PHASES` registries, and is not executable. The old
contract could independently combine an arbitrary name, movement Boolean,
capability and registry slices; those fields and the unused capability states
no longer exist.

Advancing collection now requires adding an explicit policy identity and
handling it in every projection. Flipping one capability field cannot make
`gc()` appear executable while roots, weak edges or phases remain disconnected.

The focused recursive structure guard pins the exact capability-free domain,
all six projections, registry identities, heap delegation and the host-GC
unsupported boundary.

## Passive boundary

This changes passive Rust metadata only. It does not implement tracing,
relocation, reclamation, ephemeron processing, weak clearing, finalization
cleanup or executable `gc()`. The host builtin continues to emit its explicit
unsupported throw.

```sh
cargo test -p scriptw-aot-wasm --test heap_collector_policy_structure
cargo test -p scriptw-aot-wasm --test heap_collector_phase_structure
cargo test -p scriptw-aot-wasm --test weak_edge_retention_structure
cargo test -p scriptw-aot-wasm --test heap_named_slot_storage_structure
cargo test -p scriptw-aot-wasm --lib heap::tests::heap_collector_policy_requires_all_gc_builtin_phases -- --exact --test-threads=1
cargo test -p scriptw-aot-wasm --lib heap::tests::heap_collector_policy_keeps_gc_builtin_unsupported_until_executable -- --exact --test-threads=1
cargo test -p scriptw-aot-wasm --lib tests::supports_host_gc_builtin_as_explicit_unsupported_throw -- --exact --test-threads=1
git diff --check
```

The policy, phase and weak-edge guards each pass `4/4`, and the adjusted
named-slot guard remains green at `3/3`. The exact phase inventory,
unsupported-policy and emitted host-GC throw witnesses each pass `1/1`, with
only the workspace's existing warnings. Targeted formatting and diff checks
pass. No broad workspace or conformance run was performed for this passive
transition.
