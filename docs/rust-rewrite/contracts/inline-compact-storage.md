# Inline compact property storage

Compact ordinary objects retain one immutable string-key shape. Their property
storage has a common shape-bearing Wasm GC supertype and two concrete subtypes.
The allocator selects inline storage for exactly one or two keys; other shapes
keep the existing tag/scalar/reference arrays. The common supertype has no
allocating constructor.

Inline storage holds two packed tag bytes, two complete 64-bit scalar payloads
and two nullable GC references. Reads and stores check the unsigned index against
both the shape's logical length and the two physical slots before field access.
Every store replaces the complete value tuple, including null references, and
preserves the other tag byte. A shared runtime store helper keeps this dispatch
out of repeated literal and native-function metadata allocation sites.

Property lookup, descriptors, own-key order and structural mutation retain their
existing behavior. Dictionary conversion roots the old storage until the complete
replacement graph owns every value, then publishes once. Rooted host snapshots
use the exact canonical concrete type witness, validate the shape and backing
extents, and share the existing identity, descriptor and traversal-budget paths.
Both variants belong to the same GC object model and require the experimental
Wasmtime capabilities documented for the backend.

With the copying collector's current layout, a two-reference, two-key ordinary
record uses 96 bytes instead of 144 bytes, including its object header. For the
8,192 retained records in the matched workload, that removes 384 KiB of allocated
storage and 16,384 GC allocations. These are layout calculations, not measured
process RSS savings. Extra dispatch and native code can offset them.

The changed GC types and shared helper require runtime layout 31; old runtime
artifacts are rejected. The [verified checkpoint](../memory-inline-compact-storage-20261028.md)
records 4,912 fresh native passes across 19 full targets and all 17 matched
workload checksums. The [ordinary-storage contract](compact-ordinary-data-storage.md)
describes the current schema and accounting. The allocation calculation does
not establish an overall causal RSS or execution-time saving.
