# Compact ordinary data storage

The current private runtime ABI is layout 34. Compact ordinary property storage
belongs to the single Wasm GC object model and requires the experimental
Wasmtime GC/reference feature baseline. Layout 31 adds the inline representation
and shared store kernel described in
[Inline compact property storage](inline-compact-storage.md). Programs, runtime
artifacts and trusted native images must match the actual runtime identity;
earlier layouts are not interchangeable with this schema.

Layout 18 introduced compact storage. The
[six-workstream checkpoint dated 2026-10-24](../performance-six-workstreams-20261024.md)
records its native verification, controlled gains and regressions. The subsequent
[efficiency checkpoint dated 2026-10-24](../compiler-efficiency-20261024.md)
records layout 19's indexed metadata and separate verification. These are
historical checkpoint results, not measurements or verification of layout 31.
Their selected conformance campaigns do not refresh the historical full pinned
Test262 result.

## Literal admission and ownership

A genuine source literal with 1–16 unique static string data keys and a source
execution environment creates fresh compact ordinary storage. Every source
field has writable, enumerable and configurable attributes. Empty literals
retain the ordinary empty allocator. Values evaluate once, left to right,
through normal expression and abrupt-completion emission. The compact allocator
owns a fresh ordinary header with the exact source Realm's Object prototype.
The object is not exposed while its values are being initialized. Duplicate,
computed, spread, method, accessor, prototype-setter and nonenumerable IR forms
retain the general definition path. Distinct static data literals with 17–64
fields retain the descriptor append path; larger forms retain general literal
definition.

For source literals, the immutable shape belongs to one literal site and
physical ProgramInstance. It contains primitive string keys in source insertion
order and the common data-attribute word. Literal shapes and own-data reads have
separate checked dense u32 site namespaces, shared across all source bodies in
P. Shapes never contain a receiver, prototype, Symbol or property value.

Native function `length`/`name` metadata also uses compact ordinary storage.
Its immutable shapes are shared through R's separate configurable/fixed metadata
roots, with the appropriate nonenumerable, nonwritable data attributes. This is
distinct from source-literal site ownership. Each callable still owns its
current metadata values and follows ordinary structural mutation rules.

## Physical storage and value updates

`ObjectPropertyStorage` is a fieldless nonfinal GC base. Its direct children
are final `OrdinaryPropertyStorage` and nonfinal `CompactDataStorage`.
`CompactDataStorage` contains only an immutable, nonnull `SHAPE` reference.
The compiler exposes no allocating constructor for either base. The compact
base has two final concrete children, each repeating that inherited shape
prefix in its physical schema:

- `InlineCompactDataStorage` holds mutable `PACKED_TAGS: i32`, two mutable i64
  scalar payloads and two mutable nullable eqref payloads.
- `ArrayCompactDataStorage` holds a nonnull tag-array reference and mutable
  nullable scalar-array and reference-array edges.

The allocator selects inline storage for exactly one or two shape keys. Other
shape lengths, including zero at the internal allocator boundary, select array
storage. Empty source literals do not enter this compact allocator. An ordinary
header's mutable property-storage reference owns the chosen concrete compact
storage or an ordinary dictionary.

Inline storage retains the complete generic value tuple for each slot. The two
tag bytes occupy the low two bytes of `PACKED_TAGS`; both tags start Undefined,
both scalar payloads start at zero and both references start null. A one-key
shape still has two physical slots, but its logical extent remains one. Reads
and stores reject an unsigned index outside either the shape extent or the two
physical slots before selecting fields. A store replaces the selected tag,
complete scalar bits and reference, including null, while preserving sibling
tag bits and fields. There is no numeric-only admission or per-write type
promotion.

Array storage owns one mutable-byte tag per key. Null scalar backing represents
zero bits in every slot; the first nonzero scalar write allocates an exact
shape-sized i64 array. Null reference backing represents null references in
every slot; the first nonnull reference write allocates an exact shape-sized
eqref array. Every present backing has exactly the shape extent. Once backing
exists, later zero/null writes replace its old payload and clear old GC edges.
Scalar and reference backings are independent and remain allocated after those
writes.

Both representations support every JavaScript value tag. Number payloads retain
their exact bits, and object and Symbol payloads retain their normal GC identity.
Replacing a reference with a scalar retires the old strong edge. Initialization,
existing own-data writes and native metadata initialization call the typed
runtime-owned `CompactDataStore` helper with compact storage, an i32 slot and the
full generic Value tuple. Its shared concrete-storage dispatch keeps that
dispatch out of repeated setter emission. Reads and dictionary conversion use
the corresponding central value-read path.

