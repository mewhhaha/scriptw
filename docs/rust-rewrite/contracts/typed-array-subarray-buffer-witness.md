# TypedArray `subarray` buffer witness

Current source status, 2026-10-05: the atomic Wasm-GC rewrite is authored only. Compilation, emitted Wasm, focused controls, real agents and full pinned conformance remain unverified. No status counts changed.

Subarray requires the real TypedArray brand and retains its nonthrowing source length snapshot, including zero for an out-of-bounds source. Original begin/end coercions precede species construction. The selected original source buffer, element kind, byte offset and tracking state determine the complete argv; the shared Construct owner then validates the actual result's concrete brand, write admission and Number/BigInt content type. Default constructors are saved defining-Realm intrinsics, never public globals.

Four paired strict/sloppy finite Engine cohorts in `aot_gc_binary_data_entries.rs` cover native buffers, DataView, TypedArray construction/statics/species and Atomics/Realm lifecycle. Existing CLI semantic fixtures remain; obsolete raw-spelling guards are retired rather than replaced with mirrors. The historical implementation and receipts below do not certify this batch.

## Historical record before the atomic GC rewrite


Status: focused-verified for the T17 Wasm-AOT source-length and post-species
result-validation boundaries on 2026-08-25.


The current 2026-10-03 dry species batch preserves this non-throwing source
snapshot and complete tracking/fixed argument vector. The shared private factory
now performs Proxy-aware Construct and result brand/fresh-view/content acceptance,
and final publication consumes its result owner. It imposes no minimum-length
requirement on the buffer argument-vector route. Historical source layout and
verification below remain scoped to their original checkpoint. See
[shared species ownership](typed-array-species-create-ownership.md).

## Specification boundary

