# Benchmark refresh, 2026-10-29

Both CI cohorts measure compiler `c80fd9fbb9b38111b8df0e73ec6d6250ccbf7aee`; input fingerprint `d85bf76ef26b7358dddb15588712ba09167253a58ac7f909f617aaaa92236ace`.

- [Precompiled CI 40855431176](https://github.com/mewhhaha/scriptw/actions/runs/40855431176): completed 2026-10-29T10:38:10.705013+00:00; 68 attempts across all 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- [Three-runtime CI 40855427163](https://github.com/mewhhaha/scriptw/actions/runs/40855427163): completed 2026-10-29T10:29:14.701134+00:00; 102 attempts across all 17 workloads; Rust `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

Each cohort uses two balanced rounds, ten checked warmups and seven measurements
per runtime/workload/round. The shared precompiled host disables compilation and
uses size-optimized/backtracking native code. Ordinary execution uses Fast mode
and includes in-process Scriptw/V8 compilation history; Scriptc's frontend is
separate. Interpret each cohort on its own: endpoint RSS includes runtime, code,
setup and warmup history, and is neither live heap nor peak memory.

The precompiled report retains before/after resource counters. The ordinary
schema records startup cap/swap/affinity facts without end counters; no zero-OOM
or measured-peak claim is inferred. Both existing inert validators readmitted the
raw evidence and published archives, and all four SVGs were reproduced under a
1-GiB, one-CPU, no-swap scope. Downloaded native code was never executed locally.
The refresh metadata retains the actual local validation resource counters.

Separate [unscored CI mapping evidence](benchmark-refresh-20261028-mappings.tar.xz)
retains 16 snapshots and 12 raw streams at this CI source/fingerprint, using
consumer SHA-256 prefix `283d2706`. Checks bind phases, modules and host
attribution, including exact R/P image, text and section ranges. Cold diagnostic
RSS differences below are Scriptw minus WASI; they do not change the scored charts.

| Workload | Cold RSS gap (KiB) | Host read-only / read-execute gap (KiB) |
| --- | ---: | ---: |
| `integer.js` | +4,568 | +796 / +3,232 |
| `retained-objects.js` | +4,012 | +832 / +3,264 |

Native-function ownership and execution attribution remain deferred. Measure
individual Wasmtime loading phases to identify the causal trigger. These mappings
do not establish a fault-around cause or justify a new production change. Local
`e8ef6700a` matched captures retain their narrower attribution and do not bind exact
R/P ranges.

The [implementation checkpoint](temporal-demand-20261029.json) keeps its separate local
producer `e8ef6700ab5757d06f18366c65bfbc8ef07019a3` and fingerprint
`46a22285579e3e5a762973849dbcaabff0530873e3386be5238f2be72efb843a`. Local control timings fell about 22%
for startup and 27% for eight Realms. First Temporal use was approximately
unchanged; repeated use took about 0.8% longer. RSS was mixed, so these controls
do not establish an overall performance or memory gain. They supply no bars in
these CI charts. Native acceptance and selected Test262 results retain their
actual checkpoint scope; this graph refresh does not rerun native acceptance or
the pinned Test262 publication. Earlier benchmark and optimization checkpoint
identities remain historical evidence, rather than measurements of this source.

[Precompiled table and evidence](precompiled-comparison.md) ·
[Three-runtime table and evidence](compiler-comparison.md) ·
[Refresh metadata](benchmark-refresh-20261028.json).
