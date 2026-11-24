# ProgramInstance code and data ownership

This is an UNAPPLIED, non-mergeable ABI6 component against frozen
`eb1ca63c9dbf09148703ccfbd0ca35d9629753e7`, using the ABI5 layouts and runtime
version from `next-coherent-repair-batch-eb1ca63c9-20261015-v3/candidate`.
It belongs after the separate two-Bug/eight-NotImplemented repair checkpoint.
Only the central schema and construction/reference APIs are authored here.
Allocation, entry ABI, hooks, literal initialization, metadata readers and
continuation integration remain required. No Cargo or native verification was
run. Do not merge this component as an unused API or a working dynamic-AOT path.

## Actual regression and evidence boundary

The existing ABI4 native transport diagnostic now proves that one Store can
instantiate a second genuine compiled P during a host callback, return an actual
typed GC function, force GC, invoke it to produce 42, and resume outer P at 17.
It admits the existing exact raw runtime package and hits its existing native
bundle without compiling R. The ordinary transport control exits 0 and the
old P still reports 17. This establishes that transport foundation only.

The same binary's conflict control exits 101: baseline old P reports 17, the
reentrant function reports 42 and outer P resumes 17, but after P2 main the old
P's prepared Function context returns 23 instead of 17. P2 retains a prepared
`Function('return 23')` path. This is an actual retained-P ownership corruption,
not merely an inferred hazard or a failure to transport a GC reference.

The exact owners are:

- `target/dynamic-aot-transport-eb1ca63c9-20261015-transport-v5-run/receipt.json`,
  SHA256 `5db16fbd6fa5e11a2aabc341fafaf4ba59d36c9232d3598d8761c9f471ce0b1b`.
- `target/dynamic-aot-transport-eb1ca63c9-20261015-conflict-v5-run/receipt.json`,
  SHA256 `0b24fd1cc5afdadea3dea2748b43be20d4b29608928d6d1b1461d423a7874497`.
- Diagnostic source `/tmp/scriptw-dynamic-aot-transport-eb-20261015-v2/probe.rs`,
  SHA256 `9a7b3a75e7607e9f9f4a89f8e63520e41aac0b25e73d66f275b3a1b84a014a41`.

Those target paths are under `/workspace/porffor`. The scoped scratch host
supplies the exact empty weak-finish/next-registry and real elapsed-clock
preconditions; unknown host calls trap. These observations do not prove general
dynamic code generation, a complete host surface, or this proposed ABI6 repair.

## Central interfaces

`ProgramInstance` is one internal GC record per physical P instance. Its
immutable fields retain six separately typed nullable hook references (the
sixth, LITERAL_RESOLVE since ABI 89, publishes a P literal at its first use), a
P-only PooledStringTable, its LITERAL_DESCRIPTORS rows, its compiler-prefix
ordinal base/count, and an existing ImmutableByteArray containing P metadata. An absent hook retains the existing
unprepared behavior; a present hook's type comes from its RuntimeHelperId row.
The helper signature is not duplicated in StaticSignature. No JavaScript value
tag, object-header projection, parallel object model or global selector is added.
The closed GcEntryFunctionRole keeps the existing Completion-returning entry
APIs unavailable to hook roles whose registered result shape differs. Nullable
FunctionStackReference supplies an explicit consuming null test; installed hook
calls must still use their own registered HelperArguments/result path.

`ProgramInstanceImage::from_program_pool` requires an actual ModuleKind::Program
and checks that pool against its imported R boundary. `ProgramLiteralIndex`
admits only ordinals in that P suffix. `ProgramLiteralRoots::unresolved`
allocates the P-only table of the exact literal count with every slot null and
takes the descriptor rows; only the P's resolver hook publishes a slot, at the
literal's first use. The producer must not retain a mutable alias.
`ProgramMetadataBytes::from_program_segment` uses ArrayNewData on the existing
passive P metadata segment into ImmutableByteArray. It never copies P metadata
to a shared linear static address or resets the transient byte allocator.
Copying bytes alone does not relocate their currently embedded absolute pointers.

`ProgramInstanceHooks::from_program` consumes the existing ProvidedHooks decision
and helper function-index plan. Each nullable reference has its own closed hook
role. `ProgramInstanceLocals::publish` rejects resources from different compile
images, constructs the record and transfers roots before clearing construction
locals. `hook`, `literal` and `metadata` load through that exact explicit owner.
The generic hook result is nullable: integration must retain the existing
installed/absent branch before calling it with its registered HelperArguments.

