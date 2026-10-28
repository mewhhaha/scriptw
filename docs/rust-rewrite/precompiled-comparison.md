# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-28T20:40:55.259882+00:00 through 2026-10-28T20:42:22.140199+00:00; source `f02efa8c5458ee62c3158b5d1aac2b5b59c1b560`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.481 | 0.118 | 20.889 / 18.744 / 3.830 | 12.873 / 10.662 / 2.943 |
| sort-reverse-512.js | 4.844 | 0.351 | 22.992 / 20.857 / 6.416 | 13.029 / 10.819 / 3.000 |
| sort-sorted-512.js | 2.570 | 0.345 | 21.533 / 19.384 / 4.146 | 13.039 / 10.851 / 2.994 |
| to-sorted-512.js | 5.938 | 0.399 | 24.875 / 22.734 / 7.139 | 13.049 / 10.868 / 3.010 |
| join-1024.js | 2.869 | 0.442 | 21.283 / 19.137 / 3.754 | 13.029 / 10.850 / 3.018 |
| join-4096.js | 9.963 | 1.521 | 23.020 / 20.871 / 5.490 | 13.488 / 11.271 / 3.490 |
| join-mixed-1024.js | 3.101 | 0.267 | 21.393 / 19.255 / 3.715 | 12.809 / 10.598 / 2.947 |
| numeric-loop-10000.js | 1.511 | 0.082 | 18.947 / 16.803 / 3.117 | 12.328 / 10.125 / 2.283 |
| numeric-loop-50000.js | 5.080 | 0.227 | 19.426 / 17.282 / 3.115 | 12.318 / 10.130 / 2.285 |
| folded-string-4096.js | 2.284 | 0.786 | 19.764 / 17.622 / 3.268 | 12.291 / 10.107 / 2.301 |
| dynamic-string-4096.js | 2.491 | 0.265 | 19.912 / 17.766 / 3.451 | 12.344 / 10.141 / 2.289 |
| integer.js | 4.333 | 0.050 | 20.068 / 17.927 / 3.146 | 12.303 / 10.107 / 2.287 |
| objects.js | 2.053 | 0.725 | 20.727 / 18.603 / 3.500 | 13.193 / 10.989 / 3.066 |
| retained-objects.js | 40.881 | 38.993 | 25.096 / 22.945 / 7.551 | 18.883 / 16.695 / 8.816 |
| join-empty-1024.js | 3.762 | 0.276 | 22.004 / 19.881 / 4.904 | 12.828 / 10.668 / 2.863 |
| join-singleton-1024.js | 3.972 | 0.107 | 21.979 / 19.829 / 5.006 | 12.311 / 10.108 / 2.287 |
| join-small-1024.js | 4.185 | 0.163 | 22.164 / 20.021 / 5.023 | 12.152 / 9.969 / 2.291 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
