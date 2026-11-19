# Async-generator request completion kind

Status: implemented with its durable structural guard on 2026-08-23 and
focused-verified on 2026-08-24. The source census was taken at repository commit
`dfed6ae911014c6fd512627b29ae04518e912a38`.

## Specification boundary

The edition-pinned ECMA-262
[`AsyncGeneratorRequest` Record](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-asyncgeneratorrequest-records)
stores a Completion Record in `[[Completion]]`. Whenever the specification entry
points enqueue a request, they form a closed subset of the general Completion
Record domain:

- [`%AsyncGeneratorPrototype%.next`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-asyncgenerator-prototype-next)
  enqueues `NormalCompletion(value)` unless its completed-state shortcut
  settles directly without a request;
- [`%AsyncGeneratorPrototype%.return`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-asyncgenerator-prototype-return)
  enqueues `ReturnCompletion(value)`; and
- [`%AsyncGeneratorPrototype%.throw`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-asyncgenerator-prototype-throw)
  enqueues `ThrowCompletion(exception)` unless its suspended-start or
  completed-state shortcut rejects directly without a request.

[`AsyncGeneratorEnqueue`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-asyncgeneratorenqueue)
copies that completion into the request record. No specification call site can
enqueue Break, Continue or an empty completion. Those completion kinds are
meaningful inside ECMAScript evaluation, but cannot cross this request
boundary.

The Promise capability stored beside this completion now has a separate Realm
ownership contract in `async-generator-request-promise-realm.md`. This document
continues to own only the closed completion-kind word.

The consumers preserve the same three-way distinction:

- [`AsyncGeneratorYield`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-asyncgeneratoryield)
  either unwraps the next queued Normal or Throw completion, or awaits the
  value of a queued Return completion before continuing termination; and
- [`AsyncGeneratorDrainQueue`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-asyncgeneratordrainqueue)
  resolves a queued Normal completion with a terminal iterator result, rejects
  a queued Throw completion, and awaits a queued Return completion before
  settlement.

The persisted request word is therefore not a generic completion word merely
because its three valid values reuse the general completion ABI. Its semantic
domain is exactly Normal, Return and Throw.

## Inventoried representation hazard

Scriptw persists the request's completion kind at
`HEAP_ASYNC_GENERATOR_REQUEST_COMPLETION_KIND_OFFSET`. The current writer
stores one of the generic `COMPLETION_KIND_*` integer constants, and both
readers load the word into an untyped `u32` Wasm-local index.

The generic ABI also defines Break, Continue and Empty. Passing any of those
words, a Promise state, an async-generator resume kind, or an arbitrary integer
to the generic heap writer builds successfully. The queue-drain reader happens
to end its three comparisons in `Instruction::Unreachable`, but the live-yield
reader has only explicit Return and Throw tests. Every other word falls through
as Normal and resumes the generator body.

That fallthrough makes this more than a naming cleanup. A wrong-domain or
unknown request word can silently inject a Normal completion at a live yield
instead of trapping as an impossible record.

## Closed Rust domain and ABI projection

Replace the generic integer selection with one closed domain in `heap.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncGeneratorRequestCompletionKind {
    Normal,
    Return,
    Throw,
}
```

The enum must have one private, exhaustive projection through the existing
`CompletionKind` ABI rather than restating `0`, `1` and `2`:

```rust
impl AsyncGeneratorRequestCompletionKind {
    const ALL: [Self; 3] = [Self::Normal, Self::Throw, Self::Return];

    const fn completion_kind(self) -> CompletionKind {
        match self {
            Self::Normal => CompletionKind::Normal,
            Self::Return => CompletionKind::Return,
            Self::Throw => CompletionKind::Throw,
        }
    }

    const fn word(self) -> u64 {
        self.completion_kind().code() as u64
    }
}
```

`ALL` follows the stable ABI word order used by the strict decoder; the enum's
declaration order remains semantic. The representation continues to be Normal
`0`, Throw `1`, Return `2` because `CompletionKind::code` is derived from
`CompletionKindIr::abi_code`. The request domain must not introduce a second
hand-written numeric table.

The type has no `repr`, discriminant cast, `Default`, catch-all projection,
integer constructor, Boolean constructor, unchecked decoder, or public word
accessor. Adding a variant must fail exhaustiveness until its relation to the
existing completion ABI is decided.

## Typed heap ownership

`HEAP_ASYNC_GENERATOR_REQUEST_COMPLETION_KIND_OFFSET` becomes private to
`heap.rs`. Four heap-boundary operations own it:

