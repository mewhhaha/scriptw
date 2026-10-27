# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T00:10:39.117952+00:00 through 2026-10-27T00:12:05.122301+00:00; source `7e02a8707b26468868ec939ade9cc10e12cfc3e3`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.632 | 0.145 | 25.184 / 23.042 / 11.615 | 14.057 / 11.863 / 7.547 |
| sort-reverse-512.js | 5.709 | 0.299 | 31.102 / 28.954 / 17.629 | 14.143 / 11.938 / 7.627 |
| sort-sorted-512.js | 4.623 | 0.299 | 29.139 / 27.014 / 15.695 | 14.111 / 11.919 / 7.627 |
| to-sorted-512.js | 6.338 | 0.316 | 31.264 / 29.107 / 17.525 | 14.240 / 12.035 / 7.688 |
| join-1024.js | 7.647 | 0.397 | 31.246 / 29.112 / 17.572 | 14.240 / 12.040 / 7.695 |
| join-4096.js | 31.530 | 1.218 | 26.229 / 24.073 / 11.494 | 14.609 / 12.448 / 8.117 |
| join-mixed-1024.js | 7.877 | 0.262 | 30.748 / 28.624 / 13.164 | 14.189 / 11.979 / 4.939 |
| numeric-loop-10000.js | 1.155 | 0.104 | 23.887 / 21.740 / 7.170 | 13.475 / 11.309 / 4.275 |
| numeric-loop-50000.js | 4.355 | 0.209 | 23.857 / 21.720 / 7.164 | 13.486 / 11.280 / 4.275 |
| folded-string-4096.js | 1.600 | 0.589 | 23.885 / 21.736 / 7.189 | 13.484 / 11.310 / 4.289 |
| dynamic-string-4096.js | 1.878 | 0.249 | 23.840 / 21.722 / 7.186 | 13.424 / 11.220 / 4.270 |
| integer.js | 2.928 | 0.081 | 24.480 / 22.329 / 7.193 | 13.465 / 11.268 / 4.270 |
| objects.js | 4.314 | 0.562 | 28.924 / 26.806 / 11.238 | 14.209 / 11.999 / 5.059 |
| retained-objects.js | 103.867 | 27.713 | 31.891 / 29.739 / 13.211 | 19.977 / 17.776 / 10.795 |
| join-empty-1024.js | 2.696 | 0.231 | 26.422 / 24.279 / 9.131 | 13.977 / 11.769 / 4.854 |
| join-singleton-1024.js | 2.778 | 0.130 | 26.340 / 24.200 / 9.129 | 13.410 / 11.211 / 4.270 |
| join-small-1024.js | 2.988 | 0.165 | 26.242 / 24.091 / 9.131 | 13.402 / 11.222 / 4.277 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
