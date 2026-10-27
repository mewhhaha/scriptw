# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T18:02:54.237137+00:00 through 2026-10-27T18:04:15.888614+00:00; source `a960f50acea8bd3a1de0a2c391bbb09b88f2ee4d`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.298 | 0.136 | 19.525 / 17.378 / 8.143 | 14.344 / 12.137 / 7.613 |
| sort-reverse-512.js | 4.143 | 0.298 | 22.076 / 19.926 / 10.648 | 14.271 / 12.060 / 7.678 |
| sort-sorted-512.js | 2.153 | 0.297 | 19.607 / 17.462 / 8.438 | 14.289 / 12.098 / 7.725 |
| to-sorted-512.js | 5.181 | 0.323 | 23.223 / 21.102 / 11.383 | 14.230 / 12.019 / 7.676 |
| join-1024.js | 2.244 | 0.397 | 19.689 / 17.544 / 7.955 | 14.383 / 12.185 / 7.699 |
| join-4096.js | 7.298 | 1.221 | 21.365 / 19.244 / 9.762 | 14.834 / 12.646 / 8.094 |
| join-mixed-1024.js | 2.425 | 0.262 | 19.535 / 17.394 / 8.014 | 14.254 / 12.038 / 7.553 |
| numeric-loop-10000.js | 1.333 | 0.115 | 17.869 / 15.750 / 7.311 | 13.596 / 11.397 / 7.035 |
| numeric-loop-50000.js | 4.507 | 0.204 | 17.893 / 15.779 / 7.402 | 13.688 / 11.481 / 6.953 |
| folded-string-4096.js | 1.836 | 0.593 | 18.277 / 16.127 / 7.578 | 13.656 / 11.474 / 6.990 |
| dynamic-string-4096.js | 2.198 | 0.240 | 18.508 / 16.373 / 7.664 | 13.576 / 11.375 / 4.621 |
| integer.js | 3.123 | 0.080 | 18.314 / 16.215 / 3.270 | 13.658 / 11.454 / 4.275 |
| objects.js | 1.789 | 0.557 | 18.992 / 16.836 / 3.725 | 14.430 / 12.229 / 5.057 |
| retained-objects.js | 45.954 | 27.822 | 24.713 / 22.590 / 8.418 | 20.131 / 17.915 / 10.797 |
| join-empty-1024.js | 3.112 | 0.232 | 20.170 / 18.025 / 5.123 | 14.020 / 11.816 / 4.846 |
| join-singleton-1024.js | 3.290 | 0.137 | 20.607 / 18.460 / 5.223 | 13.551 / 11.373 / 4.275 |
| join-small-1024.js | 3.473 | 0.167 | 20.623 / 18.480 / 5.238 | 13.678 / 11.490 / 4.277 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