1. `emit_store_async_generator_request_completion_kind` accepts only
   `AsyncGeneratorRequestCompletionKind`;
2. `emit_load_async_generator_request_completion_kind_strict` performs one
   heap load, validates the word against every member of `ALL`, traps after all
   misses, and returns an opaque loaded token;
3. `emit_async_generator_request_completion_kind_equals` borrows that token
   and compares it with one expected enum member; and
4. `release_loaded_async_generator_request_completion_kind` consumes the
   token and releases its private Wasm local.

The token should have this shape:

```rust
#[must_use = "a loaded request completion kind must be routed and released"]
pub(crate) struct LoadedAsyncGeneratorRequestCompletionKind(u32);
```

Its field is private. It has no `Copy`, `Clone`, raw-local accessor, arbitrary
local constructor, integer conversion, dereference implementation, or public
pattern-match surface. Only the strict loader may mint it.

The existing `emit_complete_async_generator_step` helper consumes a generic
completion-kind local because body completion and awaited-return reactions are
outside this request-field domain. Queue draining therefore needs one explicit
ABI adapter: a typed helper may copy the validated token's word into a named
generic step-completion local. That helper must not return the token's raw
local, and its only product uses are the queue-drain Normal and Throw branches.
The Return branch creates a fresh generic Throw word only if AwaitReturn setup
fails. No comparable escape is needed by the live-yield reader.

This adapter is the boundary between a validated request record and the
pre-existing general completion transport. It is not permission to expose the
request local or to make the request field generic again.

## Exact current owner census

At the inventoried commit, the raw offset appears exactly five times:

| file and owner | access | count |
|---|---|---:|
| `heap.rs` offset declaration | metadata | 1 |
| `heap.rs` request layout descriptor | metadata | 1 |
| `standard.rs::compile_standard_builtin` async-generator request arm | store | 1 |
| `promise.rs::emit_drain_async_generator_queue` | load | 1 |
| `promise.rs::emit_complete_async_generator_yield` | load | 1 |

There is one semantic writer and two semantic readers. No other product owner
is implied by the generic completion constants or by the request payload, tag,
capability, Promise and next-pointer fields.

After migration, the offset should occur only in `heap.rs`: declaration,
layout metadata, typed store and strict load. The writer and both readers name
only the typed operations.

### Sole writer: request allocation

The combined `AsyncGeneratorPrototypeNext | AsyncGeneratorPrototypeReturn |
AsyncGeneratorPrototypeThrow` arm in `compile_standard_builtin` allocates one
request record and selects exactly:

| builtin | typed request kind |
|---|---|
| `AsyncGeneratorPrototypeNext` | `Normal` |
| `AsyncGeneratorPrototypeReturn` | `Return` |
| `AsyncGeneratorPrototypeThrow` | `Throw` |

The typed kind is stored after receiver validation and Promise-capability
creation, but before the request is published through either queue head or a
previous tail's next pointer. Completion payload/tag, capability, Promise
payload/record and null next pointer must also be initialized before
publication. User-observable Promise or generator work must never see a
partially initialized request record.

### Reader one: completed queue draining

`emit_drain_async_generator_queue` loads each queue head's request kind exactly
once and routes it exhaustively:

- Normal publishes `{ value: undefined, done: true }` through
  `emit_complete_async_generator_step`;
- Throw rejects with the stored request payload and tag through the same
  complete-step helper;
- Return begins `emit_async_generator_await_return_reactions`. If reaction
  setup throws, the current emitter completion is normalized and a generic
  Throw step rejects the request. Otherwise draining stops until the AwaitReturn
  job settles the still-active request.

The active-request slot must be published before any route can settle or await
the request. Normal, Throw and failed Return remove that active queue head only
through complete-step. A pending Return keeps it live. The loaded-kind token is
reused for loop iterations at runtime, but its private Wasm local is released
only after the emitted loop and all comparisons/ABI copies are complete.

Break, Continue, Empty and unknown words trap in the strict decoder before
this routing. They may not reach the final Normal/Throw/Return decision tree.

### Reader two: continuation after a live yield

`emit_complete_async_generator_yield` first completes the current active
request with the yielded value and `done: false`. If another queue head exists,
it becomes active and its payload, tag and strictly validated kind are read
once. Routing is then:

- Return starts the yield-return Await path, sets the backend body status to
  Await and execution state to Executing, and does not resume the body;
- Normal stores the request payload/tag as the body resumption and selects the
  async-generator Normal resume kind; and
- Throw stores the same payload/tag but selects the Throw resume kind.

