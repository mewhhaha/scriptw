# JavaScript execution performance, 2026-10-25

Source [`ab97459e5`](https://github.com/mewhhaha/scriptw/commit/ab97459e54f9e0cd3ed983d098c46ccacac8fec6) changes emitted execution: stable merging for private Array sorting, geometric unpublished UTF-16 buffers for join/locale assembly, scalar Number predicates and local reads, explicit initialized-local lifecycles, and bounded literal/string addition work. [The contract](contracts/execution-hotpaths.md) records the semantic boundaries.

The comparison uses the actual frozen `4ad59d1eb` release plus its exact archived test/documentation patch, rather than relabeling it as the later checkout. The candidate is the clean product source above. CLI/runner hashes, compiler fingerprints, P/R artifact hashes and actual launcher checkout observations remain separate in the evidence.

All four observations run serially in **baseline–candidate–candidate–baseline** order. Each product has two fresh process rounds per workload, each with seven measured fresh Store/Realm or V8 contexts after ten warmups. The table pools the 14 measured samples per product and reports their median; V8 is the paired candidate observations. Separate round medians and both adjacent comparisons remain in the machine-readable results. Values include construction/checksums and completion; setup and compilation are recorded separately.

Unrelated CPU-active processes and desktop activity were observed on the shared host. Each round retains load average at start/end. These are descriptive measurements under observed ambient load, not an otherwise-idle or isolated causal comparison; they do not certify idle-machine repeatability, statistical significance or universal V8 parity. Every workload and regression is retained.

| Workload | Before ms | After ms | Change | V8 ms | After / V8 |
| --- | ---: | ---: | ---: | ---: | ---: |
| `sort-reverse-128.js` | 10.813 | 2.751 | -74.6% | 0.019 | 141.83x |
| `sort-reverse-512.js` | 139.711 | 9.861 | -92.9% | 0.098 | 101.00x |
| `sort-sorted-512.js` | 7.799 | 7.762 | -0.5% | 0.110 | 70.88x |
| `to-sorted-512.js` | 136.840 | 8.120 | -94.1% | 0.103 | 78.98x |
| `join-1024.js` | 9.897 | 8.995 | -9.1% | 0.052 | 174.06x |
| `join-4096.js` | 58.426 | 33.907 | -42.0% | 0.661 | 51.26x |
| `join-mixed-1024.js` | 9.922 | 9.676 | -2.5% | 0.203 | 47.55x |
| `numeric-loop-10000.js` | 2.149 | 1.624 | -24.4% | 0.526 | 3.09x |
| `numeric-loop-50000.js` | 7.469 | 4.873 | -34.8% | 0.877 | 5.55x |
| `folded-string-4096.js` | 4.607 | 2.171 | -52.9% | 0.155 | 14.00x |
| `dynamic-string-4096.js` | 3.614 | 3.461 | -4.3% | 0.191 | 18.15x |
| `integer.js` | 4.400 | 5.068 | +15.2% | 2.094 | 2.42x |
| `objects.js` | 5.678 | 5.608 | -1.2% | 0.056 | 100.50x |
| `retained-objects.js` | 445.283 | 431.177 | -3.2% | 2.941 | 146.63x |
| `join-empty-1024.js` | 2.901 | 2.930 | +1.0% | 0.165 | 17.77x |
| `join-singleton-1024.js` | 3.102 | 3.131 | +0.9% | 0.184 | 17.00x |
| `join-small-1024.js` | 4.173 | 3.994 | -4.3% | 0.226 | 17.65x |

The separate **existing-Realm call cohort** runs the same 17 workload bodies through a generated batch wrapper: the sole final print expression becomes the return expression, while original inputs and explicit String(checksum) conversions remain. Each process initializes one Realm, executes ten guest warmups and records seven actual callback-exit-to-callback-entry intervals; each product has two sessions in ABBA order. The table pools their 14 elapsed intervals. Construction and checksum calculation remain inside each call; setup/bootstrap, warmups, checksum printing, bundle/cache admission and native compilation are outside. There is no empty-program subtraction. These are elapsed wall intervals with marker overhead, not isolated CPU clocks, and remain distinct from the fresh-Realm table above.

| Existing-Realm workload | Before ms | After ms | Change | V8 ms | After / V8 |
| --- | ---: | ---: | ---: | ---: | ---: |
| `sort-reverse-128.js` | 9.973 | 2.241 | -77.5% | 0.004 | 577.49x |
| `sort-reverse-512.js` | 141.797 | 8.592 | -93.9% | 0.013 | 652.12x |
| `sort-sorted-512.js` | 7.003 | 6.918 | -1.2% | 0.014 | 510.39x |
| `to-sorted-512.js` | 138.951 | 7.346 | -94.7% | 0.014 | 519.16x |
| `join-1024.js` | 9.187 | 7.767 | -15.5% | 0.023 | 344.12x |
| `join-4096.js` | 57.935 | 32.573 | -43.8% | 0.033 | 986.46x |
| `join-mixed-1024.js` | 9.088 | 8.331 | -8.3% | 0.032 | 259.21x |
| `numeric-loop-10000.js` | 1.121 | 0.784 | -30.1% | 0.007 | 111.62x |
| `numeric-loop-50000.js` | 5.523 | 3.930 | -28.8% | 0.036 | 108.07x |
| `folded-string-4096.js` | 4.264 | 1.305 | -69.4% | 0.003 | 426.39x |
| `dynamic-string-4096.js` | 2.686 | 2.333 | -13.1% | 0.005 | 434.09x |
| `integer.js` | 3.944 | 3.231 | -18.1% | 1.254 | 2.58x |
| `objects.js` | 4.486 | 4.434 | -1.2% | 0.012 | 363.29x |
| `retained-objects.js` | 102.266 | 103.177 | +0.9% | 0.167 | 619.28x |
| `join-empty-1024.js` | 2.493 | 2.509 | +0.7% | 0.005 | 490.99x |
| `join-singleton-1024.js` | 2.629 | 2.729 | +3.8% | 0.030 | 90.92x |
| `join-small-1024.js` | 3.696 | 3.534 | -4.4% | 0.088 | 40.34x |

The integer/imul pooled median changed **+15.2%** at the fresh-Realm boundary and **-18.1%** in existing-Realm calls. Singleton-join calls changed **+3.8%**, and retained-object calls changed **+0.9%**. Positive changes are slower. These distinct boundaries and the small-control costs remain part of the result; no cause is inferred. Large gaps to V8 remain, and this checkpoint does not establish parity.

For the 50,000-iteration numeric workload, the median of the fresh-Realm invocation's maximum sampled process RSS rose **329.2→354.5 MiB (+7.7%)**; GC capacity stayed 16 MiB. Sampled process RSS includes setup and retained effects of warmups; it is not live JavaScript bytes or a per-call peak. This observation does not assign an allocation or GC cause.

All call results, transformed-source/original-source hashes, actual callback timestamps, session medians, adjacent comparisons, whole-child RSS and whole-program GC capacity are retained. Memory observations include setup and warmups and cannot establish per-call peak/live/allocation bytes. Raw Portable P/R bundles and V8 cachedData are excluded from the archive; their actual hashes, framing metadata, sources and regeneration commands remain.

The initial `d67cbfbf0` diagnostic exposed a missing ordinary-source path: the encoded artifact guard failed **0 passed / 1 failed**, while 15 independent focused checks passed. Ordinary source relations use general comparison IR; the corrected source consumes its actual predicate without Boolean boxing and bypasses runtime classification only for certified emitted Number results. The original artifact guard is unchanged. All 22 original focused/build receipt files and the 17-case three-sample diagnostic probe remain archived under their actual original source identities, excluded from accepted native totals and final balanced measurements.

The preceding corrected-source `4b325de34` campaign remains separate diagnostic evidence. Both complete 17-workload ABBA cohorts, their actual source/binary identities, all raw outcomes and the following tradeoffs are retained. These rows compare that earlier source with its own paired baseline observations; they do not contribute to the final source's tables or acceptance totals.

| Prior workload | Prior fresh-Realm change | Prior existing-Realm change |
| --- | ---: | ---: |
| `sort-reverse-128.js` | -73.7% | -77.2% |
| `sort-reverse-512.js` | -93.0% | -93.5% |
| `sort-sorted-512.js` | +3.7% | -0.3% |
| `to-sorted-512.js` | -93.8% | -94.7% |
| `join-1024.js` | -8.8% | -5.8% |
| `join-4096.js` | -38.8% | -43.0% |
| `join-mixed-1024.js` | -3.3% | -4.5% |
| `numeric-loop-10000.js` | -29.6% | -28.1% |
| `numeric-loop-50000.js` | -40.6% | -25.9% |
| `folded-string-4096.js` | -50.6% | -68.6% |
| `dynamic-string-4096.js` | -6.7% | -11.4% |
| `integer.js` | +19.1% | -0.6% |
| `objects.js` | -1.1% | +4.8% |
| `retained-objects.js` | -0.3% | +5.4% |
| `join-empty-1024.js` | +0.8% | +1.5% |
| `join-singleton-1024.js` | -1.2% | -0.4% |
| `join-small-1024.js` | -0.4% | -4.3% |

The predicate-only byte-patch experiment retained identical R and source metadata, inserted two canonical i32 operations into each workload version, and checked all 68 invocation outputs. Its pooled elapsed median changed **+0.9%**; it provided no measured benefit and was rejected. Both setup failures, the original missing-GNUtime attempt, exact experimental P bytes, scripts, commands and actual outcomes remain archived. No native-code cause is inferred from the earlier fresh-Realm integer variation or its flat existing-Realm result. An expanded comparison ABI remains deferred.

Native verification passed **5,717 checks across 23 complete affected targets**, with no failures or ignored tests. The engine library uses an exact disjoint union of fresh bounded processes, preserving every inventoried identity. Focused checks overlap this denominator and are not added.

The selected pinned Test262 replay completed **2,417 exact executions: 2,413 successes and the same four known runtime bugs**, with zero crashes, unsupported cases or timeouts and actual exit **1**. Eighteen complete affected families plus four explicit controls reconcile against the final CLI's complete 102,994-ID inventory. This selected checkpoint does not refresh the historical full-suite or V8 README graphs.

Every accepted native, build, benchmark and replay receipt records one CPU, a kernel-enforced **4-GiB cap**, zero swap, grouped OOM termination, serial pools and a one-entry/64-MiB retained module cache. No accepted receipt records an OOM. Process RSS and GC capacity remain distinct from live JavaScript bytes/allocation counts; the raw observations retain unavailable-counter labels.

| Native target | Passed |
| --- | ---: |
| `scriptw-aot-wasm` library | 473 |
| `scriptw-aot-wasm` emission | 32 |
| `scriptw-aot-wasm` modules_realms | 74 |
| `scriptw-aot-wasm` runtime_link | 30 |
| `scriptw-aot-wasm` structure_async | 115 |
| `scriptw-aot-wasm` structure_builtins | 166 |
| `scriptw-aot-wasm` structure_language | 152 |
| `scriptw-engine` library | 1,029 |
| `scriptw-engine` aot_async | 248 |
| `scriptw-engine` aot_async_generator_returns | 2 |
| `scriptw-engine` aot_builtins | 545 |
| `scriptw-engine` aot_gc_entries | 193 |
| `scriptw-engine` aot_generators | 167 |
| `scriptw-engine` aot_language | 347 |
| `scriptw-engine` aot_native_program_templates | 21 |
| `scriptw-engine` aot_realm_modules | 428 |
| `scriptw-engine` runtime_cache | 4 |
| `scriptw-engine` structure | 36 |
| `scriptw-engine` weak_gc_facility | 14 |
| `scriptw-runtime-image` library | 3 |
| `scriptw-ir` library | 1,628 |
| `scriptw-cli` runtime_baseline | 7 |
| `scriptw-cli` runtime_boundaries | 3 |

[Machine-readable results](execution-performance-20261025.json) retain exact commands, inventories, pins, source/binary identities, warmups, all measured observations, code sizes and memory reports. [Raw evidence and reproduction scripts](execution-performance-20261025.tar.gz) have SHA-256 `7efa80aa5fc5bff6063224ae8b13000d0da0fd1dcb8c023d24191340726e11c5`; binaries, caches and build outputs are excluded.

To refresh, check out the source commit, run serial builds and the archived `focus.py`, `engine_batches.py` and `native_final.py` under `python3 -B scripts/limited_verification.py --`, then build the ordinary CLI/runtime_baseline/runtime_boundaries products. Preserve separate immutable baseline/candidate products and run the archived `measure.py` in the balanced order above. Run `measure_calls.py` separately in its four-round ABBA order to regenerate the existing-Realm sources, Portable bundles, V8 cachedData and exact callback observations. Replay the exact archived `selected.ids` with `scripts/replay-test262-executions.py --workers 1 --reuse-case-workers` under the same limiter, retaining the original 60-second case deadlines and nonzero known-failure exit. Re-derive `analysis.json` with `analyze.py` and `analysis_calls.json` with `analysis_calls.py`; the default `publish_evidence.py` validates only and `--publish` requires every closed input. Existing receipts are audit evidence, not new executions.
