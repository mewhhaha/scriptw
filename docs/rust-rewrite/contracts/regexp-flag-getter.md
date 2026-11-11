# RegExp flag getter

Current source status (2026-10-05): the T05 semantic Wasm-GC cutover is source-authored and unverified. Earlier source recipes, caller counts, and dated verification below describe their recorded checkpoints; they do not verify this cutover. ECMAScript requirements and retained fixtures remain acceptance criteria. Semantic values and Realm ownership now use the typed GC/compiler path described in [the GC value contract](../value-heap-gc.md). The retired representation assertions add no Test262 skips or new passing results.

Status: implemented for the eight intrinsic `RegExp.prototype` Boolean flag
getters.

## Boundary

The shared flag-getter emitter accepts only the sibling-visible, non-copyable
`RegExpFlagGetter::{HasIndices, Global, IgnoreCase, Multiline, DotAll, Unicode,
UnicodeSets, Sticky}` domain. Standard builtin dispatch maps its eight exact
IDs to those named rows before entering the helper.

One borrowed exhaustive match projects the rows to the original-flags bytes
`d`, `g`, `i`, `m`, `s`, `u`, `v` and `y`. No broader builtin ID, Boolean,
equality, wildcard, default or invalid-getter fallback remains. Receiver and
Realm validation, `%RegExp.prototype%` handling, internal-slot validation and
the original-flags lookup remain shared and unchanged.

## Verification

The bounded structure target passes all `3/3` tests, and the existing accessor
CLI fixture passes `1/1`. Eight exact Test262 leaves, one for each getter, pass
both variants (`16/16`) with every failure bucket at zero. Workspace formatting
and the diff check are green. No semantic golden or broad RegExp run belongs to
this source-equivalent batch.

## Deferrals

This contract changes no flag parsing, `flags` getter ordering, RegExp
construction, matching semantics, dynamic pattern compilation or broader T19
conformance.

## Batch AZ dispatcher boundary

The getter domain and raw emitter are now private to `string.rs`. Standard
dispatch reaches them only through eight fixed RegExp flag-getter entries. The
frozen 93-line domain/emitter selection has SHA-256
`0bd635a1625364b6db7514af3ce13b96166d14614f9ec5ee5c6f7b25fbd76829`;
restoring only the former enum and emitter visibility reproduces that source
exactly. `cargo xc` passes. The strengthened flag-getter and neighboring
symbol-hook structure targets pass `4/4` and `5/5`; the complete RegExp
prototype-accessor Wasm-AOT CLI fixture passes `1/1`. No Test262 leaf or Wasm
golden was required for this source-equivalent boundary, which claims no new RegExp behavior,
conformance result or published-count change.
