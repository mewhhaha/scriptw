# `GetFunctionRealm` outcome code

Status: source-equivalent Wasm-AOT invariant, owner extraction dry-verified on
2026-08-28.

## Closed authority

`GetFunctionRealm` carries its emitter-time result through the private
`FunctionRealmOutcome::{Resolved, Revoked, Invalid}` domain. The domain derives
no cloning, copying, debugging, equality, ordering, hashing or default
capability and does not use Rust representation or discriminant order as its
run-time ABI.

One borrowed exhaustive projection owns the existing raw Wasm `i64` codes:

- `Resolved` is 0;
- `Revoked` is 1; and
- `Invalid` is 2.

Adding an outcome without assigning its code is therefore `E0004`. The three
writer transitions and both router comparisons use that projection. The router
handles Revoked first through the caller's closed route, traps Invalid, releases
the outcome local, and only then exposes `ResolvedFunctionRealmLocal`.

The complete result lifecycle has one private
`functions/function_realm.rs` owner: the outcome, raw-result and resolved
states, the revoked-route domain, and the Get, route and release methods. The
parent re-exports only `FunctionRealmRevokedRoute` for the three sibling
consumer modules and privately imports `ResolvedFunctionRealmLocal` for its
retained callers. Neither the outcome nor the raw-result state has a parent
facade.

This is narrower than a typed Wasm state machine. The run-time local remains an
`i64`; the structural guard pins its exact writes, reads, polarity and release.
It does not claim that arbitrary numeric corruption is unrepresentable in Wasm.

## Consumer closure

Every `emit_get_function_realm` result is immediately consumed by
`emit_route_function_realm_result`. The seven product pairs select exactly:

- one `ThrowTypeErrorAndBranch` route in generic construction;
- five `ThrowTypeErrorAndReturn` routes for required constructor prototypes,
  including the ordinary and derived dynamic-function constructors'
  `GetPrototypeFromConstructor`; and
- one `UseCurrentRealm` route for Promise-job callbacks.

The migration changes no caller policy, Realm selection, Proxy traversal,
TypeError route, branch depth, local reservation/release order or emitted
instruction order.

## Durable evidence

The structure test `function_realm_outcome_structure.rs`, no longer in the tree, used a
Rust-lexical recursive census to pin the private no-capability declaration, the
exhaustive 0/1/2 projection, all five projection sites, the three writers, the
Revoked-before-Invalid router and the seven Get/route pairs with their exact
policy census. It also pins the private file module, sole type and method owner,
narrow route re-export and private resolved-witness import. The extracted 272
source lines retain SHA-256
`4305eed14fcf73c1330411004824c66f73bacbf3421cf1bfcc901c25bc2ae548`.
The 278-line child has SHA-256
`a62b41ffde6966725c2d18e594fd1232ceed9cd632362389de64dc5ae3107415`.

Focused verification:

```sh
cargo test -p scriptw-aot-wasm --test function_realm_outcome_structure --quiet
bash scripts/check-module-boundaries.sh
./scripts/conformance_owners.py
rustfmt --edition 2024 --check \
  crates/scriptw-aot-wasm/src/functions/function_realm.rs \
  crates/scriptw-aot-wasm/tests/function_realm_outcome_structure.rs
git diff --check
```

The owner-extraction checkpoint runs only these scoped structure, boundary,
format and diff checks. CLI witnesses, the semantic golden, workspace compile,
Test262 and broad suites remain deferred to the coordinated shared
verification checkpoint; their earlier results are not claimed as evidence for
this extraction.

## Nonclaims

This invariant adds no Realm, constructor, Promise, Proxy or dynamic-source
behavior. It does not complete T06 or T09, prove all constructor fallbacks, or
replace the complete current-pin conformance publication.
Independent dry re-review is clean after the complete router tail and all five
owner-bounded Get/route pairs were pinned.
