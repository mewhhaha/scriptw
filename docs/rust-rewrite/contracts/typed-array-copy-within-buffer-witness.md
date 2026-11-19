# TypedArray `copyWithin` buffer witness

Status: implemented, independently reviewed, and focused-verified for the T17
Wasm-AOT `%TypedArray%.prototype.copyWithin` invariant lane on 2026-08-23.

## Specification boundary

The edition-pinned ECMA-262
[`%TypedArray%.prototype.copyWithin`](https://tc39.es/ecma262/2026/multipage/indexed-collections.html#sec-%typedarray%.prototype.copywithin)
algorithm has two distinct live observations of the receiver:

1. `ValidateTypedArray(O, seq-cst)` creates the entry witness and
   `TypedArrayLength` captures `len` before any argument coercion.
2. Only when the initially calculated `count` is greater than zero, the
   algorithm creates a fresh witness after argument coercion, rejects an
   out-of-bounds view and derives a current `len` from that observation.

The target, start and optional non-undefined end values are converted with
`ToIntegerOrInfinity` in that order. Their clamped indices and the initial
`count` are all calculated against the entry length. Growth during coercion
therefore cannot extend that initial range. The conditional second observation
may instead reduce the copy to the longest prefix that remains applicable
after shrinkage. Detachment or a fixed view becoming out of bounds during
coercion throws only when that positive-count branch performs its second
observation.

The copy itself operates on bytes so that it preserves the source values'
bit-level encodings. When the source and destination byte ranges overlap in
the hazardous direction, it copies backward; otherwise it copies forward. The
method returns the original receiver.

The compiler owner is
`FunctionBuilder::compile_typed_array_prototype_copy_within_builtin` in
`crates/scriptw-aot-wasm/src/builtins/standard.rs`. At the inventoried
pre-migration baseline it reconstructed the four private view slots, called
`emit_validate_typed_array_current_byte_length` twice and divided a byte-length
local by bytes per element after each call. Those raw observations are the
migration target. The dispatcher owner remains the single
`StandardBuiltinId::TypedArrayPrototypeCopyWithin` arm.

## One immutable view, two observations

No new witness policy is needed. After the receiver-brand guard, the compiler
must load private state exactly once with
`emit_load_typed_array_private_state` and construct exactly one immutable
`TypedArrayViewLocals` value. Both live observations must consume that same
view through the existing closed projection:

```rust
TypedArrayWitnessUse::ValidatedMethodEntry { length_local }
```

The compiler must contain exactly two static calls to
`emit_typed_array_witness`:

1. the unconditional entry call publishes `receiver_length_local`; and
2. the call inside the positive-count branch publishes
   `current_length_local` after every argument coercion.

Each call makes a fresh backing-store data and byte-length observation,
rejects detachment and fixed or tracking out-of-bounds state through the
executing builtin's Realm, floors a tracking view's available bytes to whole
elements and publishes the element length from that one observation. The name
`ValidatedMethodEntry` describes this closed validating projection; its
semantics also exactly match the algorithm's later make-witness,
out-of-bounds-check and `TypedArrayLength` sequence.

The `TypedArrayViewLocals` record remains immutable across both observations.
In particular, neither witness may replace its stored fixed byte extent with a
current byte length. A fixed view that temporarily becomes out of bounds and
later returns in bounds must retain its original extent, while a tracking view
must derive a fresh whole-element length at each observation.

The compiler may read the immutable view's byte offset and bytes per element
after the second observation to form copy addresses. It may not reconstruct
those slots independently, observe buffer byte length through another helper,
derive either element length locally or add a third witness.

## Preserved observable order

The implementation must preserve this order:

1. reject a non-TypedArray receiver;
2. load one immutable private view;
3. perform the first `ValidatedMethodEntry` observation and capture
   `receiver_length_local`;
4. coerce and clamp target against the captured length;
5. coerce and clamp start against the captured length;
6. if end is present and not `undefined`, coerce and clamp it against the
   captured length; otherwise use the captured length;
7. calculate the initial `count` solely from those captured-length indices;
8. if that count is zero, skip the second witness and byte-copy setup and
   return the receiver;
9. otherwise perform the second `ValidatedMethodEntry` observation into
   `current_length_local`;
10. cap count independently by the non-negative source and destination
    availability in that current length;
11. only then load backing data, form byte addresses, select forward or
    backward direction and perform the bytewise copy; and
12. publish the original receiver.

No copy-data/address setup may move before the second observation. The entry
witness necessarily observes backing data while validating detachment; it is
not copy setup. The second observation may not move before target, start or end
coercion. The first observation may not move after any coercion, and the second
observation must remain conditional rather than becoming an unconditional
revalidation.

## Durable source guard

The structure test `typed_array_copy_within_witness_structure.rs`, no longer in the tree,
bounded only the body from
`compile_typed_array_prototype_copy_within_builtin` to
`compile_typed_array_prototype_to_reversed_builtin`, plus its dispatcher arm.
It must require:

- one receiver-brand rejection before private-state access;
- exactly one `emit_load_typed_array_private_state`, one
  `TypedArrayViewLocals::new`, two `emit_typed_array_witness` calls and two
  `TypedArrayWitnessUse::ValidatedMethodEntry` projections;
- the first witness writing only `receiver_length_local` and the second
  writing only `current_length_local`, with no later writer to either local;
- no writer to the four locals captured by the immutable view after its one
  private-state load;
- the complete coercion and count-calculation sequence between the two
  witnesses, including target/start/end clamping to `to_local`, `from_local`
  and `final_local` respectively and the argument-count/`undefined` guards
  around end coercion;
- the second witness, both current-length caps, backing-data/address setup,
  overlap selection and complete byte-copy loop all before the matching end of
  the existing positive-count branch, with receiver publication after it;
- both current-length availability caps and the sole explicit copy-data load
  after the second witness;
- the bytewise loop, overlap-direction selection and original-receiver result;
  and
- exactly one dispatcher mapping from
  `StandardBuiltinId::TypedArrayPrototypeCopyWithin` to this compiler.

Within that bounded body the guard must reject
`emit_validate_typed_array_current_byte_length`,
`emit_typed_array_current_byte_length`, `Instruction::I64DivU`,
`emit_throw_runtime_error`, `TYPE_ERROR_NAME` and direct uses of
`HEAP_TYPED_ARRAY_VIEWED_BUFFER_OFFSET`, `HEAP_TYPED_ARRAY_BYTE_OFFSET`,
`HEAP_TYPED_ARRAY_BYTE_LENGTH_OFFSET` or
`HEAP_TYPED_ARRAY_BYTES_PER_ELEMENT_OFFSET`. Those exclusions make a bypass
of the typed witness or a reconstruction of its source slots
structure-visible without snapshotting the large standard-builtin emitter.

## Focused runtime witnesses

The durable CLI control remains
`crates/scriptw-cli/tests/fixtures/wasm_typedarray_prototype_copy_within.js`, owned
by
`typed_array::run_wasm_backend_copies_typedarray_bytes_with_spec_ordering`.
It pins descriptor shape, result identity, both overlap directions, non-zero
byte offsets, floating-point bit preservation, BigInt elements,
target/start/end coercion order, internal rather than public length, tracking
shrink and grow, fixed-view invalidation, entry detachment, detachment during
coercion and invalid receivers. It also contains a positive-count control for
detachment during end coercion and a zero-count control proving that the
conditional second observation is skipped; no parallel fixture family is
needed.

At the current Test262 pin
`e9d582d6b8b13afc5ba9a676664741592b5c7f69`, the exact focused leaves are:

- `built-ins/TypedArray/prototype/copyWithin/detached-buffer.js`;
- `built-ins/TypedArray/prototype/copyWithin/return-abrupt-from-this-out-of-bounds.js`;
- `built-ins/TypedArray/prototype/copyWithin/coerced-target-start-end-shrink.js`;
- `built-ins/TypedArray/prototype/copyWithin/coerced-target-start-grow.js`;
- `built-ins/TypedArray/prototype/copyWithin/coerced-values-start-detached.js`;
  and
- `built-ins/TypedArray/prototype/copyWithin/coerced-values-end-detached.js`.

Together they witness entry rejection before argument work, post-coercion
rejection, current-length prefix truncation and the rule that growth cannot
extend the range captured at entry. Each physical leaf must be invoked by its
complete suite-relative path with the Wasm-AOT backend, `--jobs 1`,
`--threads 1` and the repository timeout. Verification must inspect discovery
totals and every failure bucket rather than infer success from process status.

Under the shared eight-core, 22 GB cap, the durable structure suite passes
`3/3`, and the exact CLI fixture passes `1/1`. All six Test262 leaves were run
separately and produced exactly `12/12` passing Wasm-AOT variants with every
failure and non-success bucket at zero under `--jobs 1 --threads 1`. The
fixture's internal-length control now inspects the indexed values directly so
its deliberately throwing own `length` getter is not invoked by the assertion
helper itself.

## Explicit nonclaims

This lane does not change index clamping, byte-copy semantics, overlap
direction, bit preservation, result identity, SharedArrayBuffer ordering or
the TypedArray integer-indexed exotic protocol. It introduces no new witness
variant and does not change `TypedArrayViewLocals` or the shared witness's
semantics.

It does not migrate `%TypedArray%.prototype.with`, `set`, constructor
validation, species-target validation or another raw TypedArray consumer.
`slice` and `subarray` have separate buffer-witness contracts. It does not
modify Test262, materializers, harness adaptations,
published conformance counts or the README status. The current CLI fixture
does not prove created-Realm error-prototype identity; source structure proves
only that both validating observations route failures through the shared
current-function-Realm witness. This invariant lane is not a broad Test262
refresh and does not complete `copyWithin`, TypedArray or T17 by itself.
