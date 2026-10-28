# Benchmark refresh, 2026-10-28

Source `a0d1366aeb8da2acdd992e86d884519f96b5730e`; compiler-input fingerprint `b3d43fc17aca01dfc089129b33b4060cdbd7b096649a95a75119eadc24832889`.

Paired [CI 40674504933](https://github.com/mewhhaha/scriptw/actions/runs/40674504933): 68/68 attempts and 17 workload checksums, plus six native observer controls and four separately validated unscored ownership captures. Recorded Rust: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

Three-runtime [CI 40674499711](https://github.com/mewhhaha/scriptw/actions/runs/40674499711): 102 attempts and 17 workloads; Scriptc successful rows: 17/17, with actual failure labels retained. Recorded Rust: `rustc 1.99.0 (b940084d7 2026-09-28)`.

The paired cohort executes precompiled native images with both consumer compilers disabled, size-optimized/backtracking code and the shared generic ISA configuration. The three-runtime cohort uses ordinary Fast execution and includes in-process Scriptw/V8 source compilation history; Scriptc source compilation is separate. Endpoint RSS includes runtime/code/setup/warmup history, not live JS heap or a process peak. Do not subtract values across cohorts or infer causal savings.

Each cohort uses two rounds, ten checked warmups and seven measurements per runtime/workload/round; the median pools fourteen measured observations. All original 17 cases remain.

The paired measurement retains its actual resource counters. The older three-runtime schema records only startup cap/swap/single-CPU facts; before/after OOM events, peak and grouped-OOM receipts are unavailable and remain explicit. Both inert validators have separate actual 1 GiB CPU0 processing counters.

The 4,917 native-test checkpoint is historical implementation verification at 0658e6ae6, not a rerun in this refresh. Pinned Test262 status is unchanged.

[Paired table/evidence](precompiled-comparison.md) · [Three-runtime table/evidence](compiler-comparison.md) · [Refresh metadata](benchmark-refresh-20261028.json).
