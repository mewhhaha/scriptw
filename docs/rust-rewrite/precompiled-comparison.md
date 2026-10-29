# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-29T10:36:42.777150+00:00 through 2026-10-29T10:38:10.705013+00:00; source `c80fd9fbb9b38111b8df0e73ec6d6250ccbf7aee`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 0.718 | 0.140 | 19.926 / 17.928 / 3.115 | 12.588 / 10.532 / 2.957 |
| sort-reverse-512.js | 1.302 | 0.367 | 20.125 / 18.119 / 3.186 | 12.918 / 10.840 / 3.004 |
| sort-sorted-512.js | 1.194 | 0.371 | 19.961 / 17.964 / 3.182 | 12.963 / 10.884 / 3.004 |
| to-sorted-512.js | 1.300 | 0.411 | 19.990 / 17.956 / 3.203 | 12.846 / 10.788 / 3.018 |
| join-1024.js | 1.570 | 0.526 | 20.467 / 18.469 / 3.246 | 12.947 / 10.872 / 3.021 |
| join-4096.js | 4.427 | 1.735 | 20.916 / 18.901 / 3.770 | 13.121 / 11.083 / 3.502 |
| join-mixed-1024.js | 1.863 | 0.321 | 19.949 / 17.954 / 3.238 | 12.850 / 10.778 / 2.955 |
| numeric-loop-10000.js | 0.604 | 0.096 | 19.252 / 17.254 / 3.021 | 12.230 / 10.149 / 2.293 |
| numeric-loop-50000.js | 0.858 | 0.212 | 18.494 / 16.496 / 3.021 | 12.080 / 10.010 / 2.293 |
| folded-string-4096.js | 1.198 | 0.736 | 19.430 / 17.415 / 3.055 | 12.127 / 10.039 / 2.309 |
| dynamic-string-4096.js | 2.053 | 0.274 | 19.324 / 17.306 / 3.051 | 12.168 / 10.089 / 2.291 |
| integer.js | 0.761 | 0.068 | 18.711 / 16.691 / 3.070 | 12.186 / 10.107 / 2.291 |
| objects.js | 1.421 | 0.743 | 19.182 / 17.164 / 3.232 | 12.996 / 10.920 / 3.078 |
| retained-objects.js | 24.419 | 39.692 | 25.518 / 23.512 / 8.551 | 18.457 / 16.405 / 8.824 |
| join-empty-1024.js | 1.063 | 0.279 | 19.783 / 17.756 / 3.047 | 12.820 / 10.735 / 2.871 |
| join-singleton-1024.js | 1.110 | 0.133 | 19.875 / 17.869 / 3.045 | 12.238 / 10.164 / 2.291 |
| join-small-1024.js | 1.365 | 0.195 | 19.812 / 17.786 / 3.121 | 12.182 / 10.106 / 2.299 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
