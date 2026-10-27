# Shared Array.fromAsync fulfilled continuation

The fulfilled builtin shares its mapper/await path and its data-property commit
and next-source drive path inside the existing continuation function. Typed
lexical targets connect the existing closed stages; there is no additional
runtime stage, outcome flag, helper ABI or async execution model.

An InputValue fulfillment reaches mapper selection with its already-awaited
value. A MappedValue fulfillment reaches commit directly. AsyncIteratorResult
first validates the result object, reads and tests `done`, then reads `value`;
only after those operations succeed does it copy the value into the shared
rooted local and reach mapper selection. An async iterator's result value is
not awaited again. A completed iterator retains the existing finish path.

When a mapper exists, its original call, this argument, index argument, error
handling and single mapped-result await remain unchanged; the continuation
exits after scheduling that await. Without a mapper, control falls through to
the shared commit. Commit performs the same CreateDataPropertyOrThrow, advances
the index only after success, and drives exactly one next source operation.

All abrupt paths keep the original rejection and iterator-close owners.
AsyncCloseResult rejects with the saved error, and a close failure cannot
replace an already saved abrupt completion. Array-like sources retain their
non-iterator rejection behavior. Source-get, mapper and property-write ordering
is unchanged, including reentry through accessors or Proxy traps.

Captured ArrayFromAsyncState, defining Realm/program, promise capability,
iterator record, mapper, target and saved error retain their existing owners.
The shared outer value remains a GC root across calls and allocations. Dispatch
must not transfer helper-local roots into an outer path after clearing them;
iterator-result extraction copies the successful value before releasing its
pending completion. No state is reconstructed from public globals.

Controls cover all fulfilled-stage entries, mapper/no-mapper
paths, await-once behavior, source access and indexed-write ordering, close-error
precedence, GC/reentry and foreign-Realm use. The [source-bound checkpoint](../memory-regexp-backtrack-20261027.md)
records successful compilation, 4,889 native cases, 6,307 unique selected original
executions and the matched 17-workload cohort. This is not a full pinned-suite
refresh. Shared emitted code does
not establish resident-memory, peak-memory or timing savings.

The completed checkpoint covers the complete affected native inventories
(4,889 cases) and 6,307 unique original pinned Test262 executions:
2,365 earlier selected executions, 3,756 RegExp executions and 186 Array.fromAsync
executions. The narrower RegExp/Unicode selections overlap that union and are
not additional cases; overlap is not counted twice.
