# Arguments-object construction: one semantic choice and one validated map

## Decision

Each Wasm function builder carries one closed arguments-object construction
protocol. The semantic selection is:

```text
Absent
Present(Unmapped)
Present(Mapped(plan))
```

The protocol is selected once when the builder is created. Arrow functions use
`Absent` because their `arguments` reference is lexical. An ordinary
function's semantic choice is `Present`: a strict function or a function with a non-simple
parameter list uses `Unmapped`, while a sloppy function with a simple parameter
list uses `Mapped(plan)`.

This is the split made by FunctionDeclarationInstantiation. Its choice between
CreateUnmappedArgumentsObject and CreateMappedArgumentsObject depends on
`[[ThisMode]]`, strictness and whether the parameter list is simple. It does not
depend on which backend storage happened to be allocated for a parameter.

A consumed activation plan can then omit materialization when the entire
invocation has no observer. This decision does not change mapped versus
unmapped semantics or excuse invalid mapped storage.

## The invalid fallback

The former emitter recomputed the semantic choice from four independent
predicates at the point of object construction:

```text
ordinary function
and not strict
and no default or rest parameter
and every retained parameter currently has an owned environment slot
```

The last predicate was not a semantic condition. If lowering omitted a mapped
parameter's environment slot, the predicate silently changed a required mapped
arguments object into an unmapped one. That changed indexed aliasing and the
`callee` property instead of identifying malformed compiler IR.

The same emitter later recomputed duplicate-name handling and used the raw
parameter index as the environment slot when the whole list had no duplicate.
That shortcut made the output depend on an unstated coincidence between two
integer namespaces.

## Construction

The backend derives the semantic variant only from the function protocol,
strictness and parameter-list shape:

- `Arrow` and `AsyncArrow` become `Absent`;
- strict ordinary functions become `Present(Unmapped)`;
- ordinary functions with a default, rest or destructured parameter become
  `Present(Unmapped)`;
- every other ordinary function becomes `Present(Mapped(plan))`, including an
  empty simple parameter list, whose plan is intentionally empty.

Destructuring is represented by the lowered function's
`ParameterInitialization` statement. The construction protocol treats that
marker as non-simple even though the storage parameter itself has a generated
name.

Only after choosing `Mapped` does construction validate backend storage. The
plan retains the last occurrence of each duplicate parameter name, exactly as
CreateMappedArgumentsObject builds its ParameterMap in reverse list order. For
each retained parameter it records a typed pair:

```text
ArgumentIndex -> ParameterEnvironmentSlot
```

Those are distinct private newtypes. A producer cannot transpose them, pass a
raw parameter index as a storage slot, or construct an entry outside the
planner. Every retained name must resolve to exactly one owned environment
binding, and that slot must have exactly one owner. A missing, duplicate or
aliased slot is malformed lowered IR and makes Wasm emission fail. Storage
validation never reclassifies the function as unmapped.

## Synchronous leaf activations

`UserFunctionActivationPlan` first constructs the semantic protocol and
validates the original mapped slots. A private, source-bound
`UnobservableLeafActivation` proof then admits only a synchronous ordinary
function with inert simple parameters, no captured/eval/private/resumable
environment, and no owned cells except its formal parameters. Its body may
contain inert string directives and empty statements, followed by one return.
The complete return expression must consist of primitive literals, parameter
copies, fresh static data/array literals, or the admitted non-coercing operations.
Number arithmetic requires both the existing Number kinds and recursively
admitted operands; a Number result alone cannot hide an object conversion.
Other expression kinds conservatively retain the ordinary activation.

Calls, property access, computed keys, spread, methods/closures, dynamic binding
lookup, `arguments`, coercion of unknown values, default/rest/destructuring
parameters and suspension cannot acquire this proof. A sloppy function's
legacy `.arguments` may be observed by a callback, getter or `valueOf`, even
when its own source never reads `arguments`. Such functions keep the real
object and its mapped cells. This also preserves the published object when an
observer freezes the active function's legacy property.

Consuming the proof selects the exact source, omitted arguments binding and
empty mapping-only storage slice together. Parameter values then use normal
local binding allocation. The source body still executes through normal Realm
selection, checked source-depth entry, source identity, legacy capture/restore,
completion and constructor normalization. It does not take the separate empty
strict-body compilation path. The pre-existing proof for an actually empty,
strict body retains that separate entry and epilogue.

