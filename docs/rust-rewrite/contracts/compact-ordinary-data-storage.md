# Compact ordinary data storage

The current private runtime ABI is layout 59. Compact ordinary property storage
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
start on the shared empty compact storage described under shape transitions.
Values evaluate once, left to right,
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

The literal allocator selects inline storage for exactly one or two shape keys
and exact-length array storage otherwise. Shape transitions keep the same
split: one or two keys are inline, three or more use array storage whose
backings may carry spare slots. An ordinary header's mutable property-storage
reference owns the chosen concrete compact storage or an ordinary dictionary.

Inline storage retains the complete generic value tuple for each slot. The two
tag bytes occupy the low two bytes of `PACKED_TAGS`; both tags start Undefined,
both scalar payloads start at zero and both references start null. A one-key
shape still has two physical slots, but its logical extent remains one. Reads
and stores reject an unsigned index outside either the shape extent or the two
physical slots before selecting fields. A store replaces the selected tag,
complete scalar bits and reference, including null, while preserving sibling
tag bits and fields. There is no numeric-only admission or per-write type
promotion.

Array storage owns one mutable-byte tag per physical slot. The shape's key
count is the logical extent; the tag array may be longer. Null scalar backing
represents zero bits in every slot; the first nonzero scalar write allocates an
i64 array of the tag array's length. Null reference backing represents null
references in every slot; the first nonnull reference write allocates an eqref
array of that length. Every present backing has the tag array's length. Once backing
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
A complete default data definition of a new non-index String key moves to the
canonical child shape as described below. Every other definition,
attribute/accessor change, Symbol or array-index insertion, insertion past the
transition bounds, and deletion use one-way dictionary conversion.

Conversion roots the original compact graph and replacement dictionary until
every original key, current value and data attribute has been published into
the replacement. It then swaps the header's storage once. The object header,
prototype, Realm relationships and referenced values keep their identities.
Entry-returning APIs first require concrete dictionary storage; there are no
fabricated `PropertyEntry` records.

The callback-free ordinary own-value probe distinguishes present Undefined
from absence. Internal recipe decoding may allocate; accessors miss. Full Get remains responsible for prototype
traversal, accessors, proxies and receiver semantics. Reflection creates proper
fresh descriptor snapshots from current compact values without converting the
object or exposing mutable storage fields. OwnKeys snapshots use the source key
order and existing integer-key sorter.

The native graph observer admits the exact canonical concrete inline or array
storage type, rather than accepting an arbitrary subtype of the compact base.
It checks the shape, data attributes and representation-specific extent.
Inline storage must have one or two keys. Array tags must cover the key count,
every present scalar or reference backing must match the tag array's length,
and the key count is bounded by the supported compact extent of 64 keys. Each active slot passes the existing tag/scalar/reference validation.
The observer synthesizes normal data descriptors and retains the existing
identity tracking and traversal budgets without calling guest JavaScript.
Shared shape-field access does not weaken these concrete snapshot witnesses.

Compiled code does not test or cast a storage reference against the open
compact base itself. A base test is the disjunction of the sealed inline and
array layout tests; a base cast branches on those two and keeps the base cast
as its trapping remainder. The base has no allocating constructor, so the two
forms accept the same references. Wasmtime answers a sealed test with an inline
header comparison, and a test against an open type with a host subtype query
for every reference of another type. The typed cast and test helpers apply this
to any open layout with at most four sealed descendants.

## Shape transitions for plain data adds

Fresh ordinary objects without own properties start on one runtime-owned empty
compact storage: empty object literals, every source object literal outside the
compact literal plan, ordinary `[[Construct]]` receivers and object rest
results. Its shape is the canonical empty default-data shape and it has no
physical slot, so no store can reach it and all such objects share it until
their first add. Exotic and composite headers (Arrays, functions, arguments
objects, primitive wrappers, namespaces) keep the dictionary allocator, and
builtins keep their existing allocators.

Ordinary `[[DefineOwnProperty]]` first offers a statically complete data
descriptor whose writable, enumerable and configurable flags are true (known,
or runtime Boolean payloads tested at run time) to the R kernel
`CompactDataDefine`. The kernel declines without mutation unless the key is a
String, the storage is compact and its shape holds default data attributes. A
present key takes the new value in place. An absent key further requires an
extensible object, fewer than 64 shape keys, and, when no child exists yet, a
key that is not a canonical array index, fewer than 64 children on the parent
and a remaining runtime budget of created shapes (16,384). The kernel then
publishes new storage for the canonical child shape. A declined definition runs
the unchanged general algorithm, which promotes to a dictionary exactly where
it did before. OrdinarySet reaches the kernel only through its receiver's
`[[DefineOwnProperty]]` after the complete prototype walk, so inherited setters,
read-only inherited properties and Proxies on the chain behave as before.

