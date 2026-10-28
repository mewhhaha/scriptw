# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-28T01:23:04.458358+00:00 through 2026-10-28T01:24:35.608728+00:00; source `1721e1b6daf39924dabee7e85112adc2ebed4c9d`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.719 | 0.176 | 19.037 / 17.031 / 3.953 | 14.088 / 12.001 / 4.932 |
| sort-reverse-512.js | 5.472 | 0.391 | 21.482 / 19.458 / 6.564 | 14.053 / 11.972 / 4.982 |
| sort-sorted-512.js | 2.802 | 0.394 | 19.371 / 17.342 / 4.289 | 14.107 / 12.019 / 4.986 |
| to-sorted-512.js | 6.714 | 0.416 | 22.697 / 20.676 / 7.338 | 14.133 / 12.051 / 4.996 |
| join-1024.js | 2.910 | 0.514 | 19.234 / 17.234 / 3.867 | 14.119 / 12.031 / 5.000 |
| join-4096.js | 9.699 | 1.568 | 21.088 / 19.064 / 5.609 | 14.408 / 12.346 / 5.484 |
| join-mixed-1024.js | 3.186 | 0.347 | 19.143 / 17.114 / 3.844 | 14.037 / 11.948 / 4.936 |
| numeric-loop-10000.js | 1.730 | 0.135 | 17.555 / 15.539 / 3.234 | 13.287 / 11.216 / 4.271 |
| numeric-loop-50000.js | 5.840 | 0.266 | 17.381 / 15.407 / 3.236 | 13.414 / 11.376 / 4.270 |
| folded-string-4096.js | 2.362 | 0.762 | 17.812 / 15.812 / 3.387 | 13.367 / 11.285 / 4.291 |
| dynamic-string-4096.js | 2.816 | 0.309 | 17.836 / 15.816 / 3.572 | 13.326 / 11.259 / 4.275 |
| integer.js | 4.076 | 0.111 | 18.068 / 16.040 / 3.266 | 13.363 / 11.283 / 4.271 |
| objects.js | 2.321 | 0.721 | 18.609 / 16.632 / 3.727 | 14.273 / 12.207 / 5.049 |
| retained-objects.js | 59.870 | 35.479 | 24.377 / 22.371 / 8.422 | 19.938 / 17.849 / 10.795 |
| join-empty-1024.js | 4.137 | 0.302 | 19.945 / 17.930 / 5.127 | 13.984 / 11.903 / 4.854 |
| join-singleton-1024.js | 4.293 | 0.168 | 20.029 / 18.024 / 5.229 | 13.438 / 11.351 / 4.271 |
| join-small-1024.js | 4.523 | 0.216 | 19.951 / 17.973 / 5.244 | 13.395 / 11.321 / 4.281 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
