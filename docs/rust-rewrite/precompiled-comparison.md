# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T11:07:18.477617+00:00 through 2026-10-27T11:08:45.707170+00:00; source `f3df68beb76f18506f070fe54b95e80fe2db22be`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.562 | 0.180 | 20.342 / 18.350 / 4.412 | 14.135 / 12.050 / 4.939 |
| sort-reverse-512.js | 5.099 | 0.412 | 23.117 / 21.089 / 7.018 | 14.254 / 12.167 / 4.984 |
| sort-sorted-512.js | 2.620 | 0.402 | 20.682 / 18.656 / 4.742 | 14.127 / 12.058 / 4.986 |
| to-sorted-512.js | 6.182 | 0.463 | 24.227 / 22.205 / 7.791 | 14.109 / 12.024 / 4.998 |
| join-1024.js | 2.792 | 0.557 | 20.926 / 18.926 / 4.324 | 14.162 / 12.076 / 5.004 |
| join-4096.js | 9.137 | 1.730 | 22.561 / 20.540 / 6.062 | 14.660 / 12.594 / 5.486 |
| join-mixed-1024.js | 2.986 | 0.358 | 20.418 / 18.398 / 4.297 | 14.084 / 11.998 / 4.938 |
| numeric-loop-10000.js | 1.559 | 0.133 | 18.822 / 16.824 / 3.695 | 13.447 / 11.353 / 4.279 |
| numeric-loop-50000.js | 5.183 | 0.257 | 18.797 / 16.763 / 3.695 | 13.414 / 11.354 / 4.275 |
| folded-string-4096.js | 2.291 | 0.789 | 19.158 / 17.182 / 3.838 | 13.346 / 11.279 / 4.293 |
| dynamic-string-4096.js | 2.625 | 0.325 | 19.266 / 17.245 / 4.016 | 13.381 / 11.322 / 4.277 |
| integer.js | 4.010 | 0.109 | 19.314 / 17.315 / 3.715 | 13.379 / 11.319 / 4.275 |
| objects.js | 2.189 | 0.775 | 20.246 / 18.249 / 4.186 | 14.215 / 12.143 / 5.053 |
| retained-objects.js | 56.292 | 39.177 | 25.520 / 23.514 / 8.887 | 19.930 / 17.873 / 10.801 |
| join-empty-1024.js | 3.861 | 0.318 | 21.344 / 19.342 / 5.586 | 13.977 / 11.915 / 4.854 |
| join-singleton-1024.js | 3.993 | 0.174 | 21.512 / 19.494 / 5.676 | 13.326 / 11.236 / 4.275 |
| join-small-1024.js | 4.269 | 0.234 | 21.518 / 19.488 / 5.691 | 13.449 / 11.392 / 4.283 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
