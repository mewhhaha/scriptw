# Runtime fixed-cost memory checkpoint

Measured and verified source `9b423b63cb320f54f80d658da28365fd20eb34ae`; refreshed 2026-10-27.

The [matched comparison](precompiled-comparison.md) records Scriptw at **1.4–2.0 times Scriptc’s post-execution RSS** across all 17 workloads. Scriptc parity remains open.

| Workload | Scriptw RSS | Scriptc RSS |
| --- | ---: | ---: |
| Integer | 19.8 MiB | 13.5 MiB |
| Objects | 24.4 MiB | 14.1 MiB |
| Retained objects | 27.3 MiB | 20.0 MiB |

Three Sol reviews guided the batch. The trusted precompiled consumer reuses private producer-admitted foreign/runtime requirements, retaining the unsafe whole-envelope provenance boundary and actual native import/type checks. SW02 carries bounded canonical execution facts; historical SW01 remains readable only as inert benchmark evidence. Safe raw-Wasm admission retains its full checks. See the [foreign-module contract](contracts/foreign-modules.md).

Cargo runtime consumption uses a compact immutable execution receipt instead of constructing raw runtime and complete Intl catalogue data. Source, runtime key, layout, profile, target, native modes and exact Cargo origin remain checked. Intl identity and timezone requirements remain eager; catalogue data initializes once on first use and propagates admission failures. Native image backing and its ownership are unchanged.

Normalization bytes remain in their canonical virtual static range, published as a passive segment between active prefix/suffix fragments. A per-Store readiness bit installs them before first use. Locale-sensitive casing shares these combining classes and has its own first-use guards. Scratch memory starts with one spare page; its existing checked allocator grows memory before publishing a new cursor. Runtime layout is 24. See the [allocation contract](contracts/lazy-runtime-values.md).

Linux-owned GC mappings request base pages before first touch and preserve that preference through growth and relocation. This is advisory; allocation/protection failures still propagate. Ordinary linear memory and custom/malloc backing retain their own policies. No extra collection, endpoint purge or altered workload was added. Page-size preference can affect throughput; the paired execution-time graph is retained with the RSS graph. Cross-runner timing changes are not controlled evidence of a page-policy regression or gain.

Workspace/all-targets type checking passed. Eighteen complete affected native inventories passed **5,201 cases** with zero final failures or ignored cases. The engine library covers all 1,047 cases in fresh groups of at most eight. Seven selected low-level collector controls also passed, with 112 unrelated Wasmtime tests filtered out; this is not the complete Wasmtime suite.

The first type check found a missing test import. The first backend execution caught a stale assertion that the pooled-string root preceded every scalar global; the repaired test checks the actual encoded reference index and exact nullable type. Sparse-layout tests reconstruct the virtual arena and check physical segment roles, nonoverlapping fragments and RegExp alignment. All failed attempts and original streams remain retained.

The pinned Test262 selection passed **1,840 original executions**, covering normalization, locale casing, Object/Reflect prototypes and descriptors, Object.create, Array joining, Number.toString and String conversion. This is selected verification, not a full pinned-suite status refresh. The full-suite README status is unchanged.

Verification used one CPU, a 4-GiB process-tree cap, no swap, grouped OOM and bounded retained-module caches. Clean Git compiler inputs match the measured CI fingerprint. Local builds conservatively also fingerprint an ignored standalone regress Cargo.lock; its exact bytes and both source identities are retained. Local binaries are not relabelled as CI binaries.

[The matched CI run](https://github.com/mewhhaha/scriptw/actions/runs/40439597289) uses compiler-disabled consumers, identical host settings, two rounds, ten checked warmups and seven samples per round. Every original checksum passed. RSS includes code, data, runtime and warmup history; it is neither peak memory nor live JavaScript heap. Other runner revisions are descriptive observations, not controlled before/after evidence.

Fresh local mapping diagnostics are unscored and retain raw smaps plus present bits without PFNs. They admit only the exact frozen Rust executable, excluding fork-before-exec parent snapshots. Sampling competes on the same CPU, may miss transients, and does not identify the cause of CI bytes. The superseded CI campaign was cancelled explicitly after a test repair; its cancellation metadata is retained.

[Verification record](memory-fixed-floor-runtime-20261027.json) · [Lossless raw archive](memory-fixed-floor-runtime-20261027-verification.tar.xz) · [Matched measurements](precompiled-comparison.md).

The archive contains 638 regular files and 342,068,585 expanded bytes, with a recorded SHA-256 per member. Streaming verification bounds expansion to 2 GiB, members to 128 MiB and compressed size to 32 MiB. Executables and serialized native images are excluded.
