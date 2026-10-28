# Created-realm builtin function prototype context

## Current atomic GC source — 2026-10-05

The actual `RealmFunctionMaterializationContext` retains typed Realm and
FunctionObject roots together. The shared function factory completes immutable
FunctionContext, capture and specified internal prototype before publication;
it does not start from an entry-global prototype or repair headers afterward.
Callable `%Function.prototype%` and the represented generator/async intrinsic
families now have real GC owners, so the earlier Object-shaped placeholder and
specialized-family refusal below are historical.

NativeHost's created-Realm path consumes the completed shared bootstrap and
canonical global publisher. The new Global Environment excludes caller source
overrides and entry-only host extensions. Its facade callables capture the new
Realm through the same actual factory. Existing callable/Realm semantic targets
remain. This source migration does not add a runtime JavaScript parser or relax
the explicit dynamic-source policy. See
[NativeHost GC values](native-host-gc-values.md).

All source, types and controls for the atomic batch remain uncompiled and
unexecuted. Final representation/helper/guard composition also remains pending.
Earlier verification commands and results below retain their original source
scope; they are historical records, not instructions to run during the full-task
dry-source pass. Later verification follows the [batch workflow](../../../CONTRIBUTING.md#verification)
with a confirmed aggregate 4096 MiB cap, swap zero and serial execution.

## Historical predecessor record

ECMAScript built-in function objects have the `%Function.prototype%` of their
defining realm as their initial `[[Prototype]]`, unless an intrinsic explicitly
specifies another function prototype. `CreateBuiltinFunction` couples these two
facts: its `realm` argument supplies both the function's `[[Realm]]` and the
default `%Function.prototype%`.

The Wasm-AOT function allocator cannot provide that coupling by itself. Its
shared helper initially writes the entry-realm `FUNCTION_PROTOTYPE_GLOBAL_INDEX`
because ordinary entry-realm allocation is its dominant caller. Created-realm
bootstrap allocates a distinct Function prototype in a local, so merely writing
the created realm's defining-realm pointer after allocation leaves the two
fields inconsistent.

## Closed bootstrap capability

Created-realm bootstrap uses a one-way typed lifecycle:

1. reserve storage as `ReservedRealmFunctionPrototypeLocal`;
2. after the realm record and its Object prototype exist, consume that storage
   to create `RealmFunctionMaterializationContext`;
3. use that non-`Copy` context for every created-realm builtin function;
4. consume the context when bootstrap releases its retained prototype local.

The context owns three inseparable facts:

- the private `RealmRecordLocal` minted by realm allocation;
- the local containing that realm's Function prototype identity; and
- the exact runtime representation tag for that identity.

Its fields are private. A created-realm function materializer cannot be called
with an arbitrary realm and a separately chosen prototype local. Before it
returns the destination local, it installs both the defining realm and the
context's prototype payload/tag. This is the created-realm equivalent of the
specification's single `CreateBuiltinFunction(..., realm, prototype)` step.

The current partial bootstrap represents `%Function.prototype%` as an Object.
The context records that fact honestly; it does not claim that the prototype is
already callable. When callable `%Function.prototype%` lands, its initializer
must change the context's tag at the same choke point rather than repairing
every consumer.

## Ordinary and exceptional prototype topology

The context's default materializer accepts only ordinary built-in execution
kind. The other closed execution kinds — generator, async and async generator —
require their own realm-local intrinsic prototypes and fail emission explicitly
until those contexts exist. They may not inherit entry-realm globals silently.

Most created-realm functions consume the default context directly: ordinary
constructors, prototype methods, accessors, namespace methods and host/global
functions. Bootstrap may subsequently replace the internal prototype only for
a specification-defined non-default relation. The current explicit families
are:

- native Error subclass constructors, AggregateError and SuppressedError,
  which inherit the realm's Error constructor; and
- concrete TypedArray constructors, which inherit the realm's `%TypedArray%`
  constructor.

Default writes beside those exceptional graph links are redundant and must be
deleted. A newly added ordinary builtin receives the correct realm-local
Function prototype by construction instead of relying on another handwritten
header repair.

## Observable contract

Two created realms must have distinct Function prototype identities. For every
ordinary function installed in one of those realms:

```js
Object.getPrototypeOf(fn) === realm.Function.prototype
```

and that prototype must not be the entry realm's `Function.prototype` or the
other created realm's Function prototype. The durable fixture covers a
constructor, prototype method, accessor, namespace method, global function and
canonical host function so all bootstrap allocation shapes consume the same
context.

## Deferred verification

The implementation is dry-written while the low-RAM current-pin matrix owns
Cargo and Test262. After that process releases them, run:

```sh
cargo fmt --all -- --check
cargo check -p scriptw-aot-wasm --lib
cargo test -p scriptw-aot-wasm realm_function_materialization --quiet
cargo test -p scriptw-cli --test cli run_wasm_backend_uses_created_realm_builtin_function_prototypes --quiet
./target/debug/scriptw test262 run built-ins/Function/prototype/apply/this-not-callable-realm.js --execution-backend wasm --timeout-ms 180000 --threads 1
./target/debug/scriptw test262 run built-ins/Function/prototype/bind/get-fn-realm --execution-backend wasm --timeout-ms 180000 --threads 1
```

No pinned Test262 case directly walks this broad created-realm builtin graph;
the CLI fixture is therefore load-bearing. The complete T06 verification ladder
and current-SHA low-RAM publication remain required for task closure.

## Non-claims

This seam does not make `%Function.prototype%` callable, add it to the Wasm realm
intrinsic record, create realm-local generator/async intrinsic families, enable
dynamic Function or eval source, add a real global environment, assign unique
created-realm IDs, scope host hooks, implement teardown, or prove the complete
realm/Test262 matrix.