FunctionContext appends `DEFINING_PROGRAM`. Required allocation input
`FunctionProgramOwner::Defining(P)` encodes a JavaScript function's defining
physical P; `InheritCaller` encodes an R-native builtin. `capture_operand` checks
that choice against ExecutableCodeKind, rejecting source functions with no
owner or native functions assigned a defining P. R-emitted empty JavaScript
function bodies still use Defining(calling P) when created; being emitted by R
does not turn their ECMAScript code into a native builtin.

`ProgramInstanceLocals::for_function_entry` resolves a captured defining P or
inherits the explicit calling-P operand for native code. The result is a strong
frame-local root. FunctionContext's Realm, lexical/private environments, home
object and other captures remain independent ECMAScript state. A foreign-Realm
native Function constructor inherits calling P while using its actual defining
Realm for the created function and errors; a JavaScript function selects its
own defining P even when invoked from another P. Realm is never a program key.

## Required entry and lifetime integration

All product entry paths must carry an explicit effective program operand.
Returning or throwing restores the caller by ordinary Wasm frame ownership;
there is no mutable current-P global to swap, save or restore. Proper tail calls
forward the selected owner as an ABI operand. Host reentry roots outer P while
child P constructs its own record. P-owned function references and GC records
retain child module state after the host callback and after GC.

| Boundary | Required program policy | Existing semantic state |
| --- | --- | --- |
| Script/P main | Construct P resources once, publish before source function allocation and entry | Actual Script environment/Realm remain separate |
| Ordinary source call/construct and class/field/static initializer | Use callee FunctionContext defining P | Preserve actual this/new.target, Realm, lexical/private captures |
| Standard/host native call, including foreign Function constructors | Inherit explicit calling P on every invocation | Native defining Realm still controls intrinsic creation/errors |
| call/apply/Reflect.apply, bound and Proxy dispatch | Forward caller owner until actual source/native target selects its entry owner | Preserve target invocation, revoked-proxy checks and Realm rules |
| Direct eval / prepared Scripts | Code in the same P keeps that P; a separately compiled eval P owns its own code/data | Preserve caller lexical/variable/private context, strictness and eval Realm |
| Indirect eval / ShadowRealm evaluate/import | Native dispatcher receives effective calling P for its catalogue; separately compiled result owns new P | Target evaluation/module Realm stays explicit |
| Generator/async/async-generator suspension and resumption | Retain source defining owner in the activation's captured context; restore explicit local on resume | Preserve state/resume mode, environments, sent values and completion |
| Pending jobs, promise/module reactions and native callback closures | Retain the effective scheduling P explicitly where no source FunctionContext owns it | Job/handler Realm and actual callback identity remain independent |
| Modules, TLA and dynamic import | Module code/graph and body-reaction hook retain the graph's actual P | Per-module Realm, environment, instantiation guards and loader policy stay explicit |
| Private/class/ShadowRealm wrappers | Actual source/target context retains defining P; native wrapper inheritance never replaces that capture | Private names/environments and wrapped target identities are not keyed by P or Realm substitutes |

No nullable fallback may guess the last instantiated P. Host-only bootstrap
before any source P needs a distinct typed bootstrap path; it may initialize
R intrinsics but cannot invoke P hooks or allocate ownerless JavaScript code.
Standalone emission likewise needs an explicit logical P owner after its R
bootstrap. This component's image constructor accepts only a linked P; the
standalone construction authority is still an integration obligation.
Resumption/job entry with no live caller must use the retained source/scheduling
owner, rather than inventing a caller Realm or consulting a mutable global.

## Static-data migration owners

Keeping the old linear reads while adding this record is insufficient. Runtime
compiler constants remain in R's stable prefix. All P suffix references become
owner-relative GC spans/ordinals with validated ranges. Persistent P bytes must
not point into transient wire buffers. Preserve evaluation order and live reads.

