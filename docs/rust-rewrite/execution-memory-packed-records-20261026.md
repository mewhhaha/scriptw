# Packed-record memory checkpoint — 2026-10-26

Actual candidate [736f0f120](https://github.com/mewhhaha/scriptw/commit/736f0f1206c581e70048467f7d4423e1efed8bdd) versus published clean baseline [25909ef75](https://github.com/mewhhaha/scriptw/commit/25909ef750d487e8ae257cdce909635d64196995).

GC layout 21 packs StoredValue scalar/tag/reference fields and descriptor flags. Native callable name/length values use compact storage; descriptor snapshots are materialized for reflection or mutation. Observable functions and properties remain initialized and rooted. Bounded idle Cranelift contexts retain useful small scratch and retire oversized owners at function/module boundaries. Structural bounds do not claim exact heap-byte accounting. Native zstd framing/level and decoded-native digest/backing authority remain unchanged.

Each ordinary cohort contains all 17 workloads, ABBA order, two rounds/product and seven measured observations after ten warmups. Fresh Store/Realm execution and existing-Realm calls remain separate. Construction/checksums stay inside execution; V8 is paired with candidate rounds. Shared-host observations are descriptive.

| Workload | Before ms | After ms | Change | Paired V8 ms |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 2.511 | 2.068 | -17.7% | 0.017 |
| sort-reverse-512.js | 12.111 | 7.935 | -34.5% | 0.106 |
| sort-sorted-512.js | 7.437 | 6.453 | -13.2% | 0.099 |
| to-sorted-512.js | 9.923 | 6.939 | -30.1% | 0.098 |
| join-1024.js | 10.594 | 6.985 | -34.1% | 0.049 |
| join-4096.js | 32.242 | 26.613 | -17.5% | 0.572 |
| join-mixed-1024.js | 9.057 | 7.583 | -16.3% | 0.192 |
| numeric-loop-10000.js | 1.502 | 1.071 | -28.7% | 0.124 |
| numeric-loop-50000.js | 4.618 | 3.713 | -19.6% | 0.740 |
| folded-string-4096.js | 2.146 | 1.534 | -28.5% | 0.194 |
| dynamic-string-4096.js | 3.070 | 2.507 | -18.3% | 0.195 |
| integer.js | 5.397 | 4.321 | -19.9% | 1.691 |
| objects.js | 4.673 | 3.581 | -23.4% | 0.048 |
| retained-objects.js | 478.826 | 119.557 | -75.0% | 2.896 |
| join-empty-1024.js | 2.853 | 2.089 | -26.8% | 0.159 |
| join-singleton-1024.js | 3.462 | 3.003 | -13.3% | 0.164 |
| join-small-1024.js | 4.107 | 3.023 | -26.4% | 0.292 |

Ordinary post-execution process RSS, median of 14 endpoints per product/runtime. Wasmtime samples while the Store is alive; V8 samples while its context is alive. Host/runtime/setup and warmup history remain included. RSS is neither live JavaScript bytes nor child high-water memory.

| Workload | Before MiB | After MiB | Paired V8 MiB |
| --- | ---: | ---: | ---: |
| sort-reverse-128.js | 91.902 | 87.469 | 52.760 |
| sort-reverse-512.js | 99.113 | 97.184 | 53.645 |
| sort-sorted-512.js | 99.436 | 93.279 | 53.646 |
| to-sorted-512.js | 98.416 | 95.111 | 53.646 |
| join-1024.js | 98.494 | 95.850 | 53.902 |
| join-4096.js | 99.057 | 99.953 | 61.104 |
| join-mixed-1024.js | 98.799 | 96.088 | 57.219 |
| numeric-loop-10000.js | 77.480 | 79.703 | 59.209 |
| numeric-loop-50000.js | 78.018 | 78.748 | 61.068 |
| folded-string-4096.js | 82.168 | 83.139 | 59.113 |
| dynamic-string-4096.js | 81.490 | 82.986 | 61.277 |
| integer.js | 81.617 | 82.059 | 59.982 |
| objects.js | 89.057 | 88.047 | 53.854 |
| retained-objects.js | 99.568 | 100.080 | 71.549 |
| join-empty-1024.js | 85.652 | 84.578 | 58.455 |
| join-singleton-1024.js | 85.412 | 84.514 | 59.387 |
| join-small-1024.js | 85.455 | 82.844 | 60.863 |

Separate existing-Realm calls; all slowdowns remain visible.

| Workload | Before ms | After ms | Change | Paired V8 ms |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 2.036 | 1.425 | -30.0% | 0.003 |
| sort-reverse-512.js | 9.321 | 6.702 | -28.1% | 0.013 |
| sort-sorted-512.js | 10.712 | 5.035 | -53.0% | 0.011 |
| to-sorted-512.js | 8.856 | 5.502 | -37.9% | 0.013 |
| join-1024.js | 12.556 | 5.611 | -55.3% | 0.021 |
| join-4096.js | 30.409 | 23.411 | -23.0% | 0.039 |
| join-mixed-1024.js | 7.522 | 6.064 | -19.4% | 0.027 |
| numeric-loop-10000.js | 0.793 | 0.658 | -17.0% | 0.006 |
| numeric-loop-50000.js | 3.588 | 3.263 | -9.1% | 0.031 |
| folded-string-4096.js | 2.002 | 1.115 | -44.3% | 0.002 |
| dynamic-string-4096.js | 2.472 | 2.104 | -14.9% | 0.006 |
| integer.js | 3.692 | 3.685 | -0.2% | 1.434 |
| objects.js | 6.453 | 3.389 | -47.5% | 0.013 |
| retained-objects.js | 91.851 | 119.676 | +30.3% | 0.253 |
| join-empty-1024.js | 2.152 | 2.006 | -6.8% | 0.007 |
| join-singleton-1024.js | 2.268 | 2.089 | -7.9% | 0.037 |
| join-small-1024.js | 3.034 | 3.836 | +26.4% | 0.142 |

The separate Intl.NumberFormat first-use diagnostic contributes zero scored cases. First warmup, nine remaining warmups and seven steady observations remain distinct. Its process endpoints include admission, mappings and provider/process history and cannot isolate provider heap bytes.

| Intl diagnostic | First warmup RSS MiB before → after | Steady RSS MiB after |
| --- | ---: | ---: |
| baseline / wasmtime | 37.043 → 109.848 | 110.039 |
| baseline / v8 | 50.293 → 57.305 | 59.555 |
| candidate / wasmtime | 37.320 → 109.652 | 109.844 |
| candidate / v8 | 50.293 → 57.305 | 59.555 |

A separate candidate-only precompiled diagnostic produces all 17 native images in another process. Each consumer disables its compiler, admits the exact P image and Cargo runtime, then performs ten warmups and seven measured fresh Store/Realm invocations. Setup/read/native admission and first/steady execution remain separate. A 10 ms native load RSS sampler reports a sampled maximum, which may miss a shorter transient. This remains outside ordinary scores, with no baseline result for the older API. The example ELF also contains its producer/compiler code; this is compiler-disabled execution, not a separate minimal linker build. RSS/PSS/private-dirty endpoints include process/code residency; file-backed RSS alone does not establish physical savings.

| Precompiled case | Native load sampled max RSS MiB | Transport dropped RSS MiB | Steady execution RSS / PSS MiB | Execution ms |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 111.082 | 36.434 | 52.207 / 49.739 | 1.897 |
| sort-reverse-512.js | 111.141 | 36.492 | 63.602 / 61.107 | 6.990 |
| sort-sorted-512.js | 110.934 | 36.285 | 59.434 / 57.001 | 5.590 |
| to-sorted-512.js | 111.031 | 36.504 | 61.715 / 59.282 | 5.994 |
| join-1024.js | 111.367 | 36.770 | 62.555 / 60.107 | 6.154 |
| join-4096.js | 111.430 | 36.484 | 66.637 / 64.216 | 23.358 |
| join-mixed-1024.js | 111.422 | 36.770 | 62.344 / 59.904 | 6.636 |
| numeric-loop-10000.js | 111.234 | 36.805 | 48.562 / 46.107 | 1.099 |
| numeric-loop-50000.js | 111.934 | 36.145 | 48.047 / 45.614 | 3.704 |
| folded-string-4096.js | 110.891 | 36.336 | 48.469 / 46.030 | 1.536 |
| dynamic-string-4096.js | 111.441 | 36.949 | 49.016 / 46.587 | 2.672 |
| integer.js | 111.047 | 36.609 | 49.309 / 46.849 | 4.505 |
| objects.js | 111.496 | 36.812 | 54.629 / 52.169 | 3.603 |
| retained-objects.js | 111.266 | 36.609 | 66.086 / 63.592 | 113.411 |
| join-empty-1024.js | 111.207 | 36.672 | 51.512 / 49.060 | 2.089 |
| join-singleton-1024.js | 110.910 | 36.508 | 51.410 / 48.947 | 2.174 |
| join-small-1024.js | 111.340 | 36.852 | 51.746 / 49.252 | 2.932 |

The standalone allocation diagnostic uses byte-identical helper source, separately linked for each compiler revision, and retains 35 phase pairs and 70 raw mapping snapshots. Requested Rust allocation liveness, process residency and ordinary endpoints remain distinct. GlobalAlloc excludes native mappings, allocator usable or resident sizes and live JS bytes. Static GC layout counts do not measure dynamic allocations or live bytes. ALLOCATION-COMPARISON.md/JSON preserves all phases, source identity and cache limits.

Native evidence covers 7,118 checks across 32 complete targets (31 nonempty). This is mixed-source coverage: 31 unchanged scopes executed on e36, and the complete corrected structure_async target executed on final source. Original source commits, raw receipts, exact inventories and the three-modified/one-added test-file equivalence proof remain explicit. All nine e36 focused groups overlap complete inventories and add zero counts. Rustdoc/example checks are separate. This is not a fresh full native sweep on the final commit.

Selected Test262 replay: 20,264 / 20,268 Success, 4 Bug, 0 Crash, 0 NotImplemented, 0 timeouts; actual launcher exit 1. The four failure kind/detail signatures match the prior committed authority. All selected Object, Reflect, Proxy, Function, Intl, module, super, object-method and disjoint built-in metadata families are complete. This is not a full pinned Test262 refresh and establishes no universal V8 parity.

Full source tooling passed: false. Fresh 4d1/current aggregate and isolated observations retain inherited failures, raw logs and stable input hashes. No-new-diagnostic evidence does not establish full tooling acceptance.

The original `8b38b0afd` attempt stopped at an overbroad bootstrap assertion: the shared compact allocator legitimately boxes its object prototype once. The other eight focused groups passed 26 checks. Earlier Debug-bound compilation failure/fix and the dirty-source guard correction retain their original source/patch identities. Those observations remain audit history; failed/development observations add zero accepted tests, selected results or benchmark samples. The separately admitted successful e36 scopes support explicit mixed-source coverage once. Original origins and hashes are retained in the raw evidence.

Prior checkpoint/archive SHA authority is referenced without nesting its archive. Baseline products/diagnostic retain their original source paths and identities. No historical run is relabelled; no cleanup is part of this campaign.

[Machine-readable checkpoint](execution-memory-packed-records-20261026.json) · [Raw evidence/reproduction](execution-memory-packed-records-20261026.tar.gz). Archive SHA-256: `846fcb51df8e82801fcade6caab22b22f8adab994a1f5c849f5e8c93233414de`.

Reproduce with archived run_campaign.py and a real clean campaign source under one CPU, 4 GiB and no swap. Reuse the independent prior baseline or reconstruct its committed authority. All product gates include vendor sources. The publisher validates read-only by default and refuses overwrite. Complete Cargo JSONL use lossless deterministic gzip members with decoded lengths/hashes. Native images/binaries/caches remain excluded with full provenance and regeneration commands retained. Archive/report budgets remain explicitly bounded.
