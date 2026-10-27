# Compact-runtime memory checkpoint

Measured source `7e02a8707b26468868ec939ade9cc10e12cfc3e3`; native/conformance source `d0926cf6c85e6fcb2ec2a7ed515663a48dec42bc`. Verified 2026-10-27.

The [matched comparison](precompiled-comparison.md) measures Scriptw at **1.6–2.2 times Scriptc’s post-execution RSS** across all 17 workloads. Scriptc parity remains open.

| Workload | Scriptw RSS | Scriptc RSS |
| --- | ---: | ---: |
| Integer | 24.5 MiB | 13.5 MiB |
| Objects | 28.9 MiB | 14.2 MiB |
| Retained objects | 31.9 MiB | 20.0 MiB |

Three Sol reviews guided this batch. Ordinary headers store typed actual prototype references without a StoredValue box. Compact records allocate scalar backing only on the first nonzero write; zero bits preserve presence, negative zero and NaN. Small packed numeric Arrays start with four slots. Own-data indexed reads and a shared exact integer formatter reduce transient key/formatting work. The number shortcut is limited to safe integers; other values retain the shortest-decimal path.

The copying collector releases whole dead backing pages after weak/ephemeron processing, external-reference sweeping and finalized space bounds. The owning mmap allocation retains RW access and zero-on-reuse behavior. Custom/malloc backings are not passed to raw madvise; no extra collection or benchmark purge was added. Native consumption also reuses its producer’s full Wasm validation. See the [allocation contract](contracts/lazy-runtime-values.md).

Workspace/all-targets type checking passed at a68. All 15 source-identity recipes and the source-only guards passed; both complete typed structural targets are included below. All changed Rust files passed scoped formatting. The initial whole-workspace formatting check found existing unrelated drift; its original output is retained.

Fifteen complete affected native inventories passed **4,514 cases**, with zero failures or ignored cases in the final inventory. Four separate low-level collector controls passed. Engine-library execution uses fresh groups of at most eight, covering its complete 1,040-case inventory exactly under the same budget.

The first backend run caught a stale expectation for the removed prototype box. The repaired emitted-code guard requires zero boxes. All 489 backend cases then passed; the controller initially rejected libtest’s cosmetic expected-panic suffix. Corrected framing checks recovered those unchanged results and continued the remaining scopes without another build. Failed commands and raw streams remain archived.

The exact pinned Test262 selection covers Object/Reflect prototypes and descriptors, Object.create, Array joining, Number.toString and String conversion. **1,704 original executions passed**, with zero bugs, crashes or unsupported outcomes. This is selected verification, not a full pinned-suite status refresh.

Verification used one CPU, a 4-GiB process-tree cap, no swap, grouped OOM and bounded module retention. The measured 7e02 commit and later a68/d092 test-only corrections have identical clean production compiler fingerprints. Local builds additionally fingerprint an ignored standalone regress Cargo.lock; its exact bytes and the source-binding checker are retained. This identity difference is explicit, not relabelled as the measured executable.

[CI run](https://github.com/mewhhaha/scriptw/actions/runs/40422593194) is the authoritative matched Scriptw/Scriptc cohort: compiler-disabled consumers, two rounds, ten checked warmups and seven samples per round. Other runner revisions are not controlled before/after evidence. RSS includes code, runtime and warmup history; it is neither live JS heap size nor peak memory.

Separate local mapping diagnostics are unscored. Their first attempt included fork-before-exec Python mappings; those maxima were rejected and retained. Corrected attempts require the exact frozen Rust executable and retain raw maps plus present bits, without PFNs. Symbol-page coverage is nonadditive and may reflect readahead, so it does not prove function execution or explain CI bytes. Local device-identity discrepancies and large eager static tables motivate further investigation; no future saving is claimed here.

[Verification record](memory-compact-runtime-20261027.json) · [Lossless raw archive](memory-compact-runtime-20261027-verification.tar.xz) · [Matched measurements](precompiled-comparison.md).

The archive contains 818 regular files and 301,492,174 expanded bytes, with recorded SHA-256 per member. Streaming checks bound expansion to 2 GiB, members to 128 MiB and compressed size to 32 MiB. Executables and serialized native images are excluded.
