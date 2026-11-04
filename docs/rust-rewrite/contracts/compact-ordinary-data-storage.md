# Compact ordinary data storage

The current private runtime ABI is layout 51. Compact ordinary property storage
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

For source literals, the compiler interns each ordered primitive-string key
list and common data-attribute word into one checked dense u32 layout index.
Equal layouts share that index across source bodies and module units in one P;
different key order, keys or attributes remain distinct. The actual immutable
shape is created lazily in the existing table of each physical ProgramInstance.
Separately compiled programs have independent registries and instances have
independent shape roots. Shapes never contain a receiver, prototype, Symbol or
property value. Each object still obtains its prototype from its source Realm.

Only the existing validated source-literal path registers these layouts. Shape
emission uses the same keys and attribute word for both the compiler identity
and the runtime shape. Own-data read caches retain a separate checked dense
u32 namespace with one index per source read site. Layout interning changes
neither GC types, runtime helpers nor the ProgramInstance table ABI, so it does
not increment layout 51. Compiler/source identity still changes and prevents
reuse of stale compiled program or native artifacts.

Native function `length`/`name` metadata also uses compact ordinary storage.
Its immutable shapes are shared through R's separate configurable/fixed metadata
roots, with the appropriate nonenumerable, nonwritable data attributes. This is
distinct from source-literal layout ownership. Each callable still owns its
current metadata values and follows ordinary structural mutation rules.

The [shared-empty Array experiment](../runtime-ownership-20261031.json) failed
performance controls and was reverted. ArrayCreate retains independent ordinary
named-property storage; ordinary, native and frozen-template ownership remains
unchanged. This experiment does not establish a storage-mode improvement.

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
a literal String operand reach this seam. Operands retain their evaluation
order; the compiler-owned literal String needs no observable key coercion.
The runtime helper receives separate lookup and receiver values plus a completed
PropertyKey, and owns nullish errors and primitive boxing on a miss. Common Get
does not repeat user key coercion. Boxed primitive lookups retain
the original primitive getter receiver. Computed keys, Get and GetMethod retain
their existing spec-operation paths. Known primitive, Array and intrinsic-only
targets retain generic Get; unresolved object reads remain candidates.

P calls the existing typed `CachedPropertyRead` helper for every admitted read.
R owns cache registration, warm-hit guards, value loads and complete Get fallback.
P retains neither an invocation-long metadata root nor a duplicate storage-read
path. Each helper call reloads the ProgramInstance's indexed cache table, with
the same source-site identity, execution context and whole Completion result.
This ownership change adds no helper or GC layout and does not increment ABI 51;
compiler/source identity still rejects stale P and native artifacts. The extra
call and cache-table lookup versus reduced P code and native frames require
matched measurement.

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

## Closed numeric row loops

The existing scalar Number-loop proof can borrow a dense canonical Array from a
private local or authenticated captured Module cell. Masked row loops require a
positive bounded literal trip, a nonnegative literal mask, and a first private
`var` row declaration. The complete body must remain effect-free Number work;
callbacks, row escapes, writes and uninspected declarations refuse admission.
Own Number fields and primitive String UTF-16 length use live concrete-storage,
shape, descriptor, type and extent guards. A failed row guard publishes only the
completed numeric prefix before the original loop resumes. The final private
row alias is reread from that same rooted immutable packed table; no admitted
operation can mutate it or reenter JavaScript. Captured-cell TDZ, imported-cell
authentication and generic accessor/Proxy/coercion behavior keep their existing
owners. This adds no heap representation, helper or runtime ABI.

## Allocation accounting and evidence limits

For a fresh admitted ordinary source literal before structural conversion,
per-object retained GC records are:

| Representation | Records | Included records |
|---|---:|---|
| Inline, one or two keys | 2 | Ordinary header and inline compact storage |
| Array compact | 3–5 | Ordinary header, array compact storage, tags, optional scalar backing and optional reference backing |

The immutable shape and key array are shared per ordered literal layout and
physical ProgramInstance. Pooled key strings, the prototype object and canonical
empty private-element storage are not fresh per-object records. Value-expression
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
