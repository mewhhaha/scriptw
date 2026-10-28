# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-28T06:16:59.431349+00:00 through 2026-10-28T06:18:07.204472+00:00; source `a19ba7395edfa93327ec01fbe53765ce50020c82`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.074 | 0.134 | 19.039 / 16.899 / 7.977 | 14.293 / 12.133 / 7.658 |
| sort-reverse-512.js | 3.460 | 0.252 | 21.533 / 19.386 / 10.480 | 14.336 / 12.132 / 7.715 |
| sort-sorted-512.js | 1.701 | 0.253 | 19.291 / 17.145 / 8.150 | 14.479 / 12.262 / 7.775 |
| to-sorted-512.js | 4.201 | 0.253 | 22.631 / 20.483 / 11.311 | 14.459 / 12.252 / 7.699 |
| join-1024.js | 1.820 | 0.320 | 19.498 / 17.353 / 8.035 | 14.434 / 12.223 / 7.797 |
| join-4096.js | 5.959 | 0.924 | 20.928 / 18.782 / 9.639 | 14.787 / 12.602 / 8.152 |
| join-mixed-1024.js | 1.939 | 0.189 | 18.961 / 16.814 / 7.699 | 14.336 / 12.128 / 7.670 |
| numeric-loop-10000.js | 0.980 | 0.085 | 17.537 / 15.405 / 7.102 | 13.592 / 11.407 / 6.982 |
| numeric-loop-50000.js | 3.287 | 0.195 | 17.340 / 15.219 / 7.227 | 13.568 / 11.416 / 6.984 |
| folded-string-4096.js | 1.330 | 0.447 | 17.926 / 15.778 / 7.391 | 13.535 / 11.347 / 6.873 |
| dynamic-string-4096.js | 1.631 | 0.188 | 17.920 / 15.799 / 7.469 | 13.605 / 11.424 / 6.920 |
| integer.js | 2.254 | 0.080 | 17.830 / 15.682 / 7.258 | 13.580 / 11.396 / 6.979 |
| objects.js | 1.358 | 0.428 | 18.699 / 16.558 / 7.695 | 14.344 / 12.162 / 7.766 |
| retained-objects.js | 33.153 | 21.439 | 22.822 / 20.694 / 11.406 | 20.100 / 17.893 / 13.527 |
| join-empty-1024.js | 2.540 | 0.200 | 19.812 / 17.692 / 9.016 | 14.188 / 11.975 / 7.500 |
| join-singleton-1024.js | 2.659 | 0.100 | 20.082 / 17.938 / 13.336 | 13.678 / 11.461 / 7.916 |
| join-small-1024.js | 2.802 | 0.146 | 19.926 / 17.784 / 13.193 | 13.494 / 11.334 / 9.590 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
