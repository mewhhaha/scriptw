# Benchmark refresh, 2026-10-29

Both CI cohorts measure compiler `f19370fcf91018a7153cf9d8a4b38fdcf3f5f3df`; input fingerprint `c67902f8c4b5b9a4aeae7bdc2e6a87a7dac742ce55e508cab356ddb808b66882`.

- [Precompiled CI 40968350650](https://github.com/mewhhaha/scriptw/actions/runs/40968350650): completed 2026-10-29T23:15:06.482739+00:00; 68 attempts across 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- [Three-runtime CI 40968346853](https://github.com/mewhhaha/scriptw/actions/runs/40968346853): completed 2026-10-29T23:02:16.626216+00:00; 102 attempts across 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

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

The [local implementation checkpoint](binary-scalar-20261029.json) retains source
`d0a134cfd9b96c743590c14e3acc6186f913dfb7` and fingerprint `b5acdef537cdb73aeb51f5f62eb981b8d040fd0e8f9de60e62e264e20771c0b6`.
Its local before/after results and actual native verification scope remain
separate from these CI bars. This refresh does not rerun native acceptance or
the pinned Test262 publication. Earlier checkpoint and mapping identities remain
historical evidence; they are not relabeled as measurements of this CI source.

The local checkpoint accepts 21 explicitly selected native scopes
with 5081 unique package/target/test identifiers and 191 selected
original pinned Test262 executions across six scopes. This is a scoped result,
not full workspace or full Test262 acceptance. Lean-consumer control status:
`complete`; join PC profile status:
`complete`. Unrun work carries no passing result.

[Precompiled charts and evidence](precompiled-comparison.md) ·
[Three-runtime charts and evidence](compiler-comparison.md) ·
[Refresh metadata](benchmark-refresh-20261028.json).
