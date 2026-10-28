# Spec-operation evidence

`scriptw-ir/src/operations.rs` owns the closed operation vocabulary and catalog.
An implemented catalog row must be backed by an actual expression operation,
backend operation or statement emission site. A tracked-gap row cannot carry an
implementation status. Adding an operation requires an explicit result,
operand, abrupt-completion and consumer decision.

The shared expression emitter exhaustively handles SpecOperationIr. Backend
rows join to the actual backend emitter; statement rows join through the single
EmissionSite declaration. Enum variants, ALL lists and names derive from the
same row list, so omitting a new variant cannot hide it from the census.

Iterator evidence states acquisition, step, value and close obligations where
the iterator is acquired. Each discharge is either emitted or carries an
explicit implementation/vacuous premise. Closed typed fields prevent swapping
obligations. A witness is evidence about an algorithm, never permission for the
backend to skip observable iterator operations or branch to a shortcut.

The catalog does not establish Test262 success. Real emitted Wasm, behavior
controls and source-bound conformance results remain separate evidence. See
[iterator protocol](iterator-protocol.md) and
[IteratorClose](iterator-close-obligation.md).