Each shape owns a nullable, geometrically grown table of its children in
creation order. The child for a key is found by key identity, then by equal
String contents behind a prepared-hash prefilter, so objects built by the same
sequence of adds share shapes whatever String objects spell their keys. A child
repeats its parent's keys followed by the new key, inherits OWN_KEYS_ORDERED and
the default attributes, and is never mutated afterwards; only the transition
table changes, and it holds only shapes. Shapes and tables are shared by every
Realm and ProgramInstance of one runtime and retain only primitive Strings.

A storage record's SHAPE never changes: every add publishes a new record. Zero
to one and one to two keys produce inline storage; two to three moves into array
storage with eight slots. A later add shares the previous record's tag, scalar
and reference arrays while a spare slot remains, and otherwise copies the live
slots into arrays of twice the key count. The replaced record is unreachable
from the object, so shared arrays keep a single owner, and ForIn's storage
identity guard still observes every structural change.

Compact probes compare each shape key by identity and then reject a key whose
prepared raw key hash differs before comparing contents. Literal, native
metadata and transition shapes prepare their keys' hashes when created. A shape
with three or more keys probes through an open-addressing index of its keys by
raw key hash, built once on first probe and kept on the shape like its
transition table: a power-of-two bucket array more than twice the key count
whose buckets hold slots plus one. One or two keys are scanned.

## Own-data read caches

Each admitted static string read site can own a monomorphic cache in its
ProgramInstance. Both the shaped PropertyRead carrier and canonical GetV with
a literal String operand reach this seam. Operands retain their evaluation
order; the compiler-owned literal String needs no observable key coercion.
The runtime helper retains its twelve-input target Value, receiver Value,
completed PropertyKey, site, Environment and ProgramInstance contract. It checks
the actual String tag before cache registration; computed keys, super reads,
Get and GetMethod retain their existing paths. Nullish errors and getter
receivers keep their existing execution context and whole-Throw behavior.

Each P read calls the typed `CachedPropertyRead` helper with its ProgramInstance
site and evaluated operands. R owns cache registration, live-storage guards,
current-value loads, closed negative tails and complete Get. P retains no cache
metadata root or duplicate storage-read path. The shared GC type prefix and full
Completion/helper/source/resumable contracts remain unchanged. All temporary
roots clear before the read continues, including guard misses.

Each read proves the concrete lookup, current compact storage, primitive key
identity and immutable shape identity before loading the live slot value. Calls
and value-only Set cannot make an old value authoritative: the value reloads
from the current inline or array compact storage. Structural mutation replaces
storage and forces complete Get when these guards fail.

The existing signed slot has a closed encoding: nonnegative means own data,
`-1` means absence through a null prototype, and `-2` means absence through the
sealed Object.prototype layout. A null cached shape means uninitialized. Negative
registration first proves compact-own absence and an actual OrdinaryObject
lookup. The second mode additionally proves an actual OrdinaryObject prototype,
its immutable-prototype flag, null parent, and the exact intrinsic storage ADDRESS
and COUNT, then checks that the key is absent from those sealed rows. Each
negative hit in R reloads and checks that live prototype tail; prototype replacement
or intrinsic-storage promotion therefore misses. No prototype object, value,
Symbol or Realm is retained in a cache record. Function and primitive-box own
data hits remain supported, but their misses cannot register negative modes;
virtual String properties, accessors, Proxies and other exotics keep full Get.

The external `property_read_plans` emission check validates P and R, witnesses
their shared compact/cache types, the actual typed R call, absence of P cache
metadata access, R positive/negative guards and complete Get fallback. `aot_cached_read_inputs` exercises current values and attributes,
callback/GC boundaries, both negative modes, prototype promotion/replacement,
boxed Strings, callable/proxy misses and foreign-Realm mutation.

Concrete FunctionObjects join the effect-free own-data probe through their
canonical ordinary header, including dictionary storage. Each read resolves its
current descriptor and value; it retains no receiver/value cache. Active legacy
caller/arguments values keep their existing descriptor/StoredValue ownership
and invocation updates. Accessors, absence, prototypes, bound functions and
Proxies retain the complete Get path and original receiver/Realm semantics.

