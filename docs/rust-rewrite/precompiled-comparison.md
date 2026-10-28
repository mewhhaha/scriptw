# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-28T11:29:20.904511+00:00 through 2026-10-28T11:30:49.196243+00:00; source `a0d1366aeb8da2acdd992e86d884519f96b5730e`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.574 | 0.117 | 20.396 / 18.256 / 3.824 | 12.762 / 10.556 / 2.936 |
| sort-reverse-512.js | 5.168 | 0.346 | 23.781 / 21.668 / 6.439 | 12.986 / 10.798 / 2.984 |
| sort-sorted-512.js | 2.709 | 0.350 | 21.471 / 19.326 / 4.162 | 12.838 / 10.679 / 2.988 |
| to-sorted-512.js | 6.395 | 0.396 | 24.678 / 22.543 / 7.156 | 12.955 / 10.748 / 2.998 |
| join-1024.js | 3.045 | 0.444 | 21.314 / 19.169 / 3.742 | 12.977 / 10.765 / 3.006 |
| join-4096.js | 10.228 | 1.540 | 23.090 / 20.950 / 5.480 | 13.367 / 11.190 / 3.484 |
| join-mixed-1024.js | 3.256 | 0.268 | 20.516 / 18.372 / 3.707 | 12.754 / 10.550 / 2.941 |
| numeric-loop-10000.js | 1.568 | 0.086 | 19.422 / 17.278 / 3.105 | 12.113 / 9.925 / 2.275 |
| numeric-loop-50000.js | 5.201 | 0.226 | 19.615 / 17.487 / 3.107 | 12.094 / 9.877 / 2.277 |
| folded-string-4096.js | 2.354 | 0.790 | 19.965 / 17.820 / 3.260 | 12.314 / 10.110 / 2.291 |
| dynamic-string-4096.js | 2.548 | 0.261 | 19.953 / 17.827 / 3.443 | 12.115 / 9.932 / 2.275 |
| integer.js | 4.389 | 0.053 | 20.035 / 17.899 / 3.139 | 12.146 / 9.969 / 2.275 |
| objects.js | 2.292 | 0.741 | 20.885 / 18.741 / 3.543 | 12.859 / 10.657 / 3.057 |
| retained-objects.js | 60.990 | 39.810 | 24.986 / 22.844 / 7.342 | 18.639 / 16.449 / 8.809 |
| join-empty-1024.js | 3.961 | 0.277 | 21.838 / 19.686 / 4.896 | 12.768 / 10.581 / 2.854 |
| join-singleton-1024.js | 4.100 | 0.110 | 22.156 / 20.033 / 4.998 | 12.258 / 10.077 / 2.277 |
| join-small-1024.js | 4.328 | 0.166 | 21.529 / 19.393 / 5.020 | 12.221 / 10.016 / 2.283 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
