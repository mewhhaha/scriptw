# Stored descriptor role relation

Status: normative for the AOT descriptor-compatibility boundary.

In the current GC backend, descriptor VALUE has the typed PropertyValue sum;
GETTER and SETTER accept only immutable StoredValue snapshots. Compatibility
reads and descriptor exposure resolve internal intrinsic recipes before applying
SameValue. Merging a partial descriptor preserves its current VALUE edge when
no new value is supplied, and kind conversion restores the canonical undefined
snapshot. Attribute changes therefore retain lazy values without manufacturing
new callables. See [lazy runtime values](lazy-runtime-values.md).

## Closed field roles

`StoredDescriptorLocals::new` accepts the distinct
`StoredDescriptorDataLocals`, `StoredDescriptorGetterLocals`, and
`StoredDescriptorSetterLocals` roles. This is the
stored descriptor role relation: the three wrappers have private fields, and only their named
constructors accept a generic `TaggedLocals` value. A validator producer
cannot transpose data, getter, and setter locals because the constructor
rejects the wrong role at compile time.

Array and Arguments indexed storage deliberately project their shared
data-or-getter carrier into both the data and getter roles. Ordinary named
storage allocates three independent carriers and labels each before loading its
heap offsets. The existing validator then consumes the same data, getter, and
setter values in the same `SameValue` checks and releases the same temporary
locals in the same order.

## Durable guard and nonclaims

`stored_descriptor_role_relation_structure` uses a recursive Rust-lexical
census that excludes comments and normal, raw, byte, C-string and character
literals. It pins the private one-field roles, the typed aggregate constructor,
the exact three producers, and each producer's complete role set. Its lexical
probe prevents comments, nested comments, raw identifiers and literals from
making the census vacuous.

This is source-equivalent type hardening. It does not change descriptor
classification, compatibility, heap layout, Array or Arguments exotic
behavior, Proxy behavior, Realm selection, or conformance counts.

At `2026-08-27`, the dedicated structure target passes `4/4`, the neighboring
Arguments descriptor structure target passes `4/4`, and the exact Array
descriptor CLI witness passes `1/1`. The older Array descriptor structure target
passes `2/3`; its pre-existing validator-body assertion still spells a closure
receiver as `self` where the current source uses `builder`. The mapped-Arguments
CLI witness is currently blocked outside this boundary because two active
callers reference the removed `FunctionArgumentsProtocol::present` method.
Broad Object and Test262 verification remain deferred.
