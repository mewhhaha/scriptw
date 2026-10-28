# Ordered Realm bootstrap checkpoint

Verified source `a036f1144ea3d3cf502d888b8a07f442837a857f`, measured source `1721e1b6daf39924dabee7e85112adc2ebed4c9d`, runtime layout 29.

Deferred method installation and eager native constructor allocation share typed Wasm routines. Defining Realms, source/key resolution order, constructor inheritance, prototype links and independent alias descriptors retain their existing behavior. Iterator’s constructor property remains its required accessor.

The [verification record](memory-ordered-bootstrap-20261028.json) binds **4,894 native test executions across 19 targets**. Six focused controls are included once in those inventories. The fresh product CLI also passes **28 original pinned Test262 executions** from the complete Iterator.prototype.constructor and Array.prototype.values selections: 14 source files and their actual execution modes. No earlier 6,307-case selection is inherited as current-source verification, and this is not a full pinned-suite run.

The CLI build succeeded before an inspection receipt stopped on an incorrectly inferred binary filename. A separate continuation reads Cargo’s tracked binary declaration and uses the completed build. Its initial import failure and two inert packaging failures are retained with their original resource origins. None is counted as a successful test or supplied invented after counters. All native tests and original JS executions remain source-bound; no completed build or test scope was repeated to repair packaging.

[CI 40611080497](https://github.com/mewhhaha/scriptw/actions/runs/40611080497) validates all 68 attempts and 17 paired workload checksums, six exact native controls and four additional unscored ownership captures. Both runtime modes’ raw Wasm hashes match the local artifacts. Runner-specific native hashes remain separate.

The Portable initializer plus its two new helpers occupies **563,304 native bytes**, versus the historical Portable initializer’s 765,194 bytes: 201,890 bytes (26.4%) smaller. The current Portable native image is 67,007,624 bytes. These are static code/artifact sizes; they do not establish an overall RSS or execution-time saving. No Portable baseline is substituted for a NativeHost measurement.

| Warm Store-drop case | R text pages | R other image pages | Extra module MiB | Extra host RX MiB |
| --- | ---: | ---: | ---: | ---: |
| Integer | 461 | 219 | 2.117 | 2.332 |
| Retained objects | 557 | 411 | 2.672 | 2.328 |

The page size is 4,096 bytes. Warm captures follow 16 prior calls; transport drop precedes the run loop. Image/text and function/page views overlap. Presence does not prove execution, live heap ownership or causality. RSS remains 1.223–1.606× Scriptc’s in the latest paired cohort. Scriptc parity remains open.

The inert archive retains verification streams and inventories, targeted snapshots and pinned inputs, scored raw observations, ownership captures, static manifests/symbol output, source proofs and failed lineage. It excludes native executables and packed/native runtime payloads. Local verification used one CPU, a 4-GiB cap, no swap and bounded module caches; inert processing has separately recorded smaller resource scopes. V8 figures and pinned-suite status remain unchanged.
