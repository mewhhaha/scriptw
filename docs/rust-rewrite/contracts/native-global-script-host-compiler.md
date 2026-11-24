# Native global Script compiler seam

This is an unapplied IR/AOT component for the ABI7 native global Script boundary.
It adds a typed native compiler import and P factory emission, with no Engine
implementation, emitted parser, or accepted dynamic eval capability. The parent
review approved this AOT boundary; the separate Engine implementation and real
native tests remain required before this can be published as product support.

## Typed input and complete output

`lower_runtime_global_script(RuntimeGlobalScriptKind, &[u16], HostSurfacePolicy)`
accepts an already-coerced ECMAScript String. The closed role domain contains
only `IndirectEval` and `RealmScript`; even the internal lowering helper accepts
this two-role type. It cannot accept `DirectEvalContextIr` or manufacture direct
eval from a global role. ShadowRealm evaluation is also outside this API because
its result/error wrapping contract needs its own runtime owner.

Every request uses the real front end on the exact UTF-16 units:

- Indirect eval: `prepare_eval_source_utf16(..., EvalParseContext::Indirect)`.
- Realm Script: `parse_script_utf16(...)`.

The entry does not round-trip through lossy UTF-8, inherit caller strictness,
private names, super, or new.target permission, consult an AST-shape cache, or
predict a finite source catalogue. Ordinary nested preparation remains part of
the existing lowerer, with its current capability accounting and limits.

The supplied source uses the existing `lowering/prepared_script.rs` machinery
with `ScriptInstantiation::Prepared(kind)`, not fresh-entry declaration rules.
The compilation owner allocates the actual `StaticScriptId` before lowering,
so nested Script identities cannot collide with the requested entry. The
compiled root body, its lexical/environment bindings, runtime global admission
plan, strictness, template source, and eval environment are moved into the
actual `PreparedScriptUnit`. All function bodies, nested prepared Scripts,
dynamic Function rows, host requirements, stages, and diagnostics remain in the
same complete `ProgramIr`. The containing entry body is empty and its source
declaration bindings are removed. It is a compilation owner, not the requested
Script execution entry.

`RuntimeGlobalScriptProgram` privately owns that complete IR, verified entry ID,
and role. Its public read-only `program()`, `script_id()`, and `kind()` accessors
let the AOT factory emit the real compiled entry. Do not invoke `main` to
execute this request. `emit_runtime_global_script` retains the exact admitted R
Arc/profile and privately mints the P artifact with `scriptw.program_init` and
`scriptw.runtime_global_script_factory` exports. The factory aliases the owned P2
PreparedScript hook. Registry admission requires its first row to equal the
witness source/kind/root ID and to have the actual planned PreparedScript entry.
The host must verify its exact decoded request against that witness before
calling the factory; first-match dispatch therefore executes the root before
any nested row. All nested prepared/direct/Shadow Script ownership is retained.

`RuntimeGlobalScriptCompileError::Parse` retains the original classified
`ParseError`; `Lowering` retains every `IrDiagnostic`; `InvalidCompilerOutput`
reports a broken compiler invariant. A host must distinguish actual syntax
rejection from parser capability gaps or unsupported lowering. The IR API never fabricates a JavaScript SyntaxError or erases a compiler
diagnostic. Only a future host result classified from actual malformed source
may request target-Realm SyntaxError construction at the typed AOT boundary.

## Runtime ownership required before implementation

The runtime still owns evaluation/coercion order, non-string eval passthrough,
selected Realm and its intrinsic error identity, declaration admission against
the actual global environment, execution, and completion transport. Native
compilation must preserve the caller's Wasmtime Engine/Store, exact shared R
identity/profile, host-surface policy, and original deadline/budget. A fresh P
must retain its own immutable literal/metadata image and typed hooks under the
ABI6 ProgramInstance rules. No shared mutable current-P hook state is permitted.
The host must not call P main, create a replacement Realm, reset a deadline, or
silently fall back to an interpreter on a compile miss.

The existing ABI6 `PreparedScriptInputs` carries lexical environment, this,
new.target, variable environment, private environment, nullable
DirectEvalExecutionContext, and the executing ProgramInstance. The reviewed
global execution path must select its target Realm/global environment and pass
that path's actual typed inputs, then restore the caller Realm on every normal
and abrupt exit. Strict indirect eval owns its declarative variable environment;
sloppy indirect eval and Realm Script use the existing global-variable rules.
The real prepared-Script emitter owns these distinctions, rather than the host
approximating them with fresh-entry initialization.

Direct eval requires a separate future API and review. Its caller lexical,
variable and private environments, this, new.target, DirectEvalExecutionContext
and executing P cannot be substituted with global values. Its parse authority
is the actual `DirectEvalContextIr`: caller strictness, invocation category,
private-name identities, derived-constructor owner, and Script import origin.
It must preserve caller binding resolution, declaration conflicts and lexical
capture. The global enum deliberately has no direct-eval variant.