The ordinary header's prototype field is a direct nullable
`JavaScriptObject` reference, not a separately allocated `StoredValue`.
Prototype nullability and mutation retain the ordinary object algorithms.
Layout 31 changes compact storage, not prototype or Realm semantics.

## Definitions, reflection and dictionary conversion

An existing own data Set can update either compact representation when target
and receiver are the same concrete ordinary object and the shape marks the
property writable. This works on a nonextensible object because the property
already exists. Misses and distinct receivers retain the ordinary Set path.
Definition, attribute/accessor changes, Symbol insertion, new keys and deletion
use one-way dictionary conversion.

Conversion roots the original compact graph and replacement dictionary until
every original key, current value and data attribute has been published into
the replacement. It then swaps the header's storage once. The object header,
prototype, Realm relationships and referenced values keep their identities.
Entry-returning APIs first require concrete dictionary storage; there are no
fabricated `PropertyEntry` records.

The allocation-free ordinary own-value probe distinguishes present Undefined
from absence. Accessors miss. Full Get remains responsible for prototype
traversal, accessors, proxies and receiver semantics. Reflection creates proper
fresh descriptor snapshots from current compact values without converting the
object or exposing mutable storage fields. OwnKeys snapshots use the source key
order and existing integer-key sorter.

The native graph observer admits the exact canonical concrete inline or array
storage type, rather than accepting an arbitrary subtype of the compact base.
It checks the shape, data attributes and representation-specific extent.
Inline storage must have one or two keys. Array tags and every present scalar
or reference backing must match the key count, bounded by the supported compact
extent. Each active slot passes the existing tag/scalar/reference validation.
The observer synthesizes normal data descriptors and retains the existing
identity tracking and traversal budgets without calling guest JavaScript.
Shared shape-field access does not weaken these concrete snapshot witnesses.

## Own-data read caches

Each admitted static string read site can own a monomorphic cache in its
ProgramInstance. Both the shaped PropertyRead carrier and canonical GetV with
a literal String operand reach this seam. GetV retains its original operand,
ToObject and ToPropertyKey order before admission. The runtime helper receives
separate lookup and receiver values plus a completed PropertyKey; a miss uses
common Get without repeating user key coercion. Boxed primitive lookups retain
the original primitive getter receiver. Computed keys, Get and GetMethod retain
their existing spec-operation paths. Known primitive, Array and intrinsic-only
targets retain generic Get; unresolved object reads remain candidates.

The optimized runtime checks the live concrete lookup, live compact storage and
actual immutable shape before loading the current slot value through the common
read path. Every later read repeats those checks, including reads after calls.
Value-only Set keeps the shape and the cache reads the new value from either
representation. Dictionary conversion makes the old shape check fail.
Inherited/absent properties and proxies use full Get and are not cached. Cache
records retain only primitive string/shape metadata and a slot; they cannot
retain a user object, prototype, Symbol or value.

Each ProgramInstance owns a typed directory for each cache kind. Indexed
directories grow geometrically; nullable 64-slot leaf pages allocate on first
use. Untouched pages allocate no leaf slots, while a touched page reserves all
64 nullable slots. Individual metadata records allocate only when admitted.
Warmed lookup uses the checked site index rather than traversing a registry.
Directory storage scales with the highest per-kind site index divided by 64;
leaf storage scales with touched pages. Cache polymorphism, startup allocation
and large-program timing effects require separate measurement. No cache
default-policy or performance claim follows from this representation contract.

## Allocation accounting and evidence limits

For a fresh admitted ordinary source literal before structural conversion,
per-object retained GC records are:

| Representation | Records | Included records |
|---|---:|---|
| Inline, one or two keys | 2 | Ordinary header and inline compact storage |
| Array compact | 3–5 | Ordinary header, array compact storage, tags, optional scalar backing and optional reference backing |

The immutable shape and key array are shared per literal site and physical
ProgramInstance. Pooled key strings, the prototype object and canonical empty
private-element storage are not fresh per-object records. Value-expression
allocations and the ProgramInstance's shared directories, touched metadata
pages and admitted read-cache records are separate. A native callable has
additional callable/context/code records outside this ordinary-header/storage
accounting.

Dictionary storage instead retains its storage record, ordered entry table,
hash index and actual property entries, descriptors and stored key/value
records. Canonical absent-accessor values are shared. Its accounting must follow
the selected construction or conversion path and retained capacities; no fixed
dictionary-record formula is asserted here. Replaced growth backings are
allocation churn rather than necessarily retained records.

These are source-layout record counts, not native allocation measurements,
process RSS, GC capacity or live JavaScript byte totals. Declaration and emitted
call-edge controls cover reachability and representation guards; native semantic
and snapshot controls cover mutation, descriptors, key ordering, exact value bits
and rooting. Their execution results and any residency or timing observations
belong to their separately source-bound verification checkpoints.
