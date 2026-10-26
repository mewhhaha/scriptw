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

Runtime layout 23 stores [[Prototype]] as a nullable reference to the actual
JavaScript object payload. All object layouts belong to a nonallocating typed
base; strings, BigInts and Symbols do not. This removes per-header StoredValue
snapshots while retaining Array, Function, BoundFunction, Arguments and Proxy
identity and behavior. The same schema owns prototype-tag reconstruction.
Compact ordinary scalar backing starts absent (all scalar words zero) and
materializes at the first nonzero write. Reference-only records need no scalar
array; later zero writes clear prior values. Packed numeric arrays start at
four elements and retain their occupancy-based growth bounds.

On supported Linux anonymous mappings, normal copying collection releases
whole pages in the dead idle semispace after weak processing and externref
sweeping. Capacity stays unchanged and reclaimed pages remain readable/writable
with zero contents on reuse. Custom/shared/CoW backings do not receive this
operation, and GC zeal retains poison checks. This reduces dead-page retention;
it adds no collections or benchmark-specific purges. CPU and page-fault costs
must be measured with the same execution protocol.