Forced GC, reentrant callbacks, exceptions, retained closures and queued jobs
must keep the actual P and referenced execution owners alive. Cross-callback
host state must retain `OwnedRooted` references and re-root them in the receiving
Store/Caller scope. These are implementation obligations, not tested claims in
this IR-only component.

## Code reuse and acceptance

The component changes no cache size/admission policy, source limit, per-P data
ownership, selected Intl profile or timeout. The new core host import requires
a fresh ABI7 R and rejects older cache/package envelopes. Every request traverses
parse/early errors, existing IR lowering and eventual real AOT Wasm emission.
Only a separately verified complete emitted code image plus all typed ABI,
compiler, R/profile, engine/config and native-mode identity can be a future
module-sharing boundary; source/AST shape is not an equivalence proof.

Six authored IR controls cover all four kind/strictness combinations, runtime
admission of restricted global names, exact lone-surrogate/sentinel transport,
real parse errors with no caller syntax authority, retained nested Script and
Function bodies/IDs, and 257 genuinely distinct requests beyond the finite
prepared catalogue size. They assert actual ProgramIr variants and declaration
plans. They have not been compiled or executed in this packet. Formatting,
source-invariant checks and strict selected-file patch replay are preparation
checks only. Product, host ABI, Wasm validation, native execution, all-suite
retention, and dynamic eval conformance remain unverified.

## Reviewed typed native boundary

Only a Global dispatch whose actual current-P PreparedScript hook returned
Unmatched calls the distinct `CompileGlobalScript` host import. Indirect eval
is kind 0 and Realm Script is kind 1. Direct eval and ShadowRealm cannot select
this global source-miss route. The operand signature is:

`(kind: i32, source: StringValue!, targetRealm: RealmRecord!, callingP: ProgramInstance!)`

The results are the original five Completion words, `status: i32`, and a
nullable `StringValue` syntax message. Genuine execution returns Entered=2
with null syntax, including when JavaScript throws. Only a host classification
of real malformed JavaScript returns SyntaxError=1 with a non-null exact GC
message. Parser capability gaps, unsupported lowering, resource errors and
invalid protocol results trap. Unmatched or non-Normal/non-Throw/local-target
execution completions trap at the AOT boundary instead of being relabeled as
unsupported eval.

The runtime creates a syntax error directly from the passed target Realm's
SyntaxError prototype and exact GC message, without switching active Realm or
current P. It clears/stores throw diagnostics in the same way as the existing
native Function boundary. On an executable P2, its owned PreparedScript hook
enters and restores the passed target Realm on normal and abrupt completion.
The host initializes only P2 and invokes its factory as:

`(P2!, kind: i32, source: StringValue!, targetRealm: RealmRecord!) -> (Completion5, status)`

Both export signatures, the actual source/kind witness, same Engine/Store/R,
remaining deadline and outgoing reference lifetimes must be validated by the
future Engine implementation. The actual calling P remains rooted across the
request. No host implementation is supplied by this component.

Adding this distinct core host import and static GC signature changes the R/P
contract: LAYOUT_VERSION is 7. Existing ABI4 envelope rejection tests are
retained and ABI6 envelope rejection controls are added for raw cache and build
packages. These checksum-preserving header controls do not substitute for an
actual old-R/new-P native typed-link rejection proof. This component adds
no ProgramInstance hook field (it then had five; ABI 89 later added
LITERAL_RESOLVE as the sixth) and no global current-P state. GcHostLayout::RealmRecord is a declaration-owned host
projection, without a physical Realm field change.

Six additional authored AOT controls cover the exact import signature/projection,
root witness substitution negatives, four kind/strictness emitted P modules,
actual distinct R imports, and ABI6 cache/build envelope rejection. They remain
uncompiled/unexecuted. Source checks, formatting and patch replay are preparation
checks only. Native Function support, actual eval execution, independent-host
acceptance and large-instance code/data reuse remain separate gates.

## Typed Realm Script invocation admission

Opaque source for a known Realm Script callable now receives the private
`AdmittedRealmScriptInvocation` proof. Only resolution of the actual host
callable identity can mint that proof; property names and receiver spellings
cannot. Plain calls, spread calls and call-candidate analysis consume it
exhaustively, preserving the original callee, receiver and evaluated arguments.
The existing runtime callable still owns ToString, captured target Realm,
prepared-hook dispatch and native fallback. Unknown user-code effects are
invalidated before admission. Symbol conversion throws, indirect eval non-string
passthrough, direct-eval context and ShadowRealm admission stay distinct.

This is a fresh successor to the sealed IR/AOT component, required so the new
Realm Script callback is reachable for actual opaque-source invocations. It
adds no physical GC field or import and leaves ABI7 unchanged. Three authored
IR controls cover direct/spread/aliased/forwarded opaque calls, replacement
ordinary methods, and exclusion of caller-dependent/direct and Shadow source
units. These controls remain uncompiled and unexecuted.
