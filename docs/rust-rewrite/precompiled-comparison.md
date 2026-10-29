# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-29T00:55:09.264530+00:00 through 2026-10-29T00:56:34.187189+00:00; source `4751db4aa29ed56ba20820adeea040e271c60581`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 0.841 | 0.135 | 20.305 / 18.302 / 3.232 | 12.795 / 10.741 / 2.951 |
| sort-reverse-512.js | 1.364 | 0.362 | 20.277 / 18.255 / 3.299 | 12.951 / 10.873 / 3.002 |
| sort-sorted-512.js | 1.282 | 0.366 | 20.363 / 18.350 / 3.293 | 12.967 / 10.884 / 3.002 |
| to-sorted-512.js | 1.354 | 0.401 | 20.340 / 18.340 / 3.311 | 13.014 / 10.954 / 3.016 |
| join-1024.js | 1.629 | 0.519 | 20.521 / 18.496 / 3.365 | 13.020 / 10.962 / 3.025 |
| join-4096.js | 4.419 | 1.702 | 20.781 / 18.762 / 3.883 | 13.586 / 11.491 / 3.500 |
| join-mixed-1024.js | 1.928 | 0.312 | 20.322 / 18.325 / 3.348 | 12.840 / 10.765 / 2.953 |
| numeric-loop-10000.js | 0.702 | 0.090 | 19.412 / 17.413 / 3.137 | 12.195 / 10.131 / 2.293 |
| numeric-loop-50000.js | 0.945 | 0.210 | 19.354 / 17.348 / 3.135 | 12.262 / 10.218 / 2.293 |
| folded-string-4096.js | 1.258 | 0.751 | 18.730 / 16.702 / 3.170 | 12.340 / 10.264 / 2.309 |
| dynamic-string-4096.js | 2.111 | 0.270 | 19.723 / 17.725 / 3.162 | 12.244 / 10.184 / 2.291 |
| integer.js | 0.860 | 0.065 | 18.992 / 16.996 / 3.186 | 12.277 / 10.195 / 2.291 |
| objects.js | 1.517 | 0.740 | 20.223 / 18.217 / 3.342 | 13.037 / 10.961 / 3.072 |
| retained-objects.js | 23.893 | 38.453 | 25.529 / 23.507 / 8.660 | 18.779 / 16.709 / 8.820 |
| join-empty-1024.js | 1.179 | 0.273 | 18.846 / 16.824 / 3.158 | 12.834 / 10.750 / 2.873 |
| join-singleton-1024.js | 1.205 | 0.128 | 18.941 / 16.941 / 3.154 | 12.186 / 10.104 / 2.293 |
| join-small-1024.js | 1.419 | 0.190 | 20.211 / 18.196 / 3.234 | 12.170 / 10.103 / 2.297 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
