# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T20:44:48.191351+00:00 through 2026-10-27T20:45:53.290221+00:00; source `80b842693e964634f378d5813f6db684ee7d255b`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.007 | 0.125 | 19.428 / 17.332 / 8.076 | 14.336 / 12.135 / 7.621 |
| sort-reverse-512.js | 3.217 | 0.235 | 21.789 / 19.640 / 10.588 | 14.186 / 11.983 / 7.609 |
| sort-sorted-512.js | 1.654 | 0.224 | 19.979 / 17.830 / 8.428 | 14.332 / 12.125 / 7.635 |
| to-sorted-512.js | 4.044 | 0.239 | 22.955 / 20.842 / 11.297 | 14.426 / 12.226 / 7.717 |
| join-1024.js | 1.724 | 0.298 | 19.736 / 17.589 / 8.053 | 14.211 / 12.030 / 7.625 |
| join-4096.js | 5.649 | 0.880 | 21.420 / 19.287 / 9.771 | 14.818 / 12.617 / 8.219 |
| join-mixed-1024.js | 1.880 | 0.214 | 19.598 / 17.453 / 7.988 | 14.211 / 12.032 / 7.625 |
| numeric-loop-10000.js | 0.955 | 0.095 | 18.021 / 15.872 / 7.494 | 13.684 / 11.500 / 7.041 |
| numeric-loop-50000.js | 3.147 | 0.176 | 17.930 / 15.780 / 7.330 | 13.641 / 11.438 / 6.963 |
| folded-string-4096.js | 1.287 | 0.430 | 18.172 / 16.032 / 7.518 | 13.473 / 11.296 / 6.938 |
| dynamic-string-4096.js | 1.558 | 0.183 | 18.520 / 16.392 / 7.732 | 13.635 / 11.431 / 6.992 |
| integer.js | 2.093 | 0.065 | 18.400 / 16.257 / 7.430 | 13.645 / 11.438 / 6.994 |
| objects.js | 1.344 | 0.408 | 19.076 / 16.933 / 7.693 | 14.451 / 12.255 / 7.740 |
| retained-objects.js | 31.804 | 20.370 | 24.859 / 22.740 / 16.865 | 20.232 / 18.050 / 13.533 |
| join-empty-1024.js | 2.480 | 0.186 | 20.408 / 18.266 / 13.404 | 14.314 / 12.117 / 10.314 |
| join-singleton-1024.js | 2.587 | 0.119 | 20.664 / 18.522 / 13.658 | 13.635 / 11.440 / 9.654 |
| join-small-1024.js | 2.693 | 0.123 | 20.426 / 18.314 / 13.498 | 13.582 / 11.398 / 9.645 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
