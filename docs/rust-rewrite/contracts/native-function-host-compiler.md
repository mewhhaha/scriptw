# Native Function compiler host boundary

This is the native-host part of the later ABI6 same-Store compiler batch. It is
unapplied and cannot be merged separately from ProgramInstance ownership, typed
entry/call transport, the runtime-function emitter/factory, constructor miss
handling, and execution-owned compile policy. It does not establish complete
dynamic-source support or a conformance count.

## Request and result ownership

Wasm performs the actual constructor's argument processing and ordered ToString
before calling `GcHostImport::CompileDynamicFunction`. The request contains the
closed constructor kind, original normalized newTarget value, actual converted
source ValueArray, actual active constructor FunctionContext, and nullable caller
environment. Host decoding performs no JavaScript conversion or property access.
It preserves every UTF16 code unit, including NUL and unpaired surrogates, through
`wasm_gc_completion::string_units` and the UTF16 runtime-function IR entry.

The seven input words are:

1. kind I32;
2. newTarget tag I32;
3. newTarget scalar I64;
4. newTarget nullable eqref;
5. nonnull ValueArray;
6. nonnull FunctionContext;
7. nullable Environment.

The six output words are the existing Completion five-word ABI followed by a
nullable StringValue syntax message. A null message means the exact factory
Completion, including abrupt value/reference/kind/target, is returned unchanged.
A nonnull message means a genuine syntax rejection; the inactive Completion is
Undefined/Normal with zero scalar/target and null payload. Wasm constructs the
SyntaxError using the actual constructor's intrinsic Realm/prototype. The host
never creates a replacement Realm or a JavaScript Error from source text.

## Declaration-owned GC projections

`GcHostLayout`, `GcHostField`, and `GcHostStorage` project the authoritative AOT
layout declarations. No numeric GC field index is duplicated in the Engine.
The host validates the import's consumed field counts, storage, mutability,
nullability, and closed scalar signature. Source arrays/StoredValue/String roots
must have the actual canonical types obtained from that admitted R import.
Factory and initializer signatures must use the same canonical ProgramInstance
and semantic types. Actual Wasm typed linking remains the authority for the
complete recursive runtime graph; these consumed projections are not a new full
schema verifier for arbitrary forged modules.

A private RootScope owns request traversal, initializer output and factory
results. Only the outgoing message or Completion reference is promoted through
OwnedRooted and re-rooted in Caller result scope. Temporary source and initializer
roots are not retained in State. The returned function's existing source context
must retain its defining ProgramInstance; that record retains P-owned roots,
metadata and hooks independently of the calling Realm/environment.

## Native compilation and execution

`NativeFunctionCompilerContext` contains the actual already-linked R Linker for
this Store, exact Arc<RuntimeArtifact>, exact IntlCompilationProfile and
HostSurfacePolicy, and the shared original execution-budget authority. Context
construction requires these values explicitly. Standalone/test stores may hold
None; a runtime compiler import invoked without context is an explicit host
failure. Product linked execution must install the context before its first P
main. No builtin-reachability guess, ambient Realm/default profile, or source
catalogue supplies authority.

The callback clones context in a short Caller.data borrow and releases that borrow
before decoding, parsing, lowering, emission, native compilation, instantiation,
or Wasm invocation. It uses `lower_runtime_function` on the actual converted
UTF16 source tuple. The emitter returns a fresh P artifact against the exact
actual R key and raw bytes; both are checked before native compilation.

Only P bytes reach the existing bounded `compile_wasm_module` worker pool. The
callback does not compile R, create an Engine, create a Store, create a Realm,
replace the global environment, or run P main. A module start section is rejected
before instantiation. The existing R-linked Linker instantiates P in the same
Caller/Store. It calls the declared pure-GC program initializer, then the declared
factory with ProgramInstance plus the unchanged seven request operands. The
factory must report matched=1 for its validated requested root. A missing export,
incompatible canonical type, unmatched row, or invalid owner is an internal host
failure.

The factory is the real PreparedDynamicFunction path: it preserves normal
GetPrototypeFromConstructor effects/abrupts, actual constructor Realm/global
environment, source function allocation and protocol. Private/eval/module/job
ownership still belongs to the joined ABI6 consumers; this host component does
not substitute a calling Realm for a defining program owner.

## Error and resource boundaries

Parser `MalformedJavaScript` is a syntax rejection. Parser
`UnsupportedParserFeature` remains the original typed EngineError diagnostic.
Only a coded lowering diagnostic whose native error type is SyntaxError can
produce a syntax message. Unsupported lowering, invalid compiler output,
emission rejection, native compilation/linking failure, GC/native allocation
failure, and missing execution authority remain explicit host failures. No
message-text classifier changes their origin or turns them into SyntaxError.

The existing execution control is shared by Arc, armed once at the original
execution-start instant, and checked before and after compiler phases and during
source traversal. A native callback never arms a new clock, requests or cancels
a Store interrupt, resets the original deadline, or enlarges Store/resource limits.
The native compile itself uses the existing compilation pool; an elapsed budget
is reported before proceeding to subsequent phases. The existing
[execution deadline](execution-deadlines.md) continues to govern called Wasm.

No module/instance/source cache or unbounded retention vector is added. A native
Module handle is local to one request; required live instances and code follow
Wasmtime Store/GC/function-reference lifetime. The same Store resource policy
continues to apply. This minimum semantic path does not establish acceptable
performance for tens of thousands of distinct source tuples. Reuse would require
proof of complete post-lowering code and semantic identity plus request-owned
immutable data, not an AST-shape cache or a fixture-family branch.

## Verification boundary

The component includes focused unit controls for error-origin separation, closed
kind/signature admission, actual UTF16 source decoding, expiration authority, and
outgoing GC root survival. Their isolated consumed-field fixtures are not an
admitted emitted R graph. They do not stand in for actual R/P typed linking.

No Cargo, Rust type checking, native GC tests or JavaScript execution was run for
this component. Joined validation must cover the real same-Store transport and
old-P/P2 hook-conflict regressions, runtime Function construction/Realm/effect
ordering, ABI/cache admission, and unchanged prepared-source behavior before any
product capability or conformance claim.