Only Normal and Throw set `resume_body_local`. Return must remain on the await
path. An invalid word may not share Normal's fallthrough. The loaded token is
released after the last Return/Throw comparison and before earlier temporary
locals are released.

## Ordering and lifetime obligations

The type migration must preserve these source-bounded relationships:

1. Receiver and async-generator brand validation precede request allocation.
2. The typed kind and every other request field are stored before queue
   publication.
3. Both readers load one stable kind snapshot from the selected request; no
   comparison reloads the heap field.
4. Queue draining publishes the request as active before settlement or AwaitReturn.
5. Complete-step remains the sole owner of removing a settled request's queue
   head and clearing its active-request slot on per-request settled paths.
   Drain's separate empty-queue cleanup may clear the active slot only after
   no queue head remains.
6. A live-yield Return is recognized before direct body resumption; Normal and
   Throw carry the exact stored payload/tag into the resume record.
7. The opaque token is never overwritten with a completion payload, resume
   kind, execution state, Promise state or arbitrary local while live.
8. The token is consumed only after every comparison and permitted ABI copy in
   its owner body.

The Rust enum selects emitted constants while compiling Wasm. The runtime
request kind remains a heap word. The loaded token is a compiler-side proof
about one emitted Wasm local, not a host-side AsyncGeneratorRequest and not a
cache across emitted calls.

## Durable structural witness

A structure test
`async_generator_request_completion_kind_structure.rs` (not in the tree) was
planned as a bounded source guard over `heap.rs`, `standard.rs` and `promise.rs`. It
should require:

- exactly the Normal, Return and Throw variants;
- one exhaustive projection through `CompletionKind::{Normal, Return, Throw}`
  and `CompletionKind::code`, with no numeric table, `repr`, enum-discriminant
  cast, catch-all or `Default`;
- `ALL` containing each valid request kind once and no Break, Continue or
  Empty member;
- a private raw offset whose only occurrences are declaration, layout, typed
  store and strict load in `heap.rs`;
- one strict heap load whose nested `If`/`Else` validation tests all three ABI
  words before the sole unknown-word `Instruction::Unreachable`;
- an opaque non-`Copy` loaded token with exactly the strict-loader mint, typed
  comparer, permitted ABI-copy helper and consuming release authority;
- the exact one-writer/two-reader product census;
- the exact next-to-Normal, return-to-Return and throw-to-Throw writer mapping;
- complete request initialization before both queue-publication paths;
- queue-drain Normal, Throw and Return routing, including active publication,
  failed-AwaitReturn rejection and stop-draining order;
- live-yield Return-before-resume routing and exact Normal/Throw resume-kind
  selection; and
- token release after the last comparison/copy in each reader.

The guard should extract only the enum/helpers and the three named owner
bodies. It must not snapshot complete heap, Promise or standard-builtin files.

## Mutation checks

The structural witness and Rust types must reject these mutations:

- adding a request kind without updating the ABI projection;
- mapping any request variant to the wrong `CompletionKind` ABI member;
- hand-writing `0`, `1` or `2`, using a discriminant cast, or adding a second
  projection;
- admitting Break, Continue, Empty or an unknown word in the decoder;
- removing the decoder trap so an invalid live-yield request falls through as
  Normal;
- storing a generic `CompletionKind`, resume kind, state word or bare integer
  at the private request offset;
- constructing the loaded token from an arbitrary `u32`, exposing its local,
  or making it `Copy`/`Clone`;
- swapping Return and Throw at the builtin writer while preserving global
  counts;
- swapping queue-drain route bodies or moving active-request publication after
  settlement;
- allowing Return to set `resume_body_local`, or swapping Normal and Throw
  resume kinds in the live-yield reader;
- copying a validated request word into generic completion transport anywhere
  except the two named queue-drain branches; and
- adding a new raw offset owner outside the typed heap boundary.

Mutation checks may be source-level and bounded. They are not a reason to add
a second runtime test suite.

## Focused CLI witnesses

After implementation and adversarial source review, run these five exact CLI
tests serially under the shared eight-core, 22 GB cap:

1. `resource_management::wasm_await_using_async_generator_lifecycle`;
2. `resource_management::wasm_using_async_generator_lifecycle`;
3. `language_errors::run_wasm_backend_validates_async_generator_yield_star_next_across_method_wrappers`;
4. `language_errors::run_wasm_backend_validates_async_generator_yield_star_return_across_method_wrappers`; and
5. `language_errors::run_wasm_backend_validates_async_generator_yield_star_throw_across_method_wrappers`.

