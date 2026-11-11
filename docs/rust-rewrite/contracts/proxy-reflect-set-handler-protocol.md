# Proxy `[[Set]]` handler protocol in direct `Reflect.set`

Status: implemented and focused structure-verified 2026-09-01 as a bounded
T11 handler-acquisition change.

## Scope

This contract owns the Proxy branch inside `compile_reflect_set_builtin`: live
slot acquisition, `GetMethod(handler, "set")`, immediate abrupt-completion
routing and a present trap's Call. The Boolean result, post-trap descriptor
invariant, nullish target fallback, ordinary fallback, error Realm and
temporary-local release order remain unchanged. Property-key conversion and
optional receiver presence are now pinned by their adjacent bounded contracts.

This is the direct `Reflect.set` owner. Assignment and internal Set requests
continue through `emit_object_write` and are outside this boundary.

## Typed acquisition boundary

The emitter classifies the outer Object as a Proxy, then enters the shared
typed live-slot reader once with distinct `ProxyTargetLocals` and
`ProxyHandlerLocals`. `ProxyRevocationRoute::CurrentFunctionRealm` preserves
the borrowed `Reflect.set` builtin's existing revoked-handler TypeError Realm.
The retained handler payload and tag are both the base and receiver of the full
Proxy-aware property read.

The method may read `HEAP_OBJECT_BOXED_KIND_OFFSET` only to classify the outer
Proxy and a nested target during nullish fallback. It may not read raw target
payload/tag offsets, fabricate an Object handler tag, use the ordinary-only
read, or construct the revoked-handler error locally.

## Observable order

For a direct `Reflect.set(proxy, key, value, receiver)` request:

1. reject a revoked Proxy and retain its exact tagged target and handler;
2. perform the full `GetMethod(handler, "set")` read with the handler as
   receiver;
3. route an abrupt read before callable or nullish classification;
4. call a Function or callable Proxy trap with the handler as `this` and exact
   `(target, key, value, receiver)` arguments; or
5. for an `undefined` or `null` trap, continue through the retained target's
   Set path.

The Call owner performs the sole trap-call throw propagation. A normal trap
result is converted to Boolean before the existing direct-target Proxy Set
invariant runs. A nullish trap still reaches nested Proxy targets through the
existing recursive `Reflect.set` call.

## Focused evidence

`wasm_proxy_reflect_set_handler_protocol.js` covers Function, Array, arguments
and Proxy handlers; exact getter receiver and trap `this`; a Symbol property
key; an exact Function target and Array receiver; exact value; a callable Proxy
trap; abrupt handler lookup and trap-call identity; and nullish fallback to a
nested Proxy target. The existing `wasm_proxy_set_error_realm.js` continues to
cover borrowed created-Realm revoked and non-callable direct `Reflect.set`
failures.

`proxy_reflect_set_handler_protocol_structure` pins the typed acquisition,
full read, immediate abrupt checkpoint, Function-or-Proxy Call arguments,
unchanged result/invariant/fallback/release seams, both live CLI fixtures, the
module-boundary census and this contract. The module guard rejects raw live
slot reconstruction and Function-only trap dispatch in the acquisition span.

The write-phase marker `Verification pending` is retained here only as the
historical status superseded by the measured checkpoint below.

## Focused verification

The contract's focused command set is:

```sh
cargo fmt --all -- --check
git diff --check
cargo test -p scriptw-aot-wasm --test proxy_reflect_set_handler_protocol_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test modules_realms -- object_write_proxy_realm_structure:: --test-threads=1
cargo test -p scriptw-cli --test cli object::run_wasm_backend_succeeds_for_proxy_reflect_set_handler_protocol -- --exact --test-threads=1
cargo test -p scriptw-cli --test cli object::proxy_set_errors_use_the_borrowed_builtin_realm -- --exact --test-threads=1
./scripts/check-module-boundaries.sh
```

The direct handler-protocol structure target passes `4/4`, the revocation-route
target passes `4/4`, and the object-write Realm target passes `5/5`. The exact
CLI commands in this block and `cargo check -p scriptw-aot-wasm` have no
individually attributed result here; T11 owns the collective seven-CLI and
shared-gate results. No broad compile, Test262 or published conformance result
is claimed.

## Explicit nonclaims

This handler-acquisition checkpoint did not migrate assignment or internal
Set acquisition, generic Proxy Get or module namespace Set behavior. The later
[completed target source](proxy-target-descriptor-completion.md) now addresses
the shared recursive target descriptor invariants with the existing set-path
Realm authority; compilation and execution are pending. The complete Proxy
Set/Test262 tree remains open, and no published conformance count changes.
