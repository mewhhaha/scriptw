# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T09:27:52.400889+00:00 through 2026-10-27T09:29:21.425525+00:00; source `0bd2c2ea7a4a98146d3dac5734afa0f3812de5c3`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.471 | 0.149 | 20.480 / 18.352 / 8.326 | 14.246 / 12.046 / 7.621 |
| sort-reverse-512.js | 4.477 | 0.305 | 23.184 / 21.062 / 11.018 | 14.264 / 12.093 / 7.658 |
| sort-sorted-512.js | 2.319 | 0.308 | 20.811 / 18.691 / 8.801 | 14.238 / 12.032 / 7.594 |
| to-sorted-512.js | 5.534 | 0.324 | 24.396 / 22.247 / 11.912 | 14.346 / 12.138 / 7.715 |
| join-1024.js | 2.281 | 0.403 | 21.074 / 18.934 / 8.414 | 14.404 / 12.199 / 7.656 |
| join-4096.js | 7.113 | 1.228 | 22.607 / 20.485 / 9.992 | 14.889 / 12.688 / 8.199 |
| join-mixed-1024.js | 2.469 | 0.266 | 20.678 / 18.555 / 8.324 | 14.166 / 11.960 / 7.555 |
| numeric-loop-10000.js | 1.448 | 0.111 | 18.955 / 16.812 / 7.744 | 13.498 / 11.338 / 6.947 |
| numeric-loop-50000.js | 4.713 | 0.211 | 18.939 / 16.808 / 7.750 | 13.762 / 11.565 / 5.992 |
| folded-string-4096.js | 1.881 | 0.603 | 19.312 / 17.165 / 3.832 | 13.482 / 11.273 / 4.287 |
| dynamic-string-4096.js | 2.277 | 0.246 | 19.396 / 17.255 / 4.012 | 13.508 / 11.325 / 4.275 |
| integer.js | 3.217 | 0.082 | 19.508 / 17.391 / 3.713 | 13.537 / 11.350 / 4.273 |
| objects.js | 1.823 | 0.561 | 20.539 / 18.409 / 4.170 | 14.283 / 12.123 / 5.055 |
| retained-objects.js | 52.903 | 28.678 | 26.832 / 24.682 / 9.844 | 19.982 / 17.788 / 10.799 |
| join-empty-1024.js | 3.403 | 0.237 | 21.625 / 19.478 / 5.582 | 14.014 / 11.826 / 4.852 |
| join-singleton-1024.js | 3.569 | 0.132 | 21.809 / 19.686 / 5.668 | 13.514 / 11.313 / 4.275 |
| join-small-1024.js | 3.747 | 0.181 | 21.730 / 19.582 / 5.684 | 13.547 / 11.341 / 4.281 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
