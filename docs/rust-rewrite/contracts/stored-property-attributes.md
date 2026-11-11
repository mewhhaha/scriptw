# Stored property attribute shape

Current source status (2026-10-05): the T05 semantic Wasm-GC cutover is source-authored and unverified. Earlier source recipes, caller counts, and dated verification below describe their recorded checkpoints; they do not verify this cutover. ECMAScript requirements and retained fixtures remain acceptance criteria. Semantic values and Realm ownership now use the typed GC/compiler path described in [the GC value contract](../value-heap-gc.md). The retired representation assertions add no Test262 skips or new passing results.

## Closed static boundary

`StoredPropertyAttributes` is the sole crate-visible static input to a stored
descriptor-kind word. Its `Data` variant carries named `writable`, `enumerable`
and `configurable` fields. Its `Accessor` variant carries only `enumerable` and
`configurable`; an accessor descriptor cannot acquire a writable input because
that state is absent from the type.

One exhaustive `descriptor_word` projection delegates the variants to the
heap-private `DescriptorWord::of_data` and `DescriptorWord::of_accessor`
constructors. Both
raw constructors are inaccessible outside `heap.rs`; no alternate
crate-visible positional boolean route remains. The projection has no wildcard
or fallback. All fourteen data producers and two accessor producers construct
a named variant with named fields.

## Scope and evidence

This is source-equivalent type hardening around statically known attributes.
It changes neither descriptor bits nor heap layout, property definition order,
runtime descriptor validation, exotic behavior, emitted Wasm or conformance
counts.

```sh
cargo test -p scriptw-aot-wasm --test structure_language -- stored_property_attributes_structure::
```

The Batch-R boundary target passes `4/4`, and `cargo xc` is green. Batch R
makes the raw constructors private and migrates every remaining external
producer. Semantic goldens were not rerun for this source-equivalent type
hardening.
