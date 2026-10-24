# Native ordered collections use one GC object model

Map/Set native dispatch consumes actual MapObject/SetObject, entry and iterator
schemas. A separate GC hash index supplies lookup; the ordered GC entry table
and raw i64 ordinal array preserve insertion order. Bucket links encode entry
offsets plus one, never
reference addresses. Completed insertion appends; deletion unlinks before it
drops both retained key/value roots. Full tables compact live entries in order
before growing and rebuild bucket links. Clear drops old backing references
and resets physical extent without resetting insertion ordinals. Cursors retain
the next ordinal plus a physical hint and version; replacement invalidates the
hint and triggers a binary lower-bound seek. Exhaustion clears the collection
edge and is permanent.

String/BigInt hashes derive from primitive contents. Number hashing unifies
NaNs and signed zero consistently with SameValueZero. Object/Symbol keys receive
stable private IDs from one typed module counter, retained in GC fields without
observable property access. Counter exhaustion traps before reuse. The hash
index remains bounded by geometrically grown physical capacity; lifetime
insertion/deletion history does not force indefinite table growth. The native
receiver gate accepts the exact concrete reference and rejects a Proxy.

Constructors observe newTarget/prototype before their adder, then acquire one
completed cached iterator record. Entry validation, ordered key/value Gets,
conversion and adder/callback failures close that owner with the original whole
Throw. IteratorStepValue failures propagate through their direct spec path.
GetOrInsertComputed validates its callback even for an existing key, passes
canonical positive zero, and probes the current collection after the callback.

FromEntries uses own data-property definition. GroupBy converts each key once,
retains original items in ordered fresh Arrays and creates a null-prototype
Object or intrinsic Map. Set-like admission observes size/ToNumber/integer
validation/has/keys once in order. Set algebra uses its required size branch,
private difference copy, live receiver membership and duplicate-key suppression.
Union/symmetric difference call keys and cache next before copying the receiver.
Only early other-iterator predicate exits close with a Normal completion.
Results use the executing builtin's Realm and actual intrinsic prototypes.

Nine finite paired strict/sloppy Engine controls are authored for keys, GC,
live cursors, callbacks, constructor Close, computed insertion, grouping,
Set-like branch mutation, predicate Close and borrowed Realms. Current native
and selected original-case verification belongs to the
[efficiency checkpoint](../compiler-efficiency-20261024.md), with its exact
source and resource limits. This contract does not refresh the full-suite chart.

Normative algorithms: [TC39 keyed collections](https://tc39.es/ecma262/multipage/keyed-collections.html),
[FromEntries](https://tc39.es/ecma262/multipage/fundamental-objects.html#sec-object.fromentries),
and [GroupBy](https://tc39.es/ecma262/multipage/abstract-operations.html#sec-groupby).

String and Symbol key hashing is shared with the ordinary-property index through
`operations/property_key_hash.rs`. The full UTF-16 content loop, stable nonzero
Symbol identity field and tag mix are unchanged; SameValueZero and the remaining
Number/BigInt/object hashing rules stay owned by the collection implementation.
