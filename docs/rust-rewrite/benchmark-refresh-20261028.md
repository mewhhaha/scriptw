# Benchmark refresh, 2026-10-29

Both CI cohorts measure compiler `54a28246ee78d8e58b07c8c5e3c705d3c2ab03e8`; input fingerprint `d2c4f2a64b261f708eb2160d7e6e000f773b5fda1f8927e4453e8fe552ec0458`.

- [Precompiled CI 40948863776](https://github.com/mewhhaha/scriptw/actions/runs/40948863776): completed 2026-10-29T20:49:15.379341+00:00; 68 attempts across 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- [Three-runtime CI 40948858641](https://github.com/mewhhaha/scriptw/actions/runs/40948858641): completed 2026-10-29T20:52:18.078448+00:00; 102 attempts across 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

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

The [local implementation checkpoint](execution-parity-20261029.json) retains source
`a5448d285613194db3db42758307acc98262880b` and fingerprint `1c09229d0aea8576e791b4981fbecaaeabe28e1af00566c9a2303d0afd38a4f0`.
Its local before/after results and actual native verification scope remain
separate from these CI bars. This refresh does not rerun native acceptance or
the pinned Test262 publication. Earlier checkpoint and mapping identities remain
historical evidence; they are not relabeled as measurements of this CI source.

[Precompiled charts and evidence](precompiled-comparison.md) ·
[Three-runtime charts and evidence](compiler-comparison.md) ·
[Refresh metadata](benchmark-refresh-20261028.json).
