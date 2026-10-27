# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T16:48:23.165639+00:00 through 2026-10-27T16:49:50.586253+00:00; source `38c25942b63a752ce5db46b6cb42b46ed98f9a57`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.514 | 0.209 | 19.406 / 17.256 / 3.947 | 14.197 / 12.019 / 4.932 |
| sort-reverse-512.js | 4.964 | 0.436 | 22.252 / 20.117 / 6.561 | 14.312 / 12.120 / 4.984 |
| sort-sorted-512.js | 2.622 | 0.431 | 19.705 / 17.590 / 4.287 | 14.334 / 12.147 / 4.986 |
| to-sorted-512.js | 6.077 | 0.473 | 23.064 / 20.947 / 7.332 | 14.496 / 12.292 / 4.998 |
| join-1024.js | 2.808 | 0.534 | 19.555 / 17.446 / 3.865 | 14.260 / 12.076 / 5.004 |
| join-4096.js | 9.612 | 1.607 | 21.527 / 19.381 / 5.607 | 14.830 / 12.622 / 5.480 |
| join-mixed-1024.js | 3.083 | 0.353 | 19.414 / 17.300 / 3.842 | 14.209 / 12.030 / 4.934 |
| numeric-loop-10000.js | 1.526 | 0.171 | 17.879 / 15.723 / 3.230 | 13.607 / 11.399 / 4.270 |
| numeric-loop-50000.js | 5.129 | 0.313 | 17.869 / 15.755 / 3.232 | 13.604 / 11.422 / 4.271 |
| folded-string-4096.js | 2.322 | 0.878 | 18.098 / 15.950 / 3.391 | 13.627 / 11.425 / 4.287 |
| dynamic-string-4096.js | 2.585 | 0.352 | 18.350 / 16.198 / 3.578 | 13.555 / 11.345 / 4.271 |
| integer.js | 4.035 | 0.138 | 18.197 / 16.098 / 3.270 | 13.623 / 11.417 / 4.275 |
| objects.js | 2.221 | 0.815 | 19.373 / 17.217 / 3.723 | 14.523 / 12.339 / 5.053 |
| retained-objects.js | 56.840 | 39.170 | 24.598 / 22.474 / 8.422 | 20.082 / 17.900 / 10.797 |
| join-empty-1024.js | 3.739 | 0.363 | 20.178 / 18.032 / 5.123 | 14.352 / 12.139 / 4.852 |
| join-singleton-1024.js | 3.843 | 0.197 | 20.771 / 18.627 / 5.225 | 13.617 / 11.427 / 4.271 |
| join-small-1024.js | 4.164 | 0.250 | 20.449 / 18.326 / 5.238 | 13.580 / 11.379 / 4.277 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
