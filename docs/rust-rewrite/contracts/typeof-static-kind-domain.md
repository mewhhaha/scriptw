# typeof over runtime values

`compile_typeof_payload` evaluates its operand exactly once into `ValueLocals`,
then calls `emit_typeof_value` with that same complete value. Operand effects and
abrupt completions precede type observation. Inferred `ValueKind` precision,
including Object or Dynamic, does not replace this evaluation or discard the
operand's GC reference.

`emit_typeof_value` starts with `"object"` for null and non-callable heap values,
then projects Undefined, Boolean, Number, BigInt, Symbol and String runtime tags
to their exact text. It tests actual callability before publishing `"function"`:
Function objects, bound functions and the Proxy's retained call capability use
the shared callable owner. The HTMLDDA observation runs last and selects
`"undefined"`, including when that value is otherwise callable. No public
property lookup or inferred source kind substitutes for these runtime facts.

Literal equality tests over resolved `ExprIr::TypeOf` use the same runtime
facts directly. The operand still evaluates once before the tag, callability or
HTMLDDA test, including for an impossible spelling. Both operand orders and
strict/loose equality share the predicate; their callers retain Boolean
completion publication and inequality inversion. Neither the type-result String
nor the compared literal needs materialization. Dynamic comparison operands and
unresolved-identifier `typeof` retain their reference and coercion paths.

The external emission test checks actual source and spec-operation routes,
encoded helper calls and fallback controls. Engine fixtures exercise all eight
spellings, invalid spellings, TDZ, operand Throws, coercion, revoked callable and
non-callable Proxies, and concrete HTMLDDA versus bound/proxy wrappers.

```sh
cargo test -p scriptw-aot-wasm --test emission -- typeof_comparisons::
cargo test -p scriptw-engine --test aot_language -- aot_typeof_comparisons::
cargo test -p scriptw-aot-wasm --test structure_language -- typeof_static_kind_structure::
```
