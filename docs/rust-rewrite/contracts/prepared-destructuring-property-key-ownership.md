# Prepared destructuring property-key ownership

Current source status (2026-10-05): the T05 semantic Wasm-GC cutover is source-authored and unverified. Earlier source recipes, caller counts, and dated verification below describe their recorded checkpoints; they do not verify this cutover. ECMAScript requirements and retained fixtures remain acceptance criteria. Semantic values and Realm ownership now use the typed GC/compiler path described in [the GC value contract](../value-heap-gc.md). The retired representation assertions add no Test262 skips or new passing results.

Status: implemented for array and object destructuring assignment property targets.

## Boundary

`PreparedDestructuringPropertyKey` is the private closed carrier between
`prepare_destructuring_target` and `put_destructuring_target`. A static target
key owns its string and no temporary locals. A computed target key can be
constructed only after both its payload and tag locals have been reserved,
populated and checked for abrupt completion; that variant owns the raw key
shape and both locals together.

The write consumes the carrier. It installs a temporary key binding only for
the computed variant, exhaustively projects both variants to `PropertyKeyIr`,
then exhaustively releases both computed locals after PutValue. The previous
independent `Option<u32>` fields admitted a static key with locals, a computed
key without one local, or a key form that disagreed with its local pair.

The carrier derives no clone, copy, default, equality, ordering, hashing or
debug capability. It is not exported and has no fallback projection.

## Durable evidence

The retained array-destructuring abrupt-completion fixture exercises the same
prepared-property-target path and its static-key row, including target
preparation before the iterator protocol fails. Focused verification commands:

```sh
cargo test -p scriptw-cli --test cli array::run_wasm_backend_preserves_array_destructuring_iterator_abrupt_completions -- --exact --test-threads=1
cargo check -p scriptw-aot-wasm --lib
```

## Nonclaims

This is source-equivalent Rust ownership hardening. It changes no evaluation
order, temporary-local count, IR, emitted Wasm or JavaScript behavior. The
retained semantic fixture does not directly witness the computed target-key
row; that row remains structurally closed here, while broader destructuring
conformance remains part of T08.
