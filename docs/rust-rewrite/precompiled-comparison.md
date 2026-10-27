# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T04:44:32.761652+00:00 through 2026-10-27T04:46:04.141801+00:00; source `a9ad49c8e73191c4e309240811b7c30d88d15650`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.719 | 0.143 | 21.734 / 19.606 / 9.266 | 14.211 / 12.003 / 7.617 |
| sort-reverse-512.js | 6.411 | 0.300 | 25.021 / 22.883 / 12.707 | 14.201 / 12.013 / 7.617 |
| sort-sorted-512.js | 3.894 | 0.301 | 22.814 / 20.670 / 10.465 | 14.305 / 12.102 / 7.613 |
| to-sorted-512.js | 7.194 | 0.318 | 26.008 / 23.888 / 13.484 | 14.156 / 11.982 / 7.596 |
| join-1024.js | 5.600 | 0.410 | 24.002 / 21.854 / 11.186 | 14.287 / 12.070 / 7.660 |
| join-4096.js | 22.571 | 1.232 | 26.260 / 24.114 / 12.375 | 14.846 / 12.641 / 8.139 |
| join-mixed-1024.js | 5.898 | 0.274 | 23.729 / 21.600 / 11.115 | 14.115 / 11.927 / 7.592 |
| numeric-loop-10000.js | 1.352 | 0.106 | 19.732 / 17.582 / 4.436 | 13.516 / 11.306 / 6.902 |
| numeric-loop-50000.js | 4.601 | 0.205 | 19.520 / 17.401 / 4.441 | 13.477 / 11.281 / 4.273 |
| folded-string-4096.js | 1.863 | 0.589 | 19.900 / 17.759 / 4.592 | 13.439 / 11.280 / 4.287 |
| dynamic-string-4096.js | 2.181 | 0.249 | 20.193 / 18.071 / 4.771 | 13.523 / 11.311 / 4.271 |
| integer.js | 3.128 | 0.085 | 19.928 / 17.772 / 4.408 | 13.500 / 11.297 / 4.273 |
| objects.js | 3.463 | 0.553 | 22.258 / 20.136 / 5.961 | 14.287 / 12.081 / 5.055 |
| retained-objects.js | 91.473 | 28.756 | 27.875 / 25.733 / 10.521 | 20.004 / 17.794 / 10.797 |
| join-empty-1024.js | 3.219 | 0.232 | 22.277 / 20.131 / 6.324 | 13.980 / 11.806 / 4.852 |
| join-singleton-1024.js | 3.483 | 0.133 | 22.404 / 20.280 / 6.406 | 13.555 / 11.341 / 4.271 |
| join-small-1024.js | 3.520 | 0.170 | 22.537 / 20.397 / 6.436 | 13.514 / 11.316 / 4.283 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
