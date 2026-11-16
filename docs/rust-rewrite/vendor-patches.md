# Vendor patch inventory

These are applied Rust dependency patches. Cargo's `[patch.crates-io]` entries
and lockfile select the actual packages; notes alone do not enable a patch.

| Package | Applied patch notes | Contract / evidence owner |
| --- | --- | --- |
| Boa Engine 0.21.1 | [PATCHES.md](../../vendor/boa_engine-0.21.1/PATCHES.md) | Debug/differential oracle only; the patch notes bind its evidence and retained controls. |
| Boa AST 0.21.1 and Boa parser 0.21.1 | [PATCHES.md](../../vendor/boa_ast-0.21.1/PATCHES.md) | Product front end. Class static block names: engine test `a_class_static_block_declares_its_functions_as_a_function_body_does`; kept body record for direct eval: oracle only, owned by the Boa Engine notes. |
| Wasmtime 47.0.0 | [PATCHES.md](../../vendor/wasmtime-47.0.0/PATCHES.md) | [Collector weak edges](contracts/collector-weak-edges.md); byte-array fill: [host-boundary screen](benchmarks.md#host-boundary-screen-2026-11-11); Store interrupt handle: [execution deadlines](contracts/execution-deadlines.md); per-operation host costs: [host-side costs](benchmarks.md#host-side-costs-2026-11-13); GC heap null guard: [GC access code](benchmarks.md#gc-access-code-2026-11-14); global roots and young generation: [young generation](benchmarks.md#young-generation-2026-11-14); funcref intern cache with the null funcref at id zero: [branch hints and async reductions](benchmarks.md#branch-hints-the-async-requests-extra-work-and-node-2026-11-16) |
| Wasmtime Cranelift translator 47.0.0 | [PATCHES.md](../../vendor/wasmtime-internal-cranelift-47.0.0/PATCHES.md) | Idle compiler scratch bound; funcref reads and short array fills: [host-side costs](benchmarks.md#host-side-costs-2026-11-13); GC access code: [GC access code](benchmarks.md#gc-access-code-2026-11-14); generational write barrier: [young generation](benchmarks.md#young-generation-2026-11-14); frame reloads, proven references, threaded branches and set tests: [translation](benchmarks.md#frame-reloads-proven-references-and-threaded-branches-2026-11-16); branch hints at every two-way branch, cold paths and funcref stores without a call: [branch hints and async reductions](benchmarks.md#branch-hints-the-async-requests-extra-work-and-node-2026-11-16) |
| Wasmtime cache 47.0.0 | [Original patch notes](../../vendor/wasmtime-internal-cache-47.0.0/LILA-PATCH.md) | [Runtime artifact persistence](contracts/runtime-artifact-persistence.md) |
| Timezone provider 0.1.2 | [Original patch notes](../../vendor/timezone_provider-0.1.2/LILA-PATCH.md) | [Named-zone data image](contracts/intl-named-time-zone-data-image.md), [named-zone projection](contracts/intl-named-zone-projection.md) |

The two original note filenames retain the historical project identity. Commands
inside those historical notes may use retired package names; use current Cargo
metadata and the maintained workflow when replaying them.

[Test262 proposals](../../test262/errata/proposals/README.md) are separate,
unapplied reports. The four tracked upstream disagreements remain ordinary
failures against the original pinned suite. Do not apply proposal patches to
produce published conformance numbers.