| Current owner | Required migration |
| --- | --- |
| `gc_types/value.rs` pooled-string roots and `emit/body_entry/literal_roots{,/program_strings}.rs` | R initializes only its compiler literals; P fills a private P-only table, using relative ordinal after the R prefix. Every source literal load takes effective P. Retain real passive UTF-16 bytes and existing surrogate handling. |
| `program_hooks.rs`, `runtime_helpers.rs`, `gc_types.rs` | Remove shared ProgramHookGlobals and their get/set route. Read typed field from explicit P, branch on null, then call the same registered helper ABI. Hook helper signatures include/retain the actual P operand needed by generated code. |
| `emit/body_compilation.rs` program-static initialization | Remove P MemoryInit into R's shared suffix and P's PRIVATE_BYTE_CURSOR reset. Root GC metadata before any source entry; retain ordinary R-only allocation/host wire policy. |
| `data.rs` StringPool / RegExpProgramRef / RuntimeRegExpProgramTable | Encode P-relative immutable descriptor spans and source/flags ordinals. RegExp candidate selection, syntax/error outcomes and program-byte ownership use the same P. Never reinterpret an old packed absolute pointer as a GC offset. |
| `expressions/regexp_program{,/candidate}.rs` and RegExp descriptor/case-fold readers | Read P metadata through owner; retain genuine R constant domains separately and copy compiled descriptor bytes to their existing published GC program owner. Preserve exact-source admission and all syntax/unsupported outcomes. |
| `call_rows.rs` NumericCallTables / NumericCallTable | Replace absolute table cursor/linear i32/i64 loads with bounded P metadata reads. Keep per-row live GetValue, receiver/callee lookup and argument evaluation/coercion ordering. |
| `data.rs::PooledModuleBytes` and `builtins/binary_data/module_value.rs` | Read original raw Bytes Module data from its defining P's GC metadata span; retain exact bytes and actual module Realm allocation. |
| `data/compact_array_recipes.rs`, `gc_types/value/compact_array_recipes.rs` | Keep passive recipe construction scoped to the P instance and publish the existing GC recipe/array owners; no P-static-address fallback. |
| Source error strings, template/function source, class/module metadata and every remaining `StringPool.bytes` consumer | Classify each reference as stable R or actual P. Retain P roots/relative spans across callbacks, stored functions and continuations. Local source/module IDs are meaningful with their defining P, not as a cross-P global identity. |

## Integration and ABI migration checklist

1. Finish the separate ABI5 repair checkpoint before applying this component.
   Its added legacy-reflection fields are retained byte-for-byte as the base.
2. Add required FunctionProgramOwner input at the sole FunctionContext allocator
   in `functions/function_object_allocation.rs` and update every source/native
   producer. The new field intentionally leaves that old constructor incomplete
   in this isolated packet; the packet is not independently buildable/mergeable.
3. Thread calling/effective P through `module/static_signature.rs`, function
   entry planning/acquisition, call/construct/forwarding/tail dispatch,
   FunctionBuilder, prepared Scripts and all P-dependent runtime-helper rows.
   Context extraction alone cannot serve a native context whose capture is null.
4. Integrate per-P construction in module assembly/body entry and retain the
   physical P instance/root through host returns and stored exported callables.
   Retain declarative ref.func references for the exact provided hook functions.
5. Convert every hook and static-data consumer in the ownership tables. Remove
   shared hook installation, R+P pooled-string publication and shared suffix
   MemoryInit/allocator reset together; do not make global save/restore a bridge.
6. Complete activation/job/module/native-closure owner retention. Update host
   P entry/return transport with typed ProgramInstance references, without
   creating another object representation or using the execution Realm as owner.
7. ABI6 invalidates R/P keys, raw packages, source fingerprints and generated
   native images. Regenerate exact current R/P core graphs and helper signatures;
   replace hook-global census assertions with actual typed record/signature
   assertions. Keep host completion layouts and native configuration versions
   unchanged unless their own physical formats/configuration change.
   Extend `scripts/check-module-boundaries.sh` for the new private owner and
   actual integration consumers; this packet does not claim that full guard.
8. Reject genuine preserved ABI4/5 package/P inputs at current admission and
   run actual incompatible typed-link controls. Header-label changes are not
   complete schema validation. Preserve actual Wasm bytes and identities.
9. Compile the coherent complete batch once, then run native semantic controls
   sequentially. None of the component's formatting/source checks prove these.

Required acceptance starts with the exact transport and conflict source owner
above: transport stays passing and conflict must preserve old P at 17 after P2
main. Add two independently instantiated Ps with deliberately colliding literal
ordinals, source IDs and RegExp/numeric/Bytes Module metadata offsets. Retain
functions/jobs/async resumptions from both across GC and reverse call order.
Cover absent hooks, source versus foreign native constructors, direct/indirect
eval catalogue isolation, abrupt/initializer/tail exits, private closures,
modules/TLA reactions and a host callback that instantiates and reenters P2
before P1 resumes. Existing Realm, ordinary invocation, async/generator,
reflection, snapshot, cache and exact-source admission suites remain required.
The root integration owner supplies those executable controls; this component
does not claim that the diagnosed corruption is fixed.