Each ProgramInstance owns a typed directory for each cache kind. Indexed
directories grow geometrically; nullable 64-slot leaf pages allocate on first
use. Untouched pages allocate no leaf slots, while a touched page reserves all
64 nullable slots. Individual metadata records allocate only when admitted.
Warmed lookup uses the checked site index rather than traversing a registry.
Directory storage scales with the highest per-kind site index divided by 64;
leaf storage scales with touched pages. Cache polymorphism, startup allocation
and large-program timing effects require separate measurement. No cache
default-policy or performance claim follows from this representation contract.

## Static assignment write records

A plain, logical, compound or update assignment to a static String key whose
read would be cached gets its own site in the same per-ProgramInstance record
table and calls the typed `CachedPropertyWrite` helper with the target,
original receiver, completed key, value and site. Captured writes, `super`
assignments, computed keys and every other Set caller keep `OrdinarySet`.

The record holds only the key, shapes and integers. Its existing-own form is
SHAPE and SLOT: a shape with the key at that slot under a writable attribute
word. Its add form is a parent shape that lacks the key and holds default data
attributes, that shape's canonical child for the key, a holder rule and a
closed-tail word. A use requires the receiver to be the target and an actual
OrdinaryObject on compact storage with the recorded shape. The existing form
then stores the value at the slot, exactly OrdinarySet's value-only
[[DefineOwnProperty]] on the receiver. The add form further requires the
object to be extensible and its live chain to lack the key: the prototype is
the recorded closed tail (null, or an unmodified %Object.prototype% whose
immutable layout lacked the key when recorded), or one actual OrdinaryObject
holder that lacks the key, by the recorded compact shape or by a live lookup
of its dictionary, above that tail. It then publishes storage for the child
exactly as `CompactDataDefine` does. The result is the Boolean true that the
general Set returns.

Any failed guard runs `OrdinarySet` unchanged with the original operands.
Only a normal true result on a receiver that was the target and compact
refreshes the record, from immutable facts alone: the current shape's slot and
attribute word, the previous shape's absence of the key and attribute word, an
existing canonical child (a lookup never creates shapes), the holder's shape
or dictionary absence and the tail's layout. Setters, read-only and data
properties added anywhere on the chain, replaced prototypes, Proxies,
non-extensible, sealed and frozen objects, attribute changes, deletion and
foreign-Realm prototype mutation therefore all miss at the next use; strict
failure still throws in the caller. `aot_property_write_records` changes each
of these facts between two executions of one site.

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

The same closed proof can read concrete source Function headers and ordinary
objects backed by canonical dictionaries. The first row resolves each static
String key through the existing ordinary finder and retains its insertion slot
and immutable String identity as frame-local hints. Every subsequent row reloads
its actual storage and table, checks the slot against live logical LENGTH and
capacity, then requires a non-null matching-key data entry and a concrete
StoredValue. No cached table, descriptor, receiver or value authorizes a read.
Different insertion positions or String identities safely decline. Dictionary
compaction and deleted entries cannot make stale hints authoritative.

All row fields and primitive leaves are validated before the iteration body.
A getter, missing entry, changed storage mode, deferred value recipe or type
failure publishes only the completed prefix and resumes the original iteration.
The admitted body makes no calls or heap writes/allocations; the whole-region
proof still rejects callbacks, escapes, captured row aliases and dynamic with
resolution. The alias retains the original StoredValue tag and identity.
Intrinsic recipe storage, BoundFunction and Proxy rows remain generic.

For guarded row mode, first-row preparation chooses storage once. Two emitted backedges use
one arithmetic emitter and the same completed-prefix publication/continuation
owner. The ordinary inline arm retains its original concrete Object/Inline
shape guards and scratch operations; dictionary roots and per-field mode tests
are absent from that arm. The dictionary arm retains live Function/Ordinary
header, slot, key, descriptor and StoredValue checks. Every row preparation
clears its temporary roots before returning, including guard misses. A later
storage/type mismatch resumes the original loop at the completed prefix. No
source callbacks, speculative reads beyond the current row or mutable heap
snapshots are introduced. Native frames/code size and timings need measurement.

Repeated masked rows have a separate admission within this same complete region
proof. A contiguous low-bit mask establishes a complete finite period; more
remaining trips than that period establishes reuse and proves every preflight
row is source-addressable. Only the existing one or two own Number/String-length
paths qualify. Nested Array paths and other masks retain guarded row mode.
Zero-trip/TDZ/receiver checks still precede speculative work.

