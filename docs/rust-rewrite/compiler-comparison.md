# GitHub compiler comparison

Measured 2026-10-26T13:01:37.206476+00:00; Scriptw source `8517b20464d876ebc0e169770ec2c14501ff23d9`.

Scriptc `0.4.5`, Zig `0.16.0`; v24.12.0. Wasmtime 47.0.0 with the product Fast/copying-GC configuration.

All runtimes ran together on one GitHub-hosted runner, pinned to one CPU. Each case has two rounds, each with ten warmups and seven measured invocations. Source/Wasm compilation and instance/context setup are excluded from execution time. RSS is the post-execution process endpoint with the instance alive; it includes the host, compilation and retained warmup effects, and is neither live heap nor peak RSS.

Scriptw and V8 compile JavaScript in their measured processes; Scriptc compiles JavaScript in a separate process. Its host RSS includes Wasm native compilation but excludes its JavaScript frontend process. These are host-process footprints under the recorded paths.

Scriptc receives the unchanged JavaScript prefixed only with the recorded `print` adapter. Compile failures, runtime failures and checksum mismatches retain diagnostics and have no bars. These are descriptive shared-host observations, not a conformance measurement.

| Workload | Scriptw ms | V8 ms | Scriptc ms / status |
| --- | ---: | ---: | --- |
| sort-reverse-128.js | 3.500 | 0.020 | 0.229 |
| sort-reverse-512.js | 14.272 | 0.149 | 0.517 |
| sort-sorted-512.js | 11.191 | 0.150 | 0.520 |
| to-sorted-512.js | 11.506 | 0.146 | 0.572 |
| join-1024.js | 12.427 | 0.062 | 0.683 |
| join-4096.js | 47.060 | 0.476 | 2.208 |
| join-mixed-1024.js | 13.233 | 0.331 | 0.439 |
| numeric-loop-10000.js | 1.331 | 0.982 | 0.133 |
| numeric-loop-50000.js | 3.632 | 1.420 | 0.197 |
| folded-string-4096.js | 2.330 | 0.699 | 1.194 |
| dynamic-string-4096.js | 2.957 | 0.773 | 0.420 |
| integer.js | 3.694 | 2.483 | 0.126 |
| objects.js | 6.698 | 0.069 | 1.062 |
| retained-objects.js | 521.957 | 4.526 | 55.382 |
| join-empty-1024.js | 3.757 | 0.242 | 0.393 |
| join-singleton-1024.js | 3.942 | 0.285 | 0.209 |
| join-small-1024.js | 5.594 | 0.349 | 0.302 |

[Execution time](assets/execution-performance.svg) · [Process memory](assets/execution-memory.svg)

[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · [Workflow and reproduction](contracts/compiler-comparison.md).
