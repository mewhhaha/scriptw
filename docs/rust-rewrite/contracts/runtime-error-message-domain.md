# Runtime error message admission

Status: dry source implementation; compilation and execution remain pending the
coherent batch checkpoint.

Compiler-authored native error messages enter Wasm emission through
`RuntimeErrorMessage`. Its representation is private to the string-pool message
module. The catalog macro emits both the admitted constants and the rows that
`StringPool::collect` interns before any function body is emitted. All native
error object constructors, throw wrappers, diagnostic publication, forwarding
helpers and static message projections consume that type. Adding a throw with
an arbitrary string fails type checking; naming a missing catalog constant
fails name resolution. A new catalog row reaches interning through the same
macro without a second pool-table edit.

The catalog includes only messages that actual compiler error emitters or
closed error projections consume. Collection constructor failures exhaustively
match their existing collection and failure enums. RegExp matcher status rows
retain their word and Realm route and select admitted catalog constants. Intl
option rows project both the property spelling and its admitted failure text;
DurationFormat and Intl.Locale project their existing closed unit/option enums.
The former partial literal table and duplicated pool-only message rows are
removed.

Source-owned messages keep their original text. A closed authority enum carries
the actual assertion/runtime-throw expression, static RegExp compilation,
prepared script/function outcome, identifier-write error or stubbed builtin
identifier. Pool collection and emission use that same projection. The pool's
source-message map admits a handle only when that owner's projected diagnostic
text was collected in this pool. This proves text membership, not Rust identity
of the IR allocation. An owner without a diagnostic or an uncollected diagnostic
returns an emission error.
There is no admission taking `String` or `&str`.

The source handle is private and contains the pooled string slot of its text. It is produced
immediately from the consuming builder's immutable pool at each current call
site. It does not encode a generative Rust identity for distinct pool instances;
passing a previously produced source handle to a different module is outside
that handle's contract. Static catalog constants can be shared between pools.

Native error Realm selection, completion routing, message-less errors and the
JavaScript Error constructor's user-supplied message path keep their existing
semantics. Text is never replaced by the error name or an invented default.
Static pool ordering changes when duplicate seed rows are removed, so emitted
artifacts require the normal batch compile and functional checkpoint.

The production empty-program pool regression checks each admitted static
message's slot and actual UTF-16 code units. Existing CLI/Engine error-message and Realm
fixtures remain the semantic observers. Existing structural regression sources
follow the typed message projections while retaining their ordering, Realm and
completion assertions. No compilation, execution or conformance count refresh
was performed for this source draft.
