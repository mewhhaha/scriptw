# RegExp Unicode data memory checkpoint

Measured and verified product source `e7e51222101533e421ffae41bb0ccad78ba638ba`; refreshed 2026-10-27.
The [matched comparison](precompiled-comparison.md) remains **1.237–1.613× Scriptc
process RSS**. Scriptc parity is still open. The [CI run](https://github.com/mewhhaha/scriptw/actions/runs/40519969552)
passed all 17 unchanged workloads and original checksums in both runtimes.

R keeps its complete 455,304-byte RegExp Unicode property catalogue in passive
segment 2. The sole runtime property lookup copies the exact reserved range
before reading, then publishes readiness on that actual R instance. Copy and
readiness publication invoke no JavaScript. The segment remains available;
normalization has its own readiness and segment, preserving both initialization
orders and independent Stores. Alias names, code-point ranges, string properties,
all table pointers and virtual addresses retain their original representation.

R has six physical data segments: passive UTF-16, normalization and property
catalogue followed by three active fragments excluding the two deferred ranges.
P retains its four passive data roles. Immutable Cargo metadata hashes all six
physical spans and reconstructs the canonical static image in order [3,2,4,1,5].
Exact lengths, checked cursor bounds and per-span hashes remain required. Receipt
magic SCRWRM02 and runtime layout 27 bind this private contract.
Raw runtime-package/cache admission was corrected to validate all active offsets
across both deferred gaps and the complete static extent; checksums alone cannot
substitute for those structural checks. The earlier product-admission failure
at `3b9094856c195255a5ce900d535855d0803de109` is retained separately, never counted
as successful verification. [Inert segment inspection](memory-regexp-unicode-data-20261027.json) retains
`data-segments.json` for both owned Cargo runtime packages. It proves segment
placement and extents, not which pages were resident during a measured execution.
See the [data contract](contracts/regexp-unicode-property-data.md).

The release workspace/all-target check and complete source/identity/profile,
retired-JS and formatting guards passed. All **4,876 native cases across 19
disjoint target inventories** passed, retaining the prior checkpoint's IDs.
All **2,365 original pinned Test262 executions** passed in fresh batches of at
most 16, preserving the exact prior selection. Another **16 Unicode executions**
passed as a separate bounded selection with no overlapping IDs. The published
record owns their exact IDs, source files and compiler identity. This selected
verification leaves ordinary V8 results and full pinned-suite status unchanged.

An explicitly recorded excluded test-fixture repair changes only
`crates/scriptw-aot-wasm/src/tests.rs`: its preseed inspection selects physical
R segment 3 instead of 2, while retaining exact prefix bytes, reconstructed virtual
image, increasing active offsets, and R/P segment-count assertions. The original
501-pass/1-fail attempt and exact before/after patch are retained. The
rebuilt AOT library passed all 502 cases; 18 unrelated binaries are byte-identical.
Four completed original scopes totaling 420 passes retain their original streams.
Those passes are counted once. The stopped original scope has no after-resource
counters; successful resume counters describe the new bounded scope. A private
conformance-driver guard then rejected this recorded fixture before any compiler
invocation. Its raw failure and driver repair remain separate; final conformance
used the same completed Cargo producer in a fresh bounded scope.

Both runtimes use the same compiler-disabled Wasmtime host/configuration, with
two rounds, ten checked warmups per round and seven measured samples per round.
Post-execution RSS includes code, runtime and allocator history with Store alive;
it is neither live JS heap nor peak memory. Separate CI runner observations and
unscored mapping samples cannot isolate causal savings from this change. No cost
was subtracted from either runtime. The canonical report owns exact values and
both regenerated graphs.

Verification retained one CPU, a 4-GiB process-tree cap, no swap, grouped OOM and
a one-entry/64-MiB module cache. Clean CI input fingerprint: `65545230fcd34527854e06431e0917a98adbe58c2a5ba92b5518ccd99ae09321`.
Local fingerprint with the separately recorded ignored regress lock: `f95991e5a2af12a901c2986c0b0f98ac72200cc6d70627f178e8b80bf68fbb7e`.
These source closures remain distinct and explicitly bound to the same commit.

[Verification record](memory-regexp-unicode-data-20261027.json) · [Lossless raw archive](memory-regexp-unicode-data-20261027-verification.tar.xz).
Native executables and serialized images are excluded from publication. Exact
raw failures, repairs, limits, disjoint test inventories and hashes are retained.
