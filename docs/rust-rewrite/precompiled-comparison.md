# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-26T19:22:16.460898+00:00 through 2026-10-26T19:23:54.518873+00:00; source `fc714e607c5bce6c017aa06d49dba2e3de7927d2`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 3.151 | 0.200 | 61.113 / 59.107 / 9.215 | 13.912 / 11.828 / 4.924 |
| sort-reverse-512.js | 11.619 | 0.423 | 76.961 / 74.954 / 19.221 | 13.941 / 11.880 / 4.979 |
| sort-sorted-512.js | 10.222 | 0.401 | 73.123 / 71.099 / 15.234 | 13.928 / 11.863 / 4.973 |
| to-sorted-512.js | 9.636 | 0.451 | 75.092 / 73.065 / 17.238 | 13.975 / 11.898 / 4.984 |
| join-1024.js | 11.463 | 0.556 | 77.129 / 75.114 / 17.223 | 14.082 / 11.997 / 4.996 |
| join-4096.js | 41.646 | 1.734 | 81.297 / 79.295 / 21.221 | 14.504 / 12.438 / 5.473 |
| join-mixed-1024.js | 12.056 | 0.354 | 74.975 / 72.977 / 17.215 | 13.924 / 11.862 / 4.926 |
| numeric-loop-10000.js | 1.671 | 0.161 | 54.674 / 52.652 / 7.162 | 13.307 / 11.230 / 4.266 |
| numeric-loop-50000.js | 5.326 | 0.267 | 54.668 / 52.656 / 7.164 | 13.127 / 11.074 / 4.266 |
| folded-string-4096.js | 2.313 | 0.798 | 54.768 / 52.744 / 7.188 | 13.332 / 11.276 / 4.285 |
| dynamic-string-4096.js | 2.513 | 0.313 | 55.072 / 53.046 / 7.180 | 13.191 / 11.103 / 4.270 |
| integer.js | 3.709 | 0.119 | 57.084 / 55.083 / 7.188 | 13.297 / 11.208 / 4.270 |
| objects.js | 6.042 | 0.774 | 65.051 / 63.022 / 11.219 | 14.006 / 11.939 / 5.045 |
| retained-objects.js | 199.349 | 38.320 | 81.152 / 79.148 / 21.242 | 19.771 / 17.683 / 10.789 |
| join-empty-1024.js | 3.708 | 0.330 | 59.043 / 57.032 / 9.176 | 13.867 / 11.787 / 4.842 |
| join-singleton-1024.js | 3.729 | 0.177 | 58.998 / 56.980 / 9.174 | 13.359 / 11.298 / 4.268 |
| join-small-1024.js | 5.224 | 0.328 | 58.838 / 56.820 / 9.170 | 13.299 / 11.238 / 4.271 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
