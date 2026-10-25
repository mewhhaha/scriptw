# Fixed runtime memory checkpoint — 2026-10-25

Actual candidate [44e17ee4c](https://github.com/mewhhaha/scriptw/commit/44e17ee4c73eeab6cc970804199109aa2aee51a9) versus published clean baseline [9b7746531](https://github.com/mewhhaha/scriptw/commit/9b7746531c4b3bff0ecbb863d7d70dfead12455b).

Minimal Intl providers initialize on first use. Compilation uses build-admitted metadata; Cargo native images use bounded compressed storage and, on supported filesystems, private file-backed mappings. Constant-string bootstrap uses a descriptor loop, and static RegExp descriptors copy from checked program data. The [runtime byte-ownership contract](contracts/runtime-byte-ownership.md) records those boundaries.

Ordinary fresh Store/Realm execution and transformed-source existing-Realm calls each contain all 17 workloads, two rounds per product in ABBA order, seven samples after ten warmups. Construction/checksums stay inside timed execution. V8 is paired with the candidate rounds. Shared-host results are descriptive; every slowdown remains visible.

| Workload | Scriptw before ms | Scriptw after ms | Change | Paired V8 ms |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 2.835 | 2.408 | -15.1% | 0.019 |
| sort-reverse-512.js | 10.065 | 9.675 | -3.9% | 0.111 |
| sort-sorted-512.js | 7.974 | 7.338 | -8.0% | 0.104 |
| to-sorted-512.js | 8.406 | 7.753 | -7.8% | 0.107 |
| join-1024.js | 9.372 | 8.768 | -6.4% | 0.050 |
| join-4096.js | 33.990 | 33.919 | -0.2% | 0.702 |
| join-mixed-1024.js | 9.944 | 9.284 | -6.6% | 0.148 |
| numeric-loop-10000.js | 1.657 | 1.307 | -21.2% | 0.588 |
| numeric-loop-50000.js | 4.880 | 4.427 | -9.3% | 0.895 |
| folded-string-4096.js | 2.181 | 1.840 | -15.7% | 0.169 |
| dynamic-string-4096.js | 3.410 | 3.129 | -8.3% | 0.709 |
| integer.js | 6.779 | 4.811 | -29.0% | 2.021 |
| objects.js | 9.196 | 5.222 | -43.2% | 0.053 |
| retained-objects.js | 434.939 | 439.820 | +1.1% | 3.009 |
| join-empty-1024.js | 2.970 | 2.606 | -12.3% | 0.162 |
| join-singleton-1024.js | 3.157 | 2.814 | -10.9% | 0.180 |
| join-small-1024.js | 4.028 | 3.715 | -7.8% | 0.231 |

The README graph uses only latest Scriptw versus paired V8 elapsed time and post-execution process RSS. Invocation RSS includes runtime/setup and retained warmup effects; it is neither live JavaScript memory nor a peak. Sampled maxima, GC capacity and every raw timing stay in machine-readable evidence.

Ordinary post-execution process RSS, in MiB; each entry is rederived as the median of 14 endpoints from the validated fresh analysis. Wasmtime reads its final sampler while the Store remains alive; V8 reads after execution while its context remains alive. The independent allocation diagnostic below also records post-return snapshots after its Store has dropped.

| Workload | Scriptw before MiB | Scriptw after MiB | Paired V8 MiB |
| --- | ---: | ---: | ---: |
| sort-reverse-128.js | 297.068 | 127.393 | 52.799 |
| sort-reverse-512.js | 305.082 | 137.441 | 53.676 |
| sort-sorted-512.js | 305.410 | 136.990 | 53.668 |
| to-sorted-512.js | 305.119 | 137.076 | 53.738 |
| join-1024.js | 307.076 | 137.072 | 53.902 |
| join-4096.js | 305.154 | 136.994 | 61.172 |
| join-mixed-1024.js | 305.119 | 137.135 | 59.969 |
| numeric-loop-10000.js | 290.768 | 118.025 | 59.162 |
| numeric-loop-50000.js | 290.658 | 116.566 | 61.035 |
| folded-string-4096.js | 292.912 | 121.143 | 59.277 |
| dynamic-string-4096.js | 291.180 | 119.936 | 61.146 |
| integer.js | 291.289 | 119.693 | 60.035 |
| objects.js | 301.211 | 133.289 | 53.799 |
| retained-objects.js | 317.326 | 149.588 | 71.422 |
| join-empty-1024.js | 295.135 | 124.064 | 58.609 |
| join-singleton-1024.js | 295.209 | 123.973 | 59.418 |
| join-small-1024.js | 295.209 | 123.783 | 60.172 |

Separate existing-Realm calls:

| Workload | Before ms | After ms | Change | Paired V8 ms |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 2.256 | 2.282 | +1.2% | 0.004 |
| sort-reverse-512.js | 8.998 | 9.040 | +0.5% | 0.013 |
| sort-sorted-512.js | 6.958 | 7.079 | +1.7% | 0.014 |
| to-sorted-512.js | 7.321 | 7.378 | +0.8% | 0.014 |
| join-1024.js | 7.823 | 7.872 | +0.6% | 0.024 |
| join-4096.js | 32.544 | 32.983 | +1.3% | 0.033 |
| join-mixed-1024.js | 8.571 | 8.496 | -0.9% | 0.031 |
| numeric-loop-10000.js | 0.782 | 0.777 | -0.6% | 0.007 |
| numeric-loop-50000.js | 3.946 | 3.893 | -1.3% | 0.036 |
| folded-string-4096.js | 1.332 | 1.331 | -0.1% | 0.058 |
| dynamic-string-4096.js | 2.434 | 2.369 | -2.6% | 0.005 |
| integer.js | 3.420 | 3.126 | -8.6% | 1.261 |
| objects.js | 4.590 | 4.446 | -3.1% | 0.015 |
| retained-objects.js | 103.805 | 106.117 | +2.2% | 0.164 |
| join-empty-1024.js | 2.515 | 2.543 | +1.1% | 0.005 |
| join-singleton-1024.js | 2.653 | 2.673 | +0.7% | 0.028 |
| join-small-1024.js | 3.562 | 3.517 | -1.3% | 0.087 |

A separate Intl.NumberFormat first-use fixture is excluded from both 17-case scored cohorts. It retains observation 0 as the first warmup, nine further warmups and seven steady observations. First actual use constructs the whole selected Minimal provider; its shared data can remain owned afterward. Wasmtime endpoints include invocation admission, native mapping, setup and execution; V8 endpoints bracket JavaScript execution. Their changes are not an isolated Intl heap size or synchronized live-heap measurement.

| Intl diagnostic | First warmup RSS MiB before → after | Steady median RSS MiB after |
| --- | ---: | ---: |
| baseline / wasmtime | 180.930 → 292.723 | 292.727 |
| baseline / v8 | 50.293 → 57.324 | 59.574 |
| candidate / wasmtime | 74.641 → 213.000 | 213.047 |
| candidate / v8 | 50.285 → 57.301 | 59.551 |

The allocation diagnostic uses the exact same helper source with distinct linked ELFs per source, three fresh case processes and two invocations per case. All 35 phase pairs (70 raw snapshots) are retained. It explains fixed host requested-byte liveness and process residency separately from ordinary timing/graph scores.

| Diagnostic case / phase | Requested Rust live MiB before → after | RSS MiB before → after |
| --- | ---: | ---: |
| scalar / after_compilation | 6.301 → 6.359 | 28.504 → 28.605 |
| scalar / after_execution_1 | 64.766 → 6.759 | 135.652 → 35.492 |
| scalar / after_dropping_engine | 63.699 → 5.635 | 135.117 → 34.930 |
| print / after_compilation | 64.784 → 6.723 | 179.305 → 72.699 |
| print / guest_start_1 | 66.930 → 8.866 | 291.332 → 117.043 |
| print / guest_body_finished_1 | 66.930 → 8.866 | 291.332 → 117.293 |
| print / after_execution_1 | 66.074 → 8.010 | 208.508 → 105.980 |
| print / after_dropping_engine | 65.913 → 7.849 | 208.508 → 105.902 |
| retained / after_compilation | 64.784 → 6.723 | 180.961 → 74.574 |
| retained / guest_start_1 | 66.903 → 8.840 | 292.824 → 120.172 |
| retained / guest_body_finished_1 | 66.904 → 8.841 | 318.824 → 149.672 |
| retained / after_execution_1 | 66.048 → 7.984 | 210.000 → 109.109 |
| retained / after_dropping_engine | 65.886 → 7.822 | 210.000 → 109.008 |

A separate exact RegExp native-test child passed with 175.770 MiB GNU time 1.9 high-water RSS in a fresh process and empty Scriptw disk cache. The OS page cache was not flushed. This interval includes the child’s in-process compilation and execution, excludes Cargo/LLVM build work, and contributes zero new native or scored performance cases. The earlier 356d aggregate and isolated 4 GiB kernel-OOM kills remain archived development failures with a different, compile-inclusive cgroup accounting scope.

Requested Rust GlobalAlloc sizes exclude allocator usable/resident bytes and native allocations bypassing GlobalAlloc.
Anonymous mappings do not identify live JavaScript or Intl ownership; no exact causal subtraction between counters.
Private file-backed native mappings change residency/reclaimability; cold-file/page-cache state and shared pages affect RSS/PSS.
Baseline has no saved outer JSON cap receipt; original console policy confirmation remains explicit.
One-entry/64 MiB module cache cannot retain the full R image; default SDK 512 MiB retention differs.
Native perf-map hashing is disabled for allocation and ordinary measurements. RSS/capacity/virtual address space, logical requested peaks and guest live memory remain distinct.

Actual native verification passed 7,077 checks across 31 complete targets, including 1,032 exact engine identities in disjoint fresh processes. Focused overlaps are not added.

There are 30 nonempty native targets. The scriptw-intl-build-data unit target explicitly enumerates and runs zero cases; its separate authority-token compile-fail doctest passes one case and is reported outside the native-check total.

Selected Test262 replay executed 7,746: 7,742 Success, 4 Bug, 0 Crash, 0 NotImplemented, 0 timeouts; actual launcher exit 1. This is the complete selected Intl402/module-code/Function-family union and original controls, not a full pinned Test262 refresh. Actual failure identities/details and pins remain in the evidence.

Full tooling remains red: the current aggregate module-boundary guard exits 1 with 9 exact members of the committed baseline’s 18 prefixed failures, plus an isolated terminal source-guard failure that also reproduces the baseline. All 74 source-control files were verified against their committed Git blobs. This establishes no new observed diagnostic; it does not establish full tooling acceptance. Pre-existing/failed foundation observations and the first missing-vendor-input attempt remain archived outside accepted totals. Large source snapshots are excluded. The earlier 356d development campaign suffered two real 4 GiB kernel-OOM kills (aggregate and isolated RegExp compilation). Its 6,029 checks across 25 closed targets and 64 partial aggregate test lines are preserved without entering final counts. The 3914 development stage stopped on five RegExp test-fixture construction failures before the intended semantic comparisons. Its failed receipt, successful Engine prefix and 35 allocation boundaries remain historical. The development repair filter passed all six unchanged semantic test bodies under actual 3914 plus the retained exact patch; final clean-commit verification runs independently. These development observations add zero final native, performance or accepted-resource cases.

Every optional cleanup attempt deleted zero files. Automatic approval review rejected a relaxed-liveness deletion action before launch. The user later explicitly approved the 22 targets, but that attempt found the shared build directory independently missing and failed before deletion. Its disappearance has an unknown actor and is not attributed to these cleanup attempts. Final orchestration never invokes cleanup.

The separate candidate native-map observer is code-size attribution only: it hashes/faults the entire native text. Its timing and RSS never enter the ordinary performance tables or README graph. Actual R/P text extents, raw maps, native identities and observer stdout remain archived; generated large raw Wasm payloads are excluded.

[Machine-readable checkpoint](execution-memory-fixed-costs-20261025.json) · [Raw evidence and reproduction code](execution-memory-fixed-costs-20261025.tar.gz). Archive SHA-256: `cdc96a101b3926d1bbee36dd740e6eae103c428dfcdad69107d300fcd5117265`.

Reproduce with the archived serial native/product/freeze plans, full native inventory admission and selected.ids replay under the one-CPU, 4 GiB, no-swap scope. Run the separate allocation freezer/driver and `measure_all.py`, then rederive `analyze.py` and `analysis_calls.py`. `publish_evidence.py` validates read-only by default; `--publish` requires all closed inputs and refuses overwrite. The archive includes ALLOCATION-COMPARISON.md/json for all diagnostic phases. Both historical Cargo producer JSONLs are complete deterministic gzip members; archive lossless_file_wrappers records their original logical paths, decoded byte lengths and SHA-256 values for exact recovery.
