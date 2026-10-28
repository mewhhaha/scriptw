# ECMAScript trim mode

## Current GC source — 2026-10-22

`EcmaTrimMode::{Start, End, Both}` is the single crate-internal trim policy.
The private `operations/string_trim.rs` module reexports that type through
`operations`; String builtins import the same type as `StringTrimOperation`.
This lets character scans and full-span String reuse share one policy without
introducing a public Rust API or independent Boolean mode flags.

The private materializing core retains three named wrappers. The crate-internal
`emit_gc_string_trim_range_mode` selects the necessary forward/backward scans
and returns UTF-16 boundaries for the String builtins. A full-span result can
reuse the immutable input String; a partial result copies its exact span.
`Start` omits the backward scan, `End` omits the forward scan, and `Both` runs
them in that order. The policy is chosen while emitting code; there is no Wasm
mode word or per-character runtime mode dispatch.

The scans consume `emit_ecmascript_whitespace_i32` in `operations.rs` and its
complete 25-code-unit ECMAScript whitespace domain. NativeHost numeric parsing
and String-to-BigInt conversion retain that shared whitespace authority.
The structural guard checks the single internal type/reexport, builtin alias,
private materializing core, named wrappers and complete whitespace domain.
Compiled semantic and performance evidence belongs to its recorded source:
see the [bulk-string checkpoint](../v8-baseline.md#second-execution-batch--published-2026-10-22)
and [current verification inventory](../current-inventory.md).

The predecessor below describes the retired UTF-8 transport and narrower
file-private policy. It remains historical evidence; current verification uses
the [batch workflow](../../../CONTRIBUTING.md#verification), including the inherited finite
memory cap on managed cloud machines.

## Historical predecessor record

Status: normative for the Wasm-AOT ECMAScript string-trimming seam. The
implementation and bounded structural guard are independently reviewed and
focused-verified under the shared eight-core cap, 2026-08-23.

## Specification boundary

[ECMA-262 `TrimString`](https://tc39.es/ecma262/2026/multipage/text-processing.html#sec-trimstring)
accepts exactly three values for its `where` argument: `start`, `end`, or
`start+end`. `String.prototype.trim` selects `start+end`,
`String.prototype.trimStart` selects `start`, and
`String.prototype.trimEnd` selects `end`. The normative `trimLeft` and
`trimRight` aliases reach the same start-only and end-only builtin functions.

String-to-BigInt parsing also removes ECMAScript whitespace from both ends
before interpreting the remaining source. That consumer therefore shares the
`start+end` implementation but is not a String prototype method.

There is no fourth, neither-end operation at this seam. Representing the
domain as independent `trim_start` and `trim_end` Booleans admits
`(false, false)`, which would silently return the original string and has no
owner in the specification or current product callers.

## Closed Rust policy

The raw trimming core owns a private three-variant domain:

```rust
enum EcmaTrimMode {
    Start,
    End,
    Both,
}
```

The enum, owner-private `ECMASCRIPT_NON_ASCII_WHITESPACE_UTF8` table and raw
core remain private to `operations/string_trim.rs`. The table has exactly one
owner and its only two consumers are the forward and backward scans. Sibling
backend modules can reach the core only through three named wrappers that fix
Start, End, or Both. The core projects the mode through exhaustive Rust matches
with no `_` arm and no Boolean fallback. Adding a fourth mode must therefore
fail to compile until both boundary scans state what it means, while passing the
former Boolean pair must fail at the call boundary.

The mode derives no cloning, copying, formatting, equality or default
capability. The start scan borrows it, preserving the same owned decision for
the later end scan; that second exhaustive match consumes it. No cloned or
copied policy can diverge between the two ordered scans.

This is an emitter-time policy. It does not add a runtime Wasm mode word or a
runtime policy branch. The emitted start scan, end scan and final slice remain
the existing algorithms in their existing order. Both mode performs the start
scan first and the end scan second, so the end scan retains the start bound
produced by the first scan.

## Exact consumer ownership

There are exactly three consumer families:

1. `emit_string_to_bigint_locals` selects Both exactly once before unpacking
   and parsing the trimmed source.
2. The static String-method call fast path selects Both for `trim`, Start for
   `trimStart` and `trimLeft`, and End for `trimEnd` and `trimRight`.
3. The standard-builtin dispatcher selects Both for
   `StringPrototypeTrim`, Start for `StringPrototypeTrimStart`, and End for
   `StringPrototypeTrimEnd`. The intrinsic aliases already resolve to the
   corresponding Start or End builtin identity and do not form additional raw
   trim-core callers.

This inventory is exact. A new trimming consumer must choose one of the three
named operations and update this contract and its structural witness. No
consumer may call or alias the private raw core directly.

## Observable-order preservation

The migration changes only the Rust policy carrier and emitter selection. It
must preserve all existing observable and abrupt-completion order:

- String method receivers are checked for null or undefined before `ToString`;
- receiver `ToString` and its user hooks complete before trimming begins;
- the static fast path and standard builtin retain their existing completion
  routing around that coercion;
- String-to-BigInt trims once before unpacking, sign/radix handling and digit
  accumulation;
- Both mode scans the start before the end; and
- the same UTF-8 ECMAScript whitespace table, byte bounds and string-slice
  emitter remain authoritative.

No caller gains a new coercion, completion check, allocation, property access
or runtime branch.

## Durable mutation guard

A bounded source-structure regression must require:

- exactly the private `EcmaTrimMode::{Start, End, Both}` domain;
- a private raw core with one typed mode parameter and no `trim_start: bool` or
  `trim_end: bool` parameter;
- exactly three named wrappers, each forwarding one distinct variant to the
  raw core;
- exhaustive, catch-all-free Start/End/Both projection for both boundary
  scans, with the complete borrowed start body before the complete consuming
  end body, Start excluded only from the end scan and End excluded only from
  the start scan;
- an attribute-free, capability-free declaration with the exact recursive
  eleven-mention census and four appearances of each declared row;
- exactly the three consumer families and the mappings listed above;
- the Both wrapper before BigInt source unpacking and the trim wrappers after
  String receiver coercion in both String-method paths; and
- no additional direct call, method-item reference, visibility escape,
  Boolean-policy spelling or local reconstruction of the raw trim pair.

The whitespace table moved unchanged from the broad builtin namespace into its
sole private trim owner. Restoring its former `pub(crate)` declaration produces
the exact original 21-line source with SHA-256
`3b3f4cb67213c7881b83d193a979ff4ae654805c1e7c783c473d781eb5395bd8`.
This source-equivalent ownership closure has no new String behavior and changes
no row, scan order, emitted instruction, Test262 materialization or published
count.

At the Batch BR checkpoint, `cargo xc` is green, the strengthened structure
target passes `3/3`, and the exact all-whitespace `trimStart` and `trimEnd`
leaves pass all four Wasm-AOT executions with every failure bucket at zero.

The guard must fail if Start and End are inverted at any inventoried caller,
if either alias moves to Both, if String-to-BigInt stops trimming both ends, if
the raw core becomes visible, if a new bypass appears, or if either exhaustive
match is weakened.

## Focused verification

Independent review accepted the implementation and tightened the mutation
guard around scan roles, BigInt source ownership, receiver/coercion branches,
standard-dispatch membership and created-Realm alias publication. The
centralized capped lane then completed these checks:

- `cargo xc` passed for the workspace;
- `ecmascript_trim_mode_structure` passed `2/2`;
- the exact `wasm_string_annexb_substr_trim_core.js` CLI witness passed `1/1`;
  and
- the exact arbitrary-precision BigInt string CLI witness passed `1/1`,
  including ECMAScript whitespace around the parsed source.

These are focused policy, String and BigInt witnesses. No Test262 cohort or
aggregate status was refreshed, and no conformance movement is inferred from
this policy-only migration.

The 2026-08-27 capability/lifecycle checkpoint retained the structure target
green at `2/2`; the exact trim and arbitrary-precision BigInt CLI witnesses
both passed `1/1`. Scoped formatter, diff and task-plan checks also pass for
this source-equivalent hardening.

## Nonclaims

This seam does not change the ECMAScript whitespace set, UTF-8 decoding,
String allocation or slicing, receiver coercion, alias installation, BigInt
grammar, radix/sign handling, error-Realm selection or completion ABI. It does
not migrate other String algorithms, remove a Test262 rewrite, prove the full
String or BigInt trees, change published counts, or complete T04.
Independent dry review is clean. The following shared workspace checkpoint
passes `cargo fmt --all -- --check`, `cargo xc`, the recursive module-boundary
check, the task-plan check and `git diff --check`.
