# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-29T23:13:41.625735+00:00 through 2026-10-29T23:15:06.482739+00:00; source `f19370fcf91018a7153cf9d8a4b38fdcf3f5f3df`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

<table>
<tr>
<td><img src="assets/precompiled/sort-reverse-128.svg" width="440" alt="sort-reverse-128.js; Scriptw: 0.496 ms, 17.9 MiB RSS; Scriptc: 0.138 ms, 12.9 MiB RSS"></td>
<td><img src="assets/precompiled/sort-reverse-512.svg" width="440" alt="sort-reverse-512.js; Scriptw: 0.966 ms, 18.1 MiB RSS; Scriptc: 0.365 ms, 13.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/sort-sorted-512.svg" width="440" alt="sort-sorted-512.js; Scriptw: 0.874 ms, 18.0 MiB RSS; Scriptc: 0.366 ms, 13.0 MiB RSS"></td>
<td><img src="assets/precompiled/to-sorted-512.svg" width="440" alt="to-sorted-512.js; Scriptw: 0.960 ms, 18.0 MiB RSS; Scriptc: 0.407 ms, 12.9 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-1024.svg" width="440" alt="join-1024.js; Scriptw: 1.227 ms, 18.4 MiB RSS; Scriptc: 0.513 ms, 12.8 MiB RSS"></td>
<td><img src="assets/precompiled/join-4096.svg" width="440" alt="join-4096.js; Scriptw: 3.944 ms, 18.7 MiB RSS; Scriptc: 1.717 ms, 13.3 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-mixed-1024.svg" width="440" alt="join-mixed-1024.js; Scriptw: 1.361 ms, 18.0 MiB RSS; Scriptc: 0.322 ms, 12.6 MiB RSS"></td>
<td><img src="assets/precompiled/numeric-loop-10000.svg" width="440" alt="numeric-loop-10000.js; Scriptw: 0.388 ms, 16.9 MiB RSS; Scriptc: 0.094 ms, 12.1 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/numeric-loop-50000.svg" width="440" alt="numeric-loop-50000.js; Scriptw: 0.635 ms, 17.2 MiB RSS; Scriptc: 0.213 ms, 12.2 MiB RSS"></td>
<td><img src="assets/precompiled/folded-string-4096.svg" width="440" alt="folded-string-4096.js; Scriptw: 0.429 ms, 16.9 MiB RSS; Scriptc: 0.773 ms, 12.2 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/dynamic-string-4096.svg" width="440" alt="dynamic-string-4096.js; Scriptw: 0.563 ms, 17.3 MiB RSS; Scriptc: 0.270 ms, 12.0 MiB RSS"></td>
<td><img src="assets/precompiled/integer.svg" width="440" alt="integer.js; Scriptw: 0.369 ms, 17.5 MiB RSS; Scriptc: 0.063 ms, 12.2 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/objects.svg" width="440" alt="objects.js; Scriptw: 1.005 ms, 17.9 MiB RSS; Scriptc: 0.741 ms, 12.9 MiB RSS"></td>
<td><img src="assets/precompiled/retained-objects.svg" width="440" alt="retained-objects.js; Scriptw: 19.593 ms, 23.5 MiB RSS; Scriptc: 39.283 ms, 18.8 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-empty-1024.svg" width="440" alt="join-empty-1024.js; Scriptw: 0.729 ms, 17.2 MiB RSS; Scriptc: 0.276 ms, 12.8 MiB RSS"></td>
<td><img src="assets/precompiled/join-singleton-1024.svg" width="440" alt="join-singleton-1024.js; Scriptw: 0.771 ms, 17.6 MiB RSS; Scriptc: 0.129 ms, 12.2 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/precompiled/join-small-1024.svg" width="440" alt="join-small-1024.js; Scriptw: 1.000 ms, 17.8 MiB RSS; Scriptc: 0.190 ms, 12.2 MiB RSS"></td>
<td></td>
</tr>
</table>

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 0.496 | 0.138 | 17.941 / 15.886 / 3.053 | 12.852 / 10.791 / 2.953 |
| sort-reverse-512.js | 0.966 | 0.365 | 18.080 / 15.994 / 3.123 | 12.992 / 10.942 / 3.006 |
| sort-sorted-512.js | 0.874 | 0.366 | 18.018 / 15.927 / 3.119 | 12.963 / 10.866 / 3.000 |
| to-sorted-512.js | 0.960 | 0.407 | 17.996 / 15.956 / 3.137 | 12.883 / 10.820 / 3.012 |
| join-1024.js | 1.227 | 0.513 | 18.414 / 16.354 / 3.184 | 12.850 / 10.812 / 3.018 |
| join-4096.js | 3.944 | 1.717 | 18.650 / 16.559 / 3.705 | 13.295 / 11.237 / 3.494 |
| join-mixed-1024.js | 1.361 | 0.322 | 18.021 / 15.930 / 3.184 | 12.598 / 10.534 / 2.951 |
| numeric-loop-10000.js | 0.388 | 0.094 | 16.881 / 14.812 / 2.965 | 12.125 / 10.039 / 2.287 |
| numeric-loop-50000.js | 0.635 | 0.213 | 17.152 / 15.063 / 2.965 | 12.213 / 10.127 / 2.287 |
| folded-string-4096.js | 0.429 | 0.773 | 16.912 / 14.835 / 2.992 | 12.186 / 10.089 / 2.303 |
| dynamic-string-4096.js | 0.563 | 0.270 | 17.326 / 15.269 / 3.000 | 11.957 / 9.896 / 2.289 |
| integer.js | 0.369 | 0.063 | 17.527 / 15.441 / 3.014 | 12.184 / 10.112 / 2.285 |
| objects.js | 1.005 | 0.741 | 17.885 / 15.821 / 3.156 | 12.922 / 10.842 / 3.068 |
| retained-objects.js | 19.593 | 39.283 | 23.473 / 21.409 / 8.484 | 18.779 / 16.699 / 8.816 |
| join-empty-1024.js | 0.729 | 0.276 | 17.174 / 15.082 / 2.980 | 12.758 / 10.718 / 2.867 |
| join-singleton-1024.js | 0.771 | 0.129 | 17.588 / 15.547 / 2.980 | 12.199 / 10.134 / 2.287 |
| join-small-1024.js | 1.000 | 0.190 | 17.775 / 15.682 / 3.062 | 12.184 / 10.102 / 2.297 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
