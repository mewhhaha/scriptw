# GitHub compiler comparison

Measured 2026-10-28T11:17:11.914705+00:00; Scriptw source `a0d1366aeb8da2acdd992e86d884519f96b5730e`.

Scriptc `0.4.5`, Zig `0.16.0`; v24.12.0. Wasmtime 47.0.0 with the product Fast/copying-GC configuration.

All runtimes ran together on one GitHub-hosted runner, pinned to one CPU. Each case has two rounds, each with ten warmups and seven measured invocations. Source/Wasm compilation and instance/context setup are excluded from execution time. RSS is the post-execution process endpoint with the instance alive; it includes the host, compilation and retained warmup effects, and is neither live heap nor peak RSS.

Scriptw and V8 compile JavaScript in their measured processes; Scriptc compiles JavaScript in a separate process. Its host RSS includes Wasm native compilation but excludes its JavaScript frontend process. These are host-process footprints under the recorded paths.

Scriptc receives the unchanged JavaScript prefixed only with the recorded `print` adapter. Compile failures, runtime failures and checksum mismatches retain diagnostics and have no bars. These are descriptive shared-host observations, not a conformance measurement.

| Workload | Scriptw ms | V8 ms | Scriptc ms / status |
| --- | ---: | ---: | --- |
| sort-reverse-128.js | 1.580 | 0.022 | 0.176 |
| sort-reverse-512.js | 5.456 | 0.140 | 0.470 |
| sort-sorted-512.js | 2.555 | 0.146 | 0.467 |
| to-sorted-512.js | 6.568 | 0.130 | 0.538 |
| join-1024.js | 2.387 | 0.062 | 0.656 |
| join-4096.js | 7.906 | 0.422 | 2.186 |
| join-mixed-1024.js | 3.105 | 0.264 | 0.414 |
| numeric-loop-10000.js | 1.447 | 0.233 | 0.097 |
| numeric-loop-50000.js | 5.054 | 1.048 | 0.170 |
| folded-string-4096.js | 2.218 | 0.783 | 1.196 |
| dynamic-string-4096.js | 3.453 | 0.857 | 0.404 |
| integer.js | 5.241 | 2.436 | 0.082 |
| objects.js | 2.058 | 0.065 | 0.951 |
| retained-objects.js | 52.357 | 3.186 | 49.040 |
| join-empty-1024.js | 3.333 | 0.204 | 0.369 |
| join-singleton-1024.js | 3.625 | 0.218 | 0.178 |
| join-small-1024.js | 3.747 | 0.279 | 0.294 |

[Execution time](assets/execution-performance.svg) · [Process memory](assets/execution-memory.svg)

[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · [Workflow and reproduction](contracts/compiler-comparison.md).
