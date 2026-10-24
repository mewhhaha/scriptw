# GC callable captures and execution Realm ownership

This contract describes the ongoing atomic T05 source draft. MAIN remains at
integration 117. It does not establish compilation, emitted-Wasm validity,
semantic regression results or conformance. All of those results remain pending
until the full task source pass ends and capped serial verification runs.

A JavaScript function value owns a FunctionObject reference. Its ExecutableCode
contains the actual registered typed function reference for its body role;
ordinary, generator, async and async-generator entries have distinct signatures.
No integer table index or linear address represents a callable identity.

Function allocation installs the defining Realm, lexical and private environments,
home object, field keys, instance private-method definitions, template owner and
native closure capture before exposure.
The active-function back edge is closed while the record is still private.
A named function expression allocates its actual immutable named cell first and
initializes that same cell with the completed function before returning it.
Native bootstrap completes Function.prototype with its supplied Object prototype
before publishing the per-Realm identity. Immutable captures are never repaired
on an already-published callable.

Canonical constructor and intrinsic function identities live in the defining
Realm's strong intrinsic table. The actual exhaustive builtin authority maps
them to closed slots. The current table has 154 slots, including Array.prototype
at its separately typed slot. This is a layout count, not a conformance count.
An absent required intrinsic fails at the bootstrap/read boundary.

Template sources are noncopyable rooted owners. Source executions and cached
raw/cooked template arrays share those actual owners, not numeric heap tokens.
A cooked template enters the cache only after its real raw and cooked arrays
have complete immutable element descriptors and frozen flags. Dynamic compiled
source executions receive fresh owners bound to their supplied defining Realm.

Bound functions own the exact target, bound-this value and argument List. They
have no independent Realm; GetFunctionRealm follows their target. Creation first
performs the target's GetPrototypeOf, then inspects length and name in bind order.
Concatenating arguments copies immutable StoredValue references. Objects, Symbols
and primitive references preserve identity. Construction forwards NewTarget by
identity according to the target substitution rule. Call and Construct kernels
flatten bound records through the actual rooted target and argument Lists.

Public Call and Construct consumers pass whole values and rooted argument Lists
through registered helper signatures. The helper bodies must call their direct
kernels, not recursively invoke their own public helper facade. Apply finishes
length coercion and all indexed Gets before it obtains a completed List for its
call continuation. A count beyond Wasm's actual array domain is a resource failure;
it must never wrap into a shorter List or omit indexed Gets.

Ordinary calls install the concrete callee's defining Realm before class-call
errors and source this binding. Strict calls preserve this exactly. Sloppy source
calls obtain global-this or primitive wrappers from that same Realm; arrows use
their lexical this. Native methods receive their exact receiver. Ordinary calls
restore the caller's Realm after normal results and JavaScript throws. Source
Return becomes a normal call result; source fallthrough returns undefined, while
native normal values remain intact. The ordinary ABI carries the caller Realm
as an explicit final reference parameter. Base construction reads NewTarget's
prototype and creates the receiver before installing the callee Realm. Derived
constructor epilogues use the explicit caller Realm for post-body TypeError and
uninitialized-this ReferenceError creation. They preserve original throws.

Resumable calls allocate their actual typed activation and invocation frame;
saved resumption enters and leaves the retained function's defining Realm.
Generator and async-generator parameter initialization precedes prototype Get.
Async calls return their own actual Promise and route body completion through
the Promise owner's settlement producer. Remaining Promise/drain/continuation
source work must finish before those paths can be verified.

ArgumentListEvaluation uses a geometrically grown private GC ValueArray and
publishes an exact-length vector. Captured Lists share immutable StoredValue
snapshots; a partial buffer copies only its initialized prefix. Publication
consumes the builder; later constructions can share immutable snapshots but do
not mutate the published vector.
Compiler-captured Lists never become JavaScript Arrays or enter
ValueLocals. Environment bindings have a separate ARGUMENT_LIST edge; local
captures persist through suspension in PRIVATE_ARGUMENT_LISTS. Scope and final
activation retirement clear those roots, while suspension retains them. Spread
uses the actual iterator method and next method, with original whole throws.

