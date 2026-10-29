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

## Benchmark charts

<table>
<tr>
<td><img src="assets/benchmarks/sort-reverse-128.svg" width="440" alt="sort-reverse-128.js; V8: 0.012 ms, 48.1 MiB RSS; Scriptw: 0.473 ms, 50.8 MiB RSS; Scriptc: 0.093 ms, 27.4 MiB RSS"></td>
<td><img src="assets/benchmarks/sort-reverse-512.svg" width="440" alt="sort-reverse-512.js; V8: 0.082 ms, 49.0 MiB RSS; Scriptw: 0.823 ms, 51.1 MiB RSS; Scriptc: 0.293 ms, 27.4 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/sort-sorted-512.svg" width="440" alt="sort-sorted-512.js; V8: 0.094 ms, 49.0 MiB RSS; Scriptw: 0.766 ms, 51.4 MiB RSS; Scriptc: 0.284 ms, 27.4 MiB RSS"></td>
<td><img src="assets/benchmarks/to-sorted-512.svg" width="440" alt="to-sorted-512.js; V8: 0.086 ms, 49.0 MiB RSS; Scriptw: 0.821 ms, 52.5 MiB RSS; Scriptc: 0.313 ms, 27.7 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-1024.svg" width="440" alt="join-1024.js; V8: 0.040 ms, 49.2 MiB RSS; Scriptw: 0.960 ms, 51.0 MiB RSS; Scriptc: 0.413 ms, 26.8 MiB RSS"></td>
<td><img src="assets/benchmarks/join-4096.svg" width="440" alt="join-4096.js; V8: 0.698 ms, 57.0 MiB RSS; Scriptw: 2.789 ms, 52.3 MiB RSS; Scriptc: 1.394 ms, 27.2 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-mixed-1024.svg" width="440" alt="join-mixed-1024.js; V8: 0.211 ms, 55.2 MiB RSS; Scriptw: 1.430 ms, 52.4 MiB RSS; Scriptc: 0.254 ms, 27.0 MiB RSS"></td>
<td><img src="assets/benchmarks/numeric-loop-10000.svg" width="440" alt="numeric-loop-10000.js; V8: 0.618 ms, 54.8 MiB RSS; Scriptw: 0.406 ms, 50.8 MiB RSS; Scriptc: 0.054 ms, 22.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/numeric-loop-50000.svg" width="440" alt="numeric-loop-50000.js; V8: 0.832 ms, 56.3 MiB RSS; Scriptw: 0.697 ms, 50.9 MiB RSS; Scriptc: 0.106 ms, 22.0 MiB RSS"></td>
<td><img src="assets/benchmarks/folded-string-4096.svg" width="440" alt="folded-string-4096.js; V8: 0.660 ms, 54.4 MiB RSS; Scriptw: 0.766 ms, 56.3 MiB RSS; Scriptc: 0.728 ms, 21.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/dynamic-string-4096.svg" width="440" alt="dynamic-string-4096.js; V8: 0.473 ms, 55.4 MiB RSS; Scriptw: 2.274 ms, 54.3 MiB RSS; Scriptc: 0.234 ms, 21.9 MiB RSS"></td>
<td><img src="assets/benchmarks/integer.svg" width="440" alt="integer.js; V8: 1.664 ms, 55.5 MiB RSS; Scriptw: 0.511 ms, 49.9 MiB RSS; Scriptc: 0.041 ms, 22.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/objects.svg" width="440" alt="objects.js; V8: 0.040 ms, 49.1 MiB RSS; Scriptw: 1.029 ms, 51.6 MiB RSS; Scriptc: 0.615 ms, 27.3 MiB RSS"></td>
<td><img src="assets/benchmarks/retained-objects.svg" width="440" alt="retained-objects.js; V8: 2.842 ms, 67.8 MiB RSS; Scriptw: 19.954 ms, 56.4 MiB RSS; Scriptc: 35.673 ms, 33.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-empty-1024.svg" width="440" alt="join-empty-1024.js; V8: 0.163 ms, 54.1 MiB RSS; Scriptw: 0.657 ms, 50.4 MiB RSS; Scriptc: 0.212 ms, 26.8 MiB RSS"></td>
<td><img src="assets/benchmarks/join-singleton-1024.svg" width="440" alt="join-singleton-1024.js; V8: 0.171 ms, 54.8 MiB RSS; Scriptw: 0.705 ms, 51.1 MiB RSS; Scriptc: 0.100 ms, 21.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-small-1024.svg" width="440" alt="join-small-1024.js; V8: 0.223 ms, 55.6 MiB RSS; Scriptw: 0.816 ms, 50.8 MiB RSS; Scriptc: 0.167 ms, 22.0 MiB RSS"></td>
<td></td>
</tr>
</table>

[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · [Workflow and reproduction](contracts/compiler-comparison.md).