Each exact filter must discover and pass `1/1`; the focused CLI acceptance is
therefore `5/5`. The two lifecycle fixtures cover queued requests, yield and
await suspension, normal/return/throw settlement, terminal draining and
re-entrant enqueueing. The three delegation fixtures exercise the distinct
next/return/throw request routes across wrapper and Promise reactions; the
return and throw controls additionally retain explicit request payloads.

## Exact pinned Test262 cohort

At Test262 pin `e9d582d6b8b13afc5ba9a676664741592b5c7f69`, the smallest direct
cohort is five physical files:

- `built-ins/AsyncGeneratorPrototype/next/request-queue-order-state-executing.js`;
- `built-ins/AsyncGeneratorPrototype/return/request-queue-order-state-executing.js`;
- `built-ins/AsyncGeneratorPrototype/throw/request-queue-order-state-executing.js`;
- `built-ins/AsyncGeneratorPrototype/return/return-suspendedYield.js`; and
- `built-ins/AsyncGeneratorPrototype/throw/throw-suspendedYield.js`.

All five declare `flags: [async]`, which selects asynchronous harness handling
but does not limit strictness. Each physical file expands to sloppy and strict
Script execution, for exactly ten executions. The first three enqueue each
closed request kind while the generator is executing and force the queue to
preserve that identity through the next live yield and subsequent settlement.
The last two retain direct suspended-yield Return and Throw behavior at the
sole writer boundary.

Each exact suite-relative path must run separately with the Wasm-AOT backend,
`--jobs 1`, `--threads 1` and the repository timeout. Each must discover and
pass `2/2`; aggregate acceptance is `10/10`, with every unsupported, crash,
bug, timeout and other non-success bucket at zero. These are post-implementation
acceptance counts and were verified against the implementation on 2026-08-24.

## Known unrelated red baseline

`async_generator::wasm_backend_resumes_async_generator_loops_for_zero_one_and_many_iterations`,
backed by `wasm_async_generator_resumable_loop.js`, is an existing `0/1`
baseline. Unchanged `HEAD` and the prior typed complete-step implementation
produce byte-identical failing output: yielded and terminal `done` values are
correct, while later classic-loop iterations and post-yield lexical state are
lost.

That failure is continuation/state-spill debt. It does not exercise an invalid
request completion word and is not a green gate for this lane. If an
implementation unexpectedly changes its output, compare against unchanged
`HEAD` before attributing the difference. An unchanged failure is
negative-scope evidence only and must never be counted as passing.

## Verification

Verification ran once and serially after the coordinated batch compile:

1. perform bounded source review of the enum, heap helpers, writer and two
   readers; run `git diff --check` and `cargo fmt --all -- --check`;
2. run one capped `cargo xc` workspace compile checkpoint;
3. run
   `cargo test -p scriptw-aot-wasm --test async_generator_request_completion_kind_structure -- --test-threads=1`
   and require `5/5`;
4. run the five exact CLI tests above one at a time with `--exact` and
   `--test-threads=1`, expecting aggregate `5/5`;
5. run the five exact Test262 paths above one at a time through
   `./target/debug/scriptw --jobs 1 test262 run <path> --suite-root test262/vendor/test262 --execution-backend wasm --timeout-ms 180000 --threads 1`,
   expecting aggregate `10/10` and zero non-success buckets; and
6. finish with `git diff --check` and the central batch's single broad
   checkpoint. Do not run a second broad suite solely for this representation
   migration.

Long-running commands should use `scripts/run-watched.sh` around the shared
CPU/memory wrapper. No two compilation, CLI or Test262 commands may overlap.
The structure target passes `5/5`, the five exact CLI tests pass `5/5`, and the
five pinned files pass `10/10` sloppy/strict Wasm-AOT executions with every
non-success bucket at zero. `cargo fmt --all -- --check`, `cargo xc` and the
shared `187/187` Wasm-safe plus `191/191` complete fake-suite gates are green.

## Explicit nonclaims

This lane changes no valid-program behavior and claims no new conformance pass.
It does not redesign the async-generator queue, Promise capability, completion
payload/tag representation, complete-step generic completion local,
`[[AsyncGeneratorState]]`, body status, AwaitReturn jobs, delegation,
continuation spilling or GC layout. Resume kind is closed by its own focused
contract.

It does not type every `CompletionKind`, make Break/Continue globally
unrepresentable, repair the known resumable-loop failure, refresh the complete
`built-ins/AsyncGeneratorPrototype` directory, update published README counts,
or complete T14 or T15. It makes the three request-record completion kinds
closed, strictly decoded and impossible to confuse with another heap-word
domain at their sole writer and two readers.
