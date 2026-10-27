# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T14:37:40.619680+00:00 through 2026-10-27T14:39:01.421667+00:00; source `e7e51222101533e421ffae41bb0ccad78ba638ba`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.293 | 0.137 | 19.141 / 16.994 / 7.975 | 14.197 / 12.009 / 7.568 |
| sort-reverse-512.js | 4.111 | 0.296 | 22.049 / 19.912 / 10.588 | 14.238 / 12.033 / 7.619 |
| sort-sorted-512.js | 2.143 | 0.304 | 19.811 / 17.672 / 8.439 | 14.305 / 12.097 / 7.668 |
| to-sorted-512.js | 5.064 | 0.318 | 22.934 / 20.813 / 11.369 | 14.221 / 12.040 / 7.648 |
| join-1024.js | 2.219 | 0.394 | 19.799 / 17.656 / 8.004 | 14.328 / 12.121 / 7.650 |
| join-4096.js | 7.220 | 1.207 | 21.197 / 19.083 / 9.641 | 14.688 / 12.501 / 8.123 |
| join-mixed-1024.js | 2.460 | 0.263 | 19.551 / 17.415 / 7.994 | 14.357 / 12.169 / 7.682 |
| numeric-loop-10000.js | 1.332 | 0.105 | 17.959 / 15.803 / 3.234 | 13.660 / 11.448 / 4.277 |
| numeric-loop-50000.js | 4.513 | 0.205 | 17.855 / 15.712 / 3.234 | 13.568 / 11.409 / 4.275 |
| folded-string-4096.js | 1.823 | 0.587 | 18.426 / 16.305 / 3.396 | 13.465 / 11.284 / 4.293 |
| dynamic-string-4096.js | 2.181 | 0.247 | 18.424 / 16.273 / 3.582 | 13.562 / 11.381 / 4.277 |
| integer.js | 3.107 | 0.081 | 18.336 / 16.236 / 3.271 | 13.691 / 11.487 / 4.277 |
| objects.js | 1.786 | 0.555 | 18.943 / 16.824 / 3.727 | 14.322 / 12.139 / 5.055 |
| retained-objects.js | 44.936 | 27.205 | 24.879 / 22.736 / 8.422 | 20.107 / 17.907 / 10.801 |
| join-empty-1024.js | 3.112 | 0.237 | 20.162 / 18.039 / 5.127 | 14.303 / 12.091 / 4.855 |
| join-singleton-1024.js | 3.275 | 0.130 | 20.545 / 18.445 / 5.229 | 13.576 / 11.378 / 4.277 |
| join-small-1024.js | 3.445 | 0.177 | 20.656 / 18.557 / 5.246 | 13.521 / 11.336 / 4.285 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
