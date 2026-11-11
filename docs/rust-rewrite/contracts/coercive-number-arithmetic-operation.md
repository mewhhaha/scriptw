# Coercive arithmetic operation ownership

Current source status (2026-10-05): the T05 semantic Wasm-GC cutover is source-authored and unverified. Earlier source recipes, caller counts, and dated verification below describe their recorded checkpoints; they do not verify this cutover. ECMAScript requirements and retained fixtures remain acceptance criteria. Semantic values and Realm ownership now use the typed GC/compiler path described in [the GC value contract](../value-heap-gc.md). The retired representation assertions add no Test262 skips or new passing results.

`compile_coercive_binary_number_to_locals` accepts the closed
`ArithmeticBinaryOp::{Add, Sub, Mul, Div, Mod, Exp}` domain. Only operands
proved to own Number payloads by `expr_has_static_number_payload` can bypass
conversion and runtime Number/BigInt dispatch. Inferred Number kinds from
mutable storage, calls or unproven coercive results are insufficient. An
explicit unary plus can guarantee a Number result while its operand still
performs observable coercion. Both expressions evaluate left to right into
retained locals before the operation, including their effects and abrupt
completions.

Without that proof, addition delegates to the shared addition emitter before
the numeric operand locals are reserved, so both primitive conversions and
string concatenation retain their specified order. The other five operators
share evaluation, ToNumeric, mixed-type validation and runtime Number/BigInt
dispatch.

The Number branch exhaustively identifies every operation. Addition,
subtraction, multiplication and division use their binary64 instructions.
Remainder uses
`emit_number_remainder_payload`, shared with typed binary expressions and local
and property compound assignments. It reduces integer binary significands,
then reconstructs the result with the numerator's sign. This preserves exact
remainders when a floating-point quotient would overflow or round away needed
bits, including subnormal results and negative zero. NaN, zero divisors and
infinite operands take explicit branches. Exponentiation retains the shared
Number-power emitter. Addition reaches this branch only for proved Number
operands. A new IR operation requires an explicit backend decision.

The same recursive Number-payload proof allows primitive-only arithmetic
scripts to omit Realm bootstrap and its Intl and clock imports. Exponentiation
retains the runtime because its Number-power operation has a separately
planned host dependency. Dynamic and observable coercions retain the ordinary
runtime and installed callable bodies.

The remainder emitter owns five temporary locals. Its callers retain both
operands across child evaluation, so nested right-hand expressions cannot
overwrite the numerator. The planner accounts for the retained operands and
the shared emitter's temporary-local budget.

The BigInt side uses the existing exhaustive
`BigIntHelperOp::from_arithmetic` projection. Payload-only expression emission
calls this same tagged emitter instead of maintaining separate arithmetic and
error branches. The planner includes the four retained operand locals across
child evaluation and conversion/error phases, and preserves the runtime result
tag even when neither raw operand's kind advertises BigInt.

```sh
cargo test --release --locked -j2 -p scriptw-aot-wasm --lib planning::tests::
cargo test --release --locked -j2 -p scriptw-ir --test scriptw_ir_integration -- ir_cases::number_remainder::
cargo test --release --locked -j2 -p scriptw-engine --test aot_builtins -- aot_number_remainder::
```

Current verification is recorded in the
[September repair notes](../observed-later-failure-repairs.md). The earlier
operation-table ownership change was byte-equivalent; the subsequent tagged
conversion repair intentionally changes execution behavior for mixed numeric
operands and object coercion.
