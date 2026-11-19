# Stable implicit Arguments cell facts

An owner's unreassigned own `arguments` read lowers to `ExprIr::Arguments`;
reads that resolve elsewhere use their resolved binding (see
[source binding resolution](arguments-source-binding-resolution.md)).
The compiler can retain its initial Arguments kind only for a verified actual
implicit cell of an ordinary owner. A lexical arrow supplies the capture
analysis EnvironmentId; an ordinary function supplies its activation record.
Both must identify the actual private arguments slot in that source owner.
A source binding named arguments or an interposing With refuses the proof.

The proof conservatively walks the complete owner parameters/body and nested
functions, refusing mutation, calls/coercions, suspension, binding patterns and
other source forms that may execute arbitrary code. Refusal leaves the existing
Dynamic metadata and runtime resolution in place. This does not replace the
read with a copied hidden value, change binding mutability or assume that every
cell sharing the private storage spelling has the same owner. An unrelated
ordinary function's reassignment cannot erase this owner's proof.

The original ordinary/default/rest/mapped and lexical capture controls remain
unchanged. Added controls require the precise fact to coexist with the named
runtime Read and reject source reassignment, shadows, direct eval, user hooks,
With lookup, parameter effects and await. Actual compiler/native verification
is required before acceptance; private authoring ran no build or native test.
