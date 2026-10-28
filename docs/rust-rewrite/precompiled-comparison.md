# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-28T08:54:47.357528+00:00 through 2026-10-28T08:56:14.601961+00:00; source `68dae6ed898a5d31aed30f713df2c7b5add4c1e0`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.597 | 0.135 | 20.314 / 18.300 / 3.826 | 12.711 / 10.633 / 2.941 |
| sort-reverse-512.js | 5.125 | 0.363 | 22.975 / 20.954 / 6.439 | 12.760 / 10.673 / 2.994 |
| sort-sorted-512.js | 2.680 | 0.369 | 21.166 / 19.165 / 4.160 | 12.717 / 10.635 / 2.986 |
| to-sorted-512.js | 6.286 | 0.398 | 24.643 / 22.642 / 7.154 | 12.619 / 10.542 / 3.002 |
| join-1024.js | 2.804 | 0.510 | 21.396 / 19.399 / 3.738 | 12.744 / 10.657 / 3.006 |
| join-4096.js | 9.376 | 1.710 | 23.104 / 21.098 / 5.482 | 13.170 / 11.114 / 3.486 |
| join-mixed-1024.js | 3.037 | 0.312 | 20.984 / 18.962 / 3.707 | 12.646 / 10.568 / 2.938 |
| numeric-loop-10000.js | 1.588 | 0.090 | 19.320 / 17.315 / 3.105 | 12.057 / 10.019 / 2.277 |
| numeric-loop-50000.js | 5.388 | 0.205 | 19.672 / 17.658 / 3.105 | 12.080 / 10.030 / 2.275 |
| folded-string-4096.js | 2.312 | 0.735 | 19.781 / 17.760 / 3.258 | 12.004 / 9.923 / 2.295 |
| dynamic-string-4096.js | 2.622 | 0.270 | 19.785 / 17.793 / 3.443 | 11.986 / 9.925 / 2.279 |
| integer.js | 4.336 | 0.063 | 19.971 / 17.952 / 3.137 | 12.051 / 9.968 / 2.277 |
| objects.js | 2.203 | 0.733 | 20.748 / 18.733 / 3.545 | 12.891 / 10.805 / 3.057 |
| retained-objects.js | 56.380 | 38.453 | 25.016 / 23.001 / 7.340 | 18.523 / 16.464 / 8.803 |
| join-empty-1024.js | 3.818 | 0.274 | 21.793 / 19.815 / 4.896 | 12.566 / 10.488 / 2.855 |
| join-singleton-1024.js | 4.021 | 0.129 | 22.023 / 20.028 / 5.000 | 11.922 / 9.884 / 2.279 |
| join-small-1024.js | 4.216 | 0.193 | 21.971 / 19.970 / 5.016 | 11.992 / 9.913 / 2.289 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
