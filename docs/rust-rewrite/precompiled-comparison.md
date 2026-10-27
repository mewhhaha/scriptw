# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-27T06:13:02.721847+00:00 through 2026-10-27T06:14:26.501343+00:00; source `eb9ff7b0552e2b57a8c6b393ee99540b664eb615`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.300 | 0.137 | 21.156 / 19.028 / 9.178 | 14.209 / 12.006 / 7.619 |
| sort-reverse-512.js | 4.131 | 0.304 | 23.750 / 21.608 / 11.785 | 14.322 / 12.112 / 7.666 |
| sort-sorted-512.js | 2.123 | 0.299 | 21.396 / 19.253 / 9.396 | 14.244 / 12.042 / 7.645 |
| to-sorted-512.js | 4.990 | 0.325 | 24.814 / 22.691 / 12.389 | 14.258 / 12.055 / 7.662 |
| join-1024.js | 2.109 | 0.395 | 21.410 / 19.289 / 8.879 | 14.244 / 12.034 / 7.693 |
| join-4096.js | 6.713 | 1.209 | 22.998 / 20.869 / 10.541 | 14.723 / 12.517 / 8.164 |
| join-mixed-1024.js | 2.337 | 0.262 | 21.096 / 18.948 / 8.836 | 14.135 / 11.939 / 7.514 |
| numeric-loop-10000.js | 1.328 | 0.105 | 19.477 / 17.349 / 8.479 | 13.611 / 11.401 / 6.990 |
| numeric-loop-50000.js | 4.555 | 0.205 | 19.381 / 17.238 / 8.352 | 13.568 / 11.364 / 6.906 |
| folded-string-4096.js | 1.826 | 0.589 | 19.775 / 17.633 / 8.496 | 13.623 / 11.417 / 7.035 |
| dynamic-string-4096.js | 2.168 | 0.238 | 19.836 / 17.710 / 4.771 | 13.482 / 11.331 / 4.271 |
| integer.js | 3.066 | 0.081 | 20.070 / 17.943 / 4.410 | 13.520 / 11.309 / 4.273 |
| objects.js | 1.741 | 0.559 | 21.043 / 18.922 / 4.855 | 14.172 / 11.975 / 5.059 |
| retained-objects.js | 45.811 | 27.738 | 27.383 / 25.260 / 10.598 | 19.979 / 17.804 / 10.797 |
| join-empty-1024.js | 3.148 | 0.231 | 22.199 / 20.055 / 6.322 | 14.156 / 11.950 / 4.850 |
| join-singleton-1024.js | 3.295 | 0.129 | 22.053 / 19.909 / 6.410 | 13.441 / 11.282 / 4.271 |
| join-small-1024.js | 3.482 | 0.168 | 22.096 / 19.995 / 6.434 | 13.561 / 11.350 / 4.281 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
