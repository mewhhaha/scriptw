# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T12:48:52.976508+00:00 through 2026-10-27T12:50:04.043341+00:00; source `fa204ae2bdccd798b98439d4c347b576c2ee4508`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.027 | 0.128 | 20.453 / 18.354 / 8.600 | 14.238 / 12.036 / 7.609 |
| sort-reverse-512.js | 3.506 | 0.250 | 23.119 / 20.976 / 11.211 | 14.297 / 12.095 / 7.627 |
| sort-sorted-512.js | 1.736 | 0.237 | 20.881 / 18.762 / 8.945 | 14.268 / 12.058 / 7.625 |
| to-sorted-512.js | 4.235 | 0.264 | 23.990 / 21.867 / 11.846 | 14.301 / 12.098 / 7.670 |
| join-1024.js | 1.749 | 0.310 | 20.889 / 18.748 / 8.551 | 14.336 / 12.135 / 7.629 |
| join-4096.js | 5.869 | 0.906 | 22.289 / 20.151 / 10.193 | 14.746 / 12.544 / 8.156 |
| join-mixed-1024.js | 1.944 | 0.213 | 20.340 / 18.219 / 8.346 | 14.117 / 11.959 / 7.541 |
| numeric-loop-10000.js | 0.979 | 0.093 | 18.793 / 16.658 / 7.713 | 13.486 / 11.305 / 6.920 |
| numeric-loop-50000.js | 3.252 | 0.177 | 18.668 / 16.554 / 7.826 | 13.461 / 11.278 / 6.902 |
| folded-string-4096.js | 1.368 | 0.454 | 19.029 / 16.885 / 7.896 | 13.510 / 11.330 / 6.938 |
| dynamic-string-4096.js | 1.657 | 0.206 | 19.146 / 17.011 / 8.014 | 13.664 / 11.459 / 6.980 |
| integer.js | 2.319 | 0.089 | 19.066 / 16.926 / 7.680 | 13.553 / 11.348 / 6.932 |
| objects.js | 1.432 | 0.454 | 20.102 / 17.979 / 8.178 | 14.297 / 12.114 / 7.686 |
| retained-objects.js | 34.674 | 21.495 | 25.580 / 23.462 / 13.076 | 20.029 / 17.849 / 13.441 |
| join-empty-1024.js | 2.648 | 0.195 | 21.271 / 19.131 / 9.777 | 14.070 / 11.911 / 7.529 |
| join-singleton-1024.js | 2.767 | 0.128 | 21.434 / 19.295 / 9.729 | 13.592 / 11.381 / 6.947 |
| join-small-1024.js | 3.053 | 0.160 | 21.270 / 19.150 / 5.668 | 13.535 / 11.349 / 4.281 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
