# GitHub compiler comparison

Measured 2026-10-28T20:45:48.774776+00:00; Scriptw source `f02efa8c5458ee62c3158b5d1aac2b5b59c1b560`.

Scriptc `0.4.5`, Zig `0.16.0`; v24.12.0. Wasmtime 47.0.0 with the product Fast/copying-GC configuration.

All runtimes ran together on one GitHub-hosted runner, pinned to one CPU. Each case has two rounds, each with ten warmups and seven measured invocations. Source/Wasm compilation and instance/context setup are excluded from execution time. RSS is the post-execution process endpoint with the instance alive; it includes the host, compilation and retained warmup effects, and is neither live heap nor peak RSS.

Scriptw and V8 compile JavaScript in their measured processes; Scriptc compiles JavaScript in a separate process. Its host RSS includes Wasm native compilation but excludes its JavaScript frontend process. These are host-process footprints under the recorded paths.

Scriptc receives the unchanged JavaScript prefixed only with the recorded `print` adapter. Compile failures, runtime failures and checksum mismatches retain diagnostics and have no bars. These are descriptive shared-host observations, not a conformance measurement.

| Workload | Scriptw ms | V8 ms | Scriptc ms / status |
| --- | ---: | ---: | --- |
| sort-reverse-128.js | 1.372 | 0.013 | 0.101 |
| sort-reverse-512.js | 4.602 | 0.097 | 0.295 |
| sort-sorted-512.js | 1.994 | 0.230 | 0.289 |
| to-sorted-512.js | 5.349 | 0.095 | 0.324 |
| join-1024.js | 2.014 | 0.044 | 0.424 |
| join-4096.js | 6.538 | 0.538 | 1.486 |
| join-mixed-1024.js | 2.416 | 0.209 | 0.259 |
| numeric-loop-10000.js | 1.152 | 0.658 | 0.052 |
| numeric-loop-50000.js | 3.676 | 0.826 | 0.105 |
| folded-string-4096.js | 1.545 | 0.441 | 0.779 |
| dynamic-string-4096.js | 2.362 | 0.765 | 0.248 |
| integer.js | 3.940 | 1.705 | 0.040 |
| objects.js | 1.516 | 0.040 | 0.615 |
| retained-objects.js | 29.718 | 2.919 | 38.156 |
| join-empty-1024.js | 2.712 | 0.144 | 0.221 |
| join-singleton-1024.js | 2.862 | 0.119 | 0.110 |
| join-small-1024.js | 3.070 | 0.231 | 0.182 |

[Execution time](assets/execution-performance.svg) · [Process memory](assets/execution-memory.svg)

[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · [Workflow and reproduction](contracts/compiler-comparison.md).
