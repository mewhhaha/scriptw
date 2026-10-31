# Ordered shared Realm installation

Runtime layout 33 adds sealed installation rows and a counted Wasm helper for
contiguous deferred-method runs. The earlier layout-29
[verified checkpoint](../memory-ordered-bootstrap-20261028.md) records
4,894 native test executions, 28 fresh original Test262 executions and all 17
paired workload checksums. It reports static code sizes and unscored ownership
captures separately. This does not establish an overall RSS/speed saving,
Scriptc parity or a full-suite conformance result.

`DeferredIntrinsicInstall` allocates the existing one deferred-method recipe,
captures its defining Realm and exact admitted builtin, projects the target
header, resolves the first property key and publishes a fresh data descriptor.
The returned recipe is rooted by the caller while any later aliases resolve and
publish in their original order. Aliases share the recipe and its eventual
callable; their descriptors remain independent mutable records. Empty alias
lists, missing native entries, mismatched method metadata and program-owned key
ordinals fail emission. String keys cross the helper boundary as checked
runtime-pool plans; Symbol keys retain their closed `WellKnownSymbol` identities.
Neither is resolved before recipe allocation. The typed helper argument domain
accepts a real `StandardBuiltinId` or a builtin parameter read through the sealed
installation-row authority. Arbitrary integers cannot populate a recipe.

Shared method plans collect and emit the same ordered keys and builtin identities.
Their immutable 16-byte rows contain builtin, key kind/ordinal and attributes,
inside R's checked static boundary. Only the validated pool can mint a row span;
the run helper checks its alignment and bounds before reading. One compiled loop
calls the existing installer for each row. Eager/canonical methods split runs;
aliases and accessors keep their existing ordered paths. Rows contain installation
metadata, while user programs remain compiled Wasm. Every Realm still allocates
its own recipes and mutable descriptors. The new helper and static boundary require
layout-33 admission; previous packages and cache entries are replaced.

The [full deferred-method fold](../runtime-fold-20261031.json) and
[string-only fold](../runtime-strings-20261031.json) failed performance controls
and were reverted. Exact-ID leaves retain their original checked code allocation,
name resolution and source resolution, followed by the shared callable tail.
Defining-Realm roots, aliases and completed-cache publication stay ordered.

`IntrinsicConstructorAllocate` receives the caller's already rooted native
header, exact typed executable code, defining Realm and unresolved runtime source
ordinal. Its checked producer admits only uniform strict native constructors.
It retains home value, Context allocation, source resolution, undefined public
prototype cache, FunctionObject allocation and ACTIVE_FUNCTION cycle order.
Metadata publication follows the complete cycle in the caller. Typed-array
element kinds derive from the immutable executable's builtin identity using the
existing closed domain. Standard and host constructors outside the deferred
Temporal cluster retain their eager initialization.

The ordered Realm graph and property insertion order are unchanged. Bootstrap
continues to own Error and typed-array constructor inheritance; Proxy's absent
public prototype; Iterator's constructor accessor; public prototype and
cache publication; typed-array `BYTES_PER_ELEMENT`; canonical Realm-slot
publication; and the subsequent exhaustive family installer. Accessors retain
their eager allocation path. Every Realm owns its existing GC objects and
mutable descriptors; no completed Realm objects are shared across Stores.

Layout 36 defers the complete Temporal cluster through one per-Realm
`Dormant`/`Building`/`Ready` owner. The global retains its original data-property
key, order and attributes, with an internal recipe capturing the defining Realm.
Reading or exposing its value resolves the canonical namespace; internal Temporal
intrinsic reads use the same initialization helper. Membership, key enumeration,
deletion and replacement do not require the value. Deleting or replacing the
public property cannot resurrect it or remove internal Temporal identities.

Initialization creates all eight prototypes before their constructors, then
members, `Now` and the namespace in their existing orders. It uses the Realm's
canonical Object/Function prototypes, never public bindings or
the calling Realm. Only the private construction capability permits unfinished
Temporal-slot reads. The helper publishes the namespace and `Ready` after the
whole graph is complete; reentry during `Building` is an invariant failure.
Descriptor exposure, retained embedding and rooted snapshots resolve through the
ordinary property-value owner. Runtime code and data remain available regardless
of which names appeared in the source program.

Emission regressions inspect the actual R calls, allocation counts, allocation
and key-resolution order, exports and concrete GC reference ABI. Semantic
regressions cover alias descriptor independence before first observation,
String/Symbol key identity and order, mutation/deletion/freeze, collection,
foreign Realm callable identity, constructor inheritance/prototypes and typed
element kinds.

The ownership diagnostic's transport-drop snapshot precedes the run loop. Its
other snapshots select observation 16 after 16 prior executions. Last-sample
page growth cannot isolate or exclude Realm installation. Native bootstrap body
size, helper overhead and matched end-to-end R page/RSS results must be assessed
separately before claiming a saving; overlapping function pages do not establish
which functions executed.
