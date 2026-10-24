# Ordered collection cursor contract

Actual GC MapObject/SetObject retain stable insertion ordinals and a separate
live size. Their concrete iterator schemas retain the collection edge, next
ordinal, a physical index hint and storage version, closed iteration kind and
terminal done state. Bucket lookup is separate from semantic insertion order.

An update keeps its existing Map ordinal. An absent insertion receives a fresh
ordinal. Deletion retires its retained key/value roots. Full physical tables
compact live entries in order before growing. Clear drops the old backing-array
references and resets physical extent while preserving ordinal progression.
Reinsertion therefore receives a later ordinal. After storage replacement,
cursors use a binary lower-bound search to recover their next physical position;
otherwise they continue from the physical hint. Appends made before exhaustion
remain visible, including after clear, while terminal exhaustion stays permanent.

A yielded live entry advances the saved ordinal and physical hint before
producing its result. A tombstone advances the physical scan before another
entry is inspected.
Exhaustion permanently clears the collection edge and sets done. Entry pairs
and iterator-result objects use the executing builtin's intrinsic Realm.

The consumed GC schema is the layout authority. Raw brands, pointers, offsets,
AUX words and CollectionIteratorCursorState transport codecs have no native
collection consumer. MapIterationKind/SetIterationKind select closed output
shapes; they cannot select a foreign collection layout.

The native collection controls author meaningful mutation/GC/Realm cases,
including pinned cases such as MapIteratorPrototype/next/iteration-mutable.js,
SetIteratorPrototype/next/iteration-mutable.js and live iteration across clear.
The [efficiency checkpoint](../compiler-efficiency-20261024.md) records the
current source-bound verification and its limits; this contract does not publish
a new full pinned-suite result.
