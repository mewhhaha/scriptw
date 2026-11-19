# Async-generator resume-kind word

Status: focused-verified for the T14/T15 Wasm-AOT invariant lane on
2026-08-24.

## Backend boundary

An async-generator activation persists the completion with which its compiled
body must resume. The five-value async-generator resume-kind domain is:

| kind | word | producer |
|---|---:|---|
| Normal | 0 | `.next()` or a yielded Normal request |
| Return | 1 | `.return()` or a fulfilled yield-return Await |
| Throw | 2 | `.throw()`, a rejected yield-return Await or a queued Throw request |
| Fulfill | 3 | a fulfilled body Await reaction |
| Reject | 4 | a rejected body Await reaction |

Resume kind is not a resume-state label. Labels select the compiled suspension
point, while this word selects the completion delivered at that point. It is
also distinct from request completion kind, execution state and body status,
even where two domains happen to share the words Normal, Return or Throw.

## Representation defect closed

Before this migration, five public integer constants and a public heap offset
allowed generic heap stores and loads throughout the compiler. Six writer paths
could publish any integer. Five readers loaded the word into raw Wasm locals;
ordinary yield, Await, async disposal, `for-await-of` and delegation compared
only the cases they happened to route. Most readers had an implicit unknown
fallthrough, so a corrupted word could behave like Normal instead of trapping.

The same raw local in async delegation was also widened after the activation
read: words 0 through 4 were copied into the delegation pending-kind record,
then a private close-throw sentinel replaced the local with word 5. Subsequent
comparisons could not express whether they were routing the closed activation
domain or the wider pending transport. Treating that pending word as another
resume-kind member would incorrectly widen the activation field.

## Closed Rust domain

