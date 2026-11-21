# Dynamic-source AOT capability boundary

## Decision

Scriptw's Wasm-AOT artifact never contains a parser, interpreter, or VM. Known
source candidates compile through the ordinary parser, early errors, spec IR,
lowering and Wasm code generation. Runtime dispatch selects a prepared unit only
when the actual intrinsic, source and execution context match. Source outside
that registry remains a compiler capability gap, not an ECMAScript rejection
or a passing conformance result.

`%eval%` does not always evaluate source. `PerformEval` returns its argument
unchanged before selecting a realm or environment when the argument is not a
primitive String, and `%eval%` called with no arguments returns `undefined`.
Those branches are ordinary builtin execution, not dynamic-source support, and
are admitted by the closed proof below.

The implemented units cover nonempty Function-family sources, direct eval,
indirect eval and realm Scripts. Finite discovery retains runtime argument
effects and coercions; it does not substitute source expressions or bypass
callable identity. Calls with no Function-family arguments still create fresh
empty functions with the selected execution protocol and constructor realm.
The [Function](prepared-dynamic-functions.md), [Script](precompiled-realm-scripts.md)
and [direct-eval](prepared-direct-eval.md) contracts define the execution paths.

## Closed domain

`DynamicSourceKind` names the semantic operation:

- direct `eval`;
- indirect `eval`;
- realm `evalScript`;
- one of the four Function-family constructors.

`DynamicSourceGap` has private fields. Its constructors derive the requirement
from the operation:

- source not proven at AOT time requires runtime compilation;
- an unimplemented AOT-known direct-eval context identifies caller-environment debt;
- an unimplemented AOT-known indirect eval, realm evaluation, or Function-family
  context identifies target-realm debt. Implemented contexts instead enter their
  prepared Script or Function registry.

Call sites cannot construct a mismatched pair such as Function construction
plus a caller eval environment. `UnsupportedFeature::DynamicSource` is carried
by `IrDiagnostic`; consumers classify the typed value and do not parse its
display text.

`DynamicSourceIntrinsic` is the closed identity catalog for the four
Function-family constructors and realm `evalScript`. Ordinary Function uses
`StandardBuiltinId::FunctionConstructor`; derived constructors use internal
`HostBuiltinId` metadata with their actual emitted constructor bodies. They are
not additional globals. Shared candidate analysis consumes the closed
`ResolvedDynamicSourceCall` result:

| Variant | Authority retained |
| --- | --- |
| `EvalPassThrough(ProvenEvalPassThrough)` | A no-source eval result while retaining ordinary callable and argument evaluation |
| `IndirectEvalInvocation(AdmittedIndirectEvalInvocation)` | Live indirect-eval argument classification, non-String identity, and typed runtime rejection for an unmatched String |
| `FunctionInvocation(AdmittedFunctionInvocation)` | Function-family execution kind, live argument ToString and guarded source-tuple dispatch |
| `CompiledScript(ProvenCompiledScript)` | A registered Script unit selected through the actual intrinsic at runtime |
| `Unsupported(UnsupportedDynamicSourceCall)` | One typed operation/requirement and its builtin-accounting owner |

`PreparedDynamicFunctionOutcome` distinguishes `Compiled { function_id }` from
`SyntaxError { message }`. `PreparedScriptOutcome` distinguishes `Executable`
from `DeferredSyntaxError`. Only genuine ECMAScript parse/early errors enter
those deferred-error variants; parser capability failures and compiler bugs
retain their diagnostics.

Required units and optional candidates have distinct admission:
`DynamicFunctionSourceAdmission::{Intrinsic, RuntimeCandidate}` and
`PreparedScriptAdmission::{ResolvedIntrinsic, RuntimeCandidate}`. A candidate
can be discovered without proving which callable will consume it. Its presence
does not authorize removing a getter, callback, argument effect or conversion.

`DynamicSourceRuntimeOperation` records an actual intrinsic selected during
execution without a matching compiled source/environment specialization. This
includes a known Function constructor receiving an unmatched runtime source
tuple, as well as calls whose static provenance was unavailable. It is separate
from `DynamicSourceGap`: runtime identity alone does not prove direct-eval syntax,
source constancy, or a particular missing lexical environment. Its explicit
numeric ABI is `Eval=0`, `RealmEvalScript=1`, and ordinary/generator/async/async-
generator Function construction `=2/3/4/5`.

