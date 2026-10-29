# GitHub compiler comparison

Measured 2026-10-29T15:06:53.697221+00:00; Scriptw source `afe83e312b5049bbd6c81edffacb418ba60275e5`.

Scriptc `0.4.5`, Zig `0.16.0`; v24.12.0. Wasmtime 47.0.0 with the product Fast/copying-GC configuration.

All runtimes ran together on one GitHub-hosted runner, pinned to one CPU. Each case has two rounds, each with ten warmups and seven measured invocations. Source/Wasm compilation and instance/context setup are excluded from execution time. RSS is the post-execution process endpoint with the instance alive; it includes the host, compilation and retained warmup effects, and is neither live heap nor peak RSS.

Scriptw and V8 compile JavaScript in their measured processes; Scriptc compiles JavaScript in a separate process. Its host RSS includes Wasm native compilation but excludes its JavaScript frontend process. These are host-process footprints under the recorded paths.

Scriptc receives the unchanged JavaScript prefixed only with the recorded `print` adapter. Compile failures, runtime failures and checksum mismatches retain diagnostics and have no bars. These are descriptive shared-host observations, not a conformance measurement.

| Workload | Scriptw ms | V8 ms | Scriptc ms / status |
| --- | ---: | ---: | --- |
| sort-reverse-128.js | 0.473 | 0.012 | 0.093 |
| sort-reverse-512.js | 0.823 | 0.082 | 0.293 |
| sort-sorted-512.js | 0.766 | 0.094 | 0.284 |
| to-sorted-512.js | 0.821 | 0.086 | 0.313 |
| join-1024.js | 0.960 | 0.040 | 0.413 |
| join-4096.js | 2.789 | 0.698 | 1.394 |
| join-mixed-1024.js | 1.430 | 0.211 | 0.254 |
| numeric-loop-10000.js | 0.406 | 0.618 | 0.054 |
| numeric-loop-50000.js | 0.697 | 0.832 | 0.106 |
| folded-string-4096.js | 0.766 | 0.660 | 0.728 |
| dynamic-string-4096.js | 2.274 | 0.473 | 0.234 |
| integer.js | 0.511 | 1.664 | 0.041 |
| objects.js | 1.029 | 0.040 | 0.615 |
| retained-objects.js | 19.954 | 2.842 | 35.673 |
| join-empty-1024.js | 0.657 | 0.163 | 0.212 |
| join-singleton-1024.js | 0.705 | 0.171 | 0.100 |
| join-small-1024.js | 0.816 | 0.223 | 0.167 |

[Execution time](assets/execution-performance.svg) · [Process memory](assets/execution-memory.svg)

[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · [Workflow and reproduction](contracts/compiler-comparison.md).
