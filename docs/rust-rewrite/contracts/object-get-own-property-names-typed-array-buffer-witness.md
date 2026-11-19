# `Object.getOwnPropertyNames` TypedArray buffer witness

Status: focused-verified for the T17 Wasm-AOT invariant lane on 2026-08-24.

## Specification boundary

This contract is pinned to the ECMA-262 2026 edition. `Object.getOwnPropertyNames`
converts its argument to an object, obtains `[[OwnPropertyKeys]]`, and returns
only the String keys. For an integer-indexed exotic object,
[`[[OwnPropertyKeys]]`](https://tc39.es/ecma262/2026/multipage/ordinary-and-exotic-objects-behaviours.html#sec-integer-indexed-exotic-objects-ownpropertykeys)
derives one current element length from a TypedArray-with-buffer witness. It
then emits integer-index strings in ascending order before the object's
ordinary String keys. Symbol keys are not part of the returned names.

This is a non-throwing buffer observation. A detached or out-of-bounds
TypedArray contributes zero integer-index keys instead of throwing. A fixed
view that becomes valid again after backing-buffer regrowth recovers its stored
element extent. A length-tracking view exposes only complete elements, so a
trailing partial element does not contribute a key.

## Migrated owner

The sole owner in this lane is
`FunctionBuilder::compile_object_get_own_property_names_builtin` in
`crates/scriptw-aot-wasm/src/builtins/object.rs`. Its direct TypedArray branch
previously loaded the viewed buffer, byte offset, stored byte length and bytes
per element itself, called `emit_typed_array_current_byte_length`, and divided
the returned bytes locally.

The branch now loads one immutable `TypedArrayViewLocals` value through
`emit_load_typed_array_private_state` and consumes exactly one witness:

```rust
TypedArrayWitnessUse::ArrayLikeLengthSnapshot {
    length_local: typed_array_length_local,
}
```

`ArrayLikeLengthSnapshot` is the closed projection for this owner because it
publishes zero for detached or out-of-bounds views. `ValidatedMethodEntry`
would incorrectly turn `[[OwnPropertyKeys]]` into a throwing TypedArray method
entry. `IntegerIndexedProperty` would answer one candidate index rather than
produce the single length snapshot needed to construct the complete key list.

The owner may not call either raw current-byte-length emitter, read private
TypedArray view slots directly, observe the backing length or data separately,
or derive the element length with local byte arithmetic. The stored byte-length
local remains immutable view state; it is not reused for the witness result.

## Preserved dispatch and key order

The existing Proxy `ownKeys` dispatch remains before the direct TypedArray
branch, including its abrupt and invariant behavior. Primitive boxing,
ordinary objects, Arrays, Arguments objects, boxed strings and other
non-TypedArray cases keep their existing paths.

Within the direct TypedArray branch, the witness runs before allocation and
supplies the numeric-key prefix length. The owner then scans the existing
ordinary property storage, excludes Symbols and recognized array-index keys,
allocates the exact result, writes `"0"` through `length - 1` first, and writes
the retained ordinary String keys afterward. Detachment or an out-of-bounds
view changes only that numeric prefix; it does not hide ordinary String keys.

## Durable evidence

The structure test `object_get_own_property_names_typed_array_witness_structure.rs`, no longer in the tree,
bounded the owner at the following `Object.getOwnPropertySymbols` compiler. It
requires the unique standard-builtin dispatch edge, one private-state load, one
immutable view and one `ArrayLikeLengthSnapshot` witness, fixes brand-check and
Proxy-dispatch order, and rejects raw validators, direct view/backing-store
observations, local byte division and throwing witness policy. It also pins
reverse-order release of the new view locals and the focused CLI test/fixture
connection.

`crates/scriptw-cli/tests/fixtures/wasm_object_get_own_property_names_typed_array_witness.js`,
owned by
`typed_array::run_wasm_backend_get_own_property_names_uses_typedarray_buffer_witness`,
checks:

- detached and fixed out-of-bounds views retain ordinary enumerable and
  non-enumerable String keys while exposing no integer keys;
- fixed-view regrowth restores its original integer-key extent;
- odd-byte shrink and growth of a length-tracking `Uint16Array` expose only
  complete elements;
- an offset length-tracking view becomes empty when its offset is out of
  bounds; and
- Symbol keys never enter the returned name list.

At Test262 pin `e9d582d6b8b13afc5ba9a676664741592b5c7f69`, there is no direct
`Object.getOwnPropertyNames` TypedArray leaf. The smallest exact adjacent
`[[OwnPropertyKeys]]` resizable-buffer cohort is:

- `built-ins/TypedArrayConstructors/internals/OwnPropertyKeys/integer-indexes-resizable-array-buffer-auto.js`; and
- `built-ins/TypedArrayConstructors/internals/OwnPropertyKeys/integer-indexes-resizable-array-buffer-fixed.js`.

Those leaves call `Reflect.ownKeys`, so they validate the shared
integer-indexed exotic length policy but are not direct evidence for this
`Object.getOwnPropertyNames` compiler or its String-only filtering. Both exact
leaves pass all four sloppy/strict Wasm-AOT variants with every failure and
non-success bucket at zero under `--jobs 1 --threads 1`.

## Verification

`cargo fmt --all -- --check`, `cargo xc`, `git diff --check`, the task-plan
check and fixture syntax check are green. The bounded structure target passes
`3/3`, including its standard-dispatch edge, and the exact CLI fixture passes
`1/1`. The adjacent Test262 controls pass `4/4` as described above.

## Explicit nonclaims

This lane does not change Proxy own-key validation, ordinary key storage,
property-key classification, Symbol enumeration, result allocation, or any
other `Object`/`Reflect` builtin. It does not migrate the separate
`Reflect.ownKeys` compiler or the remaining raw TypedArray observations in
Array builtins or indexed object read/write emitters. The last standard-builtin
owner, `subarray`, has a separate buffer-witness contract.

It retires no Test262 rewrite, changes no aggregate or published conformance
count, and does not complete integer-indexed exotic objects, TypedArrays or
T17.
