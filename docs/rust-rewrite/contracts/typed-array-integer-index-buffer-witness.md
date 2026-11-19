# TypedArray integer-index buffer witness

Status: implementation, structure and focused runtime verified on 2026-08-24;
the pinned Test262 leaf remains blocked at the harness feature gate.

## Specification boundary

The edition-pinned ECMA-262
[`IsValidIntegerIndex`](https://tc39.es/ecma262/2026/multipage/ordinary-and-exotic-objects-behaviours.html#sec-isvalidintegerindex)
operation returns false for a detached backing buffer, a non-integral,
negative or negative-zero index, an out-of-bounds view, or an index not below
the element length derived from one TypedArray-with-buffer-witness record. None
of those states throws.

`FunctionBuilder::emit_typed_array_valid_integer_index_i32` in
`crates/scriptw-aot-wasm/src/builtins/binary_data.rs` is the shared Wasm-AOT
owner used by integer-indexed `Get`, `HasProperty`, `GetOwnProperty`,
`DefineOwnProperty`, `Set`, `Delete` and method consumers. Callers provide the
already classified numeric-index payload and receive both its integer form and
the validity bit. They retain ownership of the operation-specific result after
that predicate.

Before this migration the helper loaded the backing data pointer and four
private view slots itself, called `emit_typed_array_current_byte_length`,
divided a current byte length by bytes per element and compared the index with
that separately derived quotient. That was the last shared indexed-observation
path explicitly identified in T17 as bypassing the typed witness abstraction.

## Closed non-throwing projection

The helper initializes the validity result to false and keeps all early numeric
index rejection inside one block. Only after the numeric index has been shown
to be a representable non-negative integer does it load one immutable
`TypedArrayViewLocals` value with `emit_load_typed_array_private_state` and
consume one fresh witness through the existing closed projection:

```rust
TypedArrayWitnessUse::IntegerIndexedProperty {
    index_local,
    result_local,
}
```

That projection owns the complete backing-store observation. It reads the
current backing length once, treats a detached or fixed/tracking out-of-bounds
view as having no valid integer indices, preserves a fixed view's stored byte
extent across shrink and regrow, floors a tracking view's available bytes to
whole elements and publishes `index < elementLength` from the same witness.
It never selects `ValidatedMethodEntry` and therefore cannot turn an absent
integer-indexed property into a TypeError.

The helper may not load backing data or length independently, reconstruct view
slots with heap offsets, call either legacy current-byte-length emitter or
divide byte length locally. Its signature accepts only the TypedArray payload,
numeric index and two outputs needed by this already-branded predicate; it does
not claim or preserve an unused tag input.

## Preserved order and result policy

The emitted order is:

1. initialize `result_local` to false and open the shared early-exit block;
2. reject the non-numeric sentinel;
3. reject a fractional numeric index;
4. reject a numeric index below zero;
5. reject a numeric index at or above `2^64`, which cannot enter the unsigned
   Wasm index carrier;
6. convert the remaining numeric index to `index_local`;
7. load the immutable private view and create one live buffer witness;
8. project integer-index validity into `result_local`; and
9. leave the block with false for every rejected numeric or buffer state.

No JavaScript callback or conversion occurs inside this helper, but keeping
numeric classification before the backing-store observation preserves the
emitter's existing control-flow boundary and prevents an invalid key from
touching the buffer path. The helper remains a predicate: it does not read or
write an element, consult the prototype chain, create a descriptor or decide a
caller's return value beyond the validity bit.

## Durable structural guard

The structure test `typed_array_integer_index_witness_structure.rs`, no longer in the tree,
bounded only `emit_typed_array_valid_integer_index_i32`. It requires one
private-state load, one immutable view, one live witness and one
`IntegerIndexedProperty` projection. It pins the four early numeric exits,
their order before the index conversion and witness, the one false
initialization, the one integer output, the absence of a TypedArray tag
parameter and balanced reverse-order temporary release.

Within that body the guard rejects direct TypedArray heap offsets, backing-data
or backing-length loads, both legacy current-byte-length emitters, local
unsigned division and every throwing error path. Those exclusions make a
parallel raw observation or an accidental switch to validating method-entry
semantics visible without snapshotting unrelated binary-data code.

## Focused runtime witnesses

`crates/scriptw-cli/tests/fixtures/wasm_typedarray_has_property.js`, owned by
`typed_array::run_wasm_backend_checks_typedarray_integer_indices_through_buffer_witness`,
is the focused CLI control. It exercises numeric and BigInt views, canonical
and ordinary property keys, detached buffers, cross-Realm receivers, prototype
bypass, tracking growth/shrink/out-of-bounds behavior and fixed-view bounds.
This lane adds a Uint16 length-tracking control proving that a trailing partial
element stays absent until growth completes it, plus fixed-view shrink/regrow
controls proving that the stored extent is restored rather than overwritten by
an out-of-bounds observation.

At Test262 pin `e9d582d6b8b13afc5ba9a676664741592b5c7f69`, the smallest direct
standard control is
`built-ins/TypedArray/out-of-bounds-has.js`. It checks that fixed views become
absent when a resizable backing buffer is too short and become present again
after regrowth. No Test262 execution result is claimed until that exact leaf is
run against this implementation.

The coordinated batch verifier ran:

1. `cargo fmt --all -- --check`;
2. `cargo xc`;
3. `cargo test -p scriptw-aot-wasm --test typed_array_integer_index_witness_structure -- --test-threads=1`;
4. `cargo test -p scriptw-cli --test cli typed_array::run_wasm_backend_checks_typedarray_integer_indices_through_buffer_witness -- --exact --test-threads=1`;
5. the exact Test262 leaf above through the Wasm-AOT backend with `--jobs 1`
   and `--threads 1`, inspecting discovery and every non-success bucket.

The formatter and workspace compile are green. The structure target passes
`2/2`, and the exact CLI fixture passes `1/1`. The Test262 command discovers
the expected two sloppy/strict variants but reports `0/2` before compilation:
both are classified `NotImplemented:Unsupported` because the harness still
declares feature `resizable-arraybuffer` unsupported. This is a recorded
harness capability gap, not a Test262 pass claim and not a focused fixture
regression.

## Explicit nonclaims

This migration does not change canonical numeric index string parsing, any
integer-indexed exotic caller's descriptor/prototype/result policy, element
loads or stores, value coercion, Proxy dispatch, SharedArrayBuffer
synchronization or Atomics ordering. It does not migrate the remaining raw
validated method-entry consumers, constructed-target validation or TypedArray
constructors.

The shared predicate now consumes the universal buffer witness for its one
validity observation, but this does not make the entire integer-indexed exotic
protocol universal: key classification and each internal method's behavior
remain separate owners. It retires no Test262 rewrite, changes no published
count and does not complete TypedArray or T17.
