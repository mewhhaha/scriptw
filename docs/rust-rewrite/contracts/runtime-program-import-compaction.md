# Runtime imports in final program modules

The compiler emits P against R’s canonical function/global layout, then compacts
only unused imports from the reserved runtime namespace. All host and foreign
imports remain in their original relative order. All defined functions and
globals, GC recursive types, type indices, tables, memories, tags and segment
indices remain. R’s exports, implementation, layout and native-image contract
are unchanged.

The final producer pass collects semantic function/global references through
wasm-encoder’s exhaustive structural and operator adapters. This includes
exports, start, calls and ref.func, global/table initializers, active and passive
elements and data offsets. Names and plain declarative function lists do not
keep otherwise unused imports alive. Declarative segments remain at their
original element index, with removed imports filtered from their lists.

Function and global maps have distinct Rust index domains. A known removed
import can disappear from diagnostic names or a declaration; an out-of-range
index or an actual reference to a removed import is an error. Function/global
names and the outer function indices of local/label names are remapped. Unknown
producer custom sections are rejected for explicit index-ownership review.

Native program contexts and literal operands contain code-body and instruction
ordinals, not physical function/global indices. In particular, context `ready`
is an instruction ordinal. Keeping bodies and operators preserves those
witnesses despite shorter LEB operands. Type/field and source-owner identity
tokens remain unchanged. `ExecutableCode.SOURCE_ID` is a stable identity token
with a separate callable function reference; it is not an invocation index.

The final module validates with the existing required Wasmtime feature set.
The same final bytes remeasure function-body sizes and remap attribution indices
before the size report and optional body budget run. Native compilation,
producer execution facts and cache identities consume that final artifact.
Native-template admission still checks its ordinal witnesses and introduces its
own synthetic data imports; no unused R helper is retained solely for a later
template rewrite.

The engine instantiates R and defines only exports named by the actual
deserialized P imports. Repeated names need one definition, while P instantiation
still checks every requested type. Missing exports, linker collisions and
Store/Engine ownership errors remain errors. R remains the diagnostic and root
owner, including exports P did not request. No caller manifest replaces actual
Module imports or Wasmtime’s type checks.

Native source compilation retains that same R instance as its export owner.
A later Function or eval template resolves its actual runtime imports directly
from that owner; specialized native factories use the same on-demand lookup.
Host and foreign imports retain their linker boundary. Later compiled sources
can use runtime exports absent from the entry P without publishing every export
in advance or instantiating another R.

Focused controls cover structural and name remapping, declaration/segment
identity, wide-index LEB/body-size changes, opaque ordinal preservation, template
admission and actual native linking. Whole-batch verification and matched memory
measurements remain pending; no RSS reduction is claimed by this contract.
