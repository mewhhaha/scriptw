# Strict equality over runtime values

Strict equality evaluates both operands once, left to right, before comparing
normal results. The usual expression compiler preserves getters, coercions,
TDZ and arbitrary Throw values. A normal-result fact cannot remove operand work:
`(1n >>> 0n) === 0n` still throws before equality.

Exact `ExprIr::Null`, `Undefined` and `Void` syntax establishes one possible
normal-result tag. Both SpecOperation StrictEqualityComparison and ExprIr strict
comparisons use the same checked singleton plan. They evaluate both complete
operands, then compare the other value's tag. Effectful `void` operands retain
source order and abrupt routing. Identifier spelling and inferred ValueKind
never authorize this path; a shadowed `undefined` remains dynamic.

Other strict comparisons retain tagged equality: binary64 Number equality,
String code-unit equality, BigInt value equality and object/reference identity.
SameValue and SameValueZero keep their distinct Number rules. Loose equality
retains its conversion and HTMLDDA semantics. Callers retain inequality inversion
and whole Normal completion publication; no runtime layout or ABI changes.

The encoded test checks both emitter routes and omitted all-kind operations
against dynamic, shadowed, numeric and loose controls. Engine fixtures exercise
both operand orders, every value category, GC/reentry, whole Throws, TDZ and
HTMLDDA under strict and sloppy source modes.

```sh
cargo test -p scriptw-aot-wasm --test emission -- singleton_comparisons::
cargo test -p scriptw-engine --test aot_language -- aot_strict_singleton_comparisons::
```
