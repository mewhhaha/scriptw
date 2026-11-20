# Module binding names

`LocalName` is a name in one module's environment; `ExportName` is a name in
its export table. An import's requested name belongs to the exporting module's
ExportName domain. Local aliases, anonymous default spelling and compiler-minted
names must cross these boundaries explicitly rather than through an implicit
String conversion.

The current product uses canonical per-module activations, environments and live
binding cells, including cycles, top-level await and deferred requests. It does
not use the historical one-environment merged Script driver. Script entries
retain their own globals, strictness and completion and start module targets
through import jobs. Surviving `MergedName` helpers do not change that ownership.

## Linker cells inside a module activation

The linker declares two cells inside each unit's activation: the anonymous
`export default` binding (8.2.2 `*default*`) and the `import.meta` object.
`UnitLinkerSpelling::choose` spells them `$d<tag>$` and `$m<tag>$meta`, where
the tag is 1–4 characters of `[0-9A-Za-z_]` and is chosen per unit so that
neither name occurs among the identifiers of any unit of the graph (references
and bindings at any depth, top-level bound names, import locals). The unit's
decimal id is the first candidate, then every other tag in shortlex order;
exhausting them is an `unsupported` diagnostic. Both names are removed from the
eval-visible name table of that unit's activation environment only, so a direct
eval cannot name them; because no source identifier spells any unit's cell,
lowering resolves every reference to one (all linker-generated) to the cell
itself instead of through that runtime name table.

Consequently user code can neither read, assign, `typeof` nor shadow these
cells: a free reference spelled like one resolves through the global object, as
it must for the unspellable `*default*`. A module may declare a top-level
binding with such a spelling (`let $d1$ = 5; export default 41;`); it is an
ordinary binding, not a collision. Cells are addressed by `(unit, merged
name)`, so two units may share a spelling. The four-byte tag bound keeps both
in-place rewrites (`export default` → `let <name> =`, `import.meta` →
`<name>`) byte-length stable.

`import()` dispatcher names under `$scriptw$module$` are a separate family:
source that spells that prefix as an identifier is still rejected at record
admission.

Host-normalized `ModuleKey` identity is distinct from source request spelling.
Unicode identifier spelling has one shared authority. Reexports preserve the
resolved target and live binding rather than taking an early value snapshot.

Strict identifier calls with an authenticated cell location read that live cell
before evaluating arguments, following import targets and checking TDZ. The
held callee uses the existing dispatch with an undefined declarative receiver;
target hints never replace its value. Numeric call rows repeat the read on each
iteration. Unproven references, with/global selection and prepared direct eval
retain their existing resolution. Location evidence proves no immutable value.

Source-only JavaScript targets can be loaded and parsed without opening their
dependencies. Unsupported static source bindings/forwarded source exports fail
linking with SyntaxError; dynamic import.source rejects before evaluation,
namespace creation or cell publication. A rejection marker is not a fabricated
module-source object or runtime storage cell. See
[source-phase rejection](source-text-module-source-phase-rejection.md) and
[module request identity](module-request-identity.md).
