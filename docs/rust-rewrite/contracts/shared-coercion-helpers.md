# Shared conversion bodies

`ToPrimitive` and `ToPropertyKey` use their registered typed Wasm helpers at
every public emitter boundary. `ToPrimitiveHint` selects the Default, Number or
String declaration exhaustively; `ToPropertyKey` uses its existing declaration.
Both boundaries carry the actual value, trusted caller Environment and complete
Completion, including the original thrown reference and target. They do not
replace a thrown value with a generated error or route it to a caller's handler
inside the shared body.

The original GetMethod and OrdinaryToPrimitive algorithm remains physically
once in `emit_tagged_to_primitive_locals_pending_inner`. Each hint-specific body
compiler supplies its fixed hint and typed parameters. The PropertyKey compiler
alone calls `emit_value_to_property_key_completion_inner`; its original
String/Symbol fast path, String-hinted ToPrimitive and primitive ToString order
remain intact. That inner body calls the shared ToPrimitive boundary rather
than emitting another copy. Neither helper compiler calls its own public
facade, so helper construction cannot recurse into itself.

The helper parameter owner installs the retained caller Environment before
emission. Existing conversion lookup, calls and native-error Realm selection
consume that same environment and active execution Realm. The helper does not
have or manufacture a callable FunctionContext. User hooks still execute
through their actual callable records and defining Realms. Caller-owned abrupt
routes remain outside the shared boundary.

The raw-key `DynamicPropertyRead` kernel converts once through
`emit_value_to_property_key_locals_or_branch`. That checked producer preserves
the complete thrown result at its caller-owned exit and roots the normal
String/Symbol result as `PropertyKeyLocals` before lookup overwrites the
completion. It never sends the completed key through `ToPropertyKey` again.
The raw numeric Array own-data probe and nullish-receiver check retain their
original ordering. Emission coverage requires exactly one conversion call in
this kernel; execution coverage checks key hooks, Symbol identity, nonindex
Numbers, getter receivers, inherited Proxies and foreign getter error Realms.
The removed second call already took the primitive fast path, so this removes
redundant helper/completion transport without changing observable hook counts.

ValueToString, ValueToNumber and ValueToNumeric already use their registered
helpers. Their existing inline flags start enabled and are disabled only while
compiling that helper's own original body; those bodies now call shared
ToPrimitive too. No conversion ABI, semantic value model or GC storage changes.

The scheduling artifact regression checks actual emitted helper bodies and
non-self direct-call consumers through the original nested Array/request
fixture. Its source-function and bootstrap size ceilings remain unchanged.
The existing coercion, getter, Realm and abrupt-identity execution cohorts
remain required semantic checks. This source batch has not yet been compiled
or executed; measured size acceptance belongs to the next combined checkpoint.
