# Runtime-admission memory checkpoint — 2026-10-26

Actual candidate [25909ef75](https://github.com/mewhhaha/scriptw/commit/25909ef750d487e8ae257cdce909635d64196995) versus published clean baseline [44e17ee4c](https://github.com/mewhhaha/scriptw/commit/44e17ee4c73eeab6cc970804199109aa2aee51a9).

The batch uses compact immutable Cargo runtime metadata, typed Locale/NumberFormat/PluralRules consumers, and shared immutable undefined values for native defaults. External/custom inputs retain checked admission, and source methods retain their home-object identity. The [byte-ownership contract](contracts/runtime-byte-ownership.md) describes the boundaries.

Each ordinary cohort contains all 17 workloads, with two rounds per product in ABBA order and seven measured observations after ten warmups. Fresh Store/Realm execution and existing-Realm calls remain separate. Construction/checksums stay inside execution; V8 is paired with candidate rounds. Shared-host observations are descriptive.

| Workload | Scriptw before ms | Scriptw after ms | Change | Paired V8 ms |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 2.474 | 2.393 | -3.3% | 0.019 |
| sort-reverse-512.js | 9.499 | 9.348 | -1.6% | 0.100 |
| sort-sorted-512.js | 7.564 | 7.278 | -3.8% | 0.103 |
| to-sorted-512.js | 7.734 | 7.529 | -2.6% | 0.105 |
| join-1024.js | 8.698 | 8.678 | -0.2% | 0.050 |
| join-4096.js | 33.770 | 34.340 | +1.7% | 0.263 |
| join-mixed-1024.js | 9.357 | 9.269 | -0.9% | 0.217 |
| numeric-loop-10000.js | 1.309 | 1.303 | -0.4% | 0.350 |
| numeric-loop-50000.js | 4.430 | 4.940 | +11.5% | 0.875 |
| folded-string-4096.js | 1.846 | 1.857 | +0.6% | 0.498 |
| dynamic-string-4096.js | 3.010 | 3.127 | +3.9% | 0.375 |
| integer.js | 4.954 | 5.393 | +8.9% | 2.091 |
| objects.js | 5.185 | 4.866 | -6.1% | 0.053 |
| retained-objects.js | 434.434 | 392.023 | -9.8% | 2.723 |
| join-empty-1024.js | 2.651 | 2.619 | -1.2% | 0.159 |
| join-singleton-1024.js | 2.773 | 2.731 | -1.5% | 0.182 |
| join-small-1024.js | 3.630 | 3.629 | -0.0% | 0.220 |

Ordinary post-execution process RSS, median of 14 endpoints per product/runtime. Wasmtime samples while the Store remains alive; V8 samples after execution while its context remains alive. Host/runtime, setup and retained warmup effects remain included. These are neither live JavaScript heap bytes nor whole-child high-water RSS.

| Workload | Before MiB | After MiB | Paired V8 MiB |
| --- | ---: | ---: | ---: |
| sort-reverse-128.js | 126.945 | 89.133 | 52.777 |
| sort-reverse-512.js | 138.744 | 98.867 | 53.672 |
| sort-sorted-512.js | 138.311 | 99.066 | 53.643 |
| to-sorted-512.js | 138.770 | 98.516 | 53.664 |
| join-1024.js | 138.621 | 98.818 | 53.896 |
| join-4096.js | 139.014 | 98.912 | 61.275 |
| join-mixed-1024.js | 138.791 | 98.742 | 59.863 |
| numeric-loop-10000.js | 121.461 | 80.465 | 59.199 |
| numeric-loop-50000.js | 119.273 | 79.182 | 61.084 |
| folded-string-4096.js | 122.699 | 82.613 | 59.189 |
| dynamic-string-4096.js | 121.666 | 81.746 | 60.682 |
| integer.js | 121.613 | 81.691 | 59.988 |
| objects.js | 134.150 | 89.119 | 53.859 |
| retained-objects.js | 150.627 | 99.732 | 73.053 |
| join-empty-1024.js | 125.564 | 85.080 | 58.484 |
| join-singleton-1024.js | 125.488 | 85.186 | 59.488 |
| join-small-1024.js | 125.846 | 85.510 | 60.676 |

Separate existing-Realm calls; all slowdowns remain visible.

| Workload | Before ms | After ms | Change | Paired V8 ms |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 2.280 | 1.791 | -21.4% | 0.004 |
| sort-reverse-512.js | 8.691 | 8.683 | -0.1% | 0.014 |
| sort-sorted-512.js | 7.083 | 6.853 | -3.2% | 0.013 |
| to-sorted-512.js | 7.255 | 7.229 | -0.4% | 0.014 |
| join-1024.js | 9.028 | 7.630 | -15.5% | 0.023 |
| join-4096.js | 32.709 | 31.657 | -3.2% | 0.033 |
| join-mixed-1024.js | 8.557 | 8.270 | -3.4% | 0.032 |
| numeric-loop-10000.js | 0.790 | 0.783 | -0.9% | 0.007 |
| numeric-loop-50000.js | 3.903 | 3.938 | +0.9% | 0.036 |
| folded-string-4096.js | 1.294 | 1.299 | +0.4% | 0.058 |
| dynamic-string-4096.js | 2.361 | 2.341 | -0.8% | 0.005 |
| integer.js | 3.173 | 3.284 | +3.5% | 1.325 |
| objects.js | 4.384 | 4.324 | -1.4% | 0.013 |
| retained-objects.js | 103.846 | 102.965 | -0.8% | 0.168 |
| join-empty-1024.js | 2.586 | 2.523 | -2.4% | 0.017 |
| join-singleton-1024.js | 2.651 | 2.608 | -1.7% | 0.026 |
| join-small-1024.js | 3.525 | 3.498 | -0.8% | 0.090 |

A separate Intl.NumberFormat first-use fixture contributes zero scored cases. Observation 0 is the first warmup, followed by nine warmups and seven steady observations. The baseline initialized its whole selected Minimal provider; the candidate uses typed consumers for 11 operations (three Locale transforms, four NumberFormat and four PluralRules), while the remaining 35 operations still initialize the complete provider. Wasmtime endpoints bracket invocation admission/mapping/setup and execution; V8 endpoints bracket JavaScript. Endpoint changes do not isolate provider heap bytes or establish synchronized live-heap usage.

| Intl diagnostic | First warmup RSS MiB before → after | Steady median RSS MiB after |
| --- | ---: | ---: |
| baseline / wasmtime | 75.934 → 214.586 | 214.332 |
| baseline / v8 | 49.875 → 57.438 | 59.688 |
| candidate / wasmtime | 37.258 → 110.332 | 110.523 |
| candidate / v8 | 50.281 → 57.312 | 59.562 |

The standalone allocation diagnostic uses byte-identical helper source, separately linked for each compiler revision, and retains 35 phase pairs and 70 raw maps, including both invocations. Requested Rust allocation liveness, process residency and ordinary benchmark endpoints remain separate. Post-return helper phases occur after their Store drops; Rust GlobalAlloc counters exclude Wasmtime/native mappings, allocator usable/resident sizes and live JavaScript bytes. Native perf-map hashing is disabled for these observations. All phases and cache/identity limits are in ALLOCATION-COMPARISON.md/JSON in the archive.

Native verification passed 7,090 checks across 31 complete targets (30 nonempty). Counts come from fresh complete inventories and disjoint Engine batches. Focused overlaps, three complete rustdoc scopes and both real embedding examples are reported separately.

Selected Test262 replay: 13,858 / 13,862 Success, 4 Bug, 0 Crash, 0 NotImplemented, 0 timeouts; actual launcher exit 1. The four failure kind/detail signatures match the prior committed authority. Complete affected families and all controls are retained; this is not a full pinned Test262 refresh.

Full source tooling passed: false. Fresh source-guard baseline 86f and current aggregate/isolated source diagnostics, raw logs and stable input hashes are retained. Inherited failures remain counted; no-new-observed-diagnostic evidence does not establish full tooling acceptance.

The old checkpoint remains a committed SHA reference, including its historical OOM and source-tooling context. Its prior archive and three development campaigns are not copied into this archive. Baseline products/diagnostic keep their original paths and source identity; no old run is relabelled as the candidate. No cleanup is part of this campaign.

[Machine-readable checkpoint](execution-memory-admission-20261025.json) · [Raw evidence and reproduction code](execution-memory-admission-20261025.tar.gz). Archive SHA-256: `247e22b3216ab18cac0d5131d77d25b43ce03f070e1ace1dc3b7ae88d31f063c`.

Reproduce with the archived run_campaign.py and explicit actual clean campaign source under the one-CPU, 4 GiB, no-swap scope. Reuse the exact prior frozen baseline or reconstruct its committed authority independently. The publisher validates read-only by default and refuses overwrite on --publish. Complete Cargo producer rows are retained losslessly in mapped gzip members with decoded byte length/SHA.
