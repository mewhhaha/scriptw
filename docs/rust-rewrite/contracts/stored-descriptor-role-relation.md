# Stored descriptor role relation

Status: normative for the AOT descriptor-compatibility boundary.

Descriptor VALUE has the typed PropertyValue sum, including deferred intrinsic
recipes. GETTER and SETTER are nonnull references to immutable StoredValue
wrappers. The containing PropertyDescriptor has mutable fields; its identity
and ownership remain separate from those shared immutable edges.

## Closed field roles

The stored descriptor role relation keeps complete records at the boundary.

Compatibility receives a complete `GcLocal<PropertyDescriptor>` and a validated
partial `WasmDescriptor`. Callers cannot transpose data, getter, and setter locals
at that boundary. The closed `DescriptorField` mapping selects each named field.
Data reads resolve PropertyValue recipes; accessor reads decode StoredValue.
Compatibility retains the original SameValue checks and complete abrupt results.

A merge preserves the exact current VALUE, GETTER and SETTER edges when their
fields are omitted. Getter/setter roots start with canonical stored undefined;
kind conversion resets them before supplied fields are applied. Only a present
incoming getter/setter is boxed, inside its runtime presence guard. Both result
kinds construct a complete descriptor using those retained edges. Attribute-only
changes preserve deferred VALUE recipes without manufacturing new callables.
See [lazy runtime values](lazy-runtime-values.md).

A completed Proxy target snapshot constructs a fresh mutable descriptor while
sharing only its immutable value edges. Later redefinition updates the live
record's edges independently. Merge publication and complete-record copying
retain their existing ownership; sharing wrappers cannot alias mutable records.

## Durable guard and scope

`stored_descriptor_role_relation_structure` checks exact layouts, field-to-role
mappings, canonical initialization/reset, presence-guarded boxing, fresh snapshot
records and complete typed readers. Its recursive Rust-lexical census excludes
comments and normal, raw, byte, C-string and character literals. The closed
`emit_descriptor_value` census is five: one reader, two compatibility sites,
Arguments value and Namespace value. Merge's former two accessor decode calls
are absent because it retains those immutable edges directly.

Source guards establish these ownership and routing invariants. Actual Wasm
emission and native semantic tests separately cover partial definitions, kind
conversion, nonconfigurable properties, reentrant Proxy snapshots, GC and
deferred intrinsic aliases. Source checks alone establish neither runtime
conformance nor a performance improvement.
