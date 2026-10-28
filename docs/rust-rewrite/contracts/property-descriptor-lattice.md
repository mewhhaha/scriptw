# Property descriptors

`scriptw-ir/src/property_descriptor.rs` owns the closed descriptor domain.
`classify` derives data, accessor and generic kinds from field presence;
`[[Writable]]` alone makes a data descriptor. Absent, statically present and
runtime-dependent presence remain distinct. Validation rejects a descriptor
with both data and accessor fields before an object is mutated.

A descriptor mask describes fields to query or update; a stored descriptor word
describes a legal value. They are different types. Kind-independent attribute
updates cannot manufacture an accessor descriptor with a writable data bit.
Consumers match the closed kind and presence domains exhaustively.

`ValidateAndApplyPropertyDescriptor` finishes compatibility checks before the
first store. Non-configurable comparisons use SameValue, including NaN, signed
zero and exact reference identity. An omitted field preserves the old field;
a real kind transition supplies the new kind's defaults. A statically true
kind-change condition must not disappear merely because it has no runtime
terms. Array length changes follow successful indexed-property admission.

Descriptor-shaped JavaScript objects have one construction owner,
`objects/descriptor_object.rs`. It publishes fields in the specification's
order with CreateDataPropertyOrThrow attributes. Reading an accessor descriptor
returns its getter/setter values without invoking them.

Ordinary, Array-index, Array named-property and Arguments consumers retain
these rules through their actual exotic operations. Arguments mapping and
one-sided accessor updates must not expose stale payloads after a kind change.
See [Arguments descriptors](arguments-index-descriptor-exotic.md) and
[stored attributes](stored-property-attributes.md). The runtime representation
comes from the GC schema, not an older descriptor bitfield recipe.
