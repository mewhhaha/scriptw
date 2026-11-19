# Synchronous generator state word

Status: implemented, independently reviewed, and focused-verified for the
T04/T15 Wasm-AOT invariant lane on 2026-08-23. The owner census was inventoried
at source commit `3202d6933c8ba9f97b0f424dd83c422333e8d8ff` and is preserved by
the durable source guard.

## Specification boundary

The edition-pinned ECMA-262 table of
[`Properties of Generator Instances`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-properties-of-generator-instances)
defines exactly four values for `[[GeneratorState]]`: suspended-start,
suspended-yield, executing, and completed. The related abstract operations
give those values lifecycle meaning:

- [`CreateIteratorFromClosure`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-createiteratorfromclosure)
  initializes `[[GeneratorState]]` to suspended-start, and
  [`GeneratorStart`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-generatorstart)
  requires that state while installing the resumable generator body;
- [`GeneratorValidate`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-generatorvalidate)
  checks the generator slots and brand, reads the state once, throws a
  `TypeError` for executing, and otherwise returns the state;
- [`GeneratorResume`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-generatorresume)
  returns `{ value: undefined, done: true }` for completed and changes either
  suspended state to executing before resuming its execution context;
- [`GeneratorResumeAbrupt`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-generatorresumeabrupt)
  changes suspended-start directly to completed, handles completed without
  resuming the body, and changes suspended-yield to executing before injecting
  a Return or Throw completion; and
