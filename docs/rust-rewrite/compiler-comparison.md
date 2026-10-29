# GitHub compiler comparison

Measured 2026-10-29T00:59:42.656230+00:00; Scriptw source `4751db4aa29ed56ba20820adeea040e271c60581`.

Scriptc `0.4.5`, Zig `0.16.0`; v24.12.0. Wasmtime 47.0.0 with the product Fast/copying-GC configuration.

All runtimes ran together on one GitHub-hosted runner, pinned to one CPU. Each case has two rounds, each with ten warmups and seven measured invocations. Source/Wasm compilation and instance/context setup are excluded from execution time. RSS is the post-execution process endpoint with the instance alive; it includes the host, compilation and retained warmup effects, and is neither live heap nor peak RSS.

Scriptw and V8 compile JavaScript in their measured processes; Scriptc compiles JavaScript in a separate process. Its host RSS includes Wasm native compilation but excludes its JavaScript frontend process. These are host-process footprints under the recorded paths.

Scriptc receives the unchanged JavaScript prefixed only with the recorded `print` adapter. Compile failures, runtime failures and checksum mismatches retain diagnostics and have no bars. These are descriptive shared-host observations, not a conformance measurement.

| Workload | Scriptw ms | V8 ms | Scriptc ms / status |
| --- | ---: | ---: | --- |
| sort-reverse-128.js | 0.880 | 0.031 | 0.169 |
| sort-reverse-512.js | 1.475 | 0.173 | 0.562 |
| sort-sorted-512.js | 1.400 | 0.179 | 0.557 |
| to-sorted-512.js | 1.533 | 0.157 | 0.637 |
| join-1024.js | 1.847 | 0.081 | 0.823 |
| join-4096.js | 4.834 | 0.894 | 2.842 |
| join-mixed-1024.js | 2.444 | 0.310 | 0.493 |
| numeric-loop-10000.js | 0.825 | 0.888 | 0.086 |
| numeric-loop-50000.js | 1.513 | 1.277 | 0.173 |
| folded-string-4096.js | 1.459 | 0.688 | 1.585 |
| dynamic-string-4096.js | 3.991 | 1.020 | 0.496 |
| integer.js | 0.913 | 2.851 | 0.070 |
| objects.js | 1.827 | 0.088 | 1.181 |
| retained-objects.js | 33.665 | 4.179 | 65.374 |
| join-empty-1024.js | 1.310 | 0.250 | 0.402 |
| join-singleton-1024.js | 1.361 | 0.268 | 0.232 |
| join-small-1024.js | 1.607 | 0.366 | 0.349 |

[Execution time](assets/execution-performance.svg) · [Process memory](assets/execution-memory.svg)

[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · [Workflow and reproduction](contracts/compiler-comparison.md).
