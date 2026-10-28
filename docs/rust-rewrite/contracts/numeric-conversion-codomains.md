# Numeric conversion domains

`scriptw-ir/src/numeric_conversions.rs` owns ToIntegerOrInfinity's extended
integer domain: finite integers and both infinities. The finite payload is
private; NaN and either zero map to mathematical zero. A saturating machine
integer conversion must not replace this operation before a range check.

ToUint32, ToInt32 and ToUint16 use a nonnegative power-of-two residue. Signed
remainder needs its negative correction; clamping the input first loses the
specified residue. ToLength and ToIndex retain their distinct bounds and
failure rules. Compile-time reference algorithms and runtime emission must
agree for non-finite, negative and large inputs.

Unary plus performs ToNumber; unary minus and complement perform ToNumeric.
`UnaryNumericKind` keeps Number and BigInt consumers separate, so complement
cannot be encoded as binary XOR with a Number minus one. BigInt unsigned right
shift remains a TypeError. Coercion and abrupt completion preserve operand
order and the original thrown value.

Formatting calls first acquire the live property and evaluate their arguments,
then use the native formatting algorithm. Compiler folds must not bypass an
override, receiver coercion or argument effects. ToFixed, ToExponential and
ToPrecision retain their different ranges and non-finite ordering; their
interval reference controls are test-only, not a second product formatter.

The focused unary emission/behavior control is
`cargo test -p scriptw-aot-wasm --test structure_language -- unary_numeric_ir_structure::`.
Current types and their consuming source operations are authoritative; old
caller counts and integration instructions are not part of this contract.
