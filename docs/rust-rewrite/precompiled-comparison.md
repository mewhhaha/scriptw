# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-30T13:14:29.780341+00:00 through 2026-10-30T13:15:53.493002+00:00; source `aedaa6e4e0981462b4e31ade261ea4589a6644cf`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

<table>
<tr>
<td><img src="assets/precompiled/sort-reverse-128.svg" width="440" alt="sort-reverse-128.js; Scriptw: 0.388 ms, 17.7 MiB RSS; Scriptc: 0.138 ms, 12.9 MiB RSS"></td>
<td><img src="assets/precompiled/sort-reverse-512.svg" width="440" alt="sort-reverse-512.js; Scriptw: 0.668 ms, 17.7 MiB RSS; Scriptc: 0.372 ms, 12.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/sort-sorted-512.svg" width="440" alt="sort-sorted-512.js; Scriptw: 0.660 ms, 17.4 MiB RSS; Scriptc: 0.365 ms, 12.9 MiB RSS"></td>
<td><img src="assets/precompiled/to-sorted-512.svg" width="440" alt="to-sorted-512.js; Scriptw: 0.694 ms, 17.2 MiB RSS; Scriptc: 0.402 ms, 12.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-1024.svg" width="440" alt="join-1024.js; Scriptw: 0.982 ms, 17.5 MiB RSS; Scriptc: 0.515 ms, 13.0 MiB RSS"></td>
<td><img src="assets/precompiled/join-4096.svg" width="440" alt="join-4096.js; Scriptw: 2.957 ms, 18.4 MiB RSS; Scriptc: 1.736 ms, 13.5 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-mixed-1024.svg" width="440" alt="join-mixed-1024.js; Scriptw: 1.121 ms, 17.7 MiB RSS; Scriptc: 0.316 ms, 12.9 MiB RSS"></td>
<td><img src="assets/precompiled/numeric-loop-10000.svg" width="440" alt="numeric-loop-10000.js; Scriptw: 0.311 ms, 17.1 MiB RSS; Scriptc: 0.091 ms, 12.2 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/numeric-loop-50000.svg" width="440" alt="numeric-loop-50000.js; Scriptw: 0.382 ms, 16.4 MiB RSS; Scriptc: 0.210 ms, 12.4 MiB RSS"></td>
<td><img src="assets/precompiled/folded-string-4096.svg" width="440" alt="folded-string-4096.js; Scriptw: 0.402 ms, 17.0 MiB RSS; Scriptc: 0.752 ms, 12.3 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/dynamic-string-4096.svg" width="440" alt="dynamic-string-4096.js; Scriptw: 0.541 ms, 16.5 MiB RSS; Scriptc: 0.272 ms, 12.3 MiB RSS"></td>
<td><img src="assets/precompiled/integer.svg" width="440" alt="integer.js; Scriptw: 0.311 ms, 17.3 MiB RSS; Scriptc: 0.066 ms, 12.3 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/objects.svg" width="440" alt="objects.js; Scriptw: 0.851 ms, 17.6 MiB RSS; Scriptc: 0.755 ms, 13.0 MiB RSS"></td>
<td><img src="assets/precompiled/retained-objects.svg" width="440" alt="retained-objects.js; Scriptw: 18.848 ms, 22.6 MiB RSS; Scriptc: 39.414 ms, 18.8 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-empty-1024.svg" width="440" alt="join-empty-1024.js; Scriptw: 0.687 ms, 17.4 MiB RSS; Scriptc: 0.278 ms, 12.8 MiB RSS"></td>
<td><img src="assets/precompiled/join-singleton-1024.svg" width="440" alt="join-singleton-1024.js; Scriptw: 0.744 ms, 17.4 MiB RSS; Scriptc: 0.129 ms, 12.1 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-small-1024.svg" width="440" alt="join-small-1024.js; Scriptw: 0.953 ms, 17.5 MiB RSS; Scriptc: 0.192 ms, 12.3 MiB RSS"></td>
<td></td>
</tr>
</table>

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 0.388 | 0.138 | 17.711 / 15.626 / 7.105 | 12.930 / 10.844 / 2.947 |
| sort-reverse-512.js | 0.668 | 0.372 | 17.682 / 15.623 / 3.068 | 12.922 / 10.860 / 2.998 |
| sort-sorted-512.js | 0.660 | 0.365 | 17.426 / 15.339 / 3.066 | 12.867 / 10.805 / 2.998 |
| to-sorted-512.js | 0.694 | 0.402 | 17.242 / 15.166 / 3.113 | 12.896 / 10.813 / 3.012 |
| join-1024.js | 0.982 | 0.515 | 17.529 / 15.445 / 3.225 | 13.004 / 10.922 / 3.020 |
| join-4096.js | 2.957 | 1.736 | 18.432 / 16.352 / 3.557 | 13.523 / 11.458 / 3.494 |
| join-mixed-1024.js | 1.121 | 0.316 | 17.742 / 15.685 / 3.152 | 12.949 / 10.865 / 2.951 |
| numeric-loop-10000.js | 0.311 | 0.091 | 17.111 / 15.021 / 2.957 | 12.197 / 10.107 / 2.291 |
| numeric-loop-50000.js | 0.382 | 0.210 | 16.422 / 14.335 / 2.957 | 12.379 / 10.289 / 2.287 |
| folded-string-4096.js | 0.402 | 0.752 | 17.004 / 14.914 / 2.994 | 12.258 / 10.168 / 2.307 |
| dynamic-string-4096.js | 0.541 | 0.272 | 16.500 / 14.438 / 2.990 | 12.340 / 10.279 / 2.287 |
| integer.js | 0.311 | 0.066 | 17.293 / 15.208 / 3.035 | 12.293 / 10.254 / 2.287 |
| objects.js | 0.851 | 0.755 | 17.629 / 15.563 / 3.184 | 13.000 / 10.938 / 3.072 |
| retained-objects.js | 18.848 | 39.414 | 22.566 / 20.485 / 8.014 | 18.820 / 16.767 / 8.816 |
| join-empty-1024.js | 0.687 | 0.278 | 17.402 / 15.318 / 3.004 | 12.756 / 10.694 / 2.865 |
| join-singleton-1024.js | 0.744 | 0.129 | 17.398 / 15.309 / 3.010 | 12.094 / 10.042 / 2.289 |
| join-small-1024.js | 0.953 | 0.192 | 17.459 / 15.371 / 3.080 | 12.266 / 10.181 / 2.297 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
