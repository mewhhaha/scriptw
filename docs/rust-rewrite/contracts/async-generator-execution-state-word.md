# Async-generator execution state word

Status: focused-verified for the T14/T15 Wasm-AOT invariant lane on 2026-08-24.

## Specification boundary

The edition-pinned ECMA-262 table of
[`Properties of AsyncGenerator Instances`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-properties-of-asyncgenerator-instances)
defines the exact five-value ECMA-262 domain for
`[[AsyncGeneratorState]]`:

- suspended-start;
- suspended-yield;
- executing;
- draining-queue; and
- completed.

The async-generator abstract operations make that domain closed.
`AsyncGeneratorResume` accepts a suspended state and stores executing before
resuming the generator context. `AsyncGeneratorYield` stores suspended-yield
before the yielded iterator result escapes. Body termination and an early
`return` request use draining-queue while requests are settled, and
`AsyncGeneratorDrainQueue` stores completed after the queue becomes empty.

Await does not add another `[[AsyncGeneratorState]]` value. An async generator
whose body is awaiting remains executing while its Promise reaction owns the
continuation. Scriptw already persists that finer backend phase separately as
`AsyncGeneratorBodyStatus::Await`; it is not part of the specification's state
domain.

## Representation defect closed

Before this migration, Scriptw exposed six raw `ASYNC_GENERATOR_STATE_*` integer
constants and let generic heap helpers access
`HEAP_ASYNC_GENERATOR_EXECUTION_STATE_OFFSET`. Five words matched the
specification:

| state | word |
|---|---:|
| suspended-start | 0 |
| suspended-yield | 1 |
| executing | 2 |
| draining-queue | 3 |
| completed | 4 |

The sixth word, `SUSPENDED_AWAIT = 5`, widened `[[AsyncGeneratorState]]` with a
backend phase already represented by body status. Any arbitrary integer could
also be stored, and the async-generator prototype dispatcher treated every
unknown word like executing: it enqueued the request and silently skipped all
state routes. Two reaction-job readers compared only with word `5`; they did
not validate the field against a closed domain.

The standard-builtin writer also reused one raw Wasm local for two different
domains: receiver brand before request construction, execution state after
queue publication. That was mechanically valid because both were `u32` local
indices. A future reorder could compare the wrong value without a Rust type
error.

The word `5` is retired by the migration. Await suspension now stores executing
and the two Promise reaction readers strictly require executing, while the
independent Await body status retains the backend phase. Existing JavaScript
routing is unchanged because the prototype dispatcher already treated
executing and the old suspended-await word identically. Pre-1.0 emitted modules
are not a compatibility surface.

## Closed Rust domain

`heap.rs` owns the only representation projection:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncGeneratorExecutionState {
    SuspendedStart,
    SuspendedYield,
    Executing,
    DrainingQueue,
    Completed,
}

impl AsyncGeneratorExecutionState {
    const ALL: [Self; 5] = [
        Self::SuspendedStart,
        Self::SuspendedYield,
        Self::Executing,
        Self::DrainingQueue,
        Self::Completed,
    ];

