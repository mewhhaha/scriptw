# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-26T21:27:45.093605+00:00 through 2026-10-26T21:29:03.383924+00:00; source `b1ffc3ffd9f62d990c6f88058c4684018ba3a776`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 2.304 | 0.131 | 28.549 / 26.421 / 13.982 | 13.975 / 11.771 / 7.537 |
| sort-reverse-512.js | 8.089 | 0.250 | 37.375 / 35.234 / 22.031 | 14.084 / 11.872 / 7.637 |
| sort-sorted-512.js | 7.317 | 0.249 | 35.312 / 33.200 / 19.920 | 14.061 / 11.864 / 7.596 |
| to-sorted-512.js | 6.711 | 0.280 | 35.418 / 33.299 / 19.996 | 14.047 / 11.843 / 7.592 |
| join-1024.js | 7.796 | 0.322 | 37.240 / 35.092 / 21.844 | 14.051 / 11.863 / 7.596 |
| join-4096.js | 27.090 | 0.927 | 41.496 / 39.350 / 25.934 | 14.701 / 12.495 / 8.109 |
| join-mixed-1024.js | 7.929 | 0.220 | 37.311 / 35.166 / 21.891 | 14.057 / 11.847 / 7.584 |
| numeric-loop-10000.js | 1.086 | 0.102 | 25.598 / 23.453 / 11.838 | 13.387 / 11.183 / 6.863 |
| numeric-loop-50000.js | 3.442 | 0.198 | 25.445 / 23.346 / 11.793 | 13.428 / 11.219 / 6.916 |
| folded-string-4096.js | 1.388 | 0.443 | 25.656 / 23.511 / 7.191 | 13.424 / 11.230 / 6.939 |
| dynamic-string-4096.js | 1.682 | 0.207 | 25.770 / 23.628 / 7.184 | 13.396 / 11.190 / 4.268 |
| integer.js | 2.230 | 0.090 | 25.934 / 23.787 / 7.191 | 13.387 / 11.183 / 4.264 |
| objects.js | 4.202 | 0.437 | 30.492 / 28.346 / 11.225 | 14.236 / 12.025 / 5.053 |
| retained-objects.js | 116.345 | 21.745 | 41.391 / 39.275 / 21.248 | 19.918 / 17.713 / 10.791 |
| join-empty-1024.js | 2.482 | 0.197 | 27.980 / 25.836 / 9.178 | 13.969 / 11.809 / 4.846 |
| join-singleton-1024.js | 2.533 | 0.122 | 27.943 / 25.806 / 9.178 | 13.361 / 11.179 / 4.271 |
| join-small-1024.js | 3.232 | 0.155 | 27.986 / 25.863 / 9.178 | 13.355 / 11.152 / 4.273 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
