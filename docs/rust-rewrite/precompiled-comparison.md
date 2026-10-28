# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T22:45:59.495549+00:00 through 2026-10-27T22:47:25.521492+00:00; source `c79ab683848b6baf32adc528d144ea88a02a0fa0`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.558 | 0.194 | 19.012 / 17.016 / 3.953 | 14.061 / 12.000 / 4.938 |
| sort-reverse-512.js | 5.010 | 0.422 | 21.959 / 19.942 / 6.564 | 14.168 / 12.078 / 4.986 |
| sort-sorted-512.js | 2.602 | 0.415 | 19.482 / 17.481 / 4.291 | 14.072 / 11.982 / 4.988 |
| to-sorted-512.js | 6.112 | 0.494 | 22.986 / 20.991 / 7.334 | 13.908 / 11.870 / 5.002 |
| join-1024.js | 2.714 | 0.572 | 19.396 / 17.398 / 3.867 | 14.180 / 12.097 / 5.006 |
| join-4096.js | 8.846 | 1.761 | 21.145 / 19.114 / 5.611 | 14.697 / 12.609 / 5.484 |
| join-mixed-1024.js | 2.965 | 0.374 | 19.318 / 17.342 / 3.846 | 13.900 / 11.814 / 4.939 |
| numeric-loop-10000.js | 1.546 | 0.150 | 17.701 / 15.680 / 3.234 | 13.486 / 11.392 / 4.277 |
| numeric-loop-50000.js | 5.167 | 0.270 | 17.613 / 15.610 / 3.238 | 13.367 / 11.330 / 4.275 |
| folded-string-4096.js | 2.287 | 0.802 | 17.984 / 15.984 / 3.389 | 13.256 / 11.173 / 4.291 |
| dynamic-string-4096.js | 2.608 | 0.338 | 18.197 / 16.175 / 3.572 | 13.418 / 11.340 / 4.277 |
| integer.js | 3.941 | 0.127 | 17.928 / 15.904 / 3.273 | 13.512 / 11.431 / 4.279 |
| objects.js | 2.170 | 0.778 | 19.029 / 17.013 / 3.727 | 14.227 / 12.131 / 5.057 |
| retained-objects.js | 55.264 | 38.408 | 24.539 / 22.521 / 8.422 | 19.988 / 17.905 / 10.801 |
| join-empty-1024.js | 3.712 | 0.337 | 20.340 / 18.343 / 5.125 | 14.000 / 11.933 / 4.854 |
| join-singleton-1024.js | 3.890 | 0.185 | 20.389 / 18.361 / 5.229 | 13.393 / 11.334 / 4.277 |
| join-small-1024.js | 4.118 | 0.253 | 20.244 / 18.244 / 5.244 | 13.438 / 11.380 / 4.285 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
