# GitHub compiler comparison

Measured 2026-10-29T23:02:16.626216+00:00; Scriptw source `f19370fcf91018a7153cf9d8a4b38fdcf3f5f3df`.

Scriptc `0.4.5`, Zig `0.16.0`; v24.12.0. Wasmtime 47.0.0 with the product Fast/copying-GC configuration.

All runtimes ran together on one GitHub-hosted runner, pinned to one CPU. Each case has two rounds, each with ten warmups and seven measured invocations. Source/Wasm compilation and instance/context setup are excluded from execution time. RSS is the post-execution process endpoint with the instance alive; it includes the host, compilation and retained warmup effects, and is neither live heap nor peak RSS.

Scriptw and V8 compile JavaScript in their measured processes; Scriptc compiles JavaScript in a separate process. Its host RSS includes Wasm native compilation but excludes its JavaScript frontend process. These are host-process footprints under the recorded paths.

Scriptc receives the unchanged JavaScript prefixed only with the recorded `print` adapter. Compile failures, runtime failures and checksum mismatches retain diagnostics and have no bars. These are descriptive shared-host observations, not a conformance measurement.

| Workload | Scriptw ms | V8 ms | Scriptc ms / status |
| --- | ---: | ---: | --- |
| sort-reverse-128.js | 0.369 | 0.012 | 0.101 |
| sort-reverse-512.js | 0.676 | 0.089 | 0.281 |
| sort-sorted-512.js | 0.627 | 0.101 | 0.280 |
| to-sorted-512.js | 0.689 | 0.084 | 0.313 |
| join-1024.js | 0.861 | 0.040 | 0.405 |
| join-4096.js | 2.669 | 0.110 | 1.386 |
| join-mixed-1024.js | 1.066 | 0.206 | 0.257 |
| numeric-loop-10000.js | 0.314 | 0.603 | 0.053 |
| numeric-loop-50000.js | 0.593 | 0.866 | 0.105 |
| folded-string-4096.js | 0.390 | 0.665 | 0.764 |
| dynamic-string-4096.js | 0.561 | 0.119 | 0.238 |
| integer.js | 0.329 | 1.747 | 0.039 |
| objects.js | 0.819 | 0.040 | 0.582 |
| retained-objects.js | 17.009 | 2.655 | 35.749 |
| join-empty-1024.js | 0.495 | 0.158 | 0.200 |
| join-singleton-1024.js | 0.492 | 0.174 | 0.105 |
| join-small-1024.js | 0.654 | 0.225 | 0.171 |

## Benchmark charts

<table>
<tr>
<td><img src="assets/benchmarks/sort-reverse-128.svg" width="440" alt="sort-reverse-128.js; V8: 0.012 ms, 48.1 MiB RSS; Scriptw: 0.369 ms, 51.4 MiB RSS; Scriptc: 0.101 ms, 27.3 MiB RSS"></td>
<td><img src="assets/benchmarks/sort-reverse-512.svg" width="440" alt="sort-reverse-512.js; V8: 0.089 ms, 49.0 MiB RSS; Scriptw: 0.676 ms, 51.7 MiB RSS; Scriptc: 0.281 ms, 27.5 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/sort-sorted-512.svg" width="440" alt="sort-sorted-512.js; V8: 0.101 ms, 49.0 MiB RSS; Scriptw: 0.627 ms, 51.8 MiB RSS; Scriptc: 0.280 ms, 27.4 MiB RSS"></td>
<td><img src="assets/benchmarks/to-sorted-512.svg" width="440" alt="to-sorted-512.js; V8: 0.084 ms, 49.0 MiB RSS; Scriptw: 0.689 ms, 52.1 MiB RSS; Scriptc: 0.313 ms, 27.8 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-1024.svg" width="440" alt="join-1024.js; V8: 0.040 ms, 49.2 MiB RSS; Scriptw: 0.861 ms, 52.4 MiB RSS; Scriptc: 0.405 ms, 26.8 MiB RSS"></td>
<td><img src="assets/benchmarks/join-4096.svg" width="440" alt="join-4096.js; V8: 0.110 ms, 57.0 MiB RSS; Scriptw: 2.669 ms, 52.4 MiB RSS; Scriptc: 1.386 ms, 27.3 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-mixed-1024.svg" width="440" alt="join-mixed-1024.js; V8: 0.206 ms, 55.1 MiB RSS; Scriptw: 1.066 ms, 52.3 MiB RSS; Scriptc: 0.257 ms, 26.9 MiB RSS"></td>
<td><img src="assets/benchmarks/numeric-loop-10000.svg" width="440" alt="numeric-loop-10000.js; V8: 0.603 ms, 54.6 MiB RSS; Scriptw: 0.314 ms, 51.0 MiB RSS; Scriptc: 0.053 ms, 22.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/numeric-loop-50000.svg" width="440" alt="numeric-loop-50000.js; V8: 0.866 ms, 56.3 MiB RSS; Scriptw: 0.593 ms, 50.9 MiB RSS; Scriptc: 0.105 ms, 22.1 MiB RSS"></td>
<td><img src="assets/benchmarks/folded-string-4096.svg" width="440" alt="folded-string-4096.js; V8: 0.665 ms, 54.6 MiB RSS; Scriptw: 0.390 ms, 55.6 MiB RSS; Scriptc: 0.764 ms, 21.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/dynamic-string-4096.svg" width="440" alt="dynamic-string-4096.js; V8: 0.119 ms, 56.0 MiB RSS; Scriptw: 0.561 ms, 54.4 MiB RSS; Scriptc: 0.238 ms, 21.9 MiB RSS"></td>
<td><img src="assets/benchmarks/integer.svg" width="440" alt="integer.js; V8: 1.747 ms, 55.5 MiB RSS; Scriptw: 0.329 ms, 51.7 MiB RSS; Scriptc: 0.039 ms, 22.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/objects.svg" width="440" alt="objects.js; V8: 0.040 ms, 49.1 MiB RSS; Scriptw: 0.819 ms, 51.1 MiB RSS; Scriptc: 0.582 ms, 27.3 MiB RSS"></td>
<td><img src="assets/benchmarks/retained-objects.svg" width="440" alt="retained-objects.js; V8: 2.655 ms, 68.2 MiB RSS; Scriptw: 17.009 ms, 58.0 MiB RSS; Scriptc: 35.749 ms, 33.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-empty-1024.svg" width="440" alt="join-empty-1024.js; V8: 0.158 ms, 53.8 MiB RSS; Scriptw: 0.495 ms, 52.8 MiB RSS; Scriptc: 0.200 ms, 26.8 MiB RSS"></td>
<td><img src="assets/benchmarks/join-singleton-1024.svg" width="440" alt="join-singleton-1024.js; V8: 0.174 ms, 54.7 MiB RSS; Scriptw: 0.492 ms, 50.5 MiB RSS; Scriptc: 0.105 ms, 21.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-small-1024.svg" width="440" alt="join-small-1024.js; V8: 0.225 ms, 55.5 MiB RSS; Scriptw: 0.654 ms, 51.2 MiB RSS; Scriptc: 0.171 ms, 22.0 MiB RSS"></td>
<td></td>
</tr>
</table>

[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · [Workflow and reproduction](contracts/compiler-comparison.md).
