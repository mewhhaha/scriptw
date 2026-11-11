# OrdinaryToPrimitive receiver-kind ownership

Current source status (2026-10-05): the T05 semantic Wasm-GC cutover is source-authored and unverified. Earlier source recipes, caller counts, and dated verification below describe their recorded checkpoints; they do not verify this cutover. ECMAScript requirements and retained fixtures remain acceptance criteria. Semantic values and Realm ownership now use the typed GC/compiler path described in [the GC value contract](../value-heap-gc.md). The retired representation assertions add no Test262 skips or new passing results.

The private `OrdinaryToPrimitiveReceiverKind::{Object, Function, Array, Arguments}`
domain is the complete set of heap-record families admitted by the shared
ToPrimitive emitter. Its exhaustive projection owns the runtime `ValueKind` tag
used by property reads and calls. Each family follows observable
`@@toPrimitive`, `toString`, and `valueOf` lookup; wrappers must not bypass hooks
by reading their internal primitive payload.

An arbitrary `ValueKind` entering the emitter and accidentally reading an
unrelated record offset is unrepresentable. Adding a receiver family requires an
explicit runtime tag. The domain has no clone, copy, debug or equality capability,
and every producer moves one choice into the inner emitter. The existing
ordinary Object/Function fallback is restricted to those families so Array and
Arguments conversion cannot read an object-only brand offset.

The tagged ToPrimitive path selects each of the four families directly. The
ordinary Object wrapper remains the other live entry. Each path keeps the shared
pending completion, active catch route, and conversion error realm.

Array conversion reaches its actual prototype `toString`, which looks up the
live `join` method and returns its result. The canonical join implementation owns
indexed property reads and recursive element conversion. The former bespoke
array string loop, element conversion shortcuts, and Function-only string bridge
are removed because no caller remains. Arguments conversion likewise reaches its
actual hooks and `Object.prototype.toString`, including custom `@@toStringTag`.

The lowerer cannot infer String from array elements or the Arguments tag: mutable
own and inherited hooks may return any primitive. Those receivers retain the full
primitive result domain. Numeric operators then retain normal Number and BigInt
results, including addition after a prior conversion; mixed numeric domains throw
at runtime. Source ordering and hook effects remain observable.

The receiver boundary guard checks the exact domain and its exhaustive tag
projection, the live selections, and absence of obsolete conversion emitters.
Behavioral coverage exercises all hints, receiver identity, hook order and arity,
getters, inherited hooks, live join replacement, abrupt completion, custom string
tags, nested elements, and subsequent numeric operations.

```sh
cargo test -p scriptw-ir --test scriptw_ir_integration -- ir_cases::array_arguments_primitive::
cargo test -p scriptw-engine --test aot_builtins -- aot_array_arguments_primitive::
cargo test -p scriptw-engine --test aot_language -- aot_compound_assignment_saved_value::
cargo test -p scriptw-engine --test aot_builtins -- aot_function_coercion::
cargo test -p scriptw-aot-wasm --test structure_builtins -- pending_to_primitive_operation_identity_structure::
cargo test -p scriptw-aot-wasm --test structure_builtins -- regexp_exec_result_mode_structure::
```
