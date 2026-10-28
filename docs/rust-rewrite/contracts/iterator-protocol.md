# Iterator protocol

Acquisition performs the actual iterator-method Get/Call and object check, then
captures next exactly once. Each step calls that next method with the iterator
receiver, validates the result object, reads done before value, and stops without
reading value when done is true. Holes, getters, Proxies and custom iterators
remain observable; a static Array/String shape cannot replace these operations.

Iterator records and completion values stay rooted across callbacks and
suspension. Done is monotone. Continue to the same loop retains its active
record; escaping completion follows the acquiring algorithm's close rule.
Patterns, spread, loops and generator delegation keep their different normal,
early-exit and abrupt rules. Async consumers await the actual protocol result
rather than indexing an assumed backing representation.

`iterator_obligations.rs` records acquisition, step, value and close discharges
as distinct typed fields. Evidence readers remain private to IR; the backend
cannot use a witness as an execution shortcut. Statement emission sites and
operation rows must resolve to real consuming implementations.
See [operation evidence](spec-operations.md),
[IteratorClose](iterator-close-obligation.md) and
[consumer ownership](sync-iterator-consumer-capability.md).
