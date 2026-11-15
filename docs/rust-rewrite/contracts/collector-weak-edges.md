# Collector weak edges

The selected Wasmtime 47.0.0 copying collector has a local weak-edge patch in
[`vendor/wasmtime-47.0.0`](../../../vendor/wasmtime-47.0.0/PATCHES.md).
`WeakAnyRef` and `GcEphemeron` expose Store-scoped generational handles. Their
rooted operations never expose an unrooted reference; the raw forms take and
return the references of a running host call that cannot collect
([raw weak operands](../../../vendor/wasmtime-47.0.0/PATCHES.md#raw-weak-operands)).
Only the copying collector accepts these registrations.
Owner, target and key must be actual heap references; values may also be i31 or
null. A foreign Store is an error, and retiring a slot increments its generation
without wraparound or reuse at generation exhaustion.
The copyable handles do not deregister on drop. Entry deletion explicitly calls
`remove`; otherwise a surviving owner and key still retain a conditional value.

The registry belongs to the Store and stays outside the strong-root census.
Strong tracing runs first. A conditional value is marked only after its owner
and key are independently live. Before the space flip, the collector sorts two
dependency records per conditional edge and reserves all marking and retirement
storage. The first copy of an owner or key satisfies its independent gate;
each edge enters the ready queue once when both gates are satisfied. Newly
marked values join the ordinary worklist. The collector alternates that worklist
and the ready queue until both are empty, including when a boxed value exposes
the next key through a strong field. These raw indices are never roots. Weak
updates then forward surviving references, clear dead targets and retire dead
owners or keys. ExternRef sweeping follows this fixed point and update phase.
Dropping an in-progress copying collection finishes the worklist, conditional
marking, weak updates and sweeping phases. Root processing retains the upstream
error-path assumption that valid roots can be forwarded successfully.

`GcWeakHeapEpoch` and `GcWeakKeyFingerprint` provide opaque, non-rooting lookup
tokens for the copying collector. A token binds the Store identity, a checked
monotonic collection epoch and the current heap index. Every copying cycle
advances the epoch before movement, including cycles without weak entries.
Other collectors reject tokens unless they explicitly provide this capability.
Foreign Stores, non-heap keys and uninitialized heaps produce errors.
Tokens never replace collector handles or retain keys and values strongly,
and the product no longer uses them. Its WeakMap and WeakSet entries are
ephemerons of (collection, key, value) found through one Store-wide index
keyed by the hash of two `KEY_HASH_ID` identities that compiled code mints on
the collection and the key before a store. The index holds handles, not
references; a hit is the collector's comparison of the entry's current owner
and key with the operands, and a lookup whose collection or key has no
identity yet is answered in compiled code as absent.

An upgrade reads its slot and creates a Wasmtime root under one `AutoAssertNoGc`
guard. JavaScript's separate keep-during-job rule must retain successful
WeakRef construction and dereference results through the current job, then
release those roots at the real job boundary. Finalization requires weak targets
and tokens, strong holdings, and ordinary cleanup-job scheduling in the correct
Realm. Collector support alone does not implement those JavaScript algorithms.

The product now selects `WasmWeakReachabilityCapability::CopyingCollector`.
All sixteen routes use the typed weak host ABI and actual GC wrapper brands.
WeakMap values are complete StoredValue ephemeron edges; WeakSet memberships
use null conditional values. WeakRef construction and upgrades enter the
Store-local kept-root set, cleared after the initial script/module job and
every dequeued job. Finalization cells retain holdings strongly and targets and
tokens weakly, with one cleanup job per queued registry in its captured Realm.
Cleanup removes a cell before invoking its callback and re-reads the list after
user code. A throw ends that cleanup invocation and reaches a separate host
report event; it preserves the initial script completion.

The [earlier unavailable-facility contract](weak-unavailable-runtime-boundary.md)
records the retired product boundary. No second JavaScript heap or
strong-retaining weak substitute is introduced. Product integration acceptance
is recorded separately from the verified collector foundation.

The native collector controls in
[`weak_gc_facility/main.rs`](../../../crates/scriptw-engine/tests/weak_gc_facility/main.rs)
force actual collections and cover increment-boundary scans, dead targets, moving identity, reverse
ephemeron chains, dead owners with live keys, value/key cycles, stale and foreign
handles, explicit kept roots, ExternRef sweeping and null conditional values.
Additional controls cover long boxed chains in both insertion orders, duplicate
edges with equal owner and key, non-rooting token invalidation across movements
and Stores, and emitted collection lookup after movement, deletion and churn.
The vendored unit control drops actual collections at thirteen increment
boundaries and checks both surviving and subsequently dead references. These
controls are separate from complete pinned weak-tree and full-suite acceptance.
