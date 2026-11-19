# Synchronous generator resume-kind word

Status: focused-verified for the T15 Wasm-AOT invariant lane on 2026-08-24.

## Boundary

A synchronous generator activation persists the completion injected when its
compiled body resumes. The three-value synchronous generator resume-kind
domain is:

| kind | word | producer |
|---|---:|---|
| Normal | 0 | generator initialization and `.next(value)` |
| Return | 1 | `.return(value)` |
| Throw | 2 | `.throw(exception)` |

Resume kind is distinct from the resume-state label that selects a compiled
suspension point, from `GeneratorState`, and from the general completion ABI.
The general ABI assigns different meanings to words 1 and 2, so this field
must not be projected through `CompletionKind` merely because both domains use
the names Return and Throw.

## Closed Rust domain

`heap.rs` owns the sole stable projection:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeneratorResumeKind {
    Normal,
    Return,
    Throw,
}
```

Its private `ALL` list contains exactly those three values. The private,
exhaustive `word` match preserves words 0 through 2 without using a Rust
discriminant. There is no `repr`, catch-all arm, default, integer or Boolean
constructor, unchecked decoder, public word accessor, or second numeric table.
The retired `GENERATOR_RESUME_KIND_NORMAL`,
`GENERATOR_RESUME_KIND_RETURN`, and `GENERATOR_RESUME_KIND_THROW` integer
tokens are absent from product source.

## Typed heap ownership

`HEAP_GENERATOR_RESUME_KIND_OFFSET` is private to `heap.rs` and appears there
only in its declaration, layout metadata, typed store, and strict load.

- `emit_store_generator_resume_kind` accepts only `GeneratorResumeKind`.
- `emit_load_generator_resume_kind_strict` performs one heap load, compares
  the snapshot with every member of `ALL`, and emits `unreachable` after every
  miss.
- `emit_generator_resume_kind_equals` borrows the opaque, non-`Copy`
  `LoadedGeneratorResumeKind` token and accepts only an enum member.
- `release_loaded_generator_resume_kind` consumes the token after its owner's
  comparisons or validated transport copy.

An unknown or wrong-domain activation word therefore traps before it can fall
through as Normal.

## Fresh and resumed delegation

Delegation joins two runtime paths. The fresh delegation path has no activation
resume to read and must begin with Normal. The resumed path must read the
persisted activation word. `GeneratorResumeKindTransport` expresses that exact
branch-joined domain without exposing its Wasm local:

- `emit_initialize_generator_resume_kind_transport` reserves the private local
  and initializes the fresh delegation path from typed Normal;
- the resumed path strictly loads one `LoadedGeneratorResumeKind`, copies it
  through `emit_copy_generator_resume_kind_to_transport`, and immediately
  releases the loaded token;
- Throw and both Return decisions after the join compare only the opaque
  transport through `emit_generator_resume_kind_transport_equals`; and
- `release_generator_resume_kind_transport` consumes the transport after its
  final comparison.

The source-order guard requires the typed Normal initialization before the
fresh/resumed `Else`, the strict load/copy/release sequence inside the resumed
branch, the branch `End` before the first transport comparison, and the
transport release after all three comparisons. This prevents a later edit from
reading a stale activation word on the fresh delegation path.

## Exact owner census

There are two semantic writers:

| file | owner | typed store selections |
|---|---|---:|
| `functions.rs` | generator allocation | 1: Normal |
| `builtins/standard.rs` | suspended-yield prototype dispatch | 3 branches at one typed store: Normal, Return, Throw |

There are two strict readers:

| file | owner | strict loads | loaded-token comparisons | loaded-token releases |
|---|---|---:|---:|---:|
| `control_flow.rs` | plain generator yield | 1 | 2 | 1 |
| `generator_delegation.rs` | resumed delegation branch | 1 | 0 | 1 |

The delegation owner additionally has one typed fresh-path transport
initialization, one validated snapshot-to-transport copy, three transport
comparisons, and one consuming transport release.

The durable whole-source census counts helper definitions as well as product
calls. Columns are raw offset, typed store, strict load, loaded-token compare,
loaded-token release, transport initialization, validated copy, transport
compare, and transport release:

| source | census |
|---|---|
| `heap.rs` | `4, 1, 1, 1, 1, 1, 1, 1, 1` |
| `functions.rs` | `0, 1, 0, 0, 0, 0, 0, 0, 0` |
| `builtins/standard.rs` | `0, 1, 0, 0, 0, 0, 0, 0, 0` |
| `control_flow.rs` | `0, 0, 1, 2, 1, 0, 0, 0, 0` |
| `generator_delegation.rs` | `0, 0, 1, 0, 1, 1, 1, 3, 1` |
| total | `4, 3, 3, 3, 3, 2, 2, 4, 2` |

Every other Rust product source must remain zero for all nine operations.

## Durable witness

The structure test `generator_resume_kind_structure.rs`, no longer in the tree, pinned:

- the exact three variants, ordered `ALL` list and exhaustive stable words;
- absence of the retired integer tokens and integer conversions;
- private four-occurrence heap offset ownership and one-load unknown-word
  trapping;
- opaque loaded-token and delegation-transport construction and release;
- the complete whole-source census;
- typed allocation and prototype-dispatch writer selection and ordering;
- plain-yield Return/Throw routing from one validated snapshot; and
- fresh Normal versus resumed strict-load delegation ordering.

The neighboring generator-state structure witness names the typed resume-kind
store, so it no longer depends on the private raw offset.

## Verification

The coordinated verifier ran:

```sh
cargo fmt --all -- --check
cargo xc
cargo test -p scriptw-aot-wasm --test generator_resume_kind_structure -- --test-threads=1
cargo test -p scriptw-aot-wasm --test generator_state_word_structure -- --test-threads=1
```

Formatting, workspace compilation and diff hygiene are green. The new
structure guard passes `7/7` and the neighboring generator-state guard passes
`4/4`. Four exact generator lifecycle, suspension, exhausted-iterator and
heap-rooting CLI fixtures pass `4/4`. The six exact generator-state Test262
leaves listed in `generator-state-word.md` pass all `12/12` sloppy/strict
Wasm-AOT variants with every failure and non-success bucket at zero under
`--jobs 1 --threads 1`.

## Explicit nonclaims

This invariant does not type resume-state labels, pending-completion records,
delegated-result auxiliary flags, payloads or tags. It does not redesign
generator continuations or delegation, repair general suspension debt, change
iterator closing, or claim broader Test262 progress. Synchronous generator
state and every async-generator lifecycle/completion domain remain distinct.
