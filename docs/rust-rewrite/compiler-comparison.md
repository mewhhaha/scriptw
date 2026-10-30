# GitHub compiler comparison

Measured 2026-10-30T13:09:41.679869+00:00; Scriptw source `aedaa6e4e0981462b4e31ade261ea4589a6644cf`.

Scriptc `0.4.5`, Zig `0.16.0`; v24.12.0. Wasmtime 47.0.0 with the product Fast/copying-GC configuration.

All runtimes ran together on one GitHub-hosted runner, pinned to one CPU. Each case has two rounds, each with ten warmups and seven measured invocations. Initial source/Wasm compilation and instance/context setup are excluded from execution time. Scriptw's JavaScript Realm bootstrap and V8 tiering remain inside the timed entry. RSS is the post-execution process endpoint with the instance alive; it includes the host, compilation and retained warmup effects, and is neither live heap nor peak RSS.

Scriptw and V8 compile JavaScript in their measured processes; Scriptc compiles JavaScript in a separate process. Its host RSS includes Wasm native compilation but excludes its JavaScript frontend process. These are host-process footprints under the recorded paths.

Scriptc receives the unchanged JavaScript prefixed only with the recorded `print` adapter. Compile failures, runtime failures and checksum mismatches retain diagnostics and have no bars. These are descriptive shared-host observations, not a conformance measurement.

| Workload | Scriptw ms | V8 ms | Scriptc ms / status |
| --- | ---: | ---: | --- |
| sort-reverse-128.js | 0.404 | 0.022 | 0.135 |
| sort-reverse-512.js | 0.659 | 0.145 | 0.434 |
| sort-sorted-512.js | 0.673 | 0.149 | 0.431 |
| to-sorted-512.js | 0.673 | 0.129 | 0.502 |
| join-1024.js | 0.886 | 0.063 | 0.634 |
| join-4096.js | 2.506 | 0.757 | 2.179 |
| join-mixed-1024.js | 1.256 | 0.258 | 0.376 |
| numeric-loop-10000.js | 0.318 | 0.735 | 0.069 |
| numeric-loop-50000.js | 0.442 | 1.057 | 0.140 |
| folded-string-4096.js | 0.522 | 0.255 | 1.365 |
| dynamic-string-4096.js | 1.094 | 0.846 | 0.388 |
| integer.js | 0.327 | 2.372 | 0.054 |
| objects.js | 1.100 | 0.064 | 0.921 |
| retained-objects.js | 25.149 | 3.408 | 49.334 |
| join-empty-1024.js | 0.678 | 0.190 | 0.350 |
| join-singleton-1024.js | 0.677 | 0.233 | 0.172 |
| join-small-1024.js | 0.935 | 0.281 | 0.264 |

## Benchmark charts

<table>
<tr>
<td><img src="assets/benchmarks/sort-reverse-128.svg" width="440" alt="sort-reverse-128.js; V8: 0.022 ms, 48.1 MiB RSS; Scriptw: 0.404 ms, 52.5 MiB RSS; Scriptc: 0.135 ms, 27.4 MiB RSS"></td>
<td><img src="assets/benchmarks/sort-reverse-512.svg" width="440" alt="sort-reverse-512.js; V8: 0.145 ms, 49.0 MiB RSS; Scriptw: 0.659 ms, 52.2 MiB RSS; Scriptc: 0.434 ms, 27.6 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/sort-sorted-512.svg" width="440" alt="sort-sorted-512.js; V8: 0.149 ms, 49.0 MiB RSS; Scriptw: 0.673 ms, 52.1 MiB RSS; Scriptc: 0.431 ms, 27.6 MiB RSS"></td>
<td><img src="assets/benchmarks/to-sorted-512.svg" width="440" alt="to-sorted-512.js; V8: 0.129 ms, 49.0 MiB RSS; Scriptw: 0.673 ms, 51.4 MiB RSS; Scriptc: 0.502 ms, 27.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-1024.svg" width="440" alt="join-1024.js; V8: 0.063 ms, 49.2 MiB RSS; Scriptw: 0.886 ms, 51.7 MiB RSS; Scriptc: 0.634 ms, 26.9 MiB RSS"></td>
<td><img src="assets/benchmarks/join-4096.svg" width="440" alt="join-4096.js; V8: 0.757 ms, 56.5 MiB RSS; Scriptw: 2.506 ms, 52.3 MiB RSS; Scriptc: 2.179 ms, 27.2 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-mixed-1024.svg" width="440" alt="join-mixed-1024.js; V8: 0.258 ms, 55.0 MiB RSS; Scriptw: 1.256 ms, 52.8 MiB RSS; Scriptc: 0.376 ms, 27.0 MiB RSS"></td>
<td><img src="assets/benchmarks/numeric-loop-10000.svg" width="440" alt="numeric-loop-10000.js; V8: 0.735 ms, 54.6 MiB RSS; Scriptw: 0.318 ms, 52.2 MiB RSS; Scriptc: 0.069 ms, 22.1 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/numeric-loop-50000.svg" width="440" alt="numeric-loop-50000.js; V8: 1.057 ms, 56.5 MiB RSS; Scriptw: 0.442 ms, 51.2 MiB RSS; Scriptc: 0.140 ms, 22.1 MiB RSS"></td>
<td><img src="assets/benchmarks/folded-string-4096.svg" width="440" alt="folded-string-4096.js; V8: 0.255 ms, 54.5 MiB RSS; Scriptw: 0.522 ms, 56.6 MiB RSS; Scriptc: 1.365 ms, 22.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/dynamic-string-4096.svg" width="440" alt="dynamic-string-4096.js; V8: 0.846 ms, 55.7 MiB RSS; Scriptw: 1.094 ms, 54.0 MiB RSS; Scriptc: 0.388 ms, 22.1 MiB RSS"></td>
<td><img src="assets/benchmarks/integer.svg" width="440" alt="integer.js; V8: 2.372 ms, 55.3 MiB RSS; Scriptw: 0.327 ms, 51.4 MiB RSS; Scriptc: 0.054 ms, 22.1 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/objects.svg" width="440" alt="objects.js; V8: 0.064 ms, 49.1 MiB RSS; Scriptw: 1.100 ms, 50.9 MiB RSS; Scriptc: 0.921 ms, 27.5 MiB RSS"></td>
<td><img src="assets/benchmarks/retained-objects.svg" width="440" alt="retained-objects.js; V8: 3.408 ms, 68.1 MiB RSS; Scriptw: 25.149 ms, 56.8 MiB RSS; Scriptc: 49.334 ms, 34.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-empty-1024.svg" width="440" alt="join-empty-1024.js; V8: 0.190 ms, 53.8 MiB RSS; Scriptw: 0.678 ms, 52.3 MiB RSS; Scriptc: 0.350 ms, 27.0 MiB RSS"></td>
<td><img src="assets/benchmarks/join-singleton-1024.svg" width="440" alt="join-singleton-1024.js; V8: 0.233 ms, 54.8 MiB RSS; Scriptw: 0.677 ms, 52.2 MiB RSS; Scriptc: 0.172 ms, 22.1 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks/join-small-1024.svg" width="440" alt="join-small-1024.js; V8: 0.281 ms, 55.7 MiB RSS; Scriptw: 0.935 ms, 52.3 MiB RSS; Scriptc: 0.264 ms, 21.9 MiB RSS"></td>
<td></td>
</tr>
</table>

[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · [Workflow and reproduction](contracts/compiler-comparison.md).
