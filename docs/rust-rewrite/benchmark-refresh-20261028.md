# Benchmark refresh, 2026-10-29

Both cohorts measure compiler `4751db4aa29ed56ba20820adeea040e271c60581`; input fingerprint `6e74007417fc8589aecdb228c40d55f7f7d37021c6278d51957930a1325e6c6e`.

- [Precompiled CI 40794431032](https://github.com/mewhhaha/scriptw/actions/runs/40794431032): 68 attempts across all 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- [Three-runtime CI 40794428275](https://github.com/mewhhaha/scriptw/actions/runs/40794428275): 102 attempts across all 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

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

The [implementation checkpoint](scriptc-performance-20261028.json) retains its actual
native/selected-Test262 source identities; this graph refresh does not rerun full
native acceptance or the pinned Test262 publication.

[Precompiled table and evidence](precompiled-comparison.md) ·
[Three-runtime table and evidence](compiler-comparison.md) ·
[Refresh metadata](benchmark-refresh-20261028.json).
