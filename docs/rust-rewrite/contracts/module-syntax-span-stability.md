# Contract: span-stable module-syntax rewriting

Area: T12 source-text linking of module-only syntax

Status: normative for the source rewriter

This contract tightens and supersedes the 14-byte inline-spelling statement in
the binding-name-domain contract's earlier B1/V2 discussion. Its name-domain
separation remains authoritative; only this source-width premise changes.

## Problem

The one-artifact module linker concatenates source units and reparses the result
with the Script goal. Before that reparse it must erase static `import` and
`export` syntax and must turn an anonymous `export default` into a declaration
of the unit's `$d<tag>$` binding (see
[module binding names](module-binding-names.md)). Later rewrites and lowering
still rely on byte offsets captured from the original unit.

Two source shapes expose the same missing invariant:

- `export\n default 42` is valid Module text. `ExportDeclaration` has no
  `[no LineTerminator here]` restriction between `export` and `default`; the
  restriction in that production is only between `async` and `function`.
  Boa parses the pair with its ordinary line-terminator-skipping cursor. The
  old rewriter nevertheless rejected every anonymous default whose keyword
  trivia contained a line terminator, because its replacement was a raw
  `String` that would discard that terminator.
- Blanking a Unicode scalar with one ASCII space preserves character count but
  not byte count. A deleted module clause such as `export { x as "☿" }`
  could therefore shift every later byte offset even though the rewriter
  claimed length preservation.

These are linker defects, not parser or lowering defects. Named default
declarations already use the blanking path; the false rejection affects the
anonymous function, generator, async-function and class forms, plus default
assignment expressions.

## Invariant

Every edit emitted by `modules::source` has one of two closed kinds:

1. `Blank`: retain each ECMAScript LineTerminatorSequence and replace every
   other Unicode scalar with exactly `char::len_utf8()` ASCII spaces.
2. `Replace(SpanStableReplacement)`: the replacement can be constructed only
   against the source slice it erases. Its byte length equals that slice's byte
   length, and its ordered list of ECMAScript LineTerminatorSequences equals the
   slice's ordered list. In particular, CRLF is one sequence while a CR and LF
   separated by trivia are two.

The replacement constructor accepts generated declaration-head fragments, not
an already-built replacement string. It receives both the erased slice and the
untouched suffix, rejects a generated fragment that contains a line terminator,
and rejects a head that does not fit after reserving the erased slice's
line-terminator bytes. It then writes the declaration head and padding before
the saved sequences. If relocation would place a standalone CR immediately
before a formerly separate LF, one retained padding space remains between them;
otherwise two source lines would collapse into one CRLF sequence. This applies
both between two relocated sequences and across the edit boundary when the
last relocated sequence is CR and the untouched suffix begins LF. An internal
barrier is funded by the non-terminator trivia that originally separated the
pair; a boundary barrier is funded by the erased `default` bytes originally
between the CR and suffix LF. Consequently all sequences remain before the
initializer, and `=` prevents automatic semicolon insertion between the minted
binding and that initializer.

This gives the properties the later pipeline consumes:

- every later source token keeps its byte offset;
- every later source token keeps its line number;
- CRLF remains one CRLF sequence; separate CR/LF remain two sequences; and
  U+2028/U+2029 remain the same sequence in the same relative order;
- an inline `export default` keeps the existing generated spelling;
- a split anonymous default becomes valid Script text rather than an
  unsupported diagnostic.

The rewrite does **not** promise that a token after a split keyword pair keeps
its original source column. Moving erased trivia's terminators to the end of
the replacement span may change that column while preserving byte offset and
line number. Exact source-map column fidelity needs a richer mapping and is not
part of this seam.

## Anonymous-default byte budget

The narrowest split spelling is `export\ndefault`. Once the one-byte line
terminator is reserved, only the two keywords' 13 bytes are guaranteed for
generated code. The widest admitted declaration head is:

```text
"let " + "$d9999$" + "=" = 4 + 7 + 1 = 12 bytes
```

`var ` has the same width as `let `. The compile-time binding-name assertion
therefore targets the 13-byte non-line-terminator budget, not the 14-byte
inline spelling. The runtime constructor still checks the actual source slice,
because a span is parser/scanner data and cannot be proved by that const
assertion.

## Required regressions

- source rewriter: a max-unit-id anonymous default split by CRLF, U+2028 and
  U+2029, with non-ASCII block-comment trivia; assert identical byte length,
  identical ordered LineTerminatorSequences and a valid
  `let`/`var $d9999$ =` head;
- source rewriter: a standalone CR separated from a later standalone LF keeps
  two sequences after relocation rather than collapsing to CRLF;
- source rewriter: `export\rdefault\n42` at the max binding-name width keeps a
  barrier between the relocated CR and untouched suffix LF, with identical
  bytes, marker offset and ordered sequence list;
- source scanner: `//` ends at CR, LF, CRLF, U+2028 and U+2029 both in ordinary
  top-level scanning and in trivia between `export` and `default`;
- source scanner: non-ASCII ECMAScript whitespace is skipped at top level and
  terminates scanned module keywords and keyword lookahead, while U+0085 stays
  outside the ECMAScript whitespace set;
- source rewriter: non-ASCII static import/export clauses blank without moving
  a following marker's byte offset;
- source rewriter: the existing over-budget negative case remains an error;
- linker: a dependency exporting a split anonymous default is read by its
  default importer through the minted binding;
- engine: a Module entry with a split default assignment evaluates through the
  Wasm-AOT product path and returns the assigned value.

## Scope and nonclaims

This seam changes no parser, lowering IR, Wasm representation or runtime
wrapper. Attributed re-export retention is closed separately by the canonical
module-request identity seam. This seam does not close namespace exotic
internal methods, per-unit declaration collisions, the module-`var` global
leak, lazy dynamic-target evaluation, cyclic/deferred/async evaluation,
top-level await, or the full pinned `language/module-code` closure.

Cheap freeze gates are exact Rust formatting, `git diff --check`, module
boundaries and the task-plan validator; they pass on the current working tree.
The focused Unicode-whitespace scanner regression also passes. The remaining
module/linker gates and pinned Test262 module sweep remain centralized
verification obligations.
