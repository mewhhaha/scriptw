# Benchmark refresh, 2026-10-30

Both CI cohorts measure compiler `64cf187d62a3322b781c910d5fd033c5bad10777`; input fingerprint `d1579a87fafb4b3fe7b3f6cc1bdb91fa8a4b01f76b17c7c26421680b2272f09a`.

- [Precompiled CI 41032186356](https://github.com/mewhhaha/scriptw/actions/runs/41032186356): completed 2026-10-30T09:40:05.715817+00:00; 68 attempts across 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- [Three-runtime CI 41032182998](https://github.com/mewhhaha/scriptw/actions/runs/41032182998): completed 2026-10-30T09:32:19.641197+00:00; 102 attempts across 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

Both cohorts use two balanced rounds, ten checked warmups and seven measurements
per runtime/workload/round. The precompiled host disables compilation and uses
size-optimized/backtracking native code. Ordinary execution uses Fast mode and
includes in-process Scriptw/V8 compilation history; Scriptc's frontend is separate.
Process RSS includes setup, code, runtime and warmup history, rather than live
heap or peak memory. Interpret the two cohorts independently.

The existing inert validators readmitted raw evidence and published archives.
All 34 per-benchmark SVG cards were byte-reproduced with PNG previews under a
1-GiB, one-CPU, no-swap scope. No downloaded native code ran locally. Metadata
retains validation resource counters; ordinary evidence has no end counters,
so no zero-OOM or measured-peak claim is inferred for that cohort.

The [local implementation checkpoint](join-hotpaths-20261030.json) retains source
`64cf187d62a3322b781c910d5fd033c5bad10777` and fingerprint `d6f09d2d953874e4ac29bd5373dd71c5d7d9b1bf9e36644caf84f7dc59f1a0e3`.
Its local before/after results and actual native verification scope remain
separate from these CI bars. This refresh does not rerun native acceptance or
the pinned Test262 publication. Earlier checkpoint and mapping identities remain
historical evidence; they are not relabeled as measurements of this CI source.

The local checkpoint accepts 4 explicitly selected native scopes
with 825 unique package/target/test identifiers and 156 selected
original pinned Test262 executions across four scopes. This is a scoped result,
not full workspace, ABI-wide or full Test262 acceptance. Baseline execution-only
PC profile status: `complete`; candidate
profile status: `complete`. Profiles retain
their own source identities and are self-sample diagnostics, not inclusive costs.
Unrun work carries no passing result.

The first-candidate profile status is
`complete`. Its full
comparison and repeat controls remain historical non-promoted diagnostics;
their rows do not enter the final local ratios or these CI bars.

[Precompiled charts and evidence](precompiled-comparison.md) ·
[Three-runtime charts and evidence](compiler-comparison.md) ·
[Refresh metadata](benchmark-refresh-20261028.json).
