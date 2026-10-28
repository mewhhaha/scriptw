# `Array.prototype.push` complete-argument ownership

## Adjacent splice retirement — 2026-10-04 dry source

The separate spliceFromArray extension mentioned by the historical nonclaims
below is now retired through the existing captured-callee/spread/canonical
Splice owners. Push production and these earlier receipts are unchanged by
that batch. The ref97 combined all-target Rust type check passed; runtime
verification remains pending. See the
[Splice contract](native-array-gc-methods.md).

Status: implemented and focused-verification complete for the Wasm-AOT
compiler on 2026-08-28.

## One complete standard entry

Static `push()` lowering retains its existing strict compile-time Array
classification and the earlier custom named-property path. That branch now
passes the receiver and complete source argument list through
`emit_array_direct_builtin_method_call` to
`StandardBuiltinId::ArrayPrototypePush`.

The deleted `emit_array_push_method_call` was a second Push algorithm. Although
it iterated every syntactic argument, it compiled each argument outside the
call-argument boundary, so a spread argument was rejected instead of expanded.
It also duplicated dense Array index, inherited-setter, length and error logic.
With that owner absent, every static Array Push enters the same standard body
as a dynamically obtained or borrowed Push function.

## Arbitrary runtime argument count

The canonical standard body previously emitted eight statically indexed
argument cases for each of its two receiver paths. The call boundary could
construct a larger argv, including through spread expansion, but both paths
silently ignored every argument after index seven.

Both paths now use a runtime argument-index local. Each emitted Wasm loop
compares that index with runtime `argc`, reads the corresponding tagged value
from `argv`, performs the existing receiver-specific write, advances the target
and argument indices, and exits only when every argument has been consumed.
There is no fixed Push argument ceiling in the compiler source.

The receiver-specific policies remain separate:

- a dense Array retains its inherited-index-setter checks, Array index write,
  maximum Array length handling and non-writable length failure order; and
- a generic receiver retains `ToObject`, observable `length` Get and
  `ToLength`, the `2^53 - 1` pre-write guard, sequential property Sets and the
  final `length` Set.

All source argument expressions and spreads are evaluated completely from left
to right at the shared boundary before either standard receiver path begins.

## Durable evidence

`array_push_algorithm_owner_structure.rs` recursively pins:

- the unchanged compile-time Array classification and exact standard target;
- complete receiver and argument forwarding;
- absence of the deleted wrapper;
- two runtime argc loops, two dynamic argv reads and no fixed eight-case loop;
- balanced ownership of the runtime argument-index local;
- receiver-before-arguments-before-call ordering at the shared boundary;
- dense Array index-write-before-length-publication order; and
- generic length observation and safe-integer checks before indexed writes and
  final length publication.

The focused fixture supplies eight ordinary arguments, a custom iterable that
expands to three more, and a final twelfth value. Its iterator observes that
the target is still unchanged during complete expansion, then the fixture pins
the returned length and all thirteen final elements.

Existing structure guards that bounded the shared direct-call body at the
deleted Push emitter now use the following canonical Join compiler. The
Proxy-set Realm guard retains only the live standard Push owner.

## Verification

On 2026-08-28, the recursive Push owner target passed `4/4`. The affected
Proxy-set Realm and Concat owner targets each passed `4/4`, the String
code-unit boundary target passed `6/6`, and the FindLastIndex owner boundary
target passed `4/4`. Targeted Rust formatting and the scoped diff check passed.
The canonical Push arm is pinned at
`c954cac5f939488f2cb6d07e5b9d70fba3224d33ec57c708570e6759856cf6c8`, the
deleted wrapper has zero Rust source occurrences, and the fixed eight-case
form has zero occurrences in the canonical arm.

The exact new CLI control passes `1/1`. The pinned
`set-length-array-length-is-non-writable.js`, `length-near-integer-limit.js`
and `throws-if-integer-limit-exceeded.js` controls pass all `6/6` Wasm-AOT
executions with every failure bucket at zero. The shared `cargo xc`,
formatting, diff, module-boundary and task-plan checks are green. No broader
Test262 run belongs to this lane.

## Nonclaims

This closure does not change the earlier method-property classification,
`spliceFromArray`, another Array method, published conformance status or a
Test262 materializer. It does not claim the Array subtree green.