Arguments construction consumes the validated formal-parameter mapping plan.
Each alias is a nullable BindingCell reference in a separate ParameterMap;
descriptor updates do not encode, rediscover or restore an environment address.
The rightmost duplicate formal owns the only possible alias, and an absent
argument creates none. Descriptor reads and definitions can retain the cell
before a mutation; successful Delete or accessor/nonwritable conversion retires
only that index's map entry. The ordinary header owns length, callee and the
iterator property. Both the original Array.prototype.values identity and the
strict ThrowTypeError accessor come from the callable's defining Realm, without
reading mutable public prototypes. Rest slicing copies StoredValue references
to a private ValueArray before ArrayFromList publishes the ordinary result Array.

Class element calls allocate their complete captured environment, private
names, home object and field-key list before publication, with a distinct active
function. Private names remain identities; completed method/accessor rows belong
to the constructor's INSTANCE_PRIVATE_METHODS capture. Instance initialization
installs those rows before field initializers and definitions. Public field
creation reaches the receiver's actual DefineOwnProperty dispatcher, including
Proxy and integer-indexed receivers. Class definition creates the instance
prototype after heritage validation and prototype Get, then creates its fully
captured constructor. Computed names finish before the class name is initialized
and static elements run. Private accessor pairing replaces a completed immutable
row while the class remains private. Static private definitions belong to the
constructor's own private-element table.

Suspended named-class evaluation retains its exact Environment in an owned
activation cell's CAPTURED_ENVIRONMENT edge. The producer and consumer resolve
that cell through INVOCATION_ENVIRONMENT, independent of the current nested
scope. The environment never enters a JavaScript value. Completion clears this
temporary edge; published functions retain their defining environment. The
class evaluator consumes the actual typed resumable statement-sequence owner.
Its state gates read the current GC activation rather than a cached entry point
or a raw offset. Wider continuation and terminal-return migration remains pending.

Prepared direct eval compares the actual callee reference with the executing
Realm's once-published EvalFunction identity after argument evaluation. A
non-String first argument passes through unchanged; String dispatch compares
exact GC UTF-16 contents against the precompiled source plan. The prepared entry
receives rooted lexical/variable/private environments, a typed execution context,
and whole this/NewTarget values. Return-position handling belongs to the caller's
completion owner. Deferred SyntaxError and executed whole Throw outcomes restore
the caller Realm before propagation. A derived eval context retains all four
existing this/status/NewTarget/active-function cells together; retained arrows
observe super() updates through those same cells. Environment.DIRECT_EVAL_CONTEXT
retains the context without exposing a JavaScript binding value. Unprepared
dynamic source reports the explicit AOT unsupported case.

Prepared Function-family constructors coerce the acquired argument vector in
order, select the precompiled source outcome, then observe NewTarget.prototype.
Deferred SyntaxError does not perform that prototype Get. Allocation consumes
the constructor's defining Realm and global Environment, a null private
environment, the selected internal prototype and a fresh template owner.
Ordinary and generator-family own prototype objects are completed in that Realm
before the function becomes visible. Empty ordinary bodies have a JavaScript
entry origin even when they reuse Function.prototype's compiled empty body.

Promise diagnostic output consumes rooted UTF-16 Strings. The scalar print
boundary receives checked transient UTF-8 bytes, joins valid surrogate pairs,
and substitutes U+FFFD for lone surrogates at host output. It never receives a
JavaScript reference encoded as an address.

Native errors use actual GC ErrorData instances with their supplied prototype.
They inherit name from that prototype and define own message only when supplied.
The original whole throw remains in the Completion record through diagnostics;
noncalling diagnostic reads do not invoke accessors or Proxy traps.

Remaining source work includes unported helper bodies, resumable retirement,
remaining dynamic-constructor paths, Realm/bootstrap callers,
and other semantic families in the atomic packet. Pinned Wasmtime 47's missing
weak/ephemeron capability remains an explicit external gap. No parallel manual
semantic heap or false weak closure is accepted.

Executable acceptance requires the 4096 MiB aggregate kernel cgroup limit with
swap disabled, verified limit readback and inherited single-CPU/serial execution.
The kernel launcher is still unverified during source authoring. An unavailable
manager or limit refuses the payload; no uncapped fallback is allowed.
