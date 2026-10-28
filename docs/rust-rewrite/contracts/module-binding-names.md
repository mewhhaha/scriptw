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

Host-normalized `ModuleKey` identity is distinct from source request spelling.
Unicode identifier spelling has one shared authority. Reexports preserve the
resolved target and live binding rather than taking an early value snapshot.

Source-only JavaScript targets can be loaded and parsed without opening their
dependencies. Unsupported static source bindings/forwarded source exports fail
linking with SyntaxError; dynamic import.source rejects before evaluation,
namespace creation or cell publication. A rejection marker is not a fabricated
module-source object or runtime storage cell. See
[source-phase rejection](source-text-module-source-phase-rejection.md) and
[module request identity](module-request-identity.md).
