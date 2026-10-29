# Benchmark refresh, 2026-10-29

Both CI cohorts measure compiler `afe83e312b5049bbd6c81edffacb418ba60275e5`; input fingerprint `7f17a9cb49312aa999dc1a6bf4659a2106bd685c84b91b280b8ff3d3419b2052`.

- [Precompiled CI 40895619031](https://github.com/mewhhaha/scriptw/actions/runs/40895619031): completed 2026-10-29T15:11:33.955150+00:00; 68 attempts across all 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- [Three-runtime CI 40895618949](https://github.com/mewhhaha/scriptw/actions/runs/40895618949): completed 2026-10-29T15:06:53.697221+00:00; 102 attempts across all 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

Each cohort uses two balanced rounds, ten checked warmups and seven measurements
per runtime/workload/round. The shared precompiled host disables compilation and
uses size-optimized/backtracking native code. Ordinary execution uses Fast mode
and includes in-process Scriptw/V8 compilation history; Scriptc's frontend is
separate. Interpret each cohort on its own: endpoint RSS includes runtime, code,
setup and warmup history, and is neither live heap nor peak memory.

The precompiled report retains before/after resource counters. The ordinary
schema records startup cap/swap/affinity facts without end counters; no zero-OOM
or measured-peak claim is inferred. Both existing inert validators readmitted the
raw evidence and published archives, and all 34 per-benchmark SVGs were reproduced under a
1-GiB, one-CPU, no-swap scope. Downloaded native code was never executed locally.
The refresh metadata retains the actual local validation resource counters.

Separate [unscored CI mapping evidence](benchmark-refresh-20261028-mappings.tar.xz)
retains four cases and sixteen snapshots at this CI source and fingerprint.
Checks bind exact R/P image, text and section ranges and host-page presence;
they establish neither executed-function ownership nor a causal explanation
for memory changes. The [refresh metadata](benchmark-refresh-20261028.json)
records the consumer and archive identities; the archive manifest binds every file.

The [implementation checkpoint](loader-admission-20261029.json) keeps its separate local
producer `afe83e312b5049bbd6c81edffacb418ba60275e5` and fingerprint
`18bd65b5a90a6be83aeb88b88c7808a0604e87cb1969cef7ed337ec343aae767`. The complete local 17-workload
comparison passed 144 processes and 2,448 output checks. Scriptw's geometric
mean post-execution RSS fell 4.7% and PSS fell 5.4% against the accepted Temporal
compiler; private dirty memory was flat and execution was broadly unchanged.
Focused integer timings retained a 0.2–2.9% slowdown, so this is not a universal
CPU improvement. The paired WASI control's RSS rose 1.7%; no direct causal
subtraction is claimed. Local results supply no bars in these
CI charts. Native acceptance and selected Test262 results retain their
actual checkpoint scope; this graph refresh does not rerun native acceptance or
the pinned Test262 publication. Earlier benchmark and optimization checkpoint
identities remain historical evidence, rather than measurements of this source.

[Precompiled table and evidence](precompiled-comparison.md) ·
[Three-runtime table and evidence](compiler-comparison.md) ·
[Refresh metadata](benchmark-refresh-20261028.json).
