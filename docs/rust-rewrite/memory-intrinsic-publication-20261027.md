# Intrinsic publication memory checkpoint

Measured and verified product source `fa204ae2bdccd798b98439d4c347b576c2ee4508`; refreshed 2026-10-27.
The [matched comparison](precompiled-comparison.md) remains **1.3–1.7× Scriptc
process RSS**. Scriptc parity is still open. The [CI run](https://github.com/mewhhaha/scriptw/actions/runs/40504623872)
passed all 17 unchanged workloads and original checksums in both runtimes.

Shared Wasm helpers now publish intrinsic data and accessor descriptors through
the existing storage algorithm. These are storage-only operations: publication
neither invokes an accessor nor resolves a deferred method. Every installed
property owns its mutable descriptor; aliases share their immutable function
recipe. Attribute domains, insertion order, reflection, overwrite, deletion,
freeze and defining-Realm identity retain their required semantics.

Deferred method lookup retains its balanced exact-ID dispatch. A private metadata
witness admits only strict, ordinary, nonconstructable standard methods with the
uniform context. Leaves keep the existing typed ExecutableCode construction and
select rooted code, name, native source and length. SelectedMethod can be created
only after the complete dispatch; the shared construction tail requires that
witness. The tail closes the FunctionContext ACTIVE_FUNCTION cycle and publishes
metadata before the recipe caches its completed immutable value. Actual entry,
source ID, name, source text, length, defining Realm and alias identity remain
bound together. No accessor function is made lazy. There is one Wasm GC object
model, no fallback representation and runtime layout 26.
See the [publication contract](contracts/intrinsic-property-publication.md).

Static inspection of owned Cargo R images measured property_value_resolve at
1,172,086 → 320,556 native bytes and realm_initialize_intrinsics
at 1,002,573 → 888,703 bytes. This is a source-bound code-size
observation, not proof of which functions ran, resident-memory savings or a
causal explanation of the paired RSS results. Existing physical local reuse is
unchanged; declaration counts do not establish native frame costs.

The release workspace/all-target check and complete source/identity/profile,
retired-JS and formatting guards passed. All **4,874 native cases across
19 disjoint target inventories** passed, retaining prior test IDs. Focused checks
overlap these totals. All **2,365 exact original pinned Test262 executions** passed
in fresh disjoint batches of at most 16 IDs. This is selected verification;
the ordinary V8 comparison and full pinned-suite status remain unchanged.

Verification includes an explicitly recorded excluded test-fixture repair in
`crates/scriptw-aot-wasm/src/tests/unobserved_arguments.rs`. An absent compacted
P import now means zero calls; actual R resolution and the positive observer
controls requiring at least three retained calls remain. No product input or
test ID was changed by the repair. The original 501-pass/1-fail AOT library
attempt remains archived and is not added to the successful totals. The rebuilt
AOT library passed all 502 cases. All 18 other native binaries are byte-identical.
Four completed original scopes, totaling 420 passes, retain their execution
streams. The stopped original scope has no after-resource counters; successful
repair/resume counters describe the new bounded scope, not that earlier run.

Both runtimes use the same compiler-disabled Wasmtime host/configuration, with
two rounds, ten checked warmups per round and seven measured samples per round.
The checked report owns exact RSS/timing values and both regenerated graphs.
Asynchronous unscored mapping maxima are not phase-aligned endpoints and cannot
establish execution or causal savings. No cost was subtracted from either runtime.

Verification retained one CPU, a 4-GiB process-tree cap, no swap, grouped OOM and
a one-entry/64-MiB module cache. The clean CI source fingerprint is `e695e77969a6816908d7cb992aa5262a7b9b6e8b1422488d4e411c5ad4cf676d`;
local inputs retain the ignored standalone regress lock and fingerprint `6451a26dbc5bfa83b60d1781c8c6d3620b96da646d16452a5142e5072201b72d`.
Both input closures and that lock are recorded explicitly; their identities are
not rewritten to appear equal.

[Verification record](memory-intrinsic-publication-20261027.json) · [Lossless raw archive](memory-intrinsic-publication-20261027-verification.tar.xz).
Native executables and serialized images are excluded from publication. Exact
raw failures, repair patch, resource limits and evidence hashes are retained.
