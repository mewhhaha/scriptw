# Atomics TypedArray buffer witness

Current source status, 2026-10-05: the atomic Wasm-GC rewrite is authored only. Compilation, emitted Wasm, focused controls, real agents and full pinned conformance remain unverified. No status counts changed.

All fourteen fixed native entries retain whole arguments and concrete TypedArray/BufferOwner roots. Integer load/store/RMW and compareExchange complete entry admission and all required coercions before a fresh backing witness. Store returns the converted Number/BigInt rather than the truncated stored word. Notify on an ordinary buffer returns zero after count coercion. Waitable shared admission precedes index/value/timeout hooks. Errors and foreign thrown values remain whole completions. Shared no-tear accesses use width/alignment atomics; GC private bytes use one backing owner.

Four paired strict/sloppy finite Engine cohorts in `aot_gc_binary_data_entries.rs` cover native buffers, DataView, TypedArray construction/statics/species and Atomics/Realm lifecycle. Existing CLI semantic fixtures remain; obsolete raw-spelling guards are retired rather than replaced with mirrors. The historical implementation and receipts below do not certify this batch.

## Historical record before the atomic GC rewrite


Status: focused-verified on 2026-08-24.

## Specification boundary

The edition-pinned ECMA-262
[`ValidateIntegerTypedArray`](https://tc39.es/ecma262/2026/multipage/structured-data.html#sec-validateintegertypedarray)
operation first validates a TypedArray and returns its TypedArray-with-buffer
witness record. A detached or out-of-bounds view therefore throws a TypeError
before the index is coerced. The subsequent
[`ValidateAtomicAccess`](https://tc39.es/ecma262/2026/multipage/structured-data.html#sec-validateatomicaccess)
operation reads the element length from that record, performs `ToIndex`, and
throws a RangeError when the normalized index is not below that captured
length.

Wasm-AOT has four owners for that entry observation:

- `FunctionBuilder::emit_atomics_notify`;
- `FunctionBuilder::emit_atomics_wait_async`;
- `FunctionBuilder::emit_atomics_wait`; and
- `FunctionBuilder::emit_atomics_integer_operation`, shared by `add`, `and`,
  `compareExchange`, `exchange`, `load`, `or`, `store`, `sub` and `xor`.

Before this migration every owner loaded the TypedArray's buffer, byte offset,
stored byte length and bytes per element independently, called
`emit_typed_array_current_byte_length`, multiplied the normalized index by
bytes per element, and compared those byte quantities. That duplicated the
buffer-witness law and admitted an index whose element began before, but ended
after, an odd current byte length.

It also treated an already out-of-bounds fixed view as a zero-length valid
view. The old path therefore coerced the index and then threw a RangeError.
This migration intentionally corrects that observable behavior: initial fixed
out-of-bounds and detached states throw TypeError before `ToIndex`, as required
by `ValidateIntegerTypedArray`.

## Closed witness projection

Each owner now loads one immutable `TypedArrayViewLocals` value with
`emit_load_typed_array_private_state` and consumes exactly one witness through:

```rust
TypedArrayWitnessUse::ValidatedMethodEntry {
    length_local: element_length_local,
}
```

No Atomics-specific compatibility variant exists. `ArrayLikeLengthSnapshot`
is deliberately not admissible here because Atomics validation throws for an
initial detached or out-of-bounds view. `IntegerIndexedProperty` is also the
wrong policy because it projects invalid states to an absent property rather
than an abrupt completion.

The witness owns the current backing-length observation, fixed versus
length-tracking out-of-bounds calculation, fixed-view stored extent and
whole-element flooring. The Atomics owner compares `index_local` directly with
the witness-produced `element_length_local`; it may not reconstruct current
byte length or divide/multiply byte quantities for that bound.

## Preserved order and pointer timing

The four owners retain their existing receiver, element-kind and shared-buffer
requirements. Their emitted entry sequence is:

1. load the arguments and reject a non-TypedArray receiver;
2. load one immutable view and retain the existing buffer-brand check;
3. retain the operation's integer-element-kind and, for `wait`/`waitAsync`,
   SharedArrayBuffer requirement;
4. snapshot the backing data pointer and reject detachment with the existing
   Atomics TypeError path;
5. create one validated buffer witness and capture its element length;
6. coerce the index with `ToNumber` and `ToIndex`;
7. compare the normalized index with the captured element length and throw the
   operation-specific RangeError when it is outside that bound; and
8. only then coerce `count`, `value`, replacement value or timeout and perform
   the existing atomic operation.

The explicit data-pointer load is not a parallel length observation. Atomics
still needs the pointer to form the eventual memory address, and retaining its
pre-coercion snapshot preserves the existing pointer timing when index, count,
value or timeout coercion runs user code. This lane does not implement the
separate post-coercion `RevalidateAtomicAccess` requirement.

A valid length-tracking view whose current element length is zero is distinct
from an out-of-bounds view. Its witness succeeds, its side-effecting index is
coerced, and the access is checked against the originally captured zero. Growth
during index coercion therefore cannot make that access valid. Likewise, a
trailing partial element never becomes addressable: the witness floors bytes
to complete elements before publishing the bound.

## Durable structural guard

The structure test `atomics_typed_array_witness_structure.rs`, no longer in the tree, bounded
only the four owner bodies above. For each it requires one private-state load,
one immutable view, one `ValidatedMethodEntry` witness, one retained data
pointer snapshot and one separate element-kind load. It rejects the legacy
current-byte-length helpers, direct backing-length loads and direct private
view-slot offsets.

The guard also fixes witness-before-index ordering, the single index coercion,
the direct `index >= elementLength` bound, absence of a local writer that could
overwrite the witness result, and the focused CLI test/fixture connection.

## Focused runtime witnesses

`crates/scriptw-cli/tests/fixtures/wasm_atomics_typed_array_buffer_witness.js`,
owned by
`binary_data::run_wasm_backend_validates_atomics_access_through_typed_array_witness`,
covers all four owners. It proves that detached receivers and ordinary-buffer
fixed views already out of bounds throw TypeError without invoking a poisoned
index. It separately proves that valid zero-length views snapshot their length
before an index grows the backing buffer, and that ordinary and shared
length-tracking views floor an odd byte length before checking index 1. Poisoned
later arguments prove the RangeError precedes count, value and timeout
coercion.

At Test262 pin `e9d582d6b8b13afc5ba9a676664741592b5c7f69`, the smallest exact
adjacent cohort is:

- `built-ins/Atomics/notify/null-bufferdata-throws.js` for detached entry and a
  poisoned index;
- `built-ins/Atomics/notify/retrieve-length-before-index-coercion.js`;
- `built-ins/Atomics/wait/retrieve-length-before-index-coercion.js`; and
- `built-ins/Atomics/waitAsync/retrieve-length-before-index-coercion.js` for
  zero-length growable shared views whose index coercion grows the buffer.

The pin has no exact fixed-out-of-bounds-before-index Atomics leaf. The focused
CLI fixture owns that corrected behavior until the pinned suite adds one. No
Test262 result is claimed before the cohort is executed against this patch.

## Verification evidence

The coordinated batch ran:

1. `cargo fmt --all -- --check`;
2. `cargo xc`;
3. `cargo test -p scriptw-aot-wasm --test atomics_typed_array_witness_structure -- --test-threads=1`;
4. `cargo test -p scriptw-cli --test cli binary_data::run_wasm_backend_validates_atomics_access_through_typed_array_witness -- --exact --test-threads=1`; and
5. each exact Test262 path above through `--execution-backend wasm-aot` with
   `--jobs 1 --threads 1`.

Formatting, diff hygiene and `cargo xc` are green. The structure target passes
`3/3`, and the focused CLI fixture passes `1/1`. Each of the four exact pinned
Test262 files passes `2/2`, for an aggregate `8/8` Wasm-AOT executions with
every non-success bucket at zero.

## Explicit nonclaims

This migration does not add post-coercion atomic revalidation, change
sequentially consistent loads/stores, waiter queues, notification, agent host
transport or `waitAsync` settlement. It does not change accepted element
kinds, operation-specific return values, memory-address calculation or the
relative coercion order of index and later arguments.

It does not migrate other raw TypedArray validators, retire a Test262 harness
rewrite, refresh an aggregate baseline, change published conformance counts or
complete Atomics, TypedArray or T17.
