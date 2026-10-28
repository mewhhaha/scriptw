# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-28T04:46:21.341613+00:00 through 2026-10-28T04:47:50.567190+00:00; source `7f9bacff03c454dd505a0047c9439bee253028db`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.519 | 0.116 | 21.344 / 19.222 / 3.828 | 12.865 / 10.667 / 2.932 |
| sort-reverse-512.js | 5.066 | 0.346 | 23.818 / 21.680 / 6.436 | 12.932 / 10.728 / 2.986 |
| sort-sorted-512.js | 2.644 | 0.347 | 21.301 / 19.158 / 4.164 | 12.838 / 10.662 / 2.984 |
| to-sorted-512.js | 6.275 | 0.393 | 25.162 / 23.019 / 7.209 | 12.713 / 10.554 / 2.992 |
| join-1024.js | 2.860 | 0.454 | 21.566 / 19.416 / 3.752 | 12.936 / 10.732 / 3.002 |
| join-4096.js | 9.919 | 1.528 | 23.375 / 21.248 / 5.488 | 13.393 / 11.188 / 3.479 |
| join-mixed-1024.js | 3.101 | 0.268 | 21.361 / 19.247 / 3.711 | 12.430 / 10.224 / 2.934 |
| numeric-loop-10000.js | 1.522 | 0.082 | 20.062 / 17.941 / 3.107 | 12.100 / 9.910 / 2.273 |
| numeric-loop-50000.js | 5.150 | 0.228 | 19.852 / 17.734 / 3.109 | 12.115 / 9.904 / 2.277 |
| folded-string-4096.js | 2.338 | 0.791 | 19.264 / 17.140 / 3.258 | 12.279 / 10.068 / 2.291 |
| dynamic-string-4096.js | 2.593 | 0.261 | 20.494 / 18.339 / 3.445 | 12.354 / 10.143 / 2.273 |
| integer.js | 4.110 | 0.051 | 20.125 / 17.998 / 3.137 | 12.182 / 9.972 / 2.271 |
| objects.js | 2.235 | 0.731 | 21.006 / 18.874 / 3.598 | 12.908 / 10.728 / 3.059 |
| retained-objects.js | 58.607 | 39.543 | 26.561 / 24.411 / 8.293 | 18.680 / 16.481 / 8.801 |
| join-empty-1024.js | 3.843 | 0.276 | 22.020 / 19.877 / 4.992 | 12.332 / 10.152 / 2.852 |
| join-singleton-1024.js | 4.017 | 0.112 | 22.350 / 20.205 / 5.096 | 11.936 / 9.755 / 2.273 |
| join-small-1024.js | 4.230 | 0.161 | 22.459 / 20.314 / 5.113 | 12.113 / 9.932 / 2.283 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
