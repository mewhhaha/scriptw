# Shared eager intrinsic accessor records

Eligible standard accessors keep eager creation. A private allocation plan binds
their original StandardBuiltin entry and metadata to a checked runtime-owned
pooled source ordinal. It admits only strict ordinary call-only entries without
class machinery or environment captures. Constructors, canonical-slot accessors,
captured factories and program-owned source literals retain their existing paths.
Canonical lookup, insertion order and alias ownership remain at their old owners.

The caller still allocates the native object header and typed ExecutableCode,
using the original defining Realm and Function prototype. One typed R helper then
allocates FunctionContext, resolves the source ordinal through PooledStringResolve,
allocates FunctionObject and closes ACTIVE_FUNCTION. Source materialization stays
after context creation. Metadata publication remains after the closed cycle,
through the existing publisher and original metadata. Every creation still makes
fresh records; this changes emitted-code duplication rather than allocation count.

The plan's fields are private and the allocation wrapper accepts that plan rather
than unchecked metadata. Runtime-helper parameter/result types retain GC roots
across calls. Runtime layout 28 binds the added private helper ABI; the existing
six-segment data contract and SCRWRM02 receipt format are unchanged. This remains
the single Wasm GC backend requiring Scriptw's experimental Wasmtime capabilities.

Prepared controls cover invalid planner metadata, the actual emitted R helper and
its bootstrap callers, source/context/function/cycle order, getter metadata,
nonconstructibility, GC, borrowed foreign-Realm accessors and excluded canonical,
RegExp alias and captured Intl-function identity paths.

Whole-batch compilation, semantic verification and matched memory measurements
remain pending. Sharing code does not establish resident-memory or timing savings.
