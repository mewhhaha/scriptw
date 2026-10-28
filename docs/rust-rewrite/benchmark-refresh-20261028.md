# Benchmark refresh, 2026-10-28

Both cohorts measure compiler `f02efa8c5458ee62c3158b5d1aac2b5b59c1b560`; input fingerprint `43be01c80460b80f7fb9700a90b14c7a64cfc6d597b2c5c52d0a6f1dfb7e9b4b`.

- [Precompiled CI 40760954840](https://github.com/mewhhaha/scriptw/actions/runs/40760954840): 68 attempts across all 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- [Three-runtime CI 40760950405](https://github.com/mewhhaha/scriptw/actions/runs/40760950405): 102 attempts across all 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

Each cohort uses two balanced rounds, ten checked warmups and seven measurements
per runtime/workload/round. The shared precompiled host disables compilation and
uses size-optimized/backtracking native code. Ordinary execution uses Fast mode
and includes in-process Scriptw/V8 compilation history; Scriptc's frontend is
separate. Interpret each cohort on its own: endpoint RSS includes runtime, code,
setup and warmup history, and is neither live heap nor peak memory.

The precompiled report retains before/after resource counters. The older ordinary
schema records startup cap/swap/affinity facts without end counters; no zero-OOM
or measured-peak claim is inferred. Both existing inert validators readmitted the
original evidence and all four SVGs were regenerated under a 1-GiB, one-CPU,
no-swap scope. Downloaded native code was never executed locally.

The [implementation checkpoint](retained-runtime-20261028.json) retains its actual
native/selected-Test262 source identities; this graph refresh does not rerun full
native acceptance or the pinned Test262 publication.

[Precompiled table and evidence](precompiled-comparison.md) ·
[Three-runtime table and evidence](compiler-comparison.md) ·
[Refresh metadata](benchmark-refresh-20261028.json).
