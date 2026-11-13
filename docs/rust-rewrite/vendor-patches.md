# Vendor patch inventory

These are applied Rust dependency patches. Cargo's `[patch.crates-io]` entries
and lockfile select the actual packages; notes alone do not enable a patch.

| Package | Applied patch notes | Contract / evidence owner |
| --- | --- | --- |
| Boa Engine 0.21.1 | [PATCHES.md](../../vendor/boa_engine-0.21.1/PATCHES.md) | Debug/differential oracle only; the patch notes bind its evidence and retained controls. |
| Wasmtime 47.0.0 | [PATCHES.md](../../vendor/wasmtime-47.0.0/PATCHES.md) | [Collector weak edges](contracts/collector-weak-edges.md); byte-array fill: [host-boundary screen](benchmarks.md#host-boundary-screen-2026-11-11); Store interrupt handle: [execution deadlines](contracts/execution-deadlines.md); per-operation host costs: [host-side costs](benchmarks.md#host-side-costs-2026-11-13) |
| Wasmtime Cranelift translator 47.0.0 | [PATCHES.md](../../vendor/wasmtime-internal-cranelift-47.0.0/PATCHES.md) | Idle compiler scratch bound; funcref reads and short array fills: [host-side costs](benchmarks.md#host-side-costs-2026-11-13) |
| Wasmtime cache 47.0.0 | [Original patch notes](../../vendor/wasmtime-internal-cache-47.0.0/LILA-PATCH.md) | [Runtime artifact persistence](contracts/runtime-artifact-persistence.md) |
| Timezone provider 0.1.2 | [Original patch notes](../../vendor/timezone_provider-0.1.2/LILA-PATCH.md) | [Named-zone data image](contracts/intl-named-time-zone-data-image.md), [named-zone projection](contracts/intl-named-zone-projection.md) |

The two original note filenames retain the historical project identity. Commands
inside those historical notes may use retired package names; use current Cargo
metadata and the maintained workflow when replaying them.

[Test262 proposals](../../test262/errata/proposals/README.md) are separate,
unapplied reports. The four tracked upstream disagreements remain ordinary
failures against the original pinned suite. Do not apply proposal patches to
produce published conformance numbers.
