# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-29T20:47:54.477045+00:00 through 2026-10-29T20:49:15.379341+00:00; source `54a28246ee78d8e58b07c8c5e3c705d3c2ab03e8`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

<table>
<tr>
<td><img src="assets/precompiled/sort-reverse-128.svg" width="440" alt="sort-reverse-128.js; Scriptw: 0.447 ms, 18.0 MiB RSS; Scriptc: 0.112 ms, 12.9 MiB RSS"></td>
<td><img src="assets/precompiled/sort-reverse-512.svg" width="440" alt="sort-reverse-512.js; Scriptw: 0.805 ms, 18.1 MiB RSS; Scriptc: 0.275 ms, 13.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/sort-sorted-512.svg" width="440" alt="sort-sorted-512.js; Scriptw: 0.728 ms, 17.6 MiB RSS; Scriptc: 0.275 ms, 13.1 MiB RSS"></td>
<td><img src="assets/precompiled/to-sorted-512.svg" width="440" alt="to-sorted-512.js; Scriptw: 0.807 ms, 18.0 MiB RSS; Scriptc: 0.306 ms, 13.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-1024.svg" width="440" alt="join-1024.js; Scriptw: 1.000 ms, 18.3 MiB RSS; Scriptc: 0.388 ms, 13.1 MiB RSS"></td>
<td><img src="assets/precompiled/join-4096.svg" width="440" alt="join-4096.js; Scriptw: 2.972 ms, 18.6 MiB RSS; Scriptc: 1.214 ms, 13.4 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-mixed-1024.svg" width="440" alt="join-mixed-1024.js; Scriptw: 1.078 ms, 18.1 MiB RSS; Scriptc: 0.236 ms, 12.8 MiB RSS"></td>
<td><img src="assets/precompiled/numeric-loop-10000.svg" width="440" alt="numeric-loop-10000.js; Scriptw: 0.380 ms, 17.5 MiB RSS; Scriptc: 0.079 ms, 12.2 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/numeric-loop-50000.svg" width="440" alt="numeric-loop-50000.js; Scriptw: 0.604 ms, 17.3 MiB RSS; Scriptc: 0.177 ms, 12.1 MiB RSS"></td>
<td><img src="assets/precompiled/folded-string-4096.svg" width="440" alt="folded-string-4096.js; Scriptw: 0.410 ms, 17.3 MiB RSS; Scriptc: 0.566 ms, 12.4 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/dynamic-string-4096.svg" width="440" alt="dynamic-string-4096.js; Scriptw: 0.516 ms, 17.5 MiB RSS; Scriptc: 0.211 ms, 12.4 MiB RSS"></td>
<td><img src="assets/precompiled/integer.svg" width="440" alt="integer.js; Scriptw: 0.481 ms, 17.5 MiB RSS; Scriptc: 0.052 ms, 12.2 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/objects.svg" width="440" alt="objects.js; Scriptw: 0.851 ms, 18.2 MiB RSS; Scriptc: 0.545 ms, 13.1 MiB RSS"></td>
<td><img src="assets/precompiled/retained-objects.svg" width="440" alt="retained-objects.js; Scriptw: 16.545 ms, 23.5 MiB RSS; Scriptc: 27.699 ms, 18.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-empty-1024.svg" width="440" alt="join-empty-1024.js; Scriptw: 0.625 ms, 17.8 MiB RSS; Scriptc: 0.206 ms, 13.0 MiB RSS"></td>
<td><img src="assets/precompiled/join-singleton-1024.svg" width="440" alt="join-singleton-1024.js; Scriptw: 0.624 ms, 17.9 MiB RSS; Scriptc: 0.102 ms, 12.3 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-small-1024.svg" width="440" alt="join-small-1024.js; Scriptw: 0.794 ms, 18.0 MiB RSS; Scriptc: 0.139 ms, 12.4 MiB RSS"></td>
<td></td>
</tr>
</table>

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 0.447 | 0.112 | 17.965 / 15.780 / 7.143 | 12.949 / 10.744 / 5.961 |
| sort-reverse-512.js | 0.805 | 0.275 | 18.127 / 15.963 / 7.311 | 13.010 / 10.824 / 5.992 |
| sort-sorted-512.js | 0.728 | 0.275 | 17.588 / 15.410 / 7.188 | 13.092 / 10.887 / 6.037 |
| to-sorted-512.js | 0.807 | 0.306 | 17.979 / 15.772 / 7.053 | 12.986 / 10.774 / 6.080 |
| join-1024.js | 1.000 | 0.388 | 18.342 / 16.158 / 7.291 | 13.061 / 10.882 / 3.018 |
| join-4096.js | 2.972 | 1.214 | 18.637 / 16.438 / 3.766 | 13.367 / 11.162 / 3.492 |
| join-mixed-1024.js | 1.078 | 0.236 | 18.141 / 15.934 / 3.248 | 12.773 / 10.590 / 2.949 |
| numeric-loop-10000.js | 0.380 | 0.079 | 17.502 / 15.290 / 3.020 | 12.180 / 9.977 / 2.283 |
| numeric-loop-50000.js | 0.604 | 0.177 | 17.332 / 15.140 / 3.020 | 12.086 / 9.898 / 2.287 |
| folded-string-4096.js | 0.410 | 0.566 | 17.328 / 15.125 / 3.049 | 12.359 / 10.177 / 2.301 |
| dynamic-string-4096.js | 0.516 | 0.211 | 17.465 / 15.254 / 3.051 | 12.379 / 10.176 / 2.283 |
| integer.js | 0.481 | 0.052 | 17.469 / 15.268 / 3.074 | 12.219 / 10.010 / 2.285 |
| objects.js | 0.851 | 0.545 | 18.152 / 15.989 / 3.215 | 13.111 / 10.912 / 3.070 |
| retained-objects.js | 16.545 | 27.699 | 23.475 / 21.283 / 8.531 | 18.855 / 16.660 / 8.812 |
| join-empty-1024.js | 0.625 | 0.206 | 17.791 / 15.610 / 3.035 | 12.982 / 10.783 / 2.865 |
| join-singleton-1024.js | 0.624 | 0.102 | 17.875 / 15.691 / 3.037 | 12.330 / 10.148 / 2.287 |
| join-small-1024.js | 0.794 | 0.139 | 17.961 / 15.749 / 3.115 | 12.410 / 10.207 / 2.293 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
