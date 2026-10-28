# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-28T02:59:10.388301+00:00 through 2026-10-28T03:00:35.218042+00:00; source `7b81ba6c9a6a30189acf75fed72f3f8a434d0992`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.457 | 0.208 | 18.924 / 16.778 / 7.963 | 14.203 / 12.007 / 4.934 |
| sort-reverse-512.js | 4.648 | 0.444 | 21.467 / 19.323 / 6.438 | 14.379 / 12.175 / 4.982 |
| sort-sorted-512.js | 2.497 | 0.438 | 19.475 / 17.334 / 4.168 | 14.264 / 12.077 / 4.979 |
| to-sorted-512.js | 5.654 | 0.487 | 22.814 / 20.715 / 7.211 | 14.264 / 12.090 / 4.992 |
| join-1024.js | 2.694 | 0.540 | 19.123 / 16.983 / 3.746 | 14.338 / 12.156 / 4.998 |
| join-4096.js | 9.325 | 1.585 | 21.119 / 18.999 / 5.484 | 14.867 / 12.659 / 5.480 |
| join-mixed-1024.js | 2.941 | 0.366 | 19.068 / 16.946 / 3.715 | 14.141 / 11.963 / 4.936 |
| numeric-loop-10000.js | 1.504 | 0.175 | 17.428 / 15.310 / 3.115 | 13.570 / 11.393 / 4.271 |
| numeric-loop-50000.js | 5.058 | 0.317 | 17.652 / 15.508 / 3.115 | 13.545 / 11.334 / 4.271 |
| folded-string-4096.js | 2.280 | 0.875 | 17.611 / 15.469 / 3.264 | 13.504 / 11.335 / 4.285 |
| dynamic-string-4096.js | 2.470 | 0.350 | 17.863 / 15.765 / 3.447 | 13.602 / 11.392 / 4.270 |
| integer.js | 3.901 | 0.142 | 17.988 / 15.850 / 3.141 | 13.646 / 11.465 / 4.268 |
| objects.js | 2.146 | 0.817 | 18.738 / 16.640 / 3.604 | 14.389 / 12.184 / 5.051 |
| retained-objects.js | 55.769 | 38.450 | 24.246 / 22.125 / 8.297 | 20.125 / 17.946 / 10.795 |
| join-empty-1024.js | 3.605 | 0.373 | 19.963 / 17.841 / 5.002 | 14.154 / 11.967 / 4.850 |
| join-singleton-1024.js | 3.720 | 0.201 | 19.924 / 17.824 / 5.104 | 13.664 / 11.487 / 4.266 |
| join-small-1024.js | 4.082 | 0.254 | 20.244 / 18.125 / 5.117 | 13.598 / 11.399 / 4.277 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