    const fn word(self) -> u64 {
        match self {
            Self::SuspendedStart => 0,
            Self::SuspendedYield => 1,
            Self::Executing => 2,
            Self::DrainingQueue => 3,
            Self::Completed => 4,
        }
    }
}
```

There is no `repr`, discriminant cast, catch-all arm, `Default`, integer or
Boolean constructor, public word projection, unchecked decoder, or sixth
backend variant. Adding a state must fail exhaustiveness until its stable word
and every product route are decided.

## Typed heap boundary

The raw offset is private to `heap.rs`. Four operations own it:

1. `emit_store_async_generator_execution_state` accepts only
   `AsyncGeneratorExecutionState`;
2. `emit_load_async_generator_execution_state_strict` performs one heap load,
   validates the stable snapshot against every member of `ALL`, traps after
   all misses and returns an opaque token;
3. `emit_async_generator_execution_state_equals` borrows the token and accepts
   only one expected enum member; and
4. `release_loaded_async_generator_execution_state` consumes the token after
   its owner emits every comparison.

The token is deliberately opaque and non-`Copy`:

```rust
#[must_use = "a loaded async-generator execution state must be routed and released"]
pub(crate) struct LoadedAsyncGeneratorExecutionState(u32);
```

Only the strict loader can construct it. No product owner can obtain the raw
Wasm local, build a token from a receiver-brand local, overwrite it with a
resume kind, or forget that a stable state snapshot must be released.

## Exact owner census

The migration covers seventeen product writers and three product readers.
The helper definitions are not product owners.

| file | stores | strict loads | comparisons | releases |
|---|---:|---:|---:|---:|
| `functions.rs` | 4 | 0 | 0 | 0 |
| `builtins/promise.rs` | 2 | 2 | 2 | 2 |
| `builtins/standard.rs` | 3 | 1 | 3 | 1 |
| `control_flow.rs` | 5 | 0 | 0 | 0 |
| `generator_delegation.rs` | 3 | 0 | 0 | 0 |

The writers select these exact lifecycle states:

- async-generator allocation publishes suspended-start only after every other
  activation field is initialized and before publishing the activation through
  the generator object;
- the body driver stores executing before calling the body and stores
  draining-queue on both terminal completion paths before request settlement;
- queue draining stores completed only after clearing the now-empty active
  request;
- ordinary `yield` and delegated `yield` publish suspended-yield before their
  results escape;
- body Await, `await using`, both `for-await-of` layouts, delegated Await and
  yield-return Await store executing while the separate body status stores
  Await; and
- completed or suspended-start request shortcuts store draining-queue before
  terminal settlement.

The readers are:

- `%AsyncGeneratorPrototype%.next`, `.return` and `.throw`, which take one
  strict snapshot after publishing the first queued request, then compare it
  with completed, suspended-yield and suspended-start in the existing route
  order; and
- the ordinary Await and yield-return Await Promise reaction jobs, which each
  take one strict snapshot, require executing, separately require Await body
  status, resume the body and finally consume the state token.

Executing and draining-queue need no explicit standard-builtin comparison:
after strict validation they intentionally leave the newly enqueued request
for the active body or drain operation. Unknown words never reach that
fallthrough.

## Durable source witness

The structure test `async_generator_execution_state_structure.rs`, no longer in the tree,
pinned:

- exactly five variants, one `ALL` list and one exhaustive stable projection;
- absence of the retired state constants and suspended-await variant;
- the private four-occurrence raw offset;
- one-load strict validation with an unknown-word trap;
- opaque token construction, borrowing and consuming release;
- the complete seventeen-writer/three-reader product census;
- exact per-file state selections;
- allocation, body-entry, terminal-drain, queue-publication and token-release
  order; and
- the separation between executing lifecycle state and Await body status.

The neighboring request-completion and async-generator await-using guards now
name the typed execution-state operation instead of snapshotting a raw offset
store.

## Verification

The implementation lane performed its bounded source review, and the central
batch verifier then ran:

```sh
cargo fmt --all -- --check
cargo xc
cargo test -p scriptw-aot-wasm --test async_generator_execution_state_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test async_generator_request_completion_kind_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test structure_async -- async_generator_await_using_structure:: --test-threads=1
```

The three structure targets pass `5/5`, `5/5` and `5/5`. The five existing
async-generator lifecycle, await-using and delegated `next`/`return`/`throw`
CLI witnesses selected by the request-completion contract pass `5/5`. Its five
exact pinned Test262 files pass `10/10` sloppy/strict Wasm-AOT executions under
`--jobs 1 --threads 1`, with every non-success bucket at zero. The shared fake
Wasm-safe and complete fixture gates also pass `187/187` and `191/191`.

## Explicit nonclaims

This invariant does not type resume-state labels, pending completion records or
Promise reaction kind. Resume kind, body status and request completion kind are
closed by their own focused contracts.
It does not repair general continuation spilling, the known resumable-loop
failure, cross-realm behavior, queue ownership, GC layout or broader async
generator conformance. It changes no published README count and does not
complete T14 or T15.