The `scriptw_host.reject_runtime_semantics(i64) -> ()` import receives the
closed `RuntimeSemanticRejection` wire domain. The emitter always declares it, and
R always imports it; final P import compaction drops it, like any host function
import, when no body of P calls it, so P has no fixed index for it (calls are
remapped through their index operands). It is the sole numeric authority:
`DynamicSource(operation)` retains codes 0–5 above; `Gap(TemporalNamedTimeZone)`
uses code 6 and belongs to T22; `Gap(RegExpRuntimePatternCompilation)` uses code 7
and belongs to T19. The unary ABI is unchanged. The Wasmtime binding rejects unassigned codes and returns a typed
host error for a valid reason. Engine execution extracts this error before generic trap formatting;
Test262 classifies the typed reason as Unsupported before JavaScript negative
expectations. `EngineError::runtime_dynamic_source_operations()` returns every
distinct rejection retained from the root and its workers. A nonempty execution
failure aggregate keeps each original error, including its compile diagnostics;
worker failures stay owned until joined even after their broadcast channel closes.
An aggregate consisting only of typed source capability gaps remains Unsupported,
including a worker-start compilation diagnostic combined with another worker's
runtime rejection. A compile diagnostic does not fabricate a runtime operation.

`WasmExecutionFailureKind` separates a root JavaScript exception from dynamic
source rejection, compiler semantic gaps, concurrent failures, Wasm traps and
execution timeouts. A
runtime-negative Wasm test must observe the root JavaScript exception. Aggregates
containing real failures remain Bug or Crash even when their detail also contains
an unsupported source operation. Root and worker failures are combined after the
root completion is decoded, so neither result hides the other. Spec-exec oracle
classification and compile-negative parse/early diagnostics retain their existing
behavior. JavaScript `catch`, Promise rejection handling and worker report
serialization cannot convert the capability failure into a JavaScript error or
a passing test. The import neither receives nor compiles source.

Structured observation keeps a root JavaScript throw as an `ObservedCompletion`
when its workers succeed. If workers also fail, the finalizer retains that throw
as a typed JavaScript cause alongside the worker errors. It uses only the existing
type-level observation note and does not inspect error properties or constructor
metadata in structured mode. A normal structured completion adds no failure cause.

Runtime-negative error types compare exactly with
`EngineError::wasm_javascript_exception_constructor_name()`, projected from the
separate `throw_error_constructor_name` Wasm export. Test262's
`INTERPRETING.md` defines `negative.type` as the thrown exception's constructor
name. Diagnostic `.name`, message text, primitive string throws and names in
worker failures cannot satisfy this comparison. The constructor name is captured
once from the final root value after the job checkpoint, so a caught throw in a
finalizer or Promise job cannot replace its authority. This is a data-property
observation of `constructor.name`, not an intrinsic constructor identity check.
Accessors and Proxy traps are not invoked for metadata: if observation would
require user code, the constructor name remains unavailable and a named runtime
negative fails with that evidence. A constructor-name mismatch is a Bug even when
the JavaScript error's message contains unsupported-capability wording.

