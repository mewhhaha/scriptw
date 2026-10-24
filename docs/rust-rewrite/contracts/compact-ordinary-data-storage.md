# Compact ordinary data storage

Layout 18 introduced compact storage inside the single Wasm GC object model;
the current private ABI is layout 19.
It requires the experimental Wasmtime GC/reference feature baseline. The
[measured checkpoint](../performance-six-workstreams-20261024.md) records native
verification, controlled gains and regressions. Its selected conformance campaign
does not refresh the historical full pinned Test262 result. The subsequent
[efficiency checkpoint](../compiler-efficiency-20261024.md) records layout 19's
indexed metadata and its separate verification.

A genuine source literal with 1–16 unique static string data keys creates
fresh `CompactDataStorage`. Every field has writable, enumerable and configurable
attributes. Empty literals retain the smaller ordinary empty allocator. Values
evaluate once, left to right, through
normal expression and abrupt-completion emission. The allocator owns a fresh
ordinary header with the exact source Realm's Object prototype. The object is
not exposed while its values are being initialized. Duplicate, computed,
spread, method, accessor, prototype-setter and nonenumerable IR forms retain
the general definition path. Distinct static data literals with 17–64 fields
retain the existing descriptor append path; larger forms retain general
literal definition.

`ObjectPropertyStorage` is a fieldless nonfinal GC base with two sealed final
subtypes. It has no allocating constructor. An ordinary header's mutable
storage reference owns either compact storage or an ordinary dictionary.
Compact storage owns mutable packed-byte tags, i64 scalar bits, and nullable
eqref arrays. All JavaScript value tags are supported. Each array has exactly
one slot per source key. Replacing a reference with a scalar clears its old GC
edge; Number payloads preserve their exact bits. Object and Symbol identity
remain normal GC references.

The immutable shape belongs to one literal site and physical ProgramInstance.
It contains only source string keys in insertion order. Literal shapes and
own-data reads have separate checked dense u32 site namespaces, shared across
all source bodies in P. Shapes never contain a receiver, prototype, Symbol or
property value.

An existing own data Set can overwrite the three value arrays when target and
receiver are the same concrete ordinary object. It works on a nonextensible
object because the property already exists and remains writable. Definition,
attribute/accessor changes, Symbol insertion, new keys and deletion use one-way
dictionary conversion. The conversion roots both graphs until every original
key, value and default attribute has been published, then swaps the header's
storage once. Entry-returning APIs first require concrete dictionary storage;
there are no fabricated PropertyEntry records.

The allocation-free ordinary own-value probe distinguishes a present undefined
value from absence. Accessors miss. Full Get remains responsible for prototype
traversal, accessors, proxies and receiver semantics. Reflection creates proper
fresh descriptor snapshots from current compact values without converting the
object or exposing live SoA slots. OwnKeys snapshots use the source key order
and the existing integer-key sorter. The native graph observer reads compact
arrays directly, checks their lengths/tags/references, and creates normal data
descriptors in its host snapshot without calling guest JavaScript.

Each admitted static string read site can own a monomorphic cache in its
ProgramInstance. Both the shaped PropertyRead carrier and canonical GetV with
a literal String operand reach this seam. GetV retains its original operand,
ToObject and ToPropertyKey order before admission. The runtime helper receives
separate lookup and receiver values plus a completed PropertyKey; a miss uses
the common Get operation without repeating user key coercion. Boxed primitive
lookups therefore retain the original primitive getter receiver. Computed keys,
Get and GetMethod retain their existing spec-operation paths. Known primitive,
Array and intrinsic-only targets retain generic Get; unresolved object reads
remain candidates. The optimized runtime checks the live concrete lookup,
live storage subtype and actual immutable shape before loading the current
slot value. Every later read repeats those checks, including reads after calls.
Value-only Set keeps the shape and the cache reads the new value. Dictionary
conversion makes the old shape check fail. Inherited/absent properties and
proxies use full Get and are not cached. Cache records retain only primitive
string/shape metadata and a slot; they cannot retain a user object,
prototype, Symbol or value.

Each ProgramInstance owns a typed directory for each cache kind. Indexed
directories grow geometrically; nullable 64-slot leaf pages allocate on first
use. Untouched pages allocate no leaf slots, while a touched page reserves all
64 nullable slots. Individual metadata records allocate only when admitted.
Warmed lookup uses the checked site index rather than traversing a registry.
Directory storage scales with the highest per-kind site index divided by 64;
leaf storage scales with touched pages. Cache polymorphism, startup allocation
and large-program timing effects still need measurement.
No cache default-policy or V8 performance claim follows from this change.
Encoded declaration and actual imported-call-edge tests establish reachability
and representation guards; native semantic and snapshot tests establish the
observable mutation, descriptor, key-order and rooting behavior.

Source/layout accounting for an admitted literal is six per-object GC records:
the object header, its prototype StoredValue, compact storage and three value
arrays. The shape and its key array are shared per literal/ProgramInstance,
with one lazy metadata record per admitted executed read site. Each physical
ProgramInstance additionally owns the shared directories and touched leaf pages
described above. The descriptor-append
dictionary path retains `5 + 4n` records for `n` nondeleted fields: header,
prototype, storage, ordered table, index, and key/value/descriptor/entry records
per field. Value-expression allocations and shared pooled strings are excluded.
Old growth backings are allocation churn rather than retained records. These
are path-derived counts; neither semantic snapshots nor RSS measure dynamic
allocation totals or live JS bytes. Construction scaling, process RSS, GC
capacity and repeated application timings supply separate measured controls.
