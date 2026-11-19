# Async-generator body-status word

Status: focused-verified for the T14/T15 Wasm-AOT invariant lane on
2026-08-24.

## Backend boundary

The async-generator activation carries a backend body-status word in addition
to the specification's `[[AsyncGeneratorState]]`. The six-value
async-generator body-status domain records the only results that the compiled
body protocol can publish:

| status | word | meaning |
|---|---:|---|
| idle | 0 | the activation has been allocated but its body has not run |
| running | 1 | the body invocation has begun and has not suspended |
| await | 2 | a Promise reaction owns the body continuation |
| yield | 3 | a yielded result is ready for request settlement |
| complete | 4 | the body completed normally |
| throw | 5 | the body completed abruptly by throwing |

This is deliberately distinct from the exact five-value ECMAScript
`AsyncGeneratorExecutionState` domain. In particular, Await keeps execution
state `Executing` while body status records `Await`; body status must not add a
sixth specification lifecycle state or reuse an execution-state token.

## Representation defect closed

Before this migration, six public integer constants and a public heap offset
let every producer write arbitrary words through the generic constant-store
helper. The three consumers loaded the field into raw Wasm locals. The two
Promise reaction jobs compared only with Await and therefore rejected a
wrong valid status, but the body driver treated every unknown word like an
unrecognized valid status and fell through to its terminal settlement path.

The allocation path also initialized body status inside a generic raw
offset/value loop, and the body driver reused one compiler local first for a
resume-state word and later for body status. Those domains happened to share a
64-bit representation; Rust could not prevent a future comparison or store
from using the wrong one.

## Closed Rust domain

`heap.rs` now owns the sole stable projection:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncGeneratorBodyStatus {
    Idle,
    Running,
    Await,
    Yield,
    Complete,
    Throw,
}
```

The private `ALL` list contains exactly those six members. The private `word`
function uses an exhaustive match for the stable words 0 through 5. There is no
`repr`, discriminant cast, catch-all arm, default, integer constructor,
unchecked decoder or public word constant. Adding a status must fail
exhaustiveness until its stored representation and product routing are chosen.

## Typed heap boundary

The raw offset is private to `heap.rs`. Four operations own it:

1. `emit_store_async_generator_body_status` accepts only
   `AsyncGeneratorBodyStatus`;
2. `emit_load_async_generator_body_status_strict` performs one heap load,
   compares its stable snapshot with every member of `ALL`, traps after all
   misses and returns an opaque token;
3. `emit_async_generator_body_status_equals` borrows that token and accepts
   only one enum member; and
4. `release_loaded_async_generator_body_status` consumes the token once all
   comparisons have been emitted.

The token is non-`Copy` and exposes no raw local:

```rust
#[must_use = "a loaded async-generator body status must be routed and released"]
pub(crate) struct LoadedAsyncGeneratorBodyStatus(u32);
```

The body driver now reserves a separately named `resume_state_local`. Its body
status is created only by the strict loader, so resume labels, execution state
and body status remain separate Rust-level domains.

## Exact owner census

The migration covers fifteen product writers and three product readers. The
helper definitions are not product owners.

| file | stores | strict loads | comparisons | releases |
|---|---:|---:|---:|---:|
| `functions.rs` | 5 | 1 | 2 | 1 |
| `builtins/promise.rs` | 1 | 2 | 2 | 2 |
| `builtins/standard.rs` | 1 | 0 | 0 | 0 |
| `control_flow.rs` | 5 | 0 | 0 | 0 |
| `generator_delegation.rs` | 3 | 0 | 0 | 0 |

The writers publish:

- Idle during activation allocation, before suspended-start execution state
  and before the activation becomes reachable through the generator object;
- Running before each indirect body invocation;
- Await from ordinary body Await, async disposal, both async-iteration layouts,
  delegated Await and yield-return Await;
- Yield from ordinary and delegated yield paths; and
- Complete or Throw on the body driver's terminal routes.

The body driver strictly loads one snapshot after every body invocation. It
routes Yield first, then Running, and releases the token before terminal
settlement emission. The ordinary Await and yield-return Await reaction jobs
each strictly require both `AsyncGeneratorExecutionState::Executing` and
`AsyncGeneratorBodyStatus::Await` before resuming the body. An unknown status
traps in the strict loader before any of those routes can observe it.

## Durable source witness

The structure test `async_generator_body_status_structure.rs`, no longer in the tree, pinned:

- the exact six variants, one complete `ALL` list and exhaustive stable words;
- absence of the retired raw constants and unchecked conversions;
- the private four-occurrence offset and typed store/load/compare/release seam;
- one-load validation with an unknown-word trap and opaque token ownership;
- the complete fifteen-writer/three-reader source-tree census;
- exact per-file status selections; and
- allocation, body invocation, routing, resumption and token-release order,
  including separation from execution state and resume labels.

The neighboring execution-state, request-completion and await-using witnesses
now name the typed body-status boundary instead of depending on the private raw
offset or retired integer constants.

## Verification

The implementation lane performed source review, formatting and diff hygiene.
The central batch verifier ran:

```sh
cargo fmt --all -- --check
cargo xc
cargo test -p scriptw-aot-wasm --test async_generator_body_status_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test async_generator_execution_state_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test async_generator_request_completion_kind_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test structure_async -- async_generator_await_using_structure:: --test-threads=1
```

`cargo fmt --all -- --check`, `cargo xc` and `git diff --check` are green. The
body-status, execution-state, request-completion and await-using structure
targets each pass `5/5`. The five exact async-generator lifecycle and
delegation CLI tests pass `5/5`; their five pinned Test262 files pass `10/10`
sloppy/strict Wasm-AOT executions with every non-success bucket at zero.

## Explicit nonclaims

This invariant does not type resume-state labels, pending completion records,
Promise reaction kinds or body result payloads and tags. Resume kind is closed
by its own focused contract. It does not repair general continuation spilling,
the known resumable-loop failure, cross-realm behavior, queue ownership, GC
layout or broader async-generator conformance. It changes no published README
count and does not complete T14 or T15.
