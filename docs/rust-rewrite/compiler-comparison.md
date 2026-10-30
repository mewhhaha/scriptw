# GitHub compiler comparison

Measured 2026-10-30T09:32:19.641197+00:00; Scriptw source `64cf187d62a3322b781c910d5fd033c5bad10777`.

Scriptc `0.4.5`, Zig `0.16.0`; v24.12.0. Wasmtime 47.0.0 with the product Fast/copying-GC configuration.

All runtimes ran together on one GitHub-hosted runner, pinned to one CPU. Each case has two rounds, each with ten warmups and seven measured invocations. Source/Wasm compilation and instance/context setup are excluded from execution time. RSS is the post-execution process endpoint with the instance alive; it includes the host, compilation and retained warmup effects, and is neither live heap nor peak RSS.

Scriptw and V8 compile JavaScript in their measured processes; Scriptc compiles JavaScript in a separate process. Its host RSS includes Wasm native compilation but excludes its JavaScript frontend process. These are host-process footprints under the recorded paths.

Scriptc receives the unchanged JavaScript prefixed only with the recorded `print` adapter. Compile failures, runtime failures and checksum mismatches retain diagnostics and have no bars. These are descriptive shared-host observations, not a conformance measurement.

| Workload | Scriptw ms | V8 ms | Scriptc ms / status |
| --- | ---: | ---: | --- |
| sort-reverse-128.js | 0.356 | 0.012 | 0.100 |
| sort-reverse-512.js | 0.640 | 0.094 | 0.284 |
| sort-sorted-512.js | 0.615 | 0.089 | 0.287 |
| to-sorted-512.js | 0.665 | 0.087 | 0.325 |
| join-1024.js | 0.847 | 0.039 | 0.416 |
| join-4096.js | 2.541 | 0.711 | 1.415 |
| join-mixed-1024.js | 1.083 | 0.212 | 0.244 |
| numeric-loop-10000.js | 0.331 | 0.616 | 0.052 |
| numeric-loop-50000.js | 0.592 | 0.758 | 0.105 |
| folded-string-4096.js | 0.391 | 0.687 | 0.719 |
| dynamic-string-4096.js | 0.690 | 0.719 | 0.233 |
| integer.js | 0.339 | 1.700 | 0.040 |
| objects.js | 0.784 | 0.039 | 0.591 |
| retained-objects.js | 17.838 | 2.780 | 35.988 |
| join-empty-1024.js | 0.464 | 0.159 | 0.205 |
| join-singleton-1024.js | 0.518 | 0.170 | 0.100 |
| join-small-1024.js | 0.619 | 0.211 | 0.174 |

## Benchmark charts

<table>
<tr>
<td><img src="assets/benchmarks/sort-reverse-128.svg" width="440" alt="sort-reverse-128.js; V8: 0.012 ms, 48.1 MiB RSS; Scriptw: 0.356 ms, 52.0 MiB RSS; Scriptc: 0.100 ms, 27.4 MiB RSS"></td>
<td><img src="assets/benchmarks/sort-reverse-512.svg" width="440" alt="sort-reverse-512.js; V8: 0.094 ms, 49.0 MiB RSS; Scriptw: 0.640 ms, 51.8 MiB RSS; Scriptc: 0.284 ms, 27.4 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/sort-sorted-512.svg" width="440" alt="sort-sorted-512.js; V8: 0.089 ms, 49.0 MiB RSS; Scriptw: 0.615 ms, 51.7 MiB RSS; Scriptc: 0.287 ms, 27.6 MiB RSS"></td>
<td><img src="assets/benchmarks/to-sorted-512.svg" width="440" alt="to-sorted-512.js; V8: 0.087 ms, 49.0 MiB RSS; Scriptw: 0.665 ms, 54.1 MiB RSS; Scriptc: 0.325 ms, 27.8 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-1024.svg" width="440" alt="join-1024.js; V8: 0.039 ms, 49.3 MiB RSS; Scriptw: 0.847 ms, 51.2 MiB RSS; Scriptc: 0.416 ms, 26.7 MiB RSS"></td>
<td><img src="assets/benchmarks/join-4096.svg" width="440" alt="join-4096.js; V8: 0.711 ms, 57.1 MiB RSS; Scriptw: 2.541 ms, 52.8 MiB RSS; Scriptc: 1.415 ms, 27.3 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-mixed-1024.svg" width="440" alt="join-mixed-1024.js; V8: 0.212 ms, 55.0 MiB RSS; Scriptw: 1.083 ms, 52.3 MiB RSS; Scriptc: 0.244 ms, 26.9 MiB RSS"></td>
<td><img src="assets/benchmarks/numeric-loop-10000.svg" width="440" alt="numeric-loop-10000.js; V8: 0.616 ms, 54.6 MiB RSS; Scriptw: 0.331 ms, 51.9 MiB RSS; Scriptc: 0.052 ms, 22.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/numeric-loop-50000.svg" width="440" alt="numeric-loop-50000.js; V8: 0.758 ms, 56.2 MiB RSS; Scriptw: 0.592 ms, 50.7 MiB RSS; Scriptc: 0.105 ms, 22.0 MiB RSS"></td>
<td><img src="assets/benchmarks/folded-string-4096.svg" width="440" alt="folded-string-4096.js; V8: 0.687 ms, 54.7 MiB RSS; Scriptw: 0.391 ms, 55.1 MiB RSS; Scriptc: 0.719 ms, 21.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/dynamic-string-4096.svg" width="440" alt="dynamic-string-4096.js; V8: 0.719 ms, 55.6 MiB RSS; Scriptw: 0.690 ms, 53.9 MiB RSS; Scriptc: 0.233 ms, 21.9 MiB RSS"></td>
<td><img src="assets/benchmarks/integer.svg" width="440" alt="integer.js; V8: 1.700 ms, 55.5 MiB RSS; Scriptw: 0.339 ms, 50.5 MiB RSS; Scriptc: 0.040 ms, 22.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/objects.svg" width="440" alt="objects.js; V8: 0.039 ms, 49.1 MiB RSS; Scriptw: 0.784 ms, 52.8 MiB RSS; Scriptc: 0.591 ms, 27.3 MiB RSS"></td>
<td><img src="assets/benchmarks/retained-objects.svg" width="440" alt="retained-objects.js; V8: 2.780 ms, 66.7 MiB RSS; Scriptw: 17.838 ms, 56.4 MiB RSS; Scriptc: 35.988 ms, 33.8 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-empty-1024.svg" width="440" alt="join-empty-1024.js; V8: 0.159 ms, 53.9 MiB RSS; Scriptw: 0.464 ms, 51.8 MiB RSS; Scriptc: 0.205 ms, 26.9 MiB RSS"></td>
<td><img src="assets/benchmarks/join-singleton-1024.svg" width="440" alt="join-singleton-1024.js; V8: 0.170 ms, 54.7 MiB RSS; Scriptw: 0.518 ms, 50.7 MiB RSS; Scriptc: 0.100 ms, 21.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-small-1024.svg" width="440" alt="join-small-1024.js; V8: 0.211 ms, 55.7 MiB RSS; Scriptw: 0.619 ms, 51.6 MiB RSS; Scriptc: 0.174 ms, 21.9 MiB RSS"></td>
<td></td>
</tr>
</table>

[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · [Workflow and reproduction](contracts/compiler-comparison.md).
