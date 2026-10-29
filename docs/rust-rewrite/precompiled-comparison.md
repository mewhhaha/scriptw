# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-29T15:10:22.063718+00:00 through 2026-10-29T15:11:33.955150+00:00; source `afe83e312b5049bbd6c81edffacb418ba60275e5`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 0.467 | 0.086 | 17.932 / 15.792 / 7.104 | 12.850 / 10.661 / 5.914 |
| sort-reverse-512.js | 0.799 | 0.214 | 17.920 / 15.785 / 7.236 | 12.840 / 10.632 / 5.916 |
| sort-sorted-512.js | 0.760 | 0.210 | 17.383 / 15.260 / 6.541 | 12.818 / 10.619 / 5.936 |
| to-sorted-512.js | 0.784 | 0.229 | 17.879 / 15.729 / 7.219 | 12.992 / 10.790 / 5.975 |
| join-1024.js | 0.964 | 0.299 | 18.303 / 16.163 / 7.287 | 12.672 / 10.489 / 5.701 |
| join-4096.js | 2.821 | 0.945 | 18.826 / 16.704 / 7.832 | 13.344 / 11.163 / 6.521 |
| join-mixed-1024.js | 1.169 | 0.176 | 17.951 / 15.812 / 7.307 | 12.822 / 10.662 / 5.891 |
| numeric-loop-10000.js | 0.400 | 0.061 | 17.113 / 14.993 / 7.049 | 12.250 / 10.076 / 5.279 |
| numeric-loop-50000.js | 0.514 | 0.148 | 17.227 / 15.070 / 6.982 | 12.152 / 9.951 / 2.719 |
| folded-string-4096.js | 0.731 | 0.408 | 17.314 / 15.182 / 3.051 | 12.055 / 9.874 / 2.299 |
| dynamic-string-4096.js | 1.293 | 0.159 | 17.432 / 15.304 / 3.047 | 12.158 / 9.949 / 2.285 |
| integer.js | 0.481 | 0.042 | 17.482 / 15.363 / 3.068 | 12.230 / 10.048 / 2.285 |
| objects.js | 0.886 | 0.404 | 17.998 / 15.850 / 3.215 | 12.943 / 10.733 / 3.062 |
| retained-objects.js | 15.669 | 22.241 | 23.391 / 21.268 / 8.543 | 18.420 / 16.224 / 8.812 |
| join-empty-1024.js | 0.640 | 0.152 | 17.098 / 14.957 / 3.037 | 12.799 / 10.597 / 2.867 |
| join-singleton-1024.js | 0.647 | 0.076 | 17.693 / 15.546 / 3.033 | 11.676 / 9.497 / 2.283 |
| join-small-1024.js | 0.802 | 0.108 | 17.646 / 15.505 / 3.109 | 12.266 / 10.072 / 2.293 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