The first-row preallocation gate and every preflight row use the canonical raw
String own-data probe with cached actual keys. The same decoder accepts current
inline/array compact, dictionary and intrinsic storage; internal recipes may
allocate and normalize in their defining Realm without source/provider callbacks.
Accessors, exotics, absence and wrong primitive types decline to the original
whole loop. Streaming rows retain their separate live shape/slot guards.
Preflight writes one private existing NumberBitsI64Array. Each Number is copied
by bit reinterpretation; String leaves
use the same immutable UTF-16 length. No source body, counter, sum or row-alias
write occurs until the entire projection is valid. Any preflight miss takes the
whole original loop from its original local state, so getters and coercions
retain their original partial-iteration effects and Throw order. All projection
and borrowed roots clear before that generic continuation.

The repeated body uses projected numeric loads and the unchanged scalar
arithmetic/update emitter, preserving Number order, NaN and signed zero. The
final alias still reads the original packed row table, preserving callable tags
and identity. The complete effect proof excludes every callback, write, escape
or reentry that could invalidate the snapshot. This is one per-invocation GC
array in the existing heap, not a permanent cache or another object model. The
existing period/trip/path bounds imply finite checked projection storage; no
workload name, field spelling or measured trip count authorizes admission.
Allocation cost, guard-miss overhead and native performance remain to measure.

Computed own-data reads can also use a second canonical dense Array of primitive
String keys. The row and key indices must use the same private induction with
bounded low-bit masks whose periods are powers of two. Preflight covers aligned
pairs over the maximum period, including when the key period exceeds the row
period; it never forms a Cartesian product or converts a dynamic key. The real
array mode, length, count, capacity and current StoredValue/String guards precede
the canonical own-data probe. Accessors, holes, Proxies, non-String keys or
non-Number results decline to the whole original loop before source work.

The same existing numeric projection and scalar backedge serve both static and
cyclic keys. A direct scalar accumulation may discover its row receiver without
introducing a synthetic source alias. Existing complete body/test/update proof
still excludes writes to either input, callbacks and captured accumulators.
The per-iteration row alias cannot supply the invariant key table.
Only the row table publishes an actual optional alias; borrowed key tables and
projection roots clear before a generic continuation. No runtime ABI, permanent
cache or object layout is added.

A closed numeric branch region can project one pure strict-equality pair over
its complete repeated masked period. Both receivers pass the canonical concrete
Array, packed mode, dense logical extent and backing-capacity guards; inputs may
use PackedNumbers or PackedValues. Indexed descriptors, accessors, holes and
Proxy receivers retain the original loop. Current StoredValues preserve every
primitive payload and reference identity; no numeric kind hint changes equality.
The existing strict-equality kernel alone produces the period's Boolean results.
Periods through 64 keep one bit per result in a private i64 local; unsigned
extraction preserves bit 63 and requires no projection allocation or GC access
inside the source loop. Larger periods retain Number zero/one bits in one private
NumberBits array. Both carriers use the same complete preflight and preserve
the original ordered Number arithmetic, structured branches and
completion/publication tail. No generic Value-region
interpreter, persistent cache, helper ABI or object model is introduced.

A first private `var` index must be `(counter + phase) & mask`. Literal induction
proves `0 <= first < limit <= 2^20`, more than one full low-bit period, Increment,
and no body counter writes. After the complete body/write proof, its source
counter is sealed Signed32 and supplies the sole loop test, update and masked
index; no second mirror advances. Existing literal and integer entry checks
retain the negative-zero fallback. Phase must currently be an integral signed-32 Number, so the
original binary64 sum is exact before its observed low bits use wrapping i32.
The accumulator and all unrelated local domains retain their existing rules.

The complete body/test/update proof excludes callbacks, input writes, captured
outputs, index/counter rewrites and uninspected statements. Only direct strict
pairs at the same index enter the all-kind preflight; numeric loose equality
stays a separate Number-only operation. Each branch must have a Number statement
value. The index's previous Value is neither inspected at entry nor published on
a failed admission/zero trip. Input/backing and temporary Value roots clear
before either source loop; the projection releases at the existing exit owner.
Failed admission retains exact TDZ, getter/Proxy, conversion and partial-iteration
Throw behavior. Chunked and whole-trip transformations still refuse this
invariant-bearing region: source arithmetic is never aggregated or unrolled.

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
