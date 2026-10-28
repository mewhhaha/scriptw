# Callable `%Function.prototype%`

## Current atomic GC source — 2026-10-05

`%Function.prototype%` is published as the actual native FunctionObject with
its catalog identity, immutable FunctionContext and defining Realm. The common
GC bootstrap supplies the same typed Realm/function-prototype context to entry
and created Realms before publication. Function headers are completed during
allocation; the previous Object-placeholder and post-allocation repair are
historical representations.

The maintained `callable_function_prototype_structure` target retains its
existing CLI callable/Realm witness. Exact raw allocator, offset and duplicated
created-Realm materializer assertions are retired. The earlier specialized
prototype refusal and source census below do not describe the new typed
callable-family factories. See [NativeHost GC values](native-host-gc-values.md).

All source, types and controls for the atomic batch remain uncompiled and
unexecuted. Final representation/helper/guard composition also remains pending.
Earlier verification commands and results below retain their original source
scope; they are historical records, not instructions to run during the full-task
dry-source pass. Later verification follows the [batch workflow](../../../CONTRIBUTING.md#verification)
with a confirmed aggregate 4096 MiB cap, swap zero and serial execution.

## Historical predecessor record

`%Function.prototype%` is both the ordinary-function prototype and a built-in
function object. Its identity, value kind and call target are one intrinsic;
bootstrap may not publish an Object-shaped placeholder and repair callability
later.

## Closed intrinsic identity

The shared builtin catalog owns one exact non-constructable
`StandardBuiltinId::FunctionPrototype`. It has:

- `length` 0;
- `name` `""`;
- native source text `function () { [native code] }`; and
- a call operation that ignores `this` and every argument and returns
  `undefined`.

The intrinsic has no own `prototype` property and cannot be used as a
constructor. It is distinct from `Function`, from every function installed on
`Function.prototype`, and from the dynamically generated ordinary function
whose source is empty.

Lowering represents the value of `Function.prototype` with this exact function
target. A call through that property therefore reaches the catalogued body; it
must not be lowered as an indirect call of a value statically described as an
Object. Adding the catalog identity makes its body, length, protocol and backend
dispatch exhaustive-match obligations.

## Realm materialization

Every realm materializes a fresh function object from that shared builtin
metadata. For a realm `R`, the resulting intrinsic `P` satisfies:

```js
typeof P === "function"
P() === undefined
P(1, 2) === undefined
Object.getPrototypeOf(P) === R.Object.prototype
R.Function.prototype === P
```

Its external value tag is `Function`, while its `[[Prototype]]` payload is the
same realm's `%Object.prototype%` with an Object tag. Those two tags describe
different relations and must not be conflated. `P.[[Realm]]` is `R`, and all
ordinary built-in functions materialized for `R` use the exact identity `P` as
their initial `[[Prototype]]`.

The entry-realm global and the created-realm materialization context are the two
publication points. Both must be initialized from the catalogued function
before a Function constructor or another realm-local builtin can expose the
identity. The context carries a callable Function prototype by construction; an
arbitrary `ValueKind` field is not a valid long-term representation of this
invariant.

Created-realm allocation derives one private
`RealmFunctionInternalPrototypePolicy` from the function execution kind before
allocating the function object. Ordinary functions use the created realm's
Function prototype. Generator, async and async-generator functions fail
explicitly until their distinct realm-local intrinsic prototypes exist. The
policy derives no clone, copy, debug, equality or default capability; its four
producer rows, focused unit observations and allocation gate are exhaustive.
The structure guard pins the complete source census and keeps the specialized
error before the allocation call without changing emitted Wasm or error text.
Independent review is clean after the guard was expanded to exact-normalize the
complete materializer. The coordinated workspace formatter, `cargo xc`, diff,
module-boundary, and task-plan checks pass.

`Function`'s own `prototype` data property is the same function value, tagged
`Function`, with attributes `{ writable: false, enumerable: false,
configurable: false }`. `%Function.prototype%`'s `constructor` property points
back to the realm's `Function` constructor with the usual writable and
configurable attributes.

## Observable consequence

Because `Object.prototype.toString` dispatches from the runtime value kind, the
callable tag also makes:

```js
Object.prototype.toString.call(Function.prototype) === "[object Function]"
```

This lane closes the five current-pin physical failures
`S15.3.3.1_A1.js`, `S15.3.4_A1.js`, `S15.3.4_A2_T1.js`,
`S15.3.4_A2_T2.js` and `S15.3.4_A2_T3.js`. The 2026-08-13 artifact is
pin-current selection evidence, not a current-HEAD execution result; the
current source still proves the same Object-placeholder cause.

## Non-claims

This contract does not implement dynamic `Function` source generation,
`%Function.prototype%[@@hasInstance]`, callable generator/async intrinsic
prototypes, or complete realm bootstrap. Those are separate algorithms and
must not be approximated by this zero-return call body.

## Focused verification

The 2026-08-21 batch checkpoint verified the original compile-enforced seam,
the entry and created-realm fixtures, the five exact current-pin cases, and the
adjacent non-constructability boundary. The current capability closure used the
same focused surface:

```sh
cargo fmt --all -- --check
cargo xc
cargo test -p scriptw-aot-wasm --test structure_builtins --quiet -- callable_function_prototype_structure::
cargo test -p scriptw-aot-wasm --test realm_function_internal_prototype_policy_structure --quiet
cargo test -p scriptw-aot-wasm --lib functions::realm_function_materialization_tests::specialized_created_realm_function_prototypes_are_explicitly_unsupported -- --exact
cargo test -p scriptw-cli --test cli functions::run_wasm_backend_uses_created_realm_builtin_function_prototypes -- --exact --test-threads=1
./target/debug/scriptw --jobs 1 test262 run built-ins/Function/prototype/S15.3.3.1_A1.js --suite-root test262/vendor/test262 --execution-backend wasm-aot --threads 1 --timeout-ms 180000
./target/debug/scriptw --jobs 1 test262 run built-ins/Function/prototype/S15.3.4_A1.js --suite-root test262/vendor/test262 --execution-backend wasm-aot --threads 1 --timeout-ms 180000
./target/debug/scriptw --jobs 1 test262 run built-ins/Function/prototype/S15.3.4_A2 --suite-root test262/vendor/test262 --execution-backend wasm-aot --threads 1 --timeout-ms 180000
./target/debug/scriptw --jobs 1 test262 run built-ins/Function/prototype/S15.3.4_A5.js --suite-root test262/vendor/test262 --execution-backend wasm-aot --threads 1 --timeout-ms 180000
```

The capability structure target passes `3/3`, its exact owner unit passes
`1/1`, and the positive created-realm CLI fixture passes `1/1`. The adjacent
callable-prototype structure target currently passes `7/8`; its remaining
planning-root assertion observes zero occurrences where it expects one and is
outside the policy declaration, mapping, unit and consumer changed here.

The selected five files passed 10/10 strict and sloppy executions, and `A5`
passed 2/2. All failure buckets were zero. This focused result does not replace
a complete current-pin Wasm-AOT publication.
