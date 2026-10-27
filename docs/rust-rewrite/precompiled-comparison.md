# Compiler-free Scriptw / Scriptc comparison

Measured 2026-10-26T22:45:16.314790+00:00 through 2026-10-26T22:46:54.793944+00:00; source `99a4ff2e962b6efa8066a48447f3c077981c59ec`.

Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. Both application modules use size-optimized native compilation with backtracking register allocation and the same generic target/backtrace policy. Separate producer processes compile the original JavaScript to Wasm and serialize native images. Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.

Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific completion representations differ. Phase totals include the process-memory probes and represent instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.

RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each Store remains alive. They include native code, host/setup and retained warmup history; they are not live JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 2.057 | 0.182 | 27.654 / 25.494 / 13.787 | 14.143 / 11.962 / 7.586 |
| sort-reverse-512.js | 7.050 | 0.369 | 31.990 / 29.858 / 17.855 | 14.127 / 11.910 / 7.559 |
| sort-sorted-512.js | 5.756 | 0.369 | 29.742 / 27.615 / 15.924 | 14.264 / 12.054 / 7.637 |
| to-sorted-512.js | 8.354 | 0.387 | 34.965 / 32.833 / 19.996 | 14.191 / 11.990 / 7.600 |
| join-1024.js | 9.974 | 0.468 | 35.096 / 32.945 / 19.877 | 14.203 / 11.993 / 7.609 |
| join-4096.js | 38.547 | 1.384 | 41.035 / 38.885 / 21.180 | 14.617 / 12.412 / 8.049 |
| join-mixed-1024.js | 10.389 | 0.309 | 34.514 / 32.366 / 15.193 | 14.117 / 11.930 / 4.926 |
| numeric-loop-10000.js | 0.956 | 0.162 | 24.648 / 22.502 / 7.270 | 13.520 / 11.312 / 4.266 |
| numeric-loop-50000.js | 2.961 | 0.304 | 24.496 / 22.352 / 7.168 | 13.465 / 11.264 / 4.268 |
| folded-string-4096.js | 1.688 | 0.673 | 24.811 / 22.685 / 7.189 | 13.500 / 11.308 / 4.281 |
| dynamic-string-4096.js | 1.822 | 0.302 | 25.061 / 22.935 / 7.189 | 13.514 / 11.303 / 4.266 |
| integer.js | 2.885 | 0.128 | 25.328 / 23.173 / 7.197 | 13.434 / 11.249 / 4.266 |
| objects.js | 5.436 | 0.673 | 30.174 / 28.035 / 11.227 | 14.215 / 12.010 / 5.041 |
| retained-objects.js | 153.812 | 32.320 | 40.904 / 38.802 / 21.201 | 19.982 / 17.827 / 10.789 |
| join-empty-1024.js | 3.093 | 0.291 | 27.219 / 25.098 / 9.248 | 14.045 / 11.835 / 4.838 |
| join-singleton-1024.js | 3.265 | 0.180 | 27.420 / 25.271 / 9.184 | 13.508 / 11.292 / 4.266 |
| join-small-1024.js | 4.837 | 0.221 | 26.953 / 24.832 / 9.180 | 13.484 / 11.306 / 4.271 |

A failure in either round removes that runtime’s bars for the case; all original failure streams remain. Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain the Rust API’s explicit unsafe provenance requirement.

[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)

[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). The archive excludes executables, serialized native images and caches, retaining their identities, inert framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.

Validate or regenerate without running native code:

```sh
python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json
```
