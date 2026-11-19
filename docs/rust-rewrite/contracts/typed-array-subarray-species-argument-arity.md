# TypedArray `subarray` species argument-vector arity

Current source status, 2026-10-05: the atomic Wasm-GC rewrite is authored only. Compilation, emitted Wasm, focused controls, real agents and full pinned conformance remain unverified. No status counts changed.

The actual owner is builtins/binary_data/typed_array.rs. A tracking source with omitted or explicit undefined end builds the two-element nonnull GC ValueArray [buffer, offset]; every other source builds [buffer, offset, count]. One actual array length supplies argc and Arguments construction. No separate count/header carrier or post-construction arity patch remains. The source shape and historical focused receipt below remain useful evidence of the obligation, not execution evidence for the GC rewrite.

Four paired strict/sloppy finite Engine cohorts in `aot_gc_binary_data_entries.rs` cover native buffers, DataView, TypedArray construction/statics/species and Atomics/Realm lifecycle. Existing CLI semantic fixtures remain; obsolete raw-spelling guards are retired rather than replaced with mirrors. The historical implementation and receipts below do not certify this batch.

## Historical record before the atomic GC rewrite


Status: focused-verified for the checkpoint-13 Wasm-AOT two- versus
three-argument construction boundary on 2026-08-25.


The current 2026-10-03 dry species batch retains the complete chosen argv header
and callee count, with two arguments for a tracking source when end is omitted or
explicitly undefined, and three otherwise. Construction moves to the shared
Proxy-aware species factory and its validated result owner. All twelve immutable
defining-Realm defaults replace the earlier global selection. Historical
verification below does not establish execution of this batch. See
[shared species ownership](typed-array-species-create-ownership.md).

## Specification boundary

