# Benchmark refresh, 2026-10-30

Both CI cohorts measure compiler `aedaa6e4e0981462b4e31ade261ea4589a6644cf` with input fingerprint
`b5f22f4c755154275c6bcb711017ed34ab4dccc318ff8937318fe622e98b1d9f`.

- [Precompiled CI 41058273234](https://github.com/mewhhaha/scriptw/actions/runs/41058273234): completed 2026-10-30T13:15:53.493002+00:00; 68 attempts across 17 workloads.
- [Three-runtime CI 41058270647](https://github.com/mewhhaha/scriptw/actions/runs/41058270647): completed 2026-10-30T13:09:41.679869+00:00; 102 attempts across 17 workloads.

Both cohorts use two balanced rounds, ten checked warmups and seven measurements.
The precompiled host disables compilation and uses size-optimized/backtracking
native code. Ordinary execution uses Fast mode and includes Scriptw/V8 compilation
history. Process RSS includes setup, code and warmups; it is neither live JS heap
nor peak memory. Interpret the cohorts independently.

Existing inert validators readmitted raw evidence and published archives. All 34
SVG cards were byte-reproduced with PNG previews under a 1-GiB, one-CPU scope.
No downloaded native code ran locally. Resource counters are retained in metadata;
missing CI counters do not support a zero-OOM or measured-peak claim.

The [local implementation checkpoint](typed-regions-20261030.json) retains its own source
`6f70fffc4df3da19e000a993b7097ffa293c2ad8` and fingerprint `30cc450f603131fb913a7ebaae56ad9aca4aeba0eda60a83ca6dca761b2303fe`.
It covers 23 native scopes with 5,118 unique tests,
359 source-only guards and 679 original pinned Test262 executions
across 8 selected scopes. Full receipts remain in that checkpoint and its inert archive.
The published full Test262 matrix is unchanged.

[Precompiled evidence](precompiled-comparison.md) · [Three-runtime evidence](compiler-comparison.md).
