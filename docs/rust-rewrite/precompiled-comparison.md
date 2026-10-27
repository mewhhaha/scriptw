# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T02:54:39.828964+00:00 through 2026-10-27T02:55:56.221437+00:00; source `9b423b63cb320f54f80d658da28365fd20eb34ae`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 2.074 | 0.115 | 22.135 / 19.983 / 9.840 | 14.291 / 12.092 / 7.623 |
| sort-reverse-512.js | 7.320 | 0.241 | 27.221 / 25.072 / 15.068 | 14.260 / 12.047 / 7.629 |
| sort-sorted-512.js | 5.564 | 0.264 | 25.100 / 22.976 / 12.910 | 14.090 / 11.886 / 7.518 |
| to-sorted-512.js | 8.053 | 0.259 | 28.043 / 25.896 / 15.844 | 14.232 / 12.062 / 7.639 |
| join-1024.js | 9.429 | 0.317 | 28.283 / 26.142 / 15.887 | 14.248 / 12.067 / 7.646 |
| join-4096.js | 40.158 | 0.954 | 22.363 / 20.220 / 8.977 | 14.705 / 12.516 / 8.125 |
| join-mixed-1024.js | 9.569 | 0.229 | 27.752 / 25.616 / 15.686 | 14.145 / 11.957 / 7.467 |
| numeric-loop-10000.js | 1.053 | 0.113 | 19.281 / 17.126 / 8.346 | 13.502 / 11.296 / 6.961 |
| numeric-loop-50000.js | 3.395 | 0.199 | 19.471 / 17.344 / 8.469 | 13.498 / 11.312 / 6.918 |
| folded-string-4096.js | 1.403 | 0.446 | 19.678 / 17.554 / 8.527 | 13.539 / 11.355 / 6.934 |
| dynamic-string-4096.js | 1.708 | 0.202 | 19.641 / 17.517 / 8.703 | 13.545 / 11.338 / 6.920 |
| integer.js | 2.308 | 0.070 | 19.828 / 17.686 / 8.348 | 13.453 / 11.271 / 6.920 |
| objects.js | 5.130 | 0.451 | 24.387 / 22.262 / 12.146 | 14.127 / 11.939 / 7.584 |
| retained-objects.js | 126.010 | 22.232 | 27.348 / 25.224 / 14.018 | 20.018 / 17.834 / 13.451 |
| join-empty-1024.js | 3.300 | 0.209 | 22.338 / 20.190 / 6.811 | 14.125 / 11.943 / 4.848 |
| join-singleton-1024.js | 3.416 | 0.128 | 22.451 / 20.333 / 6.891 | 13.531 / 11.346 / 4.279 |
| join-small-1024.js | 3.512 | 0.158 | 22.373 / 20.229 / 6.920 | 13.598 / 11.389 / 4.281 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
