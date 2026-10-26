# Packed Number Array indices

The single Wasm-GC Array index store has a closed active mode:
`PackedNumbers`, `DenseDescriptors` or `SparseDescriptors`. Fresh arrays start
with canonical empty Number-bit and presence arrays. A Number occupies its
exact `i64` payload and a separate bit in a `u64` presence bitmap. Every Number,
including `NaN`, either infinity and `-0`, remains distinct from a hole.

Capacity follows occupied indices with geometric growth, an initial four
slots and a 1 Mi-slot ceiling. Growth outside existing capacity requires the
new index to stay near occupancy (`index <= 2*count+8`). Observable Array
length does not allocate either backing. A distant insertion retires packed
storage and uses the existing bounded dense-to-sparse transition. These
transitions are one way. Unsupported values, accessors or attributes likewise
use the real descriptor representation.

`emit_array_own_indexed_value` probes without allocating and returns a separate
found bit. A present `undefined` after descriptor transition is distinguishable
from a hole. Ordinary Get retains its completion-aware accessor/prototype path
on a miss. Descriptor queries for absent indices preserve packed mode, which
allows normal append/Array.push definition probes to remain packed. A request
for a present descriptor constructs a complete private dense descriptor graph
and publishes it before returning a reference. Every returned mutable
descriptor handle therefore belongs to the actual active store; no temporary
descriptor can absorb a later mutation.

Publication of replacement number and presence arrays follows both complete
allocations/copies. Old and new backings stay strongly rooted through the last
allocation. Cold descriptor materialization likewise retains the old numeric
graph until every real descriptor is rooted in the new dense graph. Count,
mode and active backing publication contain no intervening allocation or JS
callback. Deletion clears explicit presence and leaves Array length unchanged.
Deleting the last entry releases the entire active backing to canonical empty
arrays; an empty sparse store remains sparse and allocates eight buckets only
on its next insertion.

The raw `emit_array_indexed_publish_default_data` producer has the same
preconditions as the old raw descriptor publication API. Its caller already
owns validated definition, extensibility, length and error ordering; it does
not supply generic Set semantics. Each publication calls one typed shared R
helper taking the actual Array, index and complete value. Geometric growth,
copying and the cold descriptor fallback stay in that R body; even many short
source literals do not duplicate that control flow into P. This boundary keeps
native compilation bounded while leaving producer evaluation and length
ordering local. Ordinary Set can bypass its descriptor path only after proving
a concrete Array, exact target/receiver identity, a
canonical converted index, a present own packed W/E/C data slot and an incoming
Number. Missing indices still traverse prototypes and can invoke inherited
indexed setters. Proxies, distinct receivers and noncanonical/symbol keys keep
their existing semantics. The concrete packed delete probe handles canonical
indices only, whose packed attributes are known configurable.

The opt-in host graph snapshot reads raw bits and bitmap words directly, with
a typed mode decoder and declaration-derived field/layout witnesses. It checks
inactive backings are empty, capacity/bitmap bounds, unused high presence bits,
occupied count and index/length ordering. It never materializes descriptors,
invokes JS or scans logical Array length.

Required focused acceptance after the coherent batch is compiled:

```sh
python3 scripts/limited_verification.py -- cargo test -p scriptw-aot-wasm --lib gc_types::type_registration_tests -- --test-threads=1
python3 scripts/limited_verification.py -- cargo test -p scriptw-aot-wasm --test emission packed_array_storage -- --test-threads=1
python3 scripts/limited_verification.py -- cargo test -p scriptw-engine --test aot_gc_entries aot_gc_packed_array_storage -- --test-threads=1
python3 scripts/limited_verification.py -- cargo test -p scriptw-engine --test aot_language aot_packed_array_producers -- --test-threads=1
```

Broader affected controls include Array mutation/iteration, sparse arrays,
JSON, argument-list snapshots, native array producers, descriptor/integrity
operations, proxies/Reflect, weak/forced-GC and rooted graph snapshots. Original
Test262 replay must retain original IDs, modes and deadlines; passing these
focused controls alone is not a full-suite conformance claim. Performance and
allocation benefits require fresh checked evidence; raw capacity is not live
bytes, allocation count or a peak.

For occupied capacity `C`, the two packed primitive arrays carry
`8*C + 8*ceil(C/64)` payload bytes before GC headers/alignment. Complete dense
Number storage instead retains an indexed table and two value/descriptor
records per present index. Shared array/object metadata and value-expression
allocations are excluded from this source/layout accounting. It does not
measure dynamic allocation totals, temporary copies during growth or GC live
bytes. Paired execution/RSS and construction-size controls remain required.