`heap.rs` owns the sole stable projection:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncGeneratorResumeKind {
    Normal,
    Return,
    Throw,
    Fulfill,
    Reject,
}
```

The private `ALL` list contains exactly those five members. The private `word`
function exhaustively projects them to words 0 through 4. There is no `repr`,
discriminant cast, catch-all arm, default, integer constructor, unchecked
decoder, public word constant or close-throw variant. Adding a kind must fail
exhaustiveness until its stable representation and every product route are
chosen.

## Typed activation boundary

The raw activation offset is private to `heap.rs`. Four operations own it:

1. `emit_store_async_generator_resume_kind` accepts only
   `AsyncGeneratorResumeKind`;
2. `emit_load_async_generator_resume_kind_strict` performs one heap load,
   compares the snapshot with every member of `ALL`, traps after every miss and
   returns an opaque token;
3. `emit_async_generator_resume_kind_equals` borrows the token and accepts only
   one enum member; and
4. `release_loaded_async_generator_resume_kind` consumes the token after its
   owner's comparisons and any validated transport copy have been emitted.

The token is non-`Copy` and exposes no raw local:

```rust
#[must_use = "a loaded async-generator resume kind must be routed and released"]
pub(crate) struct LoadedAsyncGeneratorResumeKind(u32);
```

Unknown activation words therefore trap before Normal-like fallthrough. A
writer cannot substitute a resume-state label, completion kind, execution
state, body status or arbitrary integer at the private offset.

## Exact owner census

Runtime branches require nine typed store selections across six semantic
writer paths:

| file | store selections | writer paths |
|---|---:|---:|
| `functions.rs` | 1 | activation initialization |
| `builtins/promise.rs` | 7 | body Await, yield-return Await, rejected yield and live-yield resumption |
| `builtins/standard.rs` | 1 | `.next()` / `.return()` / `.throw()` request dispatch |

The Promise call sites select Fulfill/Reject, Return/Throw, Reject, and
Normal/Throw inside explicit Wasm branches. The standard-builtin call site
selects Normal, Return or Throw through a Rust match. Every selection crosses
the same typed store.

There are five strict readers and nine comparisons of validated activation
snapshots:

| file | strict loads | token comparisons | releases |
|---|---:|---:|---:|
| `control_flow.rs` | 4 | 9 | 4 |
| `generator_delegation.rs` | 1 | 0 | 1 |

Ordinary yield and Await route Return, Throw and Reject. Async disposal accepts
only Fulfill or Reject and explicitly traps the other three valid activation
kinds because those transitions cannot replace its active Await.
`for-await-of` normalizes Reject to its throw flag after strict validation.
The resumed delegation branch strictly loads one activation snapshot, copies
it into the widened pending local and releases it before the branch joins. The
fresh delegation branch instead initializes that pending local from typed
Normal. Fulfill, Reject and the pre-close Throw decision are therefore all
routed from the widened local after the join, rather than reusing the
activation token.

## Widened delegation pending-kind transport

The delegation pending-kind record remains a separate wider transport. It can
carry a copied activation word and the backend-only close-throw sentinel word
5. That sentinel is not an `AsyncGeneratorResumeKind` variant.

`emit_copy_async_generator_resume_kind_to_delegate_pending_kind` is the sole
validated bridge from the opaque activation snapshot into the outgoing pending
local. `emit_initialize_async_generator_delegate_pending_kind_from_resume_kind`
sets the fresh delegation path to typed Normal without inventing a heap
snapshot. `emit_async_generator_delegate_pending_kind_equals_resume_kind`
compares an incoming or already-widened pending local with a named resume-kind
encoding without constructing an activation token or touching the private
activation offset. Every route after the two paths join, including routes after
the close-throw sentinel can replace the copied word, uses this pending-kind
comparison rather than the activation token.

This lane deliberately does not claim that the widened pending field is itself
a closed Rust domain. It makes the domain crossing explicit and prevents word 5
from leaking back into the five-value activation field.

## Durable source witness

The structure test `async_generator_resume_kind_structure.rs`, no longer in the tree, pinned:

- exactly five variants, one complete `ALL` list and an exhaustive stable word
  projection;
- absence of the retired constants, integer conversions and a close-throw
  activation variant;
- the private four-occurrence activation offset;
- one-load strict validation, unknown-word trapping and opaque token lifetime;
- nine typed store selections, five readers, nine token comparisons and five
  consuming releases across the complete source tree;
- allocation, standard-builtin and Promise selection ownership;
- the validated resume-to-pending bridge and typed fresh-path Normal
  initialization of the widened delegation pending-kind transport; and
- use of pending-kind routing after word 5 can replace the bridged activation
  word; and
- explicit projection of the `i64` Promise-reaction rejection word to an
  `i32` Wasm condition before async-from-sync rejection routing.

The neighboring execution-state, body-status, request-completion and
await-using structure witnesses now name typed resume-kind operations instead
of the private offset or retired integer constants.

## Verification

The coordinated batch ran:

```sh
cargo fmt --all -- --check
cargo xc
cargo test -p scriptw-aot-wasm --test async_generator_resume_kind_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test async_generator_execution_state_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test async_generator_body_status_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test async_generator_request_completion_kind_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test structure_async -- async_generator_await_using_structure:: --test-threads=1
```

Formatting, `cargo xc` and diff hygiene are green. The resume-kind structure
target passes `7/7`; the four neighboring targets pass `20/20`, for `27/27`
related structural tests. The exact lifecycle/delegation CLI cohort passes
`5/5`, with two additional acquisition/lexical delegation controls also green.
The five pinned Test262 files pass `10/10` sloppy/strict Wasm-AOT executions
with every non-success bucket at zero.

## Explicit nonclaims

This invariant does not type resume-state labels, the widened delegation
pending-kind field, pending completion records, body-result payloads and tags,
or Promise reaction kinds. It does not repair general continuation spilling,
the known resumable-loop failure, cross-realm behavior, queue ownership, GC
layout or broader async-generator conformance. It changes no published README
count and does not complete T14 or T15.
