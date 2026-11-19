# Direct synchronous `for-of` protocol-error Realm

Status: focused verification passed on 2026-08-29.

## Scope and owners

Three direct synchronous `for-of` execution owners each expose five
algorithm-created iterator acquisition and stepping TypeErrors, for 15 checks
in total:

- `compile_for_of_iterator` owns the five checks for an ordinary direct loop;
- `compile_async_disposable_for_of_iterator` owns the five checks for a direct
  loop with an async-disposable head; and
- `compile_async_function_for_of_iterator` delegates the five checks for a
  resumable plain-async loop to `emit_get_iterator_from_value_locals` and
  `emit_sync_iterator_step_value`.

The checks, in protocol order, are:

1. a nullish source cannot supply an iterator;
2. the source's `@@iterator` property is not callable;
3. the iterator method returns a primitive;
4. the iterator's `next` property is not callable; and
5. calling `next` returns a primitive.

At the historical checkpoint, the first two checks both selected
`SyncIteratorProtocolError::NotIterable`.
The remaining checks select `MethodResultNotObject`, `NextNotCallable`, and
`NextResultNotObject`, respectively. Five semantic checks therefore pass
through four closed error variants.

## Shared error and Realm authority

All three owners select `SyncIteratorConsumer::ForOf`. Every one of the 15
checks reaches `emit_sync_iterator_protocol_type_error`; none calls a raw error
emitter. The exhaustive projection owns these messages, in variant order:

- `for-of target is not iterable`;
- `for-of iterator method must return object`;
- `for-of iterator next must be callable`; and
- `for-of iterator next result must be object`.

The shared projection selects its error constructor from the closed builder
Realm-source domain. A standard-builtin body may use its trusted self-backed
environment and `emit_throw_current_function_realm_type_error`. Main, user,
host, and runtime-helper bodies use `emit_throw_runtime_error` and the main
Realm. An ordinary lexical-environment pointer is never interpreted as
function Realm metadata. This matters once an environment owns at least 13
captured slots: slot 12's tag occupies the same byte offset as the cached
`%TypeError.prototype%` field in a function object. The iterator object,
source value, and receiver do not select the generated error's Realm.

This direct-owner checkpoint originally covered one four-variant error domain,
15 producer checks, and 29 `SyncIteratorProtocolError` mentions. The current
shared boundary has four consumer variants, 17 typed producers, 35 error
identifiers, and 16 exhaustive diagnostic rows. Array destructuring contributes
the two additional custom-step producers. See
[`sync-iterator-consumer-capability.md`](./sync-iterator-consumer-capability.md).

## Primitive lookup Realm

Both inline owners use
`emit_value_to_current_function_realm_object_locals` before reading
`@@iterator`. This includes `compile_async_disposable_for_of_iterator`, whose
primitive source must resolve a wrapper prototype from the active function
Realm. The resumable plain-async owner passes `SyncIteratorConsumer::ForOf`
into the shared acquisition helper, which selects the same current-Realm
boxing path.
Property access and method invocation retain the original source as the
observable receiver.

## Callable-Proxy method follow-up

