# Compact value backing memory checkpoint

Measured and verified product source `f3df68beb76f18506f070fe54b95e80fe2db22be`; refreshed 2026-10-27.
The original 17-workload matched comparison remains **1.3–1.7× Scriptc process
RSS**. Scriptc parity is still open. See the [complete comparison](precompiled-comparison.md)
and [measured CI run](https://github.com/mewhhaha/scriptw/actions/runs/40490939304).

Default mixed Array data slots now use nullable immutable StoredValue references.
A null slot means a hole; stored Undefined remains present. Number-only storage
keeps its existing unboxed path. Promotion, growth, deletion, empty reset and
descriptor retirement preserve exact value tags, Number bits, occupancy, length,
GC roots, aliases and Array exotic behavior.

Compact plain objects allocate reference backing on their first nonnull payload.
Null backing represents null payloads, with tags retaining present Undefined,
Null, Number and Boolean distinctions. First allocation uses the complete shape
extent; later null overwrites clear strong reference edges. Native rooted snapshots
read both representations and validate active/inactive backing and occupancy.

A shared IntrinsicMethodPublish Wasm helper factors repeated property publication
from Realm bootstrap. Each alias still owns a fresh mutable descriptor, while
aliases share the same defining-Realm immutable function recipe. Flags, ordering,
reflection, mutation and deletion retain their existing semantics. These changes
use one Wasm GC object model and runtime layout 25, with no fallback representation.
See the [storage contract](contracts/compact-value-backing.md).

The release workspace/all-target check and source, identity, runtime-profile,
immutable-pool, retired-JS and formatting guards passed. All **4,869 native cases
across 19 complete disjoint target inventories** passed; every prior target's
test IDs remain present. Focused checks overlap these counts. All **2,365 exact
original pinned Test262 executions** passed in disjoint fresh processes of at
most 16 IDs. This is selected verification, not a full pinned-suite refresh.

Ten new engine controls cover mixed tags and Number bits, holes versus present
Undefined, growth and cycles, receiver/prototype-setter behavior, descriptor and
sparse transitions, freeze, failed-shrink rollback, first reference writes,
reentrant keys, native graphs, empty reset/re-promotion, alias descriptor
independence and created-Realm identity. Nine source controls run in strict and
sloppy modes; the separate native graph control observes four storage states.
The batch was reviewed for allocation and GC-root ownership.

Verification retained one CPU, 4 GiB, no swap, grouped OOM and a one-entry/64-MiB
module cache. Exact raw resource counters are archived. Clean CI compiler inputs
fingerprint `6d7812c410263e87f5182ac0632a162b82e1d8e89e58f40428737b602ec20ed4`;
local inputs additionally include the retained ignored standalone regress lock,
fingerprint `b55f1ef7c4bee5f280503cec35f2c7eb87bca1733a43ddea7a457fd4780fcd45`.
Both input closures and that lock are retained explicitly.

Both runtimes passed all 17 unchanged workloads and checksums, with identical
Wasmtime settings in one compiler-disabled host. Each runtime/workload has two
rounds, ten checked warmups per round and 14 measured samples. The checked report
owns exact RSS/timing values and both regenerated figures. RSS is observed after
execution with each Store alive; it includes code, runtime and allocator history,
and is neither live JS heap nor peak memory. Different runner cohorts do not
establish causal savings. The ordinary V8 comparison stays separately dated.

The [preceding lifecycle diagnostic](memory-runtime-imports-20261027.md) found
resident runtime and host code after Store allocations were released. Follow-up
read-only reviews identify remaining publication sharing and accessor deferral
as candidates requiring measurement. Array header fusion could remove 16–32
structural bytes per Array and requires a new nonallocating common base to keep
ordinary/exotic semantic dispatch correct; it is deferred. No such candidate is
implemented or counted as a measured saving in this checkpoint.

[Verification record](memory-value-backings-20261027.json) · [Lossless raw archive](memory-value-backings-20261027-verification.tar.xz).
The record enumerates hashes and bounded expansion sizes. Native executables and
serialized images are excluded. Regeneration commands for the graphs live in the
paired comparison report.
