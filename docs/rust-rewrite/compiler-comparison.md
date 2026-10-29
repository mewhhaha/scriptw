# GitHub compiler comparison

Measured 2026-10-29T20:52:18.078448+00:00; Scriptw source `54a28246ee78d8e58b07c8c5e3c705d3c2ab03e8`.

Scriptc `0.4.5`, Zig `0.16.0`; v24.12.0. Wasmtime 47.0.0 with the product Fast/copying-GC configuration.

All runtimes ran together on one GitHub-hosted runner, pinned to one CPU. Each case has two rounds, each with ten warmups and seven measured invocations. Source/Wasm compilation and instance/context setup are excluded from execution time. RSS is the post-execution process endpoint with the instance alive; it includes the host, compilation and retained warmup effects, and is neither live heap nor peak RSS.

Scriptw and V8 compile JavaScript in their measured processes; Scriptc compiles JavaScript in a separate process. Its host RSS includes Wasm native compilation but excludes its JavaScript frontend process. These are host-process footprints under the recorded paths.

Scriptc receives the unchanged JavaScript prefixed only with the recorded `print` adapter. Compile failures, runtime failures and checksum mismatches retain diagnostics and have no bars. These are descriptive shared-host observations, not a conformance measurement.

| Workload | Scriptw ms | V8 ms | Scriptc ms / status |
| --- | ---: | ---: | --- |
| sort-reverse-128.js | 0.620 | 0.020 | 0.113 |
| sort-reverse-512.js | 1.056 | 0.156 | 0.431 |
| sort-sorted-512.js | 0.953 | 0.138 | 0.425 |
| to-sorted-512.js | 1.090 | 0.157 | 0.482 |
| join-1024.js | 1.268 | 0.069 | 0.598 |
| join-4096.js | 3.743 | 1.190 | 2.124 |
| join-mixed-1024.js | 1.542 | 0.343 | 0.351 |
| numeric-loop-10000.js | 0.534 | 1.007 | 0.055 |
| numeric-loop-50000.js | 0.925 | 1.456 | 0.121 |
| folded-string-4096.js | 0.692 | 1.052 | 1.095 |
| dynamic-string-4096.js | 0.776 | 1.242 | 0.326 |
| integer.js | 0.673 | 2.565 | 0.041 |
| objects.js | 1.147 | 0.072 | 0.973 |
| retained-objects.js | 21.450 | 4.967 | 55.976 |
| join-empty-1024.js | 0.866 | 0.254 | 0.309 |
| join-singleton-1024.js | 0.905 | 0.117 | 0.125 |
| join-small-1024.js | 1.080 | 0.370 | 0.216 |

## Benchmark charts

<table>
<tr>
<td><img src="assets/benchmarks/sort-reverse-128.svg" width="440" alt="sort-reverse-128.js; V8: 0.020 ms, 48.1 MiB RSS; Scriptw: 0.620 ms, 50.4 MiB RSS; Scriptc: 0.113 ms, 27.5 MiB RSS"></td>
<td><img src="assets/benchmarks/sort-reverse-512.svg" width="440" alt="sort-reverse-512.js; V8: 0.156 ms, 49.0 MiB RSS; Scriptw: 1.056 ms, 50.7 MiB RSS; Scriptc: 0.431 ms, 27.4 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/sort-sorted-512.svg" width="440" alt="sort-sorted-512.js; V8: 0.138 ms, 49.0 MiB RSS; Scriptw: 0.953 ms, 51.1 MiB RSS; Scriptc: 0.425 ms, 27.5 MiB RSS"></td>
<td><img src="assets/benchmarks/to-sorted-512.svg" width="440" alt="to-sorted-512.js; V8: 0.157 ms, 49.0 MiB RSS; Scriptw: 1.090 ms, 52.0 MiB RSS; Scriptc: 0.482 ms, 27.8 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-1024.svg" width="440" alt="join-1024.js; V8: 0.069 ms, 49.2 MiB RSS; Scriptw: 1.268 ms, 52.7 MiB RSS; Scriptc: 0.598 ms, 26.8 MiB RSS"></td>
<td><img src="assets/benchmarks/join-4096.svg" width="440" alt="join-4096.js; V8: 1.190 ms, 57.3 MiB RSS; Scriptw: 3.743 ms, 51.8 MiB RSS; Scriptc: 2.124 ms, 27.3 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-mixed-1024.svg" width="440" alt="join-mixed-1024.js; V8: 0.343 ms, 55.1 MiB RSS; Scriptw: 1.542 ms, 51.2 MiB RSS; Scriptc: 0.351 ms, 27.0 MiB RSS"></td>
<td><img src="assets/benchmarks/numeric-loop-10000.svg" width="440" alt="numeric-loop-10000.js; V8: 1.007 ms, 54.4 MiB RSS; Scriptw: 0.534 ms, 54.0 MiB RSS; Scriptc: 0.055 ms, 22.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/numeric-loop-50000.svg" width="440" alt="numeric-loop-50000.js; V8: 1.456 ms, 56.5 MiB RSS; Scriptw: 0.925 ms, 51.4 MiB RSS; Scriptc: 0.121 ms, 22.0 MiB RSS"></td>
<td><img src="assets/benchmarks/folded-string-4096.svg" width="440" alt="folded-string-4096.js; V8: 1.052 ms, 54.5 MiB RSS; Scriptw: 0.692 ms, 55.9 MiB RSS; Scriptc: 1.095 ms, 21.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/dynamic-string-4096.svg" width="440" alt="dynamic-string-4096.js; V8: 1.242 ms, 56.3 MiB RSS; Scriptw: 0.776 ms, 54.6 MiB RSS; Scriptc: 0.326 ms, 21.8 MiB RSS"></td>
<td><img src="assets/benchmarks/integer.svg" width="440" alt="integer.js; V8: 2.565 ms, 55.5 MiB RSS; Scriptw: 0.673 ms, 50.6 MiB RSS; Scriptc: 0.041 ms, 22.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/objects.svg" width="440" alt="objects.js; V8: 0.072 ms, 49.1 MiB RSS; Scriptw: 1.147 ms, 51.1 MiB RSS; Scriptc: 0.973 ms, 27.2 MiB RSS"></td>
<td><img src="assets/benchmarks/retained-objects.svg" width="440" alt="retained-objects.js; V8: 4.967 ms, 66.6 MiB RSS; Scriptw: 21.450 ms, 58.0 MiB RSS; Scriptc: 55.976 ms, 33.7 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-empty-1024.svg" width="440" alt="join-empty-1024.js; V8: 0.254 ms, 53.8 MiB RSS; Scriptw: 0.866 ms, 50.0 MiB RSS; Scriptc: 0.309 ms, 26.9 MiB RSS"></td>
<td><img src="assets/benchmarks/join-singleton-1024.svg" width="440" alt="join-singleton-1024.js; V8: 0.117 ms, 51.9 MiB RSS; Scriptw: 0.905 ms, 51.4 MiB RSS; Scriptc: 0.125 ms, 21.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-small-1024.svg" width="440" alt="join-small-1024.js; V8: 0.370 ms, 55.6 MiB RSS; Scriptw: 1.080 ms, 50.6 MiB RSS; Scriptc: 0.216 ms, 21.9 MiB RSS"></td>
<td></td>
</tr>
</table>

[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · [Workflow and reproduction](contracts/compiler-comparison.md).
