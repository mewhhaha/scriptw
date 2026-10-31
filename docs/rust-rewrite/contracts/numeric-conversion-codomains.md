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

Closed ordinary-function Number loops can keep their complete test/body/update
region in primitive Wasm locals. The bounded IR proof admits actual local binding
identities and excludes captured cells, dynamic environments, suspension and
observable callbacks. Entry tags and initialization checks select the scalar
region; misses use the original loop. Exits publish all dirty values, including
writes in the final false test, and retain the last body completion.

Invariant `Math.imul` additionally requires nonallocating cached-key and own-data
descriptor checks, the actual builtin code or immutable deferred recipe, and its
defining Realm. Shadowing, accessors, proxies and missing metadata decline without
invoking getters or materializing functions. Proven signed/unsigned integer ranges
can simplify ToUint32; arithmetic remains binary64 and arbitrary Numbers retain
the complete modulo conversion.

A private whole-trip affine proof can further collapse one exact32 state
assignment for a single `var` counter initialized by literal C, bare ascending
`counter < L`, and an independent unit increment, with `0 <= C < L <= i32::MAX`.
Its checked binding identity, positive count and one-step residue map mint the
composed coefficients; the existing eight-step constructor remains unchanged.
The actual loaded counter must match C's Number bits after the existing entry
checks. A hit updates live state, the independent last body value, final counter
L and iterated flag, then branches to the common publication exit. A miss keeps
the chunk and scalar tail; outer guard misses retain the complete classic loop.

Coefficient composition uses wrapping u32 pairs at compile time, bounded to 31
steps. Zero trips, negative zero, dynamic/fractional or larger bounds, other
initializers, counter-dependent/nonlinear bodies and observable writes keep
existing paths. No runtime exponentiation loop, GC schema, global or ABI change
is introduced. Original/strict Engine controls use independent while references;
actual encoded labels witness the guarded skip and common body-completion exit.
The existing large-loop epoch-timeout boundary remains unchanged. The
[source-bound checkpoint](../whole-trip-affine-20261031.json) records 35 selected
native checks through one fixture-only continuation, 78 original Test262
executions and balanced original/ordinary integer improvements. It retains
slower controls and separate derived-call results; no RSS benefit is claimed.

Formatting calls first acquire the live property and evaluate their arguments,
then use the native formatting algorithm. Compiler folds must not bypass an
override, receiver coercion or argument effects. ToFixed, ToExponential and
ToPrecision retain their different ranges and non-finite ordering; their
interval reference controls are test-only, not a second product formatter.

The focused unary emission/behavior control is
`cargo test -p scriptw-aot-wasm --test structure_language -- unary_numeric_ir_structure::`.
Current types and their consuming source operations are authoritative; old
caller counts and integration instructions are not part of this contract.
