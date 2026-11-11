# TypedArray witness-use ownership

## Source replacement — 2026-10-05

The GC backend reads concrete TypedArray and buffer records through typed witnesses. The five integer-address view roles and raw witness-use recipes below describe the retired object layout. The obsolete source recipe checks have been retired. Exact historical receipts and any public fixture inventory remain recorded.

The pre-retirement source is identified exactly:

- `typed_array_witness_use_ownership_structure.rs`: SHA-256 `e3894be6d65658634f605a8931dbde3702456851f4602d1f126656a652bc017e`.

The earlier verification checkpoints below do not verify the GC replacement. The atomic GC source and its finite controls are authored and unexecuted; compilation, Wasm validation and runtime conformance remain unverified.

## Historical contract and checkpoints

Status: normative for the AOT TypedArray buffer-witness use boundary.
Current owner inventory refreshed on 2026-09-14. See the
[codec checkpoint](../uint8array-codec-baseline-follow-up.md#verification) for current verification.

## Semantic boundary

`TypedArrayWitnessUse` carries the complete purpose of one fresh backing-store
observation. Its variants distinguish throwing method entry, non-throwing
Array-like length capture, integer-index presence, and the three accessor
results. Each value also owns the destination locals into which that single
observation may publish its result.

This policy is a move-only witness-use authority. It implements neither
`Clone` nor `Copy`. `emit_typed_array_witness` first borrows it for the
method-entry validation decision and completes the shared cached-length and
element-length calculation. The final consuming projection then moves that
authority and publishes exactly one result. Code after that projection cannot
reuse the authority for a second result publication. The validation
match deliberately binds none of the payload locals, while the consuming match
is the sole payload owner.

The four variants remain exhaustive in both decisions. Adding a use therefore
requires an explicit validation policy and an explicit result algorithm; no
catch-all can silently inherit the behavior of an existing consumer. View
locals remain a separate immutable description because many algorithms
legitimately take later live integer-index observations from the same view.
`TypedArrayViewLocals` itself is non-`Clone` and non-`Copy`: each of its 41
producers constructs one owned five-local carrier, and every live observation
borrows that carrier. Algorithms that require multiple observations therefore
reuse one authority by shared borrow instead of forking independent copies of
the private-slot roles.

## Durable guard

The current inventory contains 54 view-carrier references, 41 constructors,
two borrowed type boundaries, and 72 witness-use references. The 53 witness
sites comprise one definition and 52 calls. The four route counts are
`ValidatedMethodEntry 36`, `ArrayLikeLengthSnapshot 8`,
`IntegerIndexedProperty 13`, and `Accessor 4`; these include both exhaustive
matches inside the witness authority.

The guard attributes every reference to its exact source owner: `objects.rs`,
`builtins/{array,atomics,binary_data,iterators,mod,object,standard,typed_array_fill,typed_array_set,uint8array_codecs}.rs`,
and `builtins/array/find_via_predicate.rs`. The codec contributes one owned view,
one validated method-entry witness, and the two corresponding imports. Its
private-state load must precede validation, and validation must precede the
backing-pointer load. This preserves the late buffer observation after codec
option getters without changing the earlier immutable-receiver check.

The dedicated `fill` emitter contributes one owned view, two validated
observations and its two imports. Its second observation follows value, start
and end conversion unconditionally, including empty ranges. See the
[fill contract](typed-array-fill-buffer-witness.md). The updated inventory
requires the next integration checkpoint; earlier verification below predates
this new consumer.

The witness body's current fingerprint is `(8495, 0x76179fc19b197dcd)`.
Replacing only its named `TypedArrayLengthMode::Fixed.word()` projection with
the old `I64Const(0)` exactly reconstructs the earlier fingerprint
`(8433, 0xdba079dd67aaacdf)`. Both select wire zero. The current body and the
older census drift already existed on `origin/main` at `6dff6eb0d`; the codec
adds two view references, one constructor, two use references and one witness
call to that baseline. The per-file inventory prevents an unrelated added or
removed consumer from being hidden by a total-count adjustment.

## Historical verification

Batch AG extended the same guard to the view carrier. It pinned 56 exact product
mentions, 46 constructors, two borrowed type boundaries, the attribute-free
five-field declaration, and the absence of manual clone, copy, debug, default,
comparison, ordering or hashing implementations. The unchanged declaration is
`64a7e96e10f1d53150a94e915656bd69b2a050449e7fa73b2954093ddd1b5390`, its
constructor implementation is
`7ff4343576674f15b704921718176ace71d92df0927299bfed696ee008a10f80`, and
the shared witness emitter was
`61daf0915471d6f3f2ac4e62dd3792bb940a318c7a9199676fe327ea852ec226`.

That was source-equivalent ownership hardening. It did not change buffer
observation, detachment or resize behavior, add a new TypedArray consumer,
retire a Test262 rewrite, or claim full T17 conformance. Focused compilation,
the ownership guard and a neighboring witness structure target own the
checkpoint; broad conformance and semantic-golden runs remained deferred. At the
pre-AG checkpoint, the package-level ownership target passed `4/4`, the
neighboring Atomics TypedArray-witness target passes `5/5`, and the exact
TypedArray iterator CLI witness passes `1/1`. A standalone run of the older
iterator structure target is `1/2`: its Realm-validation subtest passes, while
its other subtest stops at a stale `StandardBuiltinId::ArrayPrototypeKeys`
source marker before reaching this ownership seam. That unrelated marker is
not reported as green.

Batch AG changes no producer or witness instruction body. At the shared
checkpoint, `cargo xc` is green, the expanded ownership structure target passes
`5/5`, and the exact
`typed_array::run_wasm_backend_copies_typedarray_bytes_with_spec_ordering` CLI
witness passes `1/1`. The pinned
`built-ins/TypedArray/prototype/copyWithin/resizable-buffer.js`,
`built-ins/TypedArray/prototype/copyWithin/coerced-values-start-detached.js` and
`built-ins/TypedArray/prototype/copyWithin/coerced-values-end-detached.js`
leaves pass all `6/6` Wasm-AOT executions with every failure bucket at zero.
Batch AG did not rerun the semantic golden.