Created realms initialize their own GeneratorFunction, AsyncFunction and
AsyncGeneratorFunction constructor/prototype pairs, Generator and AsyncGenerator
instance prototypes, and AsyncIterator prototype in the canonical realm record.
All callable methods use `RealmFunctionMaterializationContext`; creating a realm
does not temporarily replace the entry realm's intrinsic globals. Each derived
Function prototype inherits that realm's callable `%Function.prototype%`, as
specified by [ECMA-262 27.4.3](https://tc39.es/ecma262/2025/multipage/control-abstraction-objects.html#sec-properties-of-asyncgeneratorfunction-prototype-object).
The entry realm's former AsyncGeneratorFunction prototype parent has been
corrected to the same graph.

## Product-path invariants

1. A compile-time dynamic-source diagnostic belongs to a resolved intrinsic
   with an unsupported source/context. A runtime rejection belongs to the
   actual intrinsic lacking a prepared specialization. Identifier spelling
   alone cannot authorize execution, and textual arguments alone do not imply
   a diagnostic.
2. Direct eval requires the original identifier Reference and an evaluated
   callable equal to the caller realm's immutable original `%eval%`. The
   Reference and callable are retained before argument effects. Aliases,
   property/comma/optional calls, bound functions and proxies follow ordinary
   dispatch; a foreign intrinsic uses its own realm for indirect eval.
3. The residual gap classifier's syntax proof accepts string literals,
   no-substitution templates, parentheses and pure literal concatenation.
   Separate finite discovery may contribute guarded candidates from known
   bindings, literal records/arrays and callbacks. Neither proof permits
   removing the expression or coercion that supplies the runtime source.
4. Function parameters and body are parsed separately, followed by combined
   early errors, before independent lowering. Script units retain their parse
   goal and direct-eval caller permissions. Function candidate expansion is
   bounded at 256 values or source tuples; crossing that bound retains the
   explicit runtime limitation.
5. The typed diagnostic is a compiler gap. It has no early-error code or native
   error type and cannot satisfy a negative Test262 expectation.
6. Every successful Function-family invocation creates a fresh function with
   the correct ordinary, generator, async or async-generator execution protocol.
   Its defining realm follows the active constructor; its internal prototype
   follows `GetPrototypeFromConstructor(newTarget)`. Argument ToString occurs
   in order before tuple selection, deferred SyntaxError or typed rejection;
   an abrupt conversion retains its real JavaScript exception. Zero-argument
   calls use real empty functions, not constructor or thrower metadata.
7. Generator, async and async-generator function object shapes carry their
   respective constructor identity through the intrinsic prototype's
   `constructor` property. The identity follows aliases and property reads; a
   source identifier named `GeneratorFunction` is not evidence by itself.
8. Optional calls retain the same pre-lowering source proof as ordinary calls.
   Reanalysis of an already-lowered optional-chain prefix is marked as already
   accounted, so it cannot silently downgrade or duplicate the diagnostic.
9. The Test262 harness obtains realm `evalScript` from one typed host builtin
   admitted by `HostSurfacePolicy::Test262`. Product lowering cannot resolve
   that global, and the harness stores the resolved function value directly on
   `$262`, preserving source-candidate discovery at its eventual call site.
   The actual realm-eval callable selects its prepared Script in its defining
   realm and performs declaration validation before mutation. A missing unit
   reaches the typed runtime import; an overwritten property calls its
   replacement.

The private `DynamicSourceProof` is a non-`Clone`, non-`Copy` two-row authority
with seven lexical type mentions. Syntax classification produces it once and
the sole exhaustive gap projection consumes it into either runtime-compilation
or AOT-known environment debt. It has no debug, equality, cast, wildcard or
default route, so downstream lowering cannot duplicate, compare or reuse one
source proof after diagnostic ownership has been transferred. This proof
classifies residual gaps; it is not the authority for all prepared candidates.
Its original introduction was a source-equivalent capability closure, before
the independent prepared-unit execution paths were implemented.

`dynamic_source_proof_structure.rs` pins the exact declaration, complete
producer, final construction route and sole exhaustive gap projection. Its
recorded capability-only checkpoint passed `3/3`; the closed-operation and
requirement owner witness passed `1/1`. These are historical verification counts.

The private, non-`Clone`, non-`Copy` `UnsupportedDynamicSourceCall` owns the
resolved builtin-accounting identity and `DynamicSourceGap` as one one-shot
authority. Resolution derives both fields together; consumers can move the
authority or discard an already-accounted result, but cannot construct or
separate the pair. The sole recorder decomposes it, applies the already-derived
standard-builtin accounting projection and emits the typed diagnostic.
Construct lowering does not carry a redundant function ID beside that
authority. Its introduction was a source-equivalent unsupported-accounting closure:
that accounting change itself did not implement textual source evaluation.

`unsupported_dynamic_source_call_structure.rs` pins the exact private pair,
sole construction route, sole decomposing recorder, five diagnostic-recording
call sites and the distinct already-accounted discard route.
Focused verification commands are:

```sh
cargo test -p scriptw-ir --test scriptw_ir_integration -- ir_cases::unsupported_dynamic_source_call_structure:: --test-threads=1
cargo test -p scriptw-ir dynamic_source_diagnostics_carry_closed_operation_and_requirement -- --test-threads=1
```

Direct eval uses the original identifier call's `DirectEvalContextIr`, carried
by `CallIndirect` or `EnvironmentIdentifierIr::Call`. The evaluated callable is
compared at runtime with the caller realm's immutable original `%eval%` object.
Preparing a source candidate never grants callee authority. Aliases, property
calls, bound functions and proxies follow ordinary call dispatch; another
realm's intrinsic performs indirect eval in that realm.

The former `DirectEvalCallSite`/`ErasedDirectEvalCall` compile-time classifier is
removed. Its product callers now use the runtime identity guard and separate
prepared Script thunks described in [prepared-direct-eval.md](prepared-direct-eval.md).
`direct_eval_call_site_structure.rs` pins that authority boundary;
`aot_direct_eval_call_identity` checks original callee capture, all argument
effects, non-string passthrough and replacement receivers. These commands are
verification obligations, not claims that a fresh run has passed:

```sh
cargo test -p scriptw-ir --test scriptw_ir_integration -- ir_cases::direct_eval_call_site_structure:: --test-threads=1
cargo test -p scriptw-engine --test aot_realm_modules -- aot_direct_eval_call_identity:: --test-threads=1
```

## Proven no-source `%eval%`

`EvalPassThrough(ProvenEvalPassThrough)` is one of the five resolved-call
variants above. Its private constructors admit direct or indirect intrinsic
`%eval%` only when:

- the call has no spread and no arguments; or
- the call has no spread and its lowered first argument has a nonempty
  `KindSet` that excludes primitive `String`.

An empty kind set is not evidence. String-capable values, spreads and the other
dynamic-source operations cannot acquire this particular pass-through proof.
Indirect eval can instead use `AdmittedIndirectEvalInvocation`: the actual
intrinsic returns non-String values unchanged and rejects an unprepared String
with the typed runtime capability failure. This admits widened argument types
without claiming a static pass-through result or inventing a prepared unit.
Other operations follow their prepared-source admission or typed rejection. All
Function-family invocations use `AdmittedFunctionInvocation`, including empty
ones; no `ProvenEmptyFunction` domain remains. Every retained dynamic-source
target must admit the call before a multi-target call can proceed.

Target completeness is independent of `heap_shape` and lives in the closed
`FunctionTargetKnowledge::{Exact, Open}` lattice. `Exact(targets)` states that
the set is exhaustive; `Open(targets)` retains known candidates while admitting
additional targets. Joining two values unions their candidate sets and remains
exact only when both inputs are exact. A possible replacement widens either
variant to `Open` without discarding its known candidates. Heap-shape joins may
therefore erase incompatible shapes while exact target joins remain exhaustive.
Lowering may inspect `known_targets()` for conservative effect or
dynamic-source accounting, but exhaustive multi-target dispatch requires
`exact_targets()` and backend single-target specialization requires
`exact_single_target()`. An open set never grants either authority.

One shared candidate-analysis phase consumes this lattice for ordinary,
property, private, super and optional calls and for construction. It visits all
retained candidates through `known_targets()`, preflights dynamic-source
identities before any candidate effect can erase facts needed by a later
alternative, and adds a generic residual result and unaccounted effects only
for `Open` knowledge. For `Exact` mixed values, non-callable or
non-constructable branches throw and therefore add no normal result. A known
candidate whose signature is not registered yet can occur while class static
elements execute; that branch is explicitly treated as unaccounted instead of
silently retaining precise facts.

Construction uses the same phase but keeps the ECMAScript normal-result
codomain object-like: primitive explicit returns are projected away before
joining with the constructed receiver, reusable exact-context return facts are
consumed, literal Proxy trap hints are installed before argument lowering, and
an evaluated callee's proven common `prototype` refreshes each source
constructor instance. When no common current prototype is proven, the
definition-time prototype is removed while retaining known own instance
properties. Spread arguments widen source parameters and source-call results;
spread construction stays within a generic object-like result instead of
reusing an earlier narrow return observation. A standard builtin without a
specialized result summary falls back to its declared signature rather than
silently removing that candidate's normal result.

Consequently, exact aliases with differing function shapes preserve their full
target set through control-flow joins. Multi-target `%eval%` classification
applies the pass-through rule to every target and merges all result facts, so a
no-argument target contributes a genuine `undefined` alternative rather than
erasing the other target's result.

The proof permits lowering to retain the ordinary indirect call. It never
replaces the call with its first argument or `undefined`: the evaluated callee
and every argument remain in source order, an overwritten `eval` still wins,
and abrupt argument completion is unchanged. Its result fact is `undefined`
for no arguments and the first argument's exact `ValueInfo` otherwise.

This is intentionally not an AOT-known textual subset. No String source is
parsed, compiled, or executed by this branch, and it establishes none of the
caller-environment, target-realm, declaration-instantiation, or deferred-error
capabilities required by static Script evaluation.

## Function.prototype.call forwarding authority

The spread-free intrinsic `Function.prototype.call` forwarding path now sends
the evaluated receiver's retained function targets, the original arguments
after `thisArg`, and their lowered values through the same dynamic-source
candidate preflight as an ordinary call. Its call-site context is always
indirect: owning `%eval%` as the forwarded receiver cannot manufacture the
direct-eval caller context. Known source can register an indirect Script or
Function unit, and missing or proven non-String `%eval%` input retains its
exact pass-through result. Indirect eval with a widened argument uses its live
classification guard; unsupported source/context combinations retain their
typed gaps. A Function invocation still performs live coercions before an
unmatched source tuple rejects at runtime.

The route is considered only while `Function.prototype.call` acquisition
remains proven intrinsic. Direct mutation of that property, replacement of the
receiver's prototype, or unknown user-code effects erase that authority before
candidate preflight. An `Open` receiver without a retained heap shape therefore
does not reach forwarding preflight merely because one known target is `%eval%`.

That shared boundary returns the closed, must-use `DynamicSourceCallAdmission`.
`Rejected` returns before forwarded `this`, parameter or caller-flow
observations and before call IR emission. `Admitted` owns the retained candidate
list and result facts from the admitted resolved-call variants; private fields
prevent a sibling lowering path from manufacturing admission. Both `Exact` and `Open`
targets are preflighted through `known_targets()`, while only an exact single
target may contribute the narrow pass-through result and suppress the
underlying eval caller-flow effect.

This `.call` preflight requires a proven absence of spread because source
syntax and lowered forwarded positions must stay one-to-one. Separate finite
candidate discovery also handles supported literal-array `apply`,
`Reflect.apply` and `Reflect.construct` forms and comma callees. Finite candidate
discovery preserves runtime callable identity and exact source equality;
it does not imply unrestricted forwarding support. Bound functions and proxies
retain their ordinary runtime dispatch. A discovered unit cannot turn a
replacement callable into an intrinsic or grant indirect eval a caller scope.

Focused ownership and behavior targets are:

```sh
cargo test -p scriptw-ir --test scriptw_ir_integration -- ir_cases::forwarded_dynamic_source_call_structure:: --test-threads=1
cargo test -p scriptw-ir --test scriptw_ir_integration -- ir_cases::forwarded_dynamic_source_call:: --test-threads=1
```

## Current producer coverage

| Operation | Compiler-owned identity today | Accounting |
| --- | --- | --- |
| direct `%eval%` | Original identifier Reference, `DirectEvalContextIr` and runtime equality with the current realm's immutable `%eval%` | non-String pass-through; matching prepared Eval Script with caller records; typed limitation when no unit applies |
| indirect `%eval%`, including supported forwarding candidates | `StandardBuiltinId::EvalFunction`, ultimately the actual runtime intrinsic | non-String pass-through; matching prepared Script in the intrinsic's realm; typed gap or runtime limitation for unsupported source |
| ordinary `%Function%` | `StandardBuiltinId::FunctionConstructor` | live ToString followed by prepared tuple dispatch, fresh function or deferred SyntaxError; unmatched tuple reports a typed runtime limitation |
| Generator/Async/AsyncGenerator Function constructors | `DynamicSourceIntrinsic::Function(..)` carried by intrinsic prototype metadata | same guarded tuple protocol with the selected execution kind and constructor/newTarget realms |
| realm `evalScript` | `HostBuiltinId::RealmEvalScript`, mapped to `DynamicSourceIntrinsic::RealmEvalScript` and exposed only by `HostSurfacePolicy::Test262` | prepared target-realm Script and runtime declaration instantiation; typed gap or runtime limitation for unsupported source |

There is no lexical Test262 pre-gate for these operations. Unsupported
accounting follows the typed compilation or execution boundary above. Source
text is never replaced based on test paths. Preparing a candidate does not prove
that the runtime callable will use it; erasing static target facts does not
necessarily prevent a prepared unit from matching at runtime. Every actual
unsupported result remains a failing execution. This table describes producer
behavior, not a full-suite conformance count.

## Optional-call accounting authority

The private, capability-free `OptionalCallSource` couples source-proof
availability and diagnostic ownership for each optional call. `Syntax` borrows
the original parser arguments and owns the resulting dynamic-source diagnostic;
`AlreadyAccounted` carries no syntax and suppresses a duplicate diagnostic when
an already-lowered optional-chain prefix is analyzed again.

The chain analyzer borrows each authority exactly once and exhaustively maps it
to the shared `CallCandidateSource` domain. Both rows preserve pass-through
result facts. An unsupported `AlreadyAccounted` row reuses the prior undefined
placeholder without recording again, while a syntax-owning row records the
builtin use and typed gap. Invocation-effect tokens from every optional call
are combined and attached to the emitted chain. There is no copyable
authority, Boolean ownership projection, wildcard or default route.

Focused verification commands are:

```sh
cargo test -p scriptw-ir --test scriptw_ir_integration -- ir_cases::optional_call_source_authority_structure:: --test-threads=1
cargo test -p scriptw-ir no_source_eval_works_through_alias_optional_and_safe_multi_target_calls -- --test-threads=1
cargo test -p scriptw-ir grouped_optional_dynamic_source_prefix_is_accounted_once -- --test-threads=1
```

The recorded capability-only checkpoint passed `4/4` structure tests and
`1/1` for each exact owner witness. They covered new-call pass-through accounting
and grouped-prefix diagnostic ownership. That checkpoint also recorded passing
formatting, `cargo xc`, diff, module-boundary and task-plan checks, with the
repository's then-existing compile warnings. These historical results do not
claim that the prepared-source implementation or this documentation change
reran those commands.

## Prepared-source execution and remaining limits

[Precompiled Scripts](precompiled-realm-scripts.md) own independent Script
thunks, deferred parse/early errors and runtime GlobalDeclarationInstantiation.
All declaration checks precede mutation. Repeated realm Scripts retain their
realm's binding state while allocating fresh declared functions. These units
preserve completion values and target-realm errors without splicing statements
into the caller or manufacturing a host result.

[Direct eval](prepared-direct-eval.md) adds the caller's Eval grammar,
strictness, variable and lexical records, private context and retained
`this`/`new.target`/super authority. [Environment Records](direct-eval-environment-records.md)
share actual closure cells, retain parameter/body separation and preserve a
selected identifier Reference across RHS effects, deletion and redeclaration.

[Function-family construction](prepared-dynamic-functions.md) parses parameters
and bodies independently and allocates against the active constructor's realm.
Finite discovery includes known primitive/string values, arrays, records and
callback arguments, with a 256-candidate bound. Runtime getters, callbacks and
source coercions still execute. Stateful generation outside the prepared
registry and unimplemented source/context combinations remain typed failures.
Optional candidates may decline unsupported lowering capabilities; compiler
invariant failures remain errors, and neither can become a fake SyntaxError.

Native prepared-source targets include `aot_prepared_dynamic_function`,
`aot_prepared_script`, `aot_direct_eval`, `aot_direct_eval_environment`,
`aot_direct_eval_call_identity` and `aot_direct_eval_escaped_arrows`. Their results
belong to the coordinated repair checkpoint and
[repair notes](../observed-later-failure-repairs.md). This contract does not
publish new Test262 counts or claim arbitrary runtime compilation.
