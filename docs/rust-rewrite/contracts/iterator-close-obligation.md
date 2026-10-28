# IteratorClose ownership

The owner that acquires an iterator also records how it is closed. Array
patterns, argument/Array spread, synchronous and asynchronous loops, and
generator delegation retain their distinct completion and exhaustion rules;
replacing them with indexing or a canonical-name shortcut loses observable
iterator behavior.

IteratorClose obtains the live return method and calls it with the iterator as
receiver. Absence, noncallability, an abrupt method/call result and a nonobject
result retain their specified outcomes. An original throw takes precedence
over a close failure; other escaping completions propagate the required close
failure. The original completion and its value stay rooted throughout closing.

Normal exhaustion makes Done permanent and prevents a later close. An early
exit closes only an iterator whose obligation is still active. A failing next
or value operation must follow its own algorithm's rule, rather than sharing an
unconditional close-on-error branch. Async closing awaits its actual result.

Close errors use the invoking algorithm's selected Realm and the shared
Realm-aware error owner; they cannot be reconstructed from mutable public Error
globals. See [close-error Realm](iterator-close-error-realm.md),
[iterator protocol](iterator-protocol.md) and
[synchronous consumers](sync-iterator-consumer-capability.md).
