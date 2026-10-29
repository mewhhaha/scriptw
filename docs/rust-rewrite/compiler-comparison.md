# GitHub compiler comparison

Measured 2026-10-29T10:29:14.701134+00:00; Scriptw source `c80fd9fbb9b38111b8df0e73ec6d6250ccbf7aee`.

Scriptc `0.4.5`, Zig `0.16.0`; v24.12.0. Wasmtime 47.0.0 with the product Fast/copying-GC configuration.

All runtimes ran together on one GitHub-hosted runner, pinned to one CPU. Each case has two rounds, each with ten warmups and seven measured invocations. Source/Wasm compilation and instance/context setup are excluded from execution time. RSS is the post-execution process endpoint with the instance alive; it includes the host, compilation and retained warmup effects, and is neither live heap nor peak RSS.

Scriptw and V8 compile JavaScript in their measured processes; Scriptc compiles JavaScript in a separate process. Its host RSS includes Wasm native compilation but excludes its JavaScript frontend process. These are host-process footprints under the recorded paths.

Scriptc receives the unchanged JavaScript prefixed only with the recorded `print` adapter. Compile failures, runtime failures and checksum mismatches retain diagnostics and have no bars. These are descriptive shared-host observations, not a conformance measurement.

| Workload | Scriptw ms | V8 ms | Scriptc ms / status |
| --- | ---: | ---: | --- |
| sort-reverse-128.js | 0.492 | 0.013 | 0.098 |
| sort-reverse-512.js | 0.873 | 0.097 | 0.304 |
| sort-sorted-512.js | 0.797 | 0.100 | 0.290 |
| to-sorted-512.js | 0.812 | 0.090 | 0.325 |
| join-1024.js | 0.989 | 0.041 | 0.422 |
| join-4096.js | 2.864 | 0.783 | 1.446 |
| join-mixed-1024.js | 1.465 | 0.221 | 0.258 |
| numeric-loop-10000.js | 0.425 | 0.637 | 0.053 |
| numeric-loop-50000.js | 0.724 | 0.890 | 0.107 |
| folded-string-4096.js | 0.807 | 0.454 | 0.748 |
| dynamic-string-4096.js | 2.286 | 0.740 | 0.259 |
| integer.js | 0.534 | 1.862 | 0.043 |
| objects.js | 1.048 | 0.040 | 0.613 |
| retained-objects.js | 19.647 | 2.677 | 36.235 |
| join-empty-1024.js | 0.653 | 0.151 | 0.205 |
| join-singleton-1024.js | 0.680 | 0.174 | 0.102 |
| join-small-1024.js | 0.817 | 0.211 | 0.189 |

[Execution time](assets/execution-performance.svg) · [Process memory](assets/execution-memory.svg)

[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · [Workflow and reproduction](contracts/compiler-comparison.md).
