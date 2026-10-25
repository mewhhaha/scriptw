# JavaScript execution memory, 2026-10-25

Source [`9b7746531`](https://github.com/mewhhaha/scriptw/commit/9b7746531c4b3bff0ecbb863d7d70dfead12455b) removes a redundant allocation of immutable raw runtime Wasm from Cargo-owned build packages. The runtime now borrows the validated static package payload under its existing build-output authority. External packages and source-free Portable bundles retain independently owned admitted bytes. The [runtime byte-ownership contract](contracts/runtime-byte-ownership.md) records that lifetime boundary.

The accepted baseline is the actual **clean `ab97459e5` release**, with its original three frozen executable hashes and Cargo producer receipts. Candidate native tests, products, selected Test262 and measurements all identify the clean source above. Wasm layout, foreign ABI and the default Fast/16-MiB/SinglePass policy are unchanged.

The mapped runtime contains **40,496,440 raw Wasm bytes (38.62 MiB)**. Borrowing removes a second host allocation of those bytes where Cargo owns the package. It does not eliminate the native runtime image, the Wasmtime code mapping, Realm state or GC storage. The external-bundle call cohort remains owned; its costs are retained rather than relabelled as borrowed-Cargo measurements.

## What consumes memory before the workload

At the acknowledged `startup.js` guest-start boundary, after Realm bootstrap but before the body, the baseline's median process RSS is **331.19 MiB**. Mapping groups show **153.00 MiB** in other anonymous ranges, **116.11 MiB** in file-backed ranges and **62.03 MiB** in anonymous executable ranges. Kernel `[heap]`/`[stack]` groups remain separate. These are process mappings, not a measured JS heap or precise component-allocation totals.

The mapped R text spans 61.37 MiB; the admitted serialized Portable native image is 78.67 MiB. R text and the serialized image are different extents. Intersections retain whole mappings without inventing proportional RSS per function. Virtual reservations, observed residency and GC capacity remain separate.

## A separately measured remaining cost: Intl data

A standalone helper linked the actual default-feature optimized `scriptw-intl` rlib, without an Engine, Wasmtime or JavaScript execution. A counting `System` GlobalAlloc wrapper records successful requested sizes and releases. Four snapshots bracket Minimal selection construction, resolution of all 12 services and their identity, and dropping local selection/frame leases. The final snapshot retains **58.063 MiB of requested-live Rust bytes**, with a **65.201-MiB cumulative logical requested-live high-water**. The 12 serialized frame lengths total **21.382 MiB**. The final allocation state is unchanged by dropping local leases, consistent with retained provider/component OnceLocks.

These are requested Rust allocation counters in a separate diagnostic process, not JavaScript bytes, allocator usable/resident size, RSS or OS allocation totals. Reallocation accounting includes each full successful new request and releases its old size; its logical high-water excludes allocator-internal overlap and native allocations bypassing GlobalAlloc. Frame lengths are serialized payload extents, not separately attributed resident allocations. This result does **not** assign any whole 64-MiB anonymous execution mapping to Intl. Actual Cargo producer metadata, rlib/executable SHA-256 values, source, commands and all transcripts remain archived; the executable and rlib payloads do not.

## Acknowledged mapping comparison

Four mapping rounds run serially in **baseline–candidate–candidate–baseline** order. Each round uses three checked controls, three measured invocations after one warmup, and guest-start/body-finished callbacks. The parent reads and saves `/proc/PID/smaps` while the real Wasm child awaits acknowledgement. Each product has six snapshots per case/boundary; the table uses their median RSS. Native R/P address/hash records, source transformations, original source bytes, checked outputs and every raw snapshot remain archived. Callback holds and snapshot reads make these attribution diagnostics, **not ordinary execution scores**.

| Workload | Boundary | Before RSS MiB | After RSS MiB | Change |
| --- | --- | ---: | ---: | ---: |
| `startup.js` | guest-start | 331.19 | 292.14 | -11.8% |
| `startup.js` | guest-body-finished | 331.19 | 292.14 | -11.8% |
| `integer.js` | guest-start | 332.21 | 292.80 | -11.9% |
| `integer.js` | guest-body-finished | 334.21 | 294.80 | -11.8% |
| `retained-objects.js` | guest-start | 331.92 | 293.28 | -11.6% |
| `retained-objects.js` | guest-body-finished | 357.93 | 319.29 | -10.8% |

The original unreceipted baseline preflight remains separately preserved: its actual runner, sources, checksums and mappings are checked, but it supplies no accepted mapping samples and no retrospective kernel cap/OOM claim. Its print control already has **330.99 MiB** resident at guest-start, with anonymous/file-backed/anonymous-executable group medians **153.00 / 115.91 / 62.03 MiB**. Those three-sample preflight medians are diagnostic context, separate from the accepted table above. All four accepted rounds have their own complete resource receipts.

| Original preflight control | Guest-start RSS MiB | Body-finished RSS MiB |
| --- | ---: | ---: |
| `startup.js` | 330.99 | 330.99 |
| `integer.js` | 332.00 | 334.00 |
| `retained-objects.js` | 331.35 | 357.36 |

## Fresh-Realm performance and memory

All 17 ordinary workload bodies run in separate fresh-process rounds, again in baseline–candidate–candidate–baseline order. Each runtime has seven measured fresh Store/Realm or V8 contexts after ten warmups per round. Tables pool the 14 measured observations per product; V8 uses its paired candidate rounds. Construction, checksums and completion stay in execution; setup and native compilation are recorded separately. Every control and slowdown is retained. Shared-host activity/load averages remain recorded; this is descriptive evidence rather than statistical significance or universal V8 parity.

| Workload | Before ms | After ms | Change | Paired V8 ms |
| --- | ---: | ---: | ---: | ---: |
| `sort-reverse-128.js` | 2.455 | 2.371 | -3.4% | 0.017 |
| `sort-reverse-512.js` | 8.940 | 8.576 | -4.1% | 0.087 |
| `sort-sorted-512.js` | 7.143 | 6.753 | -5.5% | 0.086 |
| `to-sorted-512.js` | 7.371 | 7.118 | -3.4% | 0.088 |
| `join-1024.js` | 8.210 | 7.903 | -3.7% | 0.045 |
| `join-4096.js` | 34.115 | 29.364 | -13.9% | 0.603 |
| `join-mixed-1024.js` | 8.932 | 8.402 | -5.9% | 0.180 |
| `numeric-loop-10000.js` | 1.494 | 1.454 | -2.7% | 0.173 |
| `numeric-loop-50000.js` | 4.266 | 4.154 | -2.6% | 0.744 |
| `folded-string-4096.js` | 1.899 | 1.869 | -1.6% | 0.341 |
| `dynamic-string-4096.js` | 2.927 | 2.951 | +0.8% | 0.436 |
| `integer.js` | 4.723 | 4.465 | -5.5% | 1.689 |
| `objects.js` | 5.218 | 4.912 | -5.9% | 0.047 |
| `retained-objects.js` | 409.021 | 398.091 | -2.7% | 2.302 |
| `join-empty-1024.js` | 2.698 | 2.554 | -5.3% | 0.137 |
| `join-singleton-1024.js` | 2.765 | 2.702 | -2.3% | 0.112 |
| `join-small-1024.js` | 3.442 | 3.465 | +0.7% | 0.194 |

The next table reports medians of **post-execution process RSS endpoints**, the same values used by the current README bar chart. It includes runtime/setup and retained warmup effects, and is neither whole-child peak RSS nor live JavaScript bytes. Sampled maximum RSS and GC-capacity observations remain in the machine-readable evidence; they are distinct counters and are not silently substituted for endpoints.

| Workload | Before RSS MiB | After RSS MiB | Change | Paired V8 RSS MiB |
| --- | ---: | ---: | ---: | ---: |
| `sort-reverse-128.js` | 336.21 | 297.11 | -11.6% | 52.67 |
| `sort-reverse-512.js` | 344.11 | 305.18 | -11.3% | 53.61 |
| `sort-sorted-512.js` | 344.18 | 305.12 | -11.3% | 53.60 |
| `to-sorted-512.js` | 344.53 | 305.28 | -11.4% | 53.62 |
| `join-1024.js` | 346.14 | 306.64 | -11.4% | 53.86 |
| `join-4096.js` | 343.79 | 304.96 | -11.3% | 61.56 |
| `join-mixed-1024.js` | 344.66 | 305.20 | -11.4% | 59.86 |
| `numeric-loop-10000.js` | 329.73 | 290.79 | -11.8% | 59.04 |
| `numeric-loop-50000.js` | 329.88 | 290.78 | -11.9% | 61.13 |
| `folded-string-4096.js` | 331.45 | 292.80 | -11.7% | 59.34 |
| `dynamic-string-4096.js` | 329.93 | 291.11 | -11.8% | 60.80 |
| `integer.js` | 329.94 | 291.08 | -11.8% | 59.90 |
| `objects.js` | 340.37 | 301.29 | -11.5% | 53.72 |
| `retained-objects.js` | 355.94 | 317.35 | -10.8% | 71.09 |
| `join-empty-1024.js` | 334.38 | 295.03 | -11.8% | 58.53 |
| `join-singleton-1024.js` | 334.42 | 294.90 | -11.8% | 59.44 |
| `join-small-1024.js` | 333.49 | 295.20 | -11.5% | 57.36 |

## Separate existing-Realm calls

The second 17-case ABBA cohort loads an independently admitted source-free Portable bundle into each fresh process. Generated wrappers preserve the original workload bodies and inputs; their final print expression becomes the checked return. Ten guest warmups precede seven callback-exit-to-entry intervals per session; two sessions per product give 14 elapsed calls. Construction/checksum work remains inside each call. Setup/bootstrap, printing, source-free bundle/cache admission and native compilation stay outside; no empty-control subtraction is made. These wrappers and boundaries differ from the ordinary fresh-Realm corpus. External bundle payloads remain owned in this implementation.

| Workload | Before ms | After ms | Change | Paired V8 ms |
| --- | ---: | ---: | ---: | ---: |
| `sort-reverse-128.js` | 1.954 | 1.952 | -0.1% | 0.006 |
| `sort-reverse-512.js` | 7.851 | 10.946 | +39.4% | 0.012 |
| `sort-sorted-512.js` | 5.948 | 6.041 | +1.6% | 0.012 |
| `to-sorted-512.js` | 6.262 | 6.310 | +0.8% | 0.012 |
| `join-1024.js` | 6.890 | 6.715 | -2.5% | 0.027 |
| `join-4096.js` | 29.284 | 28.417 | -3.0% | 0.032 |
| `join-mixed-1024.js` | 7.486 | 7.241 | -3.3% | 0.028 |
| `numeric-loop-10000.js` | 0.701 | 0.658 | -6.2% | 0.006 |
| `numeric-loop-50000.js` | 3.377 | 3.265 | -3.3% | 0.031 |
| `folded-string-4096.js` | 1.182 | 1.086 | -8.2% | 0.029 |
| `dynamic-string-4096.js` | 2.085 | 2.001 | -4.0% | 0.005 |
| `integer.js` | 2.617 | 2.809 | +7.3% | 1.075 |
| `objects.js` | 3.784 | 3.914 | +3.4% | 0.014 |
| `retained-objects.js` | 92.408 | 99.548 | +7.7% | 0.215 |
| `join-empty-1024.js` | 2.185 | 2.291 | +4.8% | 0.005 |
| `join-singleton-1024.js` | 2.307 | 2.280 | -1.2% | 0.024 |
| `join-small-1024.js` | 3.091 | 3.047 | -1.4% | 0.082 |

Several existing-Realm controls were slower: reverse-512 sort by **+39.4%**, retained objects by **+7.7%**, and integer by **+7.3%** in the pooled medians. Reverse-sort's adjacent round changes were +92.3% and +0.2%; retained objects were +5.5% and +7.1%. The shared host and round variation prevent a causal attribution to the ownership change. These observations do not establish unchanged call performance.

Whole-child RSS for this cohort includes setup/warmups and remains separate from invocation endpoints. Raw P/R bundles, V8 cachedData and executables are excluded from this archive; their actual hashes, framing metadata, original/transformed sources and regeneration commands remain.

## Verification and resources

The final source passed **5,718 native checks across 23 complete affected targets**, with no failures or ignored tests. The engine library uses an exact disjoint union of bounded fresh processes. Focused runtime ownership checks overlap those target totals and are not added again.

The selected original Test262 replay completed **984 executions: 980 Success / 4 known Bug / 0 Crash / 0 NotImplemented**, with zero timeouts and actual exit **1**. Complete module-code and Function apply/bind/call families plus four explicit failure controls reconcile to the final compiler's full **102,994-ID** inventory. The same four diagnostic hashes and pinned fixture bytes are retained. This is selected acceptance; it does not refresh the full Test262 chart.

Every accepted build, native run, replay, map round and benchmark receipt records one CPU, a kernel-enforced **4-GiB cap**, zero swap, grouped OOM policy, serial pools and a one-entry/64-MiB retained module cache. No accepted receipt reports OOM. Additional/partial observations, if any, are separately retained with their actual identities and outcomes.

| Native target | Passed |
| --- | ---: |
| `scriptw-aot-wasm` library | 474 |
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

[Machine-readable results](execution-memory-20261025.json) and [raw evidence/reproduction scripts](execution-memory-20261025.tar.gz) retain exact commands, complete inventories, pins, actual executable/source identities and all observations. The archive SHA-256 is `3e18aa86ce7703e935f7a5c6dcfe1a628f98817a567b6f56a6b42d3ce0585e52`; compiler/test executables, caches and unused raw bundle payloads are excluded.

To refresh, check out the measured source, perform the archived serial build/focus/native plan under `python3 -B scripts/limited_verification.py --`, freeze the three actual ordinary release products, validate the complete CLI inventory and replay `selected.ids` with one worker/reused native workers and the original 60-second deadlines. Run `measure_maps_all.py` for four acknowledged mapping rounds; run `measure_all.py` for separate fresh/call ABBA cohorts. Re-derive both analyses from raw observations. `publish_evidence.py` validates only by default; `--publish` requires every closed input and refuses to overwrite an existing checkpoint. The README graph can be reproduced with `scripts/render-execution-performance.py --report docs/rust-rewrite/execution-memory-20261025.json`. Existing receipts remain past observations, never new executions.
