# Proxy revocation route ownership

Status: the 2026-10-04 helper and four-operation descriptor source is authored;
compilation and runtime verification are pending. Historical source and
execution checkpoints are retained below.

`ProxyRevocationRoute` is a crate-private, capability-free policy. Its variants
select Realm and throw routing; they carry no native resource or heap ownership.

## Delete active-handler routing — 2026-10-04 dry source

The actual Delete revocation producer now selects ProxyExecutionRealmToActiveHandler.
Generated noncallable and invariant errors use the same execution Realm and
propagate into the active source catch/finally handler. Builtin/helper calls
retain their existing completion return when no handler exists. Original
user-thrown completions retain identity and prior/finally effects.

CurrentCompletion has no remaining producer, so its variant and consuming arm
are retired. Three actual routes remain: CurrentFunctionRealm,
ProxyExecutionRealmToActiveHandler and ObjectMutationRealmToActiveHandler,
with seventeen source identifier mentions across the consumed producers/router.
The maintained existing guard follows this domain. Paired semantic controls
and compilation/runtime verification remain unrun. See the
[completed target contract](proxy-target-descriptor-completion.md).

## Traversal helper Realm handoff — 2026-10-04 dry source

The real recursive GetPrototypeOf, IsExtensible and PreventExtensions helpers
receive the outer operation's trusted execution Realm context in parameter6
and install it in their current-environment local. Existing closed Proxy
execution and object-read error source projections retain that context through
recursive traversal, handler Get and trap dispatch. Their scalar signatures
and target parameters are unchanged.

All three helper producers consume `ProxyExecutionRealmToActiveHandler`: it
selects the trusted Proxy execution Realm before preserving active-handler throw
routing. Generated noncallable, invalid-result and inconsistent-target errors
use the same Realm projection; original trap/getter thrown values are preserved.
The former `ActiveHandler` route has no producer and is deleted together with
its fallback router arm. The other producer policies and Proxy slot layout are
unchanged. At this helper checkpoint the same ten actual producers consumed
four routes with eighteen source mentions. The newer Delete source above
retains three routes and seventeen mentions. Existing Realm and revocation
guards follow the current consumed domain.

Object.preventExtensions separately uses its called builtin's defining Realm
for a false-result TypeError. Reflect.preventExtensions keeps the Boolean;
primitive Object behavior and the existing completed trap/request roles are
preserved. Finite paired Engine controls cover all three own-key consumers,
both GetPrototypeOf consumers and both PreventExtensions consumers in both
borrowed Realm directions. They retain native error prototypes, arbitrary
foreign marker identity, prior assignment/finally effects, early error ordering
and normal result identity/policies after mutable globals are clobbered.
Compilation/runtime verification remains pending; historical receipts below do
not verify this revision. Full T11, other trap families and semantic GC remain
open.

## Historical ten-producer boundary

`ProxyRevocationRoute::{CurrentFunctionRealm, ActiveHandler,
ObjectMutationRealmToActiveHandler, CurrentCompletion}` is the crate-private
one-shot authority that decides both how the shared live-Proxy slot reader
selects a revoked-Proxy TypeError Realm and how that completion leaves the
current body. It has ten exact producers and one consuming exhaustive router:

- Proxy `defineProperty`, `ownKeys`, `getOwnPropertyDescriptor` and direct
  `Reflect.set` use `CurrentFunctionRealm`; the shared `HasProperty` helper
  uses the same route with its trusted caller Realm argument;
- Proxy `getPrototypeOf`, `preventExtensions` and `isExtensible` use
  `ActiveHandler`;
- Proxy `setPrototypeOf` uses `ObjectMutationRealmToActiveHandler`; and
- Proxy `deleteProperty` uses `CurrentCompletion`.

The authority derives no cloning, copying, formatting, equality, ordering,
hashing or default-construction capability. The router consumes it before any
live handler tag or target word is exposed. Reusing one route for a second
routing decision is therefore a move error, while adding a variant requires an
explicit throw policy in the exhaustive router before the crate builds.

## Durable evidence

Focused verification commands:

```sh
cargo test -p scriptw-cli --test cli object::run_wasm_backend_succeeds_for_proxy_define_property_handler_protocol -- --exact --test-threads=1
cargo test -p scriptw-cli --test cli object::run_wasm_backend_succeeds_for_supported_proxy_get_prototype_of_fixture -- --exact --test-threads=1
cargo test -p scriptw-cli --test cli object::run_wasm_backend_succeeds_for_proxy_set_prototype_of_handler_protocol -- --exact --test-threads=1
cargo test -p scriptw-cli --test cli object::run_wasm_backend_succeeds_for_supported_proxy_delete_property_fixture -- --exact --test-threads=1
cargo test -p scriptw-aot-wasm --test proxy_reflect_set_handler_protocol_structure -- --test-threads=1
cargo test -p scriptw-cli --test cli object::run_wasm_backend_succeeds_for_proxy_reflect_set_handler_protocol -- --exact --test-threads=1
```

At the prior eight-producer checkpoint, the ownership target passed `4/4`.
The exact define-property current-Realm, get-prototype active-handler and
delete-property current-completion Wasm-AOT fixtures each passed `1/1`. At the
expanded checkpoint, the ten-producer structure target passes `4/4`; the exact
SetPrototypeOf Realm-aware active-handler and direct Reflect Set current-
function-Realm commands have no individually attributed current result here.
T11 owns the collective seven-CLI result. This does not claim a broad compile,
Test262 or published conformance result.

## Nonclaims

The SetPrototypeOf route is deliberately Realm-correcting: it retains the
existing active-handler completion route and message while selecting the
standard builtin's trusted object-mutation Realm instead of the main-Realm
runtime-error fallback. The accompanying handler-acquisition correction changes
SetPrototypeOf slot loads and trap lookup as specified by
`proxy-set-prototype-of-handler-protocol.md`; those changes are not attributed
to the route type itself. Adding direct `Reflect.set` remains source-equivalent:
it preserves its prior current-function-Realm route while its handler
acquisition changes under `proxy-reflect-set-handler-protocol.md`. Realm
forwarding into all three outlined traversal helpers is authored in the dry
follow-up above. This contract changes no Proxy slot layout and
does not close the complete Proxy or Object task, recursive Proxy descriptor
protocols or broad Test262 coverage.