Strict ordinary functions also admit an arguments-only omission when their
complete inspected body has no arguments reference, direct eval or newly created
callable/class. Capture reads, property access, coercion and callbacks are allowed:
the normal strict caller barrier prevents legacy activation reflection. This
source-bound proof retains every existing owned environment binding and normal
entry/epilogue; it removes only the implicit object and its metadata publication.
Non-simple parameters, resumable/class protocols and uninspected IR forms retain
materialization. Named Module environment metadata alone is not an eval observer.

Strict simple parameters no longer force environment cells just to seed
arguments mapping: strict arguments are unmapped. Capture discovery, eval
visibility, separate body/parameter environments and resumable ownership still
retain every cell they require. That selection happens before physical
environment chains and capture hops are finalized. Sloppy simple parameters
keep their original mapped-slot validation; only the consumed leaf proof can
elide their mapping-only storage later.

## Consumption

Parameter binding, local-count planning and root-var reuse consume `Absent`
versus `Present` directly. Arguments-object emission accepts only the narrower
present protocol and exhaustively selects:

- the current environment plus the mapped `callee` data property for
  `Mapped(plan)`;
- no parameter environment plus the unmapped poison-pill `callee` accessor for
  `Unmapped`.

Indexed descriptor emission iterates only the prevalidated mapped entries. It
does not inspect parameter syntax, search environment bindings, recalculate
duplicates or infer an environment slot from an argument index.

Internal Wasm callables retain their existing strict/unmapped object, while the
script main builder uses `Absent`. User-function builder creation is fallible,
and the public emitter propagates protocol-construction errors.

## Synthetic class-element calls

Static blocks and field initializer bodies use the ordinary function-entry
protocol. A class definition's shared context carries its lexical environment,
home object, computed field keys and private environment; it is not an active
function identity. Before a direct class-element call, the backend materializes
the exact synthetic function and copies that definition context into the fresh
function context. The canonical function allocator supplies the active function
and its defining Realm. The definition context is not mutated or reused as an
invocation identity.

This preserves the required nonzero active-function invariant used when an
unmapped Arguments object's poison accessors load `%ThrowTypeError%`. The
former static-block call passed a shared context whose active-function word was
zero and trapped at that invariant. Static blocks still execute their ordinary
entry path; the fix does not omit Arguments construction or substitute a
caller's Realm when internal state is missing.

The static-block FunctionDeclarationInstantiation requirement follows
[EvaluateClassStaticBlockBody](https://tc39.es/ecma262/multipage/ecmascript-language-functions-and-classes.html#sec-runtime-semantics-evaluateclassstaticblockbody).

## Enforced invariants

1. A function has either no own arguments object or exactly one present object
   kind; impossible cross-products cannot be assembled.
2. Function shape and strictness are the only inputs to mapped versus unmapped
   semantics.
3. `Mapped(empty)` remains distinct from `Unmapped`.
4. Duplicate names map only their last parameter occurrence.
5. Argument indexes and environment slots cannot be interchanged at a typed
   producer/consumer boundary.
6. Every mapped entry names validated, unique environment storage; malformed IR
   is an emission error, not a semantic fallback.
7. Adding another present object kind requires updating every exhaustive
   construction consumer or the backend fails to compile.
8. Arguments elision and mapping-only storage elision share one consumed,
   source-bound activation plan; a nonempty return body retains its execution.
9. A source callback or unknown object conversion cannot acquire leaf authority.

## Verification boundary

Backend module tests cover absence for arrows, unmapped strict/default/rest/
destructured functions, the empty mapped plan, valid unique and duplicate-name
maps, and emission rejection for a lowered function whose required parameter
slot is removed. Leaf controls inspect actual encoded P bodies for omitted
ArgumentsObject/Environment/BindingCell allocations and metadata calls, retained
source-depth guards, and the corresponding retained allocations at observer
boundaries. IR controls check strict parameter retention for capture, eval and
resumption and hops through a storage-free intermediate activation.

Focused engine controls cover fresh return identities and roots, duplicate
parameters, callback/getter/coercion reflection, active-property freezing,
captured arguments and parameters, eval and non-simple parameter fallback,
strict transitive/resumable storage, constructor returns and cross-Realm
new-target prototype selection. Existing mapped descriptor and unmapped
`callee` regressions remain required.

Cargo and runtime verification of this leaf batch remain deferred to the
centralized bounded verification checkpoint. These controls are not a claim
that the new batch has already passed or that a benchmark speedup was measured.

## Nonclaims

This protocol does not add lazy arguments, specialize unknown parameters with
numeric guards, replace the arguments representation, change descriptor layout,
or broaden the supported language surface. Bodies requiring reentry keep their
ordinary construction path. T09 remains in progress.