This contract is pinned to the ECMA-262 2026
[`%TypedArray%.prototype.subarray`](https://tc39.es/ecma262/2026/multipage/indexed-collections.html#sec-%typedarray%.prototype.subarray)
algorithm and vendored Test262 content tree
`aa55200d1310384c5cf69ea95b2a2ecba457007b`.

After `subarray` captures and normalizes its source range, it selects exactly
one argument list for `TypedArraySpeciesCreate`:

- a length-tracking source with `end` omitted passes `(buffer,
  beginByteOffset)`; and
- every fixed-length source, plus a length-tracking source with an explicit
  `end`, passes `(buffer, beginByteOffset, newLength)`.

The chosen argument count has two runtime carriers in the Wasm-AOT call ABI:
the count supplied to the callee and the `HEAP_LEN_OFFSET` stored on the argv
object. They are one semantic value. A user constructor creates its
`arguments` object from the argv header, so changing only the call count does
not hide a preallocated third entry from JavaScript.

## Isolated pre-fix gap

The pre-fix `StandardBuiltinId::TypedArrayPrototypeSubarray` arm constructed a
three-entry vector through `emit_pre_evaluated_arg_vector`. For a
length-tracking source with omitted `end`, it then overwrote only `argc_local`
with two before the species construct. The callee received two formal
arguments, but its arguments-object construction read the unchanged vector
length of three and exposed a phantom third indexed entry.

The failure is not a general escaped-arguments or nested-construction lifetime
problem. An escaped three-entry arguments object survives an ordinary call, and
a fixed-source species can save its three entries, perform nested
`new TA(buffer, offset, length)`, return, and retain them. The mismatch is
specific to the pre-fix subarray branch that reduced one arity carrier while
leaving the other at three.

## Product invariant

The sole semantic owner is the
`StandardBuiltinId::TypedArrayPrototypeSubarray` arm in
`crates/scriptw-aot-wasm/src/builtins/standard.rs`. It now selects between two
complete vector constructions before one shared species construct:

```text
length-tracking source and end is undefined
    => build [buffer, beginByteOffset]
otherwise
    => build [buffer, beginByteOffset, newLength]
```

Each `emit_pre_evaluated_arg_vector` call establishes both the call count and
the heap-visible vector length from the same Rust slice. The subarray arm must
not build three entries and later patch only `argc_local`. The common construct
then consumes the already coherent pair without reconstructing or truncating
the vector.

`emit_pre_evaluated_arg_vector` remains the shared vector producer, and
`emit_arguments_object_payload` remains the consumer that makes the header
observable. Neither needs subarray-specific behavior or a new arguments-object
protocol.

## Durable focused evidence

The structure test
`typed_array_subarray_witness_structure.rs`, no longer in the tree, was
the bounded source owner. It requires:

- one two-entry and one three-entry `emit_pre_evaluated_arg_vector` call in the
  subarray arm;
- the length-tracking-and-undefined predicate before the two-entry call;
- both vector constructions before the sole
  `emit_function_handle_construct_with_argv` call; and
- no raw `LocalSet(argc_local)` arity patch in the bounded arm.

The existing
`crates/scriptw-cli/tests/fixtures/wasm_typedarray_subarray_buffer_witness.js`
fixture, owned by
`typed_array::run_wasm_backend_subarray_uses_non_throwing_typed_array_buffer_witness`,
inspects the saved species `arguments` object after `subarray` returns. It
covers:

- fixed source with omitted `end`: length three and exact buffer, byte-offset
  and element-length entries;
- length-tracking source with omitted `end`: length two, exact buffer and
  byte-offset entries, and no own index `2`; and
- length-tracking source with explicit `end`: length three with the explicit
  fixed result length.

These six fixture controls use sloppy mapped arguments objects and cover both
Number and BigInt element types. The raw current-pin cohort supplies the strict
unmapped evidence as well as a second sloppy execution for each physical file.

## Exact current-pin cohort

The smallest direct raw-source cohort is:

- `built-ins/TypedArray/prototype/subarray/speciesctor-get-species-custom-ctor-invocation.js`;
  and
- `built-ins/TypedArray/prototype/subarray/BigInt/speciesctor-get-species-custom-ctor-invocation.js`.

Neither source has a strictness-limiting flag. Each therefore discovers one
sloppy and one strict Wasm-AOT execution, for four variants total. The pre-fix
audit reports `0/4`: both sloppy executions are `Runtime/Bug` with
`Constructor called with arguments`, and both strict executions are
`Runtime/Bug` with Boa's `Cannot assign to property` TypeError.

After the coherent vector selection landed, the Number leaf passes `2/2` and
the BigInt leaf passes `2/2`, for `4/4` total with every failure and non-success
bucket at zero.

The adjacent Number and BigInt subarray custom-species files that do not read
the constructor's arguments object control for successful construction. The
Number and BigInt `map` and `slice` custom-species invocation files control for
escaped one-entry arguments objects. They do not replace the direct two-entry
length-tracking witness.

## Recorded verification

On 2026-08-25, the bounded structure target passes `4/4`, the existing exact
CLI fixture passes `1/1`, and the two raw Test262 files pass their `2/2` Number
and `2/2` BigInt variants. Every Test262 failure and non-success bucket is zero.
This records a direct `0/4` pre-fix to `4/4` post-fix transition without
refreshing an aggregate status block.

## Explicit nonclaims

This lane does not change general arguments-object construction, mapped versus
unmapped selection, ParameterMap aliasing, descriptor behavior, escaped-object
lifetime or nested-construction reentrancy. It does not introduce a generic
argument-vector API or migrate unrelated call and construct sites.

It does not change begin/end coercion, the captured source-length witness,
species lookup, constructor selection, constructor `this`, result-brand or
buffer-state validation, Number/BigInt content-type validation, or result
publication. Resizable-buffer growth, shrinkage, detachment and out-of-bounds
behavior remain owned by the existing subarray buffer-witness contract.

The nullish-species default at that checkpoint came from entry globals. The
current dry species batch replaces it with immutable defining-Realm identity;
its execution remains unverified. This checkpoint does not retire a Test262
rewrite, refresh aggregate or published status counts, complete the subarray
tree, or complete TypedArray or T17.
