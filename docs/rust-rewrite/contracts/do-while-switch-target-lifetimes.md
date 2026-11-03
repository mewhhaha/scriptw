# Do-while and switch branch-target lifetimes

## Demonstrated failures

The checkpoint9 Wasm-AOT replay of
`language/statements/try/S12.14_A9_T3.js` panics during compilation because
`branch_depth_to` receives a label whose Wasm frame has already closed. The
same failure was reproduced on checkpoint8. The failing completion dispatcher
belongs to the last do-while/finally combination, inside an outer catch.

`compile_do_while` registered its continue target and label bindings but never
removed them. Later completion dispatch enumerated those stale registrations
and attempted to emit a branch to a closed frame. The sink's live-label check
correctly rejected that invalid compiler state.

The target-owner audit found the converse error in `compile_switch`: it
removed its label bindings without registering them. A checkpoint9 probe with
a labelled switch inside a labelled while loop failed with `unknown label
selected`; its expected observable output is `SO`.

## Ownership repair

The do-while body owns the continue Block and its source labels. Immediately
after emitting the body, the compiler removes those label bindings and pops
its loop target before closing that Block. The condition and loop back-edge
then execute with the enclosing target stacks restored. Break registration
continues to belong to the outer loop-exit Block. This is the same ownership
order already used by the ordinary for-loop emitter.

A labelled switch registers its supplied labels against its own break Block,
with no continue target, before emitting case bodies. Its existing cleanup
then removes exactly those registrations. Enclosing loop and block labels
remain live, including when a switch break passes through a finalizer or a
finalizer replaces a pending throw with that break.

The production repair is three added lines. Completion dispatch, label
identity checks, environment unwinding and invalid-target traps are unchanged;
stale targets are removed at their owner boundary, never ignored at dispatch.

## Dense integer dispatch

Ordinary switches with effect-free integer literal selectors may use a validated
bounded dense table. The original discriminant and lexical setup execute once.
A Number-tag and exact signed-i32 roundtrip guard selects the table; other values
reach default without coercion. Duplicate keys retain the first clause, including
both signs of zero. Nested case-entry blocks keep source-order fallthrough,
default position and the existing break/label/finally cleanup owners. Effectful,
sparse, unbounded and resumable selectors retain ordered linear dispatch.

## Evidence and limits

Seven native regressions added to `aot_control_flow` cover a completed loop
before a later finalizer, nested loop cleanup, retained continue/condition
ordering, a finalizer break replacing a throw, retained outer breaks, labelled
switch breaks and chained/reused switch labels through finalizers.

Stage formatting and source review are complete. Compilation, native execution
and the exact Test262 replay remain pending the integrating checkpoint. The
existing sink live-label invariant remains the structural rejection boundary.
The audit found no other push/pop count mismatch among current source loop,
switch and labelled-statement target owners; that census is not a claim that
all completion or suspension semantics are implemented. Published Test262
counts remain unchanged.