This contract is pinned to the ECMA-262 2026
[`%TypedArray%.prototype.subarray`](https://tc39.es/ecma262/2026/multipage/indexed-collections.html#sec-%typedarray%.prototype.subarray)
algorithm and Test262 revision `e9d582d6b8b13afc5ba9a676664741592b5c7f69`.

`subarray` is deliberately not a validated TypedArray method-entry
observation. After requiring the TypedArray internal slots, it creates one
TypedArray-with-buffer witness and derives `srcLength`. A detached or currently
out-of-bounds receiver contributes length zero; it does not throw before
argument coercion. The method then:

1. coerces and clamps `start` against that captured length;
2. defaults `end` to the same length or coerces and clamps an explicit value;
3. computes `beginByteOffset` from the immutable stored source byte offset;
4. passes only `(buffer, beginByteOffset)` when the source is length-tracking
   and `end` is `undefined`, preserving a length-tracking result;
5. otherwise passes `(buffer, beginByteOffset, newLength)`, producing a fixed
   result; and
6. performs species construction and validates that the returned TypedArray is
   neither detached nor currently out of bounds; and
7. checks that the validated result has the same Number/BigInt content type.

The source witness is not repeated after coercion or species lookup. Growth and
shrinkage during either coercion cannot change the captured index range. The
selected species constructor may observe the passed source buffer when it
processes the buffer, byte offset and optional length. After construction, the
distinct result witness observes the returned TypedArray's backing-store state;
that result may or may not share the source buffer.

For a detached source, the selected constructor normally throws after both
arguments and species have been observed. When species resolution selects an
explicit constructor, that TypeError belongs to the selected constructor's
Realm, not to the executing `subarray` builtin. A custom species may ignore the
detached buffer and return a compatible in-bounds TypedArray, in which case
`subarray` succeeds. A detached or currently out-of-bounds result is rejected
through the executing `subarray` builtin's Realm. An initially out-of-bounds
resizable source can likewise become in bounds during coercion, but its
start/end normalization still uses the earlier zero-length snapshot while its
stored byte offset is retained for construction.

## Migrated owner

The sole product owner is the
`StandardBuiltinId::TypedArrayPrototypeSubarray` arm in
`crates/scriptw-aot-wasm/src/builtins/standard.rs`. Before this lane it loaded the
source viewed buffer, byte offset, stored byte length and bytes per element
directly, called `emit_typed_array_current_byte_length`, divided the returned
bytes locally and overwrote the stored-byte-length carrier.

The arm now loads one immutable view:

```rust
let typed_array_view = TypedArrayViewLocals::new(
    receiver_payload_local,
    buffer_payload_local,
    byte_offset_local,
    stored_byte_length_local,
    bytes_per_element_local,
);
```

and consumes exactly one source projection:

```rust
TypedArrayWitnessUse::ArrayLikeLengthSnapshot {
    length_local,
}
```

`ArrayLikeLengthSnapshot` owns the source's one cached backing byte-length and
data observation, fixed versus length-tracking out-of-bounds calculation,
fixed-view stored extent and whole-element flooring. `ValidatedMethodEntry` is
forbidden for that source observation: it would incorrectly throw before
`start` and `end` coercion. `IntegerIndexedProperty` and the accessor projection
expose different result domains.

After species construction and the result brand check, the arm loads a separate
immutable result view and consumes exactly one result projection:

```rust
TypedArrayWitnessUse::ValidatedMethodEntry {
    length_local: result_length_local,
}
```

That projection implements the `ValidateTypedArray` state boundary for the
constructed result. It rejects a detached or currently out-of-bounds result
through the executing builtin's Realm before the result element kind is read.
The source and result views use distinct locals, so post-species validation
cannot overwrite or repeat the captured source length.

The separate source length-tracking flag and element kind remain direct
immutable metadata reads. The former selects the normative two- versus
three-argument species construction shape; the latter selects the intrinsic
default constructor and participates in the result content-type comparison.
Neither is a second backing-store length observation.

## Preserved observable order and result shape

The emitted order remains:

1. reject a receiver without the TypedArray brand through the executing
   builtin's Realm;
2. load one immutable source view, source element kind and source
   length-tracking flag;
3. create the non-throwing witness and capture `length_local`;
4. load, convert and clamp `begin` against that snapshot;
5. initialize `endIndex` from the snapshot, then convert and clamp an explicit
   non-`undefined` `end` against it;
6. compute `newLength` and `beginByteOffset` using the immutable element size
   and stored source byte offset;
7. select the intrinsic constructor from the source element kind;
8. read `receiver.constructor` and its `@@species` property;
9. prepare three arguments, reducing the actual count to two only for a
   length-tracking source with `end === undefined`;
10. construct the result and require the TypedArray brand;
11. load a separate immutable result view and validate that its buffer is
    attached and the view is currently in bounds;
12. reject a Number versus BigInt content-type mismatch; and
13. publish the existing normal completion.

The result's buffer sharing, byte offset, fixed/length-tracking shape and
element-kind selection remain constructor-owned. This lane does not add a late
source revalidation or copy elements; `subarray` creates a view over the passed
buffer.

## Durable structural and runtime evidence

The structure test `typed_array_subarray_witness_structure.rs`, no longer in the tree, bounded
only the `TypedArrayPrototypeSubarray` arm through the following `DateNow` arm.
It requires one private-state load, one immutable view and one
`ArrayLikeLengthSnapshot` witness, plus one separate result private-state load,
immutable result view and `ValidatedMethodEntry` witness. It rejects both
legacy byte-length emitters, direct view-slot or backing-store observations,
wrong witness projections, local byte division and writes to immutable source
view or witness-result locals.

The guard pins the public `"subarray"` installation to
`TypedArrayPrototypeSubarray`, then pins the receiver check; source view, element-kind and
length-tracking metadata order; source-witness-before-coercion boundary;
begin/end and species order; exact length-tracking two-argument override;
construction and result-brand validation before the distinct result witness;
result witness before content-type validation; reverse-order local release; and
focused CLI fixture connection.

`crates/scriptw-cli/tests/fixtures/wasm_typedarray_subarray_buffer_witness.js`,
owned by
`typed_array::run_wasm_backend_subarray_uses_non_throwing_typed_array_buffer_witness`,
checks:

- a fixed `Uint16Array` result becomes out of bounds after shrink and restores
  its stored extent after regrowth;
- omitting `end` on a length-tracking source creates a length-tracking result,
  while explicit `end` creates a fixed result;
- odd available byte lengths are floored to complete `Uint16` elements;
- begin and end coercion precede species construction and pass the expected
  buffer, byte offset and element length;
- an initially out-of-bounds fixed source snapshots zero length, still coerces
  begin, regrows, and constructs at the stored byte offset;
- a detached source still coerces both arguments before its intrinsic
  constructor throws, while a custom species can return a compatible result;
- borrowing another Realm's method onto an entry-Realm detached receiver keeps
  the later explicitly selected constructor TypeError in that constructor's
  Realm;
- a custom species returning an already-detached TypedArray is rejected with a
  TypeError from the borrowed method's Realm;
- a custom species that makes a fixed result view out of bounds before returning
  it is rejected with a TypeError from the borrowed method's Realm; and
- default `Uint16Array` and `BigUint64Array` results retain their element kind.

## Exact pinned Test262 inventory

The smallest direct cohort covering the migrated observation and retained
construction boundary is:

- `built-ins/TypedArray/prototype/subarray/resizable-buffer.js`;
- `built-ins/TypedArray/prototype/subarray/result-byteOffset-from-out-of-bounds.js`;
- `built-ins/TypedArray/prototype/subarray/coerced-begin-end-grow.js`;
- `built-ins/TypedArray/prototype/subarray/coerced-begin-end-shrink.js`;
- `built-ins/TypedArray/prototype/subarray/detached-buffer.js`; and
- `built-ins/TypedArray/prototype/subarray/byteoffset-with-detached-buffer.js`.

The resizable-buffer leaves cover fixed and tracking sources, entry snapshots,
initial out-of-bounds recovery and resize during coercion. The detached leaves
distinguish the non-throwing snapshot from the later default/custom species
boundary. The CLI fixture and structural guard pin the two- versus
three-argument construction shape directly.

No leaf at this pin directly isolates a species constructor returning a
detached or currently out-of-bounds TypedArray. The two new CLI controls directly
target that result-validation boundary. The six-leaf cohort remains regression
evidence for the earlier source-length migration.

None has a strictness-limiting flag at this pin. Each discovers two
sloppy/strict Wasm-AOT variants. All six leaves pass their `12/12` variants at
vendored suite content tree
`aa55200d1310384c5cf69ea95b2a2ecba457007b`, with every failure and
non-success bucket at zero.

The Number and BigInt
`speciesctor-get-species-custom-ctor-invocation.js` files are adjacent
construction controls rather than part of the migrated source-length cohort.
They are now owned by the separate
[species argument-vector arity contract](typed-array-subarray-species-argument-arity.md).
At vendored suite content tree
`aa55200d1310384c5cf69ea95b2a2ecba457007b`, their pre-fix sloppy and strict
executions report `0/4`: both sloppy executions throw
`Constructor called with arguments`, while both strict executions reach Boa's
`Cannot assign to property` TypeError. The isolated cause is an argc-only
reduction from the prebuilt three-entry vector for a length-tracking source with
omitted `end`, not the source or result buffer-witness boundary documented here.

## Recorded verification

The coordinated checkpoint ran:

```sh
cargo test -p scriptw-aot-wasm --test typed_array_subarray_witness_structure -- --test-threads=1
cargo test -p scriptw-cli --test cli typed_array::run_wasm_backend_subarray_uses_non_throwing_typed_array_buffer_witness -- --exact --test-threads=1
```

On 2026-08-25, the source-witness checkpoint recorded the exact CLI fixture at
`1/1` and the six direct Test262 leaves at `12/12` variants under
`--execution-backend wasm-aot --jobs 1 --threads 1`. That checkpoint also
recorded `cargo check -p scriptw-aot-wasm`, `cargo xc` and the shared format and
diff gates green. Its first CLI run exposed that created Realms did not
materialize their own `subarray` builtin; after adding the method to the
created-Realm TypedArray inventory, the borrowed method rejects invalid species
results through its own Realm.

The separate species argument-vector arity correction is focused-verified: the
expanded structure target passes `4/4`, the same exact CLI target passes `1/1`,
and the raw Number and BigInt invocation leaves pass `2/2` each with every
failure and non-success bucket at zero. This records their `0/4` pre-fix to
`4/4` post-fix transition without changing the earlier source-witness cohort
claim; no broader cargo or aggregate-status result is inferred from this
focused rerun.

## Explicit nonclaims

This lane does not change argument conversion, clamping, species lookup,
constructor invocation, the target brand check, Number/BigInt content-type
policy, result publication or the underlying TypedArray constructor. It adds
only the missing post-construction target state validation. It does not add a
post-coercion source witness or claim that shrinkage during coercion must throw;
the constructor decides whether the captured offset/length arguments remain
valid for the current buffer.

It does not migrate the remaining raw current-length observations in indexed
object read/write emitters or Array builtins, change SharedArrayBuffer behavior,
retire a Test262 rewrite, refresh aggregate status or published counts, or
complete `subarray`, TypedArray or T17.

One adjacent `subarray` debt remains explicit. A nullish species fallback still
selects its default TypedArray constructor from entry globals rather than the
executing builtin's Realm. This lane does not alter or verify that constructor
selection behavior.