[`GetIteratorFromMethod`](https://tc39.es/ecma262/2026/multipage/abstract-operations.html#sec-getiteratorfrommethod)
calls the iterator method with the source as its receiver, and
[`IteratorNext`](https://tc39.es/ecma262/2026/multipage/abstract-operations.html#sec-iteratornext)
calls the cached `next` method with the iterator as its receiver. Both calls
use the general `IsCallable` and `Call` operations. A callable Proxy is
therefore valid in either position even though its runtime value has the
Object tag rather than the Function tag.

The ordinary direct owner now uses `emit_is_callable_i32` for both gates and
`emit_function_or_proxy_call_leave_throw_completion` for both calls. The
iterator method receives the original iterable and no arguments. `next`
receives the iterator and no arguments. The existing propagation remains
between each call and its object-result check, so an apply-trap throw or a
revoked-Proxy error is not replaced by a protocol diagnostic. Acquisition or
stepping failure occurs before a body completion owns the iterator and does
not enter `IteratorClose`.

The direct async-disposable and resumable shared owners already used the same
general callability and Proxy-aware Call operations. This follow-up changes
only the two stale sites in `compile_for_of_iterator`. The bounded Rust guard
pins exactly two general callability checks, two Proxy-aware calls, no
Function-only call, no Function-tag comparison, the two receiver mappings,
and call-to-propagation-to-result-check order.

## Runtime witness boundary

`crates/scriptw-cli/tests/fixtures/wasm_for_of_protocol_type_errors.js` covers all
five error conditions, their exact diagnostics, and a valid control. It
lists the failures in protocol order. It is registered as
`iterator::run_wasm_backend_reports_direct_for_of_protocol_type_errors`.

The 2026-10-10 GC continuation distinguishes the earlier nullish ToObject
failure from iterator lookup. A nullish source throws the entry Realm TypeError
with `Cannot convert undefined or null to object` before reading `@@iterator`;
a non-callable iterator method retains `for-of target is not iterable`.
The fixture keeps both prototype and exact-message assertions, and all method,
next/result and valid-loop controls. These messages are product diagnostics;
ECMA-262 requires the error type and ordering, not particular message text.

That fixture executes a loop-owning user function in the entry Realm. It can
detect wrong branch selection and messages, but it cannot distinguish a
legitimate current-function Realm construction from the main-Realm result.
The structure target owns the source-order invariant. A
Realm-distinguishing witness would require the compiled user function that
contains the loop to be defined in a created Realm. The Wasm-AOT boundary does
not dynamically compile `eval`, `Function`, or cross-Realm Function
constructors. Created-Realm user-function observation is therefore a runtime
test limitation, not a claimed result.

`crates/scriptw-cli/tests/fixtures/wasm_direct_for_of_callable_proxy_methods.js`
is the separate Call witness. It covers callable Proxy iterator and `next`
methods, their exact receivers and empty argument lists, once-only `next`
lookup, apply-trap completion identity, non-callable Proxy diagnostics, and
revoked callable Proxies in both positions. Thirteen initialized captured
bindings make the lexical-environment/function-layout alias deterministic;
both a primitive and a non-callable Proxy iterator method must still produce
the entry `%TypeError.prototype%` and exact diagnostic. Its abrupt `next`
cases require a zero `return` count. The pinned Test262 checkout has no direct leaf with a
callable Proxy iterator method or callable Proxy `next`; `iterator-as-proxy.js`
only proxies the iterator object.

## Nonclaims

This contract does not absorb the two errors created by shared
`emit_iterator_close`; those remain governed by
[`iterator-close-error-realm.md`](./iterator-close-error-realm.md). At this
historical checkpoint it did not cover the asynchronous iterator protocol,
Array destructuring, ArrayAccumulation, or `Math.sumPrecise`. The later
four-consumer contract supersedes the synchronous part of that nonclaim.
User-thrown accessor, Proxy, and callee completions propagate unchanged.
Assignment, disposal, and close errors retain their separate owners. This
follow-up does not change Proxy `[[Call]]` itself. Its revoked proxies are made
in the entry Realm, so it does not prove the Realm of a cross-Realm
Proxy-internal TypeError. That limitation belonged to T11 at this checkpoint;
the later
[`proxy-call-construct-execution-realm.md`](./proxy-call-construct-execution-realm.md)
contract owns the cross-Realm Proxy behavior and the raw
`built-ins/Proxy/apply/null-handler-realm.js` source.

This checkpoint makes no complete iterator, Test262, semantic-golden,
published-status, or broad workspace claim. It does not change the
`29 + 2 + 5 + 10 = 46` spec-operation catalog census.

## Focused verification

At this historical checkpoint, `cargo fmt --all -- --check` and
`cargo check -p scriptw-aot-wasm` passed. The direct Realm, protocol-error
ownership, then-current selector-capability, Array, String,
IteratorClose, plain-async synchronous-iterator, synchronous-using, and
async-disposable-head structure targets pass `37/37`. The exact error fixture
and the four affected success-path CLI controls pass `5/5`. The fixture also
passes `node --check`.

Four pinned direct `for-of` leaves pass all `8/8` sloppy/strict Wasm-AOT
executions with every failure and non-success bucket at zero:

- `head-expr-to-obj.js`;
- `head-expr-primitive-iterator-method.js`;
- `head-expr-obj-iterator-method.js`; and
- `iterator-next-result-type.js`.

The callable-Proxy and body-Realm-source follow-up also passes the all-target
`scriptw-aot-wasm`/`scriptw-cli` compile and formatting check. The five directly
affected structure targets pass `23/23`; the callable-Proxy, retained direct
protocol-error, `Math.sumPrecise`, Array-destructuring, and ArrayAccumulation
CLI controls pass `5/5`; and the fixture passes `node --check`. Eight unchanged
direct iterator and Proxy-apply Test262 leaves pass all `16/16` sloppy/strict
executions with every failure and non-success bucket at zero. Module-boundary,
task-plan, shortcut-inventory, and diff checks are green; the shortcut total
remains exactly 240.

The focused commands included:

```sh
cargo check -p scriptw-aot-wasm
cargo test -p scriptw-aot-wasm --test direct_sync_for_of_protocol_error_realm_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test sync_iterator_protocol_error_ownership_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test structure_async -- sync_iterator_consumer_capability_structure:: --test-threads=1
cargo test -p scriptw-cli --test cli iterator::run_wasm_backend_reports_direct_for_of_protocol_type_errors -- --exact
cargo test -p scriptw-cli --test cli iterator::run_wasm_backend_preserves_direct_for_of_callable_proxy_methods -- --exact
./target/debug/scriptw --jobs 1 test262 run language/statements/for-of/head-expr-to-obj.js --suite-root test262/vendor/test262 --execution-backend wasm-aot --timeout-ms 180000 --threads 1
./target/debug/scriptw --jobs 1 test262 run language/statements/for-of/head-expr-primitive-iterator-method.js --suite-root test262/vendor/test262 --execution-backend wasm-aot --timeout-ms 180000 --threads 1
./target/debug/scriptw --jobs 1 test262 run language/statements/for-of/head-expr-obj-iterator-method.js --suite-root test262/vendor/test262 --execution-backend wasm-aot --timeout-ms 180000 --threads 1
./target/debug/scriptw --jobs 1 test262 run language/statements/for-of/iterator-next-result-type.js --suite-root test262/vendor/test262 --execution-backend wasm-aot --timeout-ms 180000 --threads 1
./target/debug/scriptw --jobs 1 test262 run language/statements/for-of/iterator-as-proxy.js --suite-root test262/vendor/test262 --execution-backend wasm-aot --timeout-ms 180000 --threads 1
./target/debug/scriptw --jobs 1 test262 run built-ins/Proxy/apply/call-parameters.js --suite-root test262/vendor/test262 --execution-backend wasm-aot --timeout-ms 180000 --threads 1
```

The source-structure target was
`direct_sync_for_of_protocol_error_realm_structure.rs`; it is not in the tree.
Its intended checks are
`direct_for_of_owners_project_five_typed_protocol_errors`,
`async_disposable_for_of_boxes_primitives_in_the_current_function_realm`,
`resumable_sync_for_of_delegates_five_typed_protocol_checks`, and
`for_of_protocol_errors_project_from_the_closed_body_realm_source`.

No semantic golden, published-status refresh, complete Test262 directory, or
broad workspace suite was run. The focused results do not change published
aggregate counts.
