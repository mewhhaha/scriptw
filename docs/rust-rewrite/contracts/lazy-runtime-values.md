# Lazy runtime values

Scriptw retains one Wasm GC object model. Runtime layout 22 introduced lazy
canonical strings and deferred native method values; it rejects earlier R/P
layouts rather than admitting incompatible fields or helper indices.

R initializes a fixed nullable canonical-string cache. Its sealed descriptor
rows are indexed by the checked runtime ordinal, independently of lexical
interning order. The resolver checks that ordinal against the sealed R prefix,
returns a cached StringValue or constructs and publishes one from its original
UTF-16 span. The passive UTF-16 data remains available for unresolved entries.
Well-known-symbol descriptions use that resolver after cache publication.
ProgramInstance runtime-domain operands use the same resolver, while P-owned
strings retain their checked separate table. A program cannot widen R's cache.

Intrinsic method properties retain real data descriptors, keys, insertion order
and attributes from realm creation. A fieldless nonfinal PropertyValue is the
sealed GC base of immutable StoredValue and DeferredIntrinsicValue. Descriptor
VALUE accepts this base; getter/setter and general value arrays accept only
StoredValue. A recipe holds its defining RealmRecord, a closed native builtin
identity and a nullable cached completed snapshot. One installation's aliases
share a recipe and keep independently mutable descriptors.

Only nonconstructable method groups without a canonical Realm slot or global
exposure are deferred in this rollout. Constructors, prototypes, accessors and
canonical-slot functions retain eager initialization. Their existing intrinsic
table remains the sole canonical identity authority. Deferred function creation
uses the original Realm's intrinsic Function prototype and actual native entry;
it reads no public constructor or prototype property and runs no JavaScript.
A balanced exact-ID dispatch selects the planned native factory. After complete
function publication the recipe caches one immutable StoredValue.

Key enumeration, presence checks and deletion can inspect actual entries without
creating function values. Semantic reads, descriptor exposure and SameValue
validation resolve the recipe. Copying complete descriptors may share that
internal edge; replacement and deletion cannot regenerate an overwritten entry.
Opt-in rooted graph snapshots resolve deferred values through an internal Wasm
helper under their existing Store limits and traversal budget. No JavaScript
getter or parallel host function object implements this laziness.

Packed numeric Arrays also probe their own indexed data before generic
HasProperty. The probe is repeated for each index, so callback mutation remains
observable. Holes, accessors, inherited properties and Proxy receivers retain
the generic path; an own numeric value avoids descriptor expansion and string
key allocation. The representation preserves undefined, NaN and negative zero.

Memory and execution effects must be measured with the unchanged matched
[precompiled protocol](precompiled-comparison.md). Lazy allocation does not imply
that a Wasmtime heap reservation or every process code page becomes smaller.

Runtime layout 24 stores [[Prototype]] as a nullable reference to the actual
JavaScript object payload. All object layouts belong to a nonallocating typed
base; strings, BigInts and Symbols do not. This removes per-header StoredValue
snapshots while retaining Array, Function, BoundFunction, Arguments and Proxy
identity and behavior. The same schema owns prototype-tag reconstruction.
For the Array variant of compact ordinary storage, scalar backing starts absent
(all scalar words zero) and materializes at the first nonzero write; later zero
writes clear prior values. One/two-slot records instead retain full scalar and
reference fields inline; see [current storage](compact-ordinary-data-storage.md). Packed numeric arrays start at
four elements and retain their occupancy-based growth bounds.

On supported Linux anonymous mappings, normal copying collection releases
whole pages in the dead idle semispace after weak processing and externref
sweeping. Capacity stays unchanged and reclaimed pages remain readable/writable
with zero contents on reuse. Custom/shared/CoW backings do not receive this
operation, and GC zeal retains poison checks. This reduces dead-page retention;
it adds no collections or benchmark-specific purges. CPU and page-fault costs
must be measured with the same execution protocol.

Normalization tables keep their complete canonical bytes and fixed addresses,
but use a passive R data segment. Two active fragments omit the deferred span.
A typed per-Store readiness guard installs the complete region before valid
normalization and locale-casing operations that read its combining classes.
Coercion can reenter before initialization; the flag publishes only after the
copy completes. UTF-16 and normalization segment roles have distinct types.
Raw admission reconstructs and checks the complete virtual static image.

Linear scratch memory starts with one spare Wasm page. Its existing allocator
checks alignment, overflow, limits and growth before publishing a new cursor.
Linux mmap-backed GC heaps prefer base pages before initialization, and retain
that preference through in-place growth and relocation. Advice refusal keeps
valid memory usable; custom/malloc backing retains its own allocation policy.
These policies do not request extra collections or purge benchmark state.

Runtime layout 33 keeps native function source text deferred independently of
callable creation. FunctionObject stores a nullable cached source StringValue
and the producer-admitted runtime-pool ordinal. Constructor, accessor and
method allocation retain that ordinal without resolving the source. User
JavaScript source and program-owned native text retain their original materialized
strings. No native source recipe object or additional string table is allocated.

Function.prototype.toString resolves an absent source through the existing
canonical runtime string cache, then publishes the completed StringValue on the
actual callable. It reads no public name, prototype or toString property and
executes no user code. Bound functions and callable proxies retain their existing
native-string path. Borrowed and cross-Realm calls use the original callable's
source; collection before or after first observation preserves that source.
The schema change requires matching runtime/program artifacts. Allocation and
residency improvements require the matched measurements, not source counts.
