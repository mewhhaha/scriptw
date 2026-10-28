# Array default-data publication and value probes

`emit_array_indexed_publish_default_data` is a storage operation for a complete
writable, enumerable, configurable own data value. It has the same caller
preconditions as `emit_array_indexed_publish_descriptor`: it does not perform
Array exotic validation, extensibility checks, inherited setter lookup, length
updates or completion routing. The packed Number path can publish scalar bits
without first allocating a StoredValue and PropertyDescriptor; other values
take the complete descriptor path.

Publication crosses a typed runtime helper boundary. The packed growth/copy
kernel and descriptor fallback belong to R, so a program containing many short
Array literals does not repeat that control flow for every element. The caller
still owns element evaluation, abrupt completion and final length publication.

Small dense fixed literals have a separate fresh-only initialization path:
up to eight elements are evaluated once into rooted values, then a Number-tag
guard admits complete packed backings before the Array escapes. The mixed-value
fallback calls the shared publisher using those captured values. This grants
no publication authority over an existing Array and changes no generic Set
semantics; abrupt element evaluation still prevents later effects.

Only callers that already constructed that exact default data descriptor before
raw storage publication use the new operation:

- Fixed literals own a fresh unpublished Array and checked uint32 length. Each
  occupied ordinal is published once, below 2^32-1; holes remain absent.
- Compact recipes admit only validated pure literals and balanced private Array
  frames. Their source-order ordinals and lengths remain recipe-owned.
- JSON parser frames validate Array-value state and unique ordinal. The caller
  retains the `< 2^32-1` branch and updates logical length after publication;
  higher keys still use generic String-property definition.
- Native Object.keys/values/entries results own a fresh private Array, enforce
  preallocated capacity and compact the length before publication.
- Native string/RegExp result builders retain their private Array, ordinal and
  logical-length protocol, including numeric match-index pairs.
- CreateArrayFromList owns a new Realm-selected Array after the complete private
  argument List exists. Its loop reads only that immutable List and checked
  unsigned count. Rest Arrays and other users share this same constructor.

Generic CreateDataPropertyOrThrow, species results, Array.push, Array exotic
DefineOwnProperty and merged descriptors retain their existing semantic checks.
Frozen template elements retain their non-default descriptor attributes. None
of these paths gain raw publication authority from having an Array reference.

`emit_array_own_indexed_value` returns a whole value plus a separate found bit.
Found means an own data value, including own undefined; holes and accessors do
not become hits. It neither follows prototypes nor calls getters. Array-like
argument List construction tries this probe only for a concrete Array and
performs the original full Get otherwise. It captures length once, observes
each subsequent mutation in order, then allocates one final immutable
StoredValue for that argument snapshot. Private native string-result reads use
the probe only where an existing invariant requires a present own data value.

Encoded-body controls require fixed/compact literal and native producer bodies
to reach the shared helper and its actual Number backing writes. A many-small-
literal control also bounds program code growth. Focused engine controls cover IEEE
values, mixed reference transitions and GC, default attributes, inherited
setters, species/length/extensibility boundaries, and apply/spread snapshots
through holes, accessors, mutations and proxies. The
[source-bound checkpoint](../performance-six-workstreams-20261024.md) records the
completed native verification and measured tradeoffs.
