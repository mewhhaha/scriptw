# Local arguments and one canonical source body

Eligible strict ordinary source functions with at most five simple formals use
one canonical body whose incoming arguments are rooted Wasm value triples.
Zero-to-two-formal bodies retain their original two-slot signature; three-to-five
formals use a separate five-slot signature. Short calls gain no padding or
signature growth. The existing complete arguments-observer proof admits the entry; captures, calls,
property operations, source depth and strict caller barriers retain their normal
semantics. Arguments observers, eval, defaults/rest, class protocols and
resumables retain the vector body ABI.

The original callable index remains its ordinary vector entry and source ID.
Its adapter validates vector/count agreement, pads omitted formals with Undefined,
and tail-enters the canonical body with the actual callable, receiver,
`new.target`, caller Realm and calling program. It allocates no argument objects,
vectors or activation records. Extra incoming values were already evaluated;
the absence proof permits the adapter to discard their transport.

The existing ExecutableCode owns separate nullable narrow and wide typed
local-arguments entries. Its source-bound plan publishes exactly one matching
entry; builtin and resumable code publishes neither. A call evaluates the actual
callable,
receiver and arguments once, retaining roots across later operand effects. For
zero to two nonspread actuals, a nonnull narrow entry accepts those values
directly; three to five actuals can enter a published wide entry. Missing wide
slots receive Undefined. A mismatched extent, spread, or more than five actuals
retains ordinary compiled dispatch and its exact original argument vector;
the vector adapter unpacks the canonical body's matching extent. Arguments
observers never publish either local entry. The fast path uses the actual
FunctionObject's context and defining Realm/program, including foreign programs
and replaced closures. Source depth, captures, exception completion and strict
reflection remain in the canonical body. A proper tail call retires the caller
at the existing boundary before its typed Wasm tail transfer.

Mutable captured declarations retain only an Open original-source candidate,
validated against their owning environment and cell slot. Held-Reference calls
carry these callee candidates separately from their unknown result facts.
Argument effects never strengthen that information. A known single candidate
can select a direct canonical Wasm call only after actual source-ID and defining
program guards; identity misses use the actual published typed entry.

This is one body with an adapter, not a second heap or execution backend. It
removes argument transport and dispatch work. A direct call to the body is not
by itself evidence that a native compiler inlined it.

Runtime layout ABI 53 adds the wide typed entry and its shared function type.
Older portable pairs and native images must be rebuilt together; they cannot
share this layout. The narrow parameter layout and its calling work stay intact.

Native argument-list slicing retains a fresh ValueArray whose entries refer to
the original immutable StoredValue snapshots. Its fill seed is the existing
shared Undefined snapshot, initialized by the runtime's immutable nonnull global
constant expression before any function runs. Filling and then replacing array
entries cannot mutate that snapshot or expose its identity. Function.call/bind
and Promise argument tails retain their original extent, order and tail-call
lifecycle; this removes only the otherwise unused seed allocation.