- [`GeneratorYield`](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html#sec-generatoryield)
  changes executing to suspended-yield before returning the iterator result to
  the caller. Generator body termination changes executing to completed before
  a final result or throw crosses the generator boundary.

The resulting state machine is closed:

| current state | accepted boundary | required result state and behavior |
|---|---|---|
| suspended-start | `next` | executing before the body runs |
| suspended-start | `return` or `throw` | completed without running the body |
| suspended-yield | `next`, `return`, or `throw` | executing before the body resumes |
| executing | re-entrant generator method | `TypeError`; the active body later yields or completes |
| executing | body yield | suspended-yield before publishing `done: false` |
| executing | body return or throw | completed before publishing or propagating the completion |
| completed | `next`, `return`, or `throw` | remains completed; the body is never resumed |

Completed is absorbing. In particular, an external catch that observes a
terminal generator throw must already observe that generator as completed, and
a caller receiving a yielded iterator result must already observe it as
suspended-yield.

## Inventoried representation hazard

Scriptw stores the synchronous state in the i64 word at
`HEAP_GENERATOR_STATE_OFFSET`. At the inventoried pre-migration baseline, four
crate-visible `u64` constants spelled its persisted encoding:

| specification state | current word |
|---|---:|
| suspended-start | 0 |
| executing | 1 |
| completed | 2 |
| suspended-yield | 3 |

The generic heap helpers accepted an arbitrary offset and arbitrary `u64`
value. A producer could therefore store a generator resume kind, an
async-generator state, a completion kind, or an unknown integer in this field
and still build. The pre-migration consumer also treated an unknown word as
suspended-start on the ordinary `next` fallthrough. That was an internal
record-integrity bug class: JavaScript cannot forge the word directly, but a
future emitter omission or wrong-domain transposition could silently select
executable behavior.

The word is distinct from `GENERATOR_RESUME_STATE_INITIALIZING`, the typed
`GeneratorResumeKind` injection direction, pending-completion records, and
every `AsyncGeneratorExecutionState` word. Equal numeric values do not make
those domains interchangeable.

## Closed Rust domain and sole projection

Replace the four raw state constants with one closed domain in `heap.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeneratorState {
    SuspendedStart,
    Executing,
    Completed,
    SuspendedYield,
}

impl GeneratorState {
    const ALL: [Self; 4] = [
        Self::SuspendedStart,
        Self::Executing,
        Self::Completed,
        Self::SuspendedYield,
    ];

    const fn word(self) -> u64 {
        match self {
            Self::SuspendedStart => 0,
            Self::Executing => 1,
            Self::Completed => 2,
            Self::SuspendedYield => 3,
        }
    }
}
```

The declaration order follows the stable persisted words; the exhaustive
match, not a Rust discriminant, defines the representation. `word` is the only
projection to an integer and remains private to `heap.rs`. The type has no
`repr(u64)`, `as u64` projection, `Default`, catch-all match, integer or Boolean
constructor, unchecked decoder, or second list of numeric constants. Adding a
variant must fail compilation until the projection is defined.

The existing `0, 1, 2, 3` words are preserved. This is an internal emitted-Wasm
representation migration, not permission to renumber live generator records.

## Typed heap ownership

`HEAP_GENERATOR_STATE_OFFSET` becomes private to `heap.rs`. Its only semantic
owners are three heap-boundary operations:

1. `emit_store_generator_state` accepts a `GeneratorState` and stores its
   `word()`;
2. `emit_load_generator_state_strict` loads the word once, compares it with
   every member of `GeneratorState::ALL`, and emits Wasm `unreachable` if none
   matches; and
3. `emit_generator_state_equals` compares a strictly loaded state with an
   expected `GeneratorState` by using the same sole `word()` projection.

The strict loader must return an opaque, private-field token such as
`LoadedGeneratorState`, not the underlying `u32` local index. Only the strict
loader may construct that token; the comparison helper borrows it, and a
release helper consumes it after the dispatcher has emitted all state tests.
The token has no `Copy`, `Clone`, raw-local accessor, arbitrary-local
constructor, or integer conversion. This makes passing the receiver-brand
local, resume-kind local, or any other i64 local to the state comparison a Rust
type error rather than a convention violation.

The loader validates one snapshot, matching `GeneratorValidate`; comparisons
must not reload the heap word independently. The layout descriptor may name
the private offset, but allocation and builtin emitters may not. After the
migration, the raw offset therefore appears exactly four times in the bounded
`heap.rs` implementation: declaration, layout metadata, typed store, and
strict load. A direct raw load, store, or comparison outside that boundary is
forbidden.

## Exact product-owner census

At the inventory baseline, the product path has three state owners, nine
dynamic raw-offset accesses, and four raw comparisons. Every one must cross
the typed boundary; no additional product owner is implied.

### Generator allocation

`FunctionBuilder::emit_function_handle_call_with_argv_inner` in
`crates/scriptw-aot-wasm/src/functions.rs` owns one store:

- `GeneratorState::SuspendedStart` after the generator object brand is stored
  and before the function, receiver, argument, resume, and pending-completion
  fields are published with the returned generator object.

The function's five source callers are
`emit_function_handle_call_with_throw_propagation`,
`emit_function_handle_call_with_argv`,
`emit_function_handle_call_with_argv_without_throw_propagation`,
`emit_method_call`, and `compile_function_call_helper`. They are ordinary call
entry points, not five independent state owners. The one
`can_call_generator` allocation branch owns this transition.

### Shared suspended-yield resume

`FunctionBuilder::emit_generator_resume_call` in
`crates/scriptw-aot-wasm/src/builtins/standard.rs` owns three stores:

- `GeneratorState::Executing` before the indirect generator-body call;
- `GeneratorState::SuspendedYield` after the body reports a live suspension
  and before its `done: false` result or delegated suspension escapes; and
- `GeneratorState::Completed` on the terminal path before a Throw completion
  propagates or a `done: true` result is created.

It has exactly one source caller: the suspended-yield branch shared by
`GeneratorPrototypeNext`, `GeneratorPrototypeReturn`, and
`GeneratorPrototypeThrow` inside `compile_standard_builtin`.

### Generator-prototype dispatcher

The `GeneratorPrototypeNext | GeneratorPrototypeReturn |
GeneratorPrototypeThrow` arm of
`FunctionBuilder::compile_standard_builtin` owns:

- the sole strict state load, after object/brand validation;
- four comparisons: executing once, suspended-yield twice, and completed once;
- `Executing`, `SuspendedYield`, and `Completed` stores around the inline
  suspended-start `next` body call; and
- one further `Completed` store for the non-resuming `return`/`throw` path.

The second suspended-yield comparison in the inline `next` section is
currently dominated by the earlier suspended-yield branch, which emits a
function exit through `emit_generator_resume_call`. It is not a fourth
lifecycle decision. This minimal state-word contract requires it to use the
typed comparison if retained, but neither requires its retention nor expands
the lane into consolidation of the two generator-body call emitters.

The complete migration census is therefore:

| operation | count | selected states |
|---|---:|---|
| strict state load | 1 | every valid state, unknown word traps |
| typed state store | 8 | suspended-start 1, executing 2, suspended-yield 2, completed 3 |
| typed state comparison | 4 currently | executing 1, suspended-yield 2, completed 1 |

Across stores and current comparisons, the source selects
`SuspendedStart` once, `Executing` three times, `SuspendedYield` four times,
and `Completed` four times. Counts alone are insufficient: swapping two
variants can preserve every count while inverting observable lifecycle
behavior, so the owner and order obligations below are normative.

## Ordering and lifetime obligations

The type migration must preserve these source-bounded relationships:

1. Receiver object and generator-brand validation precede the strict state
   load. Executing rejection precedes every resume payload write and body call.
2. A suspended-yield call stores the payload, tag, and typed resume kind before
   selecting `Executing`; `Executing` is visible before `CallIndirect` can run
   user code. This is what makes re-entrant `next`, `return`, or `throw` fail.
3. A first `next` from suspended-start also stores `Executing` before its
   indirect body call. The object must not remain suspended-start while user
   generator code is active.
4. A live suspension stores `SuspendedYield` before returning or propagating
   the yielded suspension. The state remains in the heap-owned generator
   record across arbitrary caller work and heap growth; it is not a temporary
   emitter-local lifetime.
5. A terminal body path stores `Completed` before either propagating its Throw
   completion or materializing the final `done: true` result.
6. `return` or `throw` from suspended-start stores `Completed` without calling
   the body. The same terminal path may idempotently store `Completed` for an
   already-completed receiver, but it may never select `Executing` again.
7. The strictly loaded state token is released only after every emitted state
   comparison. Its opaque local may not be overwritten with a brand, resume
   kind, completion word, or result value while it is live.

The Rust enum chooses constants while compiling Wasm; the authoritative state
at JavaScript runtime remains the generator object's heap word. The enum must
not be mistaken for a host-side generator instance or used to cache state
across emitted calls.

## Durable structural witness

A structure test `generator_state_word_structure.rs` (not in the tree) was to
be a source-bounded guard over `heap.rs`, `functions.rs`, and
`builtins/standard.rs`. It should require:

- exactly the four enum variants, `GeneratorState::ALL`, the stable
  `[0, 1, 2, 3]` projection, and an exhaustive `word()` match;
- no raw `GENERATOR_STATE_*` constants, `repr`, discriminant cast, `Default`,
  catch-all, unchecked integer decoder, or alternative word projection;
- a private raw offset with exactly the declaration, layout, typed-store, and
  strict-load occurrences, and no occurrence in either producer file;
- a strict load derived from `GeneratorState::ALL` whose nested `If`/`Else`
  arms route every valid word around the sole `Instruction::Unreachable`, with
  that trap emitted only after all four misses and before the matching `End`
  closures;
- an opaque non-`Copy` loaded-state token whose tuple-struct declaration and
  strict-loader mint are the only constructor-shaped occurrences, borrowed
  only by the typed comparer, and consumed by the release helper;
- exactly one source caller of `emit_generator_resume_call`, in the shared
  suspended-yield dispatcher branch;
- the exact one-load, eight-store, and current four-comparison census above,
  with every selection bound to its named owner body rather than checked only
  as a global total; and
- source-order checks for brand-before-load, executing-before-call,
  suspended-yield-before-yield-exit, and completed-before-terminal-exit in
  both body-call emitters, plus completed-before-branch-selection for the
  non-resuming suspended-start `return`/`throw` path.

The guard must recognize the dominated second suspended-yield comparison if it
remains so the migration cannot leave that one raw. If a dedicated cleanup
removes the dominated block, the guard should deliberately reduce the compare
count to three; it must not preserve dead code merely to retain a count.

Mutation checks must show that:

- adding a state without updating `word()` fails Rust exhaustiveness;
- swapping two stable words fails the exact projection assertion;
- passing a resume kind, async-generator state, bare integer, or raw local to
  a typed state operation fails to compile;
- bypassing the private offset or reintroducing a raw state constant fails the
  source-bound guard;
- adding an alternate projection, unchecked decoder, token constructor, or
  second resume-helper caller fails the closed-authority census;
- removing strict unknown-word trapping fails the decoder guard; and
- swapping `SuspendedYield` and `Completed` at two owners fails owner-specific
  assertions even though the global variant counts remain unchanged.

The witness should extract the enum, heap helpers, allocation branch,
`emit_generator_resume_call`, and only the generator-prototype builtin arm. It
must not snapshot entire heap, function-call, or standard-builtin modules.

## Focused verification

Under the shared eight-core, 22 GB cap, the durable structure suite passes
`4/4`. The four narrow CLI gates below each pass `1/1`, for `4/4`, and the six
exact Test262 leaves pass `12/12` Wasm-AOT variants with every failure and
non-success bucket at zero under `--jobs 1 --threads 1`:

- `crates/scriptw-cli/tests/cli/resource_management.rs::wasm_using_plain_generator_lifecycle`, backed by
  `wasm_using_plain_generator_lifecycle.js`, for suspended-start laziness,
  repeated yield/resume, normal completion, external `return` and `throw`, and
  the absorbing completed state;
- `crates/scriptw-cli/tests/cli/language_errors.rs::run_wasm_backend_preserves_property_reference_across_generator_suspension`,
  backed by `wasm_generator_suspended_property_reference.js`, for values and
  References retained across ordinary and delegated suspension;
- `crates/scriptw-cli/tests/cli/iterator.rs::run_wasm_backend_succeeds_for_iterator_to_array_exhausted_generator_fixture`,
  backed by `wasm_iterator_to_array_exhausted_generator.js`, for immediate
  completion and repeated consumption of an exhausted generator; and
- `crates/scriptw-cli/tests/cli/heap.rs::run_wasm_backend_succeeds_for_heap_rooted_generator_fixture`,
  backed by `wasm_heap_rooted_generator.js`, for a suspended generator record
  surviving intervening heap growth.

At the repository's declared Test262 revision
`e9d582d6b8b13afc5ba9a676664741592b5c7f69`, the smallest direct state cohort
is:

- `built-ins/GeneratorPrototype/next/from-state-executing.js`;
- `built-ins/GeneratorPrototype/next/consecutive-yields.js`;
- `built-ins/GeneratorPrototype/return/from-state-suspended-start.js`;
- `built-ins/GeneratorPrototype/return/from-state-completed.js`;
- `built-ins/GeneratorPrototype/throw/from-state-suspended-start.js`; and
- `built-ins/GeneratorPrototype/throw/from-state-completed.js`.

Each exact suite-relative path was run separately through Wasm-AOT with
`--jobs 1`, `--threads 1`, and the repository timeout; the completed run's
discovery totals and every failure bucket were inspected. The README reports
the complete synchronous
`%GeneratorPrototype%.return` and `.throw` leaves at `23/23` and `22/22` as of
2026-07-19, and separately reports the plain-generator `using` lifecycle
fixture and its exact Test262 leaf green. Those are existing historical
witnesses, separate from the focused current-working-tree evidence above.

## Known unrelated red baseline

`async_generator::wasm_backend_resumes_async_generator_loops_for_zero_one_and_many_iterations`,
backed by `wasm_async_generator_resumable_loop.js`, is a known red. The
[`async-generator-complete-step-kind.md`](async-generator-complete-step-kind.md)
baseline records byte-identical failure before and after its typed
complete-step migration: yielded and terminal `done` values are correct, while
classic-loop continuation and post-yield lexical state are lost.

That fixture exercises async-generator activation and resumption, not the
synchronous `HEAP_GENERATOR_STATE_OFFSET` owned here. It is not a green gate
for this lane, must not be hidden or counted as a pass, and a repeated identical
failure is negative-scope evidence rather than a regression caused by this
contract.

## Explicit nonclaims

This lane does not change valid-program behavior or claim a new conformance
pass. It does not redesign generator continuations, activation spills,
`yield*`, pending completions, completion words, exception transport, resource
disposal, or the generic iterator-result `done: bool` materializer. The
distinct generator resume kind is closed by its own focused contract. This
lane does not type `[[AsyncGeneratorState]]`, repair the known async-loop red,
consolidate the duplicated generator-body call emitters, or establish GC
reachability beyond the existing heap record.

It does not claim the complete generator, iterator, Promise, or Test262 trees
are green, change a published status count, or complete T04 or T15.
