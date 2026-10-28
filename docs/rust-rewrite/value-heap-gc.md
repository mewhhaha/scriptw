# Value, heap and garbage-collection architecture

## Current atomic source — 2026-10-05

The source draft now carries complete tag/scalar/reference values, completion
records, callable contexts, captures, environments, frames, jobs and host
boundaries through the sole typed GC schema. Its recursive declarations and
consumed codecs own the actual records and arrays. The final heap cleanup
retires the old passive byte-layout/root/weak inventories and representation
mirrors; scalar domains and checked private byte transport remain.

Temporal production source is sealed. Compiler-control and source-guard
composition have finished independent source review. The complete source
checkpoint includes types, meaningful controls and documentation. The complete
source and integration repairs pass the whole-workspace, all-feature, all-target
Rust type check on 2026-10-05 under the confirmed 4 GiB aggregate cap. Authored
Rust controls typecheck; source guards, emitted Wasm validation and semantic
execution remain pending under the verification ladder.
The selected weak/ephemeron facility remains explicitly unavailable, without a
strong substitute or another object model. See the current
[record architecture](../../crates/scriptw-aot-wasm/docs/heap-layout.md) and
[development workflow](../../CONTRIBUTING.md#verification).

## Historical architecture and implementation checkpoints

The dated descriptions below preserve earlier source and evidence. Their
scalar ABI, linear allocations, passive inventories and remaining migration
claims describe those predecessors; they do not override the current source
scope above or provide execution evidence for the atomic GC batch.

## Callable body and invocation roles — 2026-10-04 dry source

The actual function planner now owns callable origin, the existing source
protocol and both function-index spaces together. User, native and prepared
Script origins project their permitted body role from that one owner. The
builder retains the planned entry while emitting the body; a completed body
carries it into the joint declaration/code publisher. A runtime helper remains
an existing closed helper identity, rather than acquiring callable parameter
access merely because it returns multiple values.

Private ordinary, generator, async, async-generator and prepared-Script input
bundles drive the actual direct/indirect calls. Ordinary inputs contain semantic
this/new.target; each suspended family accepts only its own activation input.
Prepared Script inputs retain the complete lexical/variable/private environment
and direct-eval context lifecycle. The scalar encoder preserves the existing
seven- or ten-word shape and seventeen static signature ordinals. Internal
activation inputs cannot be passed as semantic new.target through these bundles.

Dynamic dispatch loads environment, family flags and table identity from one
validated Function object. A private branch encloses the matching role projection
and actual call. Resume calls consume the saved record's family-specific fields.
New.target first follows retained direct-eval or Arrow lexical ownership; an
activation-backed generator, async or async-generator body otherwise evaluates
it as undefined. Async Arrows therefore retain their ordinary lexical owner's
new.target across await rather than reading an activation word.

Paired strict/sloppy semantic controls cover ordinary calls/construction,
alternate newTarget, all suspended families, escaped lexical/direct-eval Arrows
and prepared created-Realm entries. Only source inspection and Rust formatting
are complete. Compilation, Wasm validation, execution and broad verification
remain pending. This joins the current scalar ABI's semantic role authority;
it does not move values, callable references, allocations, completions, frames,
jobs or host roots/decoding to Wasm GC. The atomic semantic migration and full
T05 acceptance remain open.

## Function declarations follow actual bodies — 2026-10-04 dry source

ModuleCode now privately owns both FunctionSection and CodeSection. Its sole
push consumes an actual EmittedFunction, derives the declaration from that
body's closed identity, and appends declaration, body and attribution together.
Prepared Script bodies carry a distinct identity with the existing ten-parameter
signature; ordinary user/builtin/host bodies retain their seven-parameter shape
and runtime helpers retain their existing exhaustive signature authority.
All seventeen static type ordinals and shapes are unchanged. Prepared Script
names and the script report category are preserved.

The final module package consumes both sections from this one body owner.
Assembly can no longer accept an independent function section. The positional
prepared-body index-range inference and second declaration list are removed,
along with the count assertion that checked those independent lists. Actual
main/source/prepared/builtin/shared-stub/host/helper body order and conditional
helper selection remain unchanged. Existing package guards and unit consumers
are maintained around the new joint lifecycle; no new test groups are added.

This earlier checkpoint closed the consumed declaration/body authority. It
left the seven-I64 semantic context/activation roles for the newer callable-role
source above. Migration of callable/value/completion, frame/job, host roots and
decoding to GC references remains open. Current product semantic
allocations remain linear. Compilation, Wasm validation, focused runtime and
complete phase3/atomic migration acceptance remain pending.

## Direct-eval context and retained capture edge — 2026-10-04 dry source

The central registry now includes the actual direct-eval invocation context
and its all-present derived-binding group, raising the declaration count from
149 to 151 while preserving every existing type and field ordinal. A completed
context retains its nullable FunctionContext, home-object StoredValue, optional
group and mutable this/new.target snapshots. The group holds the four original
this/status/new.target/active-function BindingCells together, preserving the
actual aliasing instead of independently nullable or copied cells.

Environment gains a nullable mutable DIRECT_EVAL_CONTEXT strong edge. During
the atomic migration, a direct PreparedScript root publishes incoming context
parameter9 after its Environment allocation; escaping arrows read the exact
owner selected by the existing lexical capture hops. Only all-Arrow ancestry
inherits this caller context. A nearest-nonnull ancestor search would cross an
ordinary function boundary and is forbidden. Nested direct-eval roots reuse
the context identity; arrows read without republishing. The snapshots refresh
on each reused invocation and derived super() observes the original shared cells.

These declarations describe the existing 88-byte record and retained lexical
capture lifecycle. The recursive encoder and generated typed field accessors
consume them through the existing sealed registry. Actual product allocations,
capture reads and callable parameters still use the current linear ABI: the
internal Object-tagged hidden binding must become this typed edge during the
atomic switch, never a StoredValue containing an internal context address.
No early semantic GC allocation, integer/reference bridge, copied object model
or mirrored declaration test is added. Compilation, runtime, complete phase2/3
coverage and the semantic cutover remain open.

## Function-body environment edge — 2026-10-04 dry source

The Environment schema now includes the actual parameter environment's strong
FUNCTION_BODY link. Existing environment creation initializes that link to
absent, resumable preparation publishes the body environment later, and resume
entry traverses parameter/parent records until it finds the retained body. The
central registry declares it as a nullable mutable reference to Environment,
generating concrete recursive reference storage and owner-specific accessors.
The InvocationFrame's own environment fields do not replace this traversed edge.

This corrects one concrete phase2 declaration omission within the existing
149-owner registry. It adds no semantic allocation, integer/reference bridge,
object model, runtime fallback or new test that mirrors the declaration.
Existing declaration checks will run at the combined checkpoint. Compilation,
runtime, full phase2/3 coverage and the atomic semantic switch remain open.

## Current dry declaration foundation — 2026-10-03

The implementation-first batch adds central GC struct/array declarations,
recursive membership, typed indices and accessors consumed by the actual
product type registry. The existing anchor/holder construction and reads use
that same declaration authority. Function registration is a consumed
singleton prefix; recursive registration freezes the type section. One
opaque owner retains frozen types and matching rooted globals through main
compilation and final assembly, exposing only immutable encoding views.

The current host runtime-tag authority also derives its concrete enum, raw-tag
admission and semantic-kind projection from one row domain. Both structured
observation and human completion rendering consume it exhaustively. All twelve
valid wire representations are preserved; compiler-only Dynamic cannot be
constructed as a runtime tag. This closes the active host decoder domain and
does not migrate the semantic value/call/completion/root ABI to GC references.
Compilation and execution remain pending.

Declared families include values, objects/properties, functions/environments,
primitive records, indexed data/buffers, suspended execution/jobs,
Realms/modules, collections, disposal, template caches and host resources.
The consumed registry now has 151 owners, including explicit current Intl and
Temporal configurations, RegExp program bytes, binary views, collection cursors,
private names/elements, Promise/job payloads, module records, suspended disposal
and Atomics waiters. Actual construction lifecycles select nullable mutable
initial links; the FunctionContext active-function back edge is backpatched.
Current typed-array iterators retain their owner after completion.
Helper iterator and function protocol declarations retain concrete native
closure captures. Mutable Promise capability executor state is separate from
completed capability publication; async disposal retains its nullable function
Realm context according to the actual producers.

They are declarations, with no early semantic GC allocation. Protocol codecs,
capture validity, state/value constructors, completed capability publication,
callable/value ABIs,
host roots and the atomic semantic migration remain open. JavaScript objects
still use the linear heap, `gc()` is unsupported and weak reachability is
unavailable. Validation regression sources are authored but unexecuted;
compilation and runtime checks remain deferred. Earlier evidence below
retains its original source scope.

## Actual weak capability enforcement — 2026-10-03 dry source

The actual mixed ABI audit confirms that current semantic objects, references,
environments, callable arguments/returns, completions, Realms, suspended frames,
jobs/modules and host decoding still use the linear identity ABI. Its complete
producer/consumer and root migration must remain atomic; an isolated object
family conversion would require the forbidden integer/reference bridge.

The selected runtime's `WasmWeakReachabilityCapability::Unavailable` now also
governs actual weak builtin dispatch. It rejects a successful weak construction
or storage route through the typed, uncatchable unavailable-capability reason
before any weak record, target retention, ephemeron or registry cell is exposed.
Ordinary argument/prototype errors and installed intrinsic surfaces retain
their existing owners. Strong-retaining weak producers are deleted, with no
strong GC-reference substitute. Passive layouts remain inventory only.
Compilation, emitted Wasm and semantic controls remain unverified. This is a
truthful boundary correction, not a weak facility or semantic GC switch; see
[the contract](contracts/weak-unavailable-runtime-boundary.md).

## Current candidate verification

Wasmtime 47.0.0 and Rust 1.94.0 are now covered by an actual complete locked
dependency admission, verified registry archives and fresh current-tree
offline metadata. The default CLI closure excludes scriptw-spec-exec. Both real
Fast and SizeOptimized product engines pass the startup policy witness with
a 32 MiB maximum Wasm stack and 64 MiB async/worker budgets. The explicit
Copying collector and required Wasm features remain checked. Both engines
also enable host shared-memory support and pass a real one-page allocation.
JavaScript
semantic records still use the unreclaimed linear heap; gc() is unsupported
and weak reachability is Unavailable. T05/T21 migration criteria remain open.

The candidate continuation revalidates 84 focused stages with 1,856 selected Rust test invocations on the exact same Source. Compilation,
one separate both-engine startup invocation and the default-features CLI
build belong to the original focused run. The continuation freezes that
CLI unchanged and executes all 151 selected pinned modes from 82 files.
The original pin-identity validation failure remains recorded.

This is candidate verification. MAIN installation and a fresh complete
MAIN broad checkpoint remain required. The earlier session 30300 is
INCOMPLETE without an owned terminal; its exit and cause remain unknown.
Full pinned Test262 conformance and task acceptance remain open. The
published status span is unchanged. The authentic continuation terminal is `bb22495c5187c67c269adeffd0e8efd91c3cbe97be18a79fb86c263945bc2798`; its Root-owned exit is `b399f9e5c16d0847ff4854a885e40c1b0b55f5428791d0186919db6dc64d0b1a`. The revalidated same-Source prefix is `7d0ce67ed3ce18e2646639461ba9eeb7f5d4e0793275b0937cf687ec96bf5bc9`; its original enclosing Root1 is `12a3deab22796d658bebdce50eaf263cf2a1443b1f03f0269dcdba951f9c77c1`.

The preparation and dated verification statements below retain their
original scope and failures. This checkpoint supersedes only the
unexecuted state of the named selected controls described above.


This document is the source of truth for T05. It describes the object model
Scriptw is moving to; it does not describe the current linear heap as complete.
The migration must preserve one product object model at every commit.

## Ground truth

The current Wasm-AOT path represents a JavaScript value as integer payload/tag
parts. Identity-bearing values are integer addresses into a bump-allocated
linear-memory heap. `heap.rs` contains extensive layout, root, weak-edge and
collector tables, but those tables do not drive an executable collector. The
current `gc()` path is unsupported. Former active weak-reference records held
ordinary strong integer addresses; those producers are now retired and actual
weak operations enforce the unavailable capability. Remaining passive layout
and edge declarations are inventory, not proof of GC or weak semantics.

The isolated runtime foundation proposal requires exactly Wasmtime 47.0.0
and Rust 1.94.0. Every product engine consumes one
`WasmtimeRuntimePolicy`: reference types, typed function references, Wasm GC
and exception handling are required explicitly, and
`Collector::Copying` is selected directly. Its dependency request includes
`gc-copying` and excludes `gc-drc` and `gc-null`; neither automatic collector
selection nor a non-collecting fallback satisfies the policy. Engine creation
must reject unsupported features, and the shared factory validates the actual
engine's collector and required features before caching either native compiler
profile. The native size-optimized retry retains this same runtime policy.

At foundation proposal preparation, the full product 47 dependency lock was
pending a Root-owned offline Cargo
resolution in an isolated workspace. The retained 38 lock has not been renamed
or hand-edited. The cache 47 crate archive is absent from the checked local
probe closure. A separate Root read of the official tagged source confirms
the existing cache API; it does not authenticate that missing archive. Source API checks and authored regressions are not a
product build or runtime pass. Those preparation facts predate the current candidate dependency and runtime
receipts above; MAIN runtime admission is still a separate gate.

The tagged 47 collector documentation still includes an obsolete-looking
“not yet functional” qualification. Genuine separately recorded standalone
WAT probes exercised typed cyclic reclamation, rooted preservation and OOM
recovery under an actual 1 MiB heap capacity; the denied 2 MiB growth request did
not raise that capacity. Those results validate the isolated capability probe
only. They do not establish product 47 readiness, JavaScript semantic roots,
reclamation of the current linear heap, or weak reachability.

`WasmWeakReachabilityCapability::Unavailable` remains independent of strong
cycle collection: the selected runtime exposes no required weak-reference or
ephemeron interface for the product. Both capabilities flow through the same
policy reporting and typed engine-setup error context. JavaScript objects,
closures, suspended frames, jobs and completions still use the existing linear
representation; `gc()` remains unsupported. The complete semantic producer,
consumer and root migration must be atomic, with no second object model.

Primary sources are the tagged 47 public policy and GC APIs; the local cached
copies and genuine standalone receipts are bound by the foundation overlay:

- <https://github.com/bytecodealliance/wasmtime/blob/v47.0.0/crates/wasmtime/src/config.rs>
- <https://github.com/bytecodealliance/wasmtime/blob/v47.0.0/crates/wasmtime/src/runtime/store/gc.rs>
- <https://github.com/WebAssembly/gc/blob/main/proposals/gc/Overview.md>

## Non-negotiable invariants

1. Every identity-bearing ECMAScript object is a Wasm-GC reference. No object,
   environment, property table or closure is represented sometimes by a GC
   reference and sometimes by a linear-memory integer handle.
2. A GC reference is never cast to, packed into, or recovered from an integer.
   The Wasm type checker and Rust schema must retain the distinction through
   locals, fields, globals, calls, returns, exceptions and host transitions.
3. Linear memory contains bytes, limbs and transient host-I/O buffers only. A
   linear address is not an object identity and cannot participate in the
   JavaScript reference graph.
4. Every dynamic linear span has exactly one statically named GC owner and no
   owning aliases. Interior views borrow an owner plus a checked range; they do
   not own the backing allocation.
5. Strong GC fields, weak edges and external resources are different domains.
   A missing weak-reference facility cannot be approximated with a strong
   `GcRef`, and a host resource handle cannot become a second object model.
6. All roots are real Wasm references in typed locals, fields, globals, tables,
   exception payloads or host rooting scopes. A parallel integer root registry
   is not part of the target architecture.
7. A module that requires this ABI fails at the runtime boundary when Wasm GC
   or the required collector capability is absent. There is no non-GC backend.

These invariants imply an atomic semantic cutover. Gradually teaching a few
builtins to return GC references while the rest consume integer heap handles
would create two object models and require a bridge expressly forbidden by
invariants 1 and 2.

## Value representation

SSA computation uses typed value parts:

- a closed JavaScript value tag;
- scalar bits for `undefined`, `null`, Boolean, Number and small internal
  sentinels; and
- a nullable, typed GC-reference slot for String, BigInt, Symbol, Object and
  internal records.

The active slot is determined by the closed tag. Rust builders must construct
and consume the whole value, so a reference-bearing tag without a reference is
not expressible at an emitter call site. The reference slot remains a Wasm
reference in function signatures and locals; it is not squeezed into today's
`i64` payload ABI.

Stored values use a central GC layout carrying the same tag/scalar/reference
parts. This may box a value when it crosses from SSA into a property,
environment or job record, but it keeps primitive fast paths allocation-free
inside an expression. Layout-specific records may use narrower typed fields
when the ECMAScript specification fixes the field's domain.

The schema vocabulary in `crates/scriptw-aot-wasm/src/gc_types.rs` starts these
compile-time distinctions:

- `GcTypeIndex<T>` prevents indices for different layouts from being swapped;
- `GcField<Owner, Value, Mutability, Nullability>` binds every field ordinal to
  its owner and complete storage contract;
- nullable scalar fields do not type-check;
- `GcRef<T>` is a zero-sized strong-reference schema marker with no integer
  representation; and
- `GcRootGlobal<T>` names only a mutable, nullable Wasm global carrying a
  strong reference to `T`; it cannot name a scalar global or contain a linear
  address; and
- `LinearAddr<Owner>` and validated `LinearSpan<Owner>` cannot be substituted
  for GC references or for another layout's side storage.

The capability-anchor subset of this vocabulary is now wired through the
central type-section registry, not an individual builtin. The remaining
semantic layouts stay schema-only until the atomic object-model cutover.

The schema module is also the sole raw Wasm-GC encoder boundary. Type-index and
field-ordinal construction/extraction, plus typed GC-root construction and
extraction, stay private there. Module assembly first emits every fixed and
dynamic scalar global into one open builder. One consume-once finalization owns
the type registry and that builder: it derives the root slot from the encoded
section's actual length, appends the typed root, and returns an opaque,
non-cloneable package containing the finalized sections and their private
`RuntimeModuleSchema`. There is no planned raw root index or copyable schema for
another caller to recompute, supply or pair with a different section. A
dedicated main-compilation transition consumes that exact package, compiles main
internally against its private lifecycle, and retains the main body in the
package's code-section builder. After the remaining bodies are supplied, the
sealed compiled package has one consuming module-assembly operation; it emits
its type, global and code sections around the other owned core sections in Wasm
section order. No independent main/type/global/code append surface exists, so
two finalized packages cannot be split and recombined through normal assembly.
Function emission cannot extract interchangeable `u32` indices or construct
`struct.new`/`struct.get` instructions itself. The typed accessor boundary pairs
a field with its owner and, for reference fields, with its target type through
Rust generics before the final `wasm_encoder` call.

## Runtime GC anchor

`RuntimeGcAnchor` and `RuntimeGcAnchorHolder` are the first executable schema
types. Neither is a JavaScript object. The anchor's single field is an
immutable, non-nullable `i32` ABI version; the holder's single field is an
immutable, non-null `GcRef<RuntimeGcAnchor>`. The emitter now:

1. appends the anchor and then the holder to the module's type section, retaining
   both typed indices;
2. encodes the holder field through a typed reference-field builder that
   consumes `GcTypeIndex<RuntimeGcAnchor>` and derives its nullability and
   mutability from the `GcField` type;
3. appends one unexported, mutable, nullable `GcRootGlobal<RuntimeGcAnchor>`
   after every pre-existing fixed and dynamic global, so no established global
   index moves;
4. constructs the anchor and holder before any other main instruction,
   traverses the holder field, and stores the recovered reference in that
   typed root;
5. keeps the root live across main initialization, calls, source execution and
   the final job checkpoint; and
6. on every real main return, loads and non-null-checks the root, reads the
   anchor's ABI-version field, traps if it differs from
   `RuntimeGcAnchorSchema::ABI_VERSION`, then clears the root to null.

That sequence makes the Wasm validator and runtime exercise a concrete strong
GC edge, a real Wasm global root and struct construction/field traversal,
without introducing a live semantic object or changing the current heap.
`ModuleTypeRegistry` owns the section and assigns both indices in dependency
order. Consuming the type registry and open global-section builder is the only
way to obtain the finalized runtime package: the same operation binds the root
to the section's actual next index, appends it, and seals the section against
further globals. The private schema is neither `Copy` nor exposed. The only main
compiler input is a closed plan constructed by the emitter; the main-compilation
transition consumes it internally against the package's exact globals and
immediately stores the resulting main in package-owned code. The resulting
compiled package is the only owner of those type, global and code sections, and
a single consume-once append operation emits all three. That package owns raw
declaration, access, lifecycle and assembly as one opaque operation surface, so
a holder field cannot be paired with the anchor type, a type index cannot be
used as a global index, and neither a separately predicted root index/schema nor
a main compiled against another package can drift from the completed global
section.

The holder becomes unreachable as soon as its edge is transferred to the
global. The anchor then remains live only through the global until the shared
main exit verifies and clears it. This is an executable root-lifecycle witness,
not a JavaScript value. It proves neither reclamation nor cyclic collection,
does not establish roots for semantic values in calls, exceptions, suspended
frames or pending jobs, and adds no weak edge. A Wasm trap before the shared
main exit may retain the witness until Store teardown; Store teardown remains
the owner of that exceptional cleanup.

The proposed engine seam uses the closed
`WasmGcCapability::CopyingWithCycleCollection` value for explicit collector
selection, engine validation, trace reporting and typed setup-failure context.
The capability concerns actual Wasm-GC values, including the anchor witness.
It does not satisfy T05's JavaScript cyclic-graph criterion while semantic
objects and their root inventory remain in linear memory. The dependency,
compile and product verification boundaries remain pending for this overlay.

## GC layout families

One central registry will declare all struct/array types and their recursion
groups. The registry, not builtin-local offsets, assigns type indices and field
ordinals. The required families are:

- the stored JavaScript value record;
- ordinary objects, property descriptors and indexed/property tables;
- functions, bound functions, executable code identities and closures;
- declarative/object/module environments and mutable binding cells;
- strings, BigInts and Symbols;
- Arrays, ArrayBuffers, views and typed arrays;
- iterator, generator, async activation, Promise and job records;
- realms and intrinsic tables; and
- host/external resource handles.

Prototype, environment, closure, property-value, pending-job and completion
links are strong typed references. The registry must generate both
`wasm_encoder` field declarations and the typed accessors used by emitters; a
separate descriptive table is not sufficient. Adding a layout without its
field schema, or a field without an exhaustive encoder mapping, must fail
`cargo check`.

## Linear side storage

The default representation for dynamically owned semantic data is a Wasm-GC
array: packed code units, BigInt limbs, property entries and unshared buffer
bytes can then die with their owner without a finalizer protocol. Immutable
compiler data may stay in linear memory for the lifetime of the instance, and
host calls may use checked, call-scoped linear buffers.

Dynamic linear side storage is allowed only after its reclamation mechanism is
real. Wasm GC currently provides no destructor callback for a collected struct,
so merely storing `LinearAddr<Owner>` in a GC object would leak. Until a
cycle-capable runtime also supplies a suitable resource/finalization facility,
the cutover must not emit dynamically owned `LinearSpan<Owner>` values.
`LinearSpan` exists now to make ownership and memory32 bounds explicit for the
remaining static/transient uses, not to claim lifetime integration.

If a future host-owned resource is necessary (for example a shared backing
store), the Wasm-GC object remains the sole JavaScript identity. Its field holds
a typed external-resource reference. The host resource owns bytes only, has no
properties/prototype/environment, and is released by runtime-supported
resource lifetime—not by a second JavaScript heap or an integer handle table.

Host imports may borrow linear memory only for the dynamic extent of the call.
Re-entrancy must establish a host rooting scope for every reference passed out
of Wasm. No host pointer or unrooted Wasmtime reference survives a call.

## Weak reachability

The current WebAssembly GC surface has no weak-reference or ephemeron field.
Consequently:

- `GcRef<T>` always denotes a strong edge;
- WeakMap/WeakSet keys, WeakRef targets and FinalizationRegistry targets cannot
  use it without changing observable reachability;
- DRC's inability to reclaim cycles independently blocks ordinary cyclic
  garbage; and
- the current linear weak-edge tables and records are inventory only.

Correct weak semantics require a runtime capability that can observe the
Wasm-GC graph and provide weak/ephemeron processing, or a Wasm proposal/runtime
extension with equivalent semantics. A host sidecar that merely stores object
IDs cannot learn that a Wasm reference is unreachable, and rooting references
in that sidecar makes them strong. Neither is acceptable.

The eventual facility must support ephemeron fixpoint processing, clearing weak
targets after strong tracing, holding finalizer holdings strongly, treating
unregister tokens according to their specified reachability, and queueing
cleanup jobs without promising when collection occurs. Test262's `gc()` hook
must request a real full collection cycle; it may not clear tables directly or
schedule finalizers deterministically as a substitute.

The engine boundary now encodes that capability as explicitly unavailable.
Weak builtins remain blocked until a real facility is selected and replaces
that variant. This is a truthful unsupported capability, not a silent skip and
not permission to preserve the current strong behavior.

The passive linear-heap weak-edge inventory keeps its retention vocabulary
closed even while that facility is unavailable. `HeapWeakEdgeKind` exhaustively
derives one of three meanings: an edge that does not retain its target, an
ephemeron value retained only when its key is reachable through the fixpoint,
or finalizer holdings retained strongly until cleanup. A slot cannot separately
attach a Boolean strength claim that contradicts its kind. This makes the
future collector obligation precise; it does not make the inventory executable
or give the current linear records weak semantics.

## Atomic cutover plan

Each phase has an invariant gate. Phases 0–3 add no second semantic object path;
phase 4 is the single product-model switch.

### Phase 0 — schema and measured baseline (landed)

- Check in this architecture and the typed schema vocabulary.
- Record the current collector selection, cycle blocker and weak-edge blocker.
- Keep the current emitter and engine behavior unchanged until phase 1.

Gate at the phase-0 boundary: source checks showed no `GcRef` integer payload
and no GC instructions emitted by the new module. Phase 1 has now superseded
the latter condition with the capability anchor below.

### Phase 1 — explicit runtime capability anchor (implementation landed)

- Centralize engine GC configuration and explicitly select the supported
  collector; reject missing Wasm-GC/reference capabilities (landed).
- Encode weak-reference and ephemeron availability independently of the
  collector as `WasmWeakReachabilityCapability::Unavailable` (landed; a real
  facility remains phase 6 work).
- Remove `gc-null` from the product feature graph (landed).
- Emit and traverse the `RuntimeGcAnchorHolder -> RuntimeGcAnchor` strong edge
  through the central type registry, including the anchor ABI assertion
  (landed; runtime-boundary verification remains).
- Transfer that edge into a typed nullable Wasm global before main can call or
  allocate, retain it through the final job checkpoint, and verify/clear it on
  every shared main exit without moving existing global indices (landed;
  runtime-boundary verification remains).
- Keep raw GC type-index and field-ordinal construction/extraction, typed-root
  construction/extraction and struct instructions inside the schema module;
  consuming the type/global builders derives and appends the sole root from the
  actual encoded section length, and main borrows only that exact non-cloneable
  package's opaque lifecycle operations (landed; compile and focused runtime
  verification remain).

Gate: a module containing the anchor/holder/root probe validates and executes
on the pinned lower bound, and fails clearly when GC is disabled. This proves
typed strong-edge and global-root feature wiring, not reclamation, cycle
collection, semantic call/frame/job roots, weak semantics or JavaScript heap
migration.

### Phase 2 — complete generated layout registry

- Declare every layout family, recursion group, field and array element once.
- Generate encoder declarations and typed field accessors from that registry.
- Choose GC arrays for all dynamic data lacking a real side-storage release
  mechanism.

Gate: every planned semantic record has an exhaustive typed schema; no product
emitter consumes it yet, and layout additions cannot omit an encoder mapping.

### Phase 3 — closed value and host ABIs

- Define the scalar/reference value parts and stored-value record.
- Define typed function, completion, exception, global/table and host-call
  signatures.
- Make rooting scopes and external resources explicit at the Rust boundary.

Gate: all producers and consumers have a compile-time migration mapping. There
is no conversion from a linear object address to a GC reference.

### Phase 4 — atomic semantic switch

In one coherent batch, change every JavaScript-value producer and consumer:
script functions, runtime helpers, builtins, objects, environments, realms,
jobs, exceptions, host imports/exports and result decoding. At the same time,
delete linear allocation/layout code for identity-bearing semantic records and
remove their integer root/weak metadata.

Gate: the emitted module contains no semantic-object allocation through
`heap_alloc`; every reference-bearing value is carried as a Wasm reference;
there is no bridge or fallback representation. Static/transient byte allocation
may remain under the side-storage rules above.

### Phase 5 — lifetime and stress closure

- Require a cycle-capable collector at the engine boundary.
- Exercise roots across calls, exceptions, suspended frames, generators,
  promises, realms and host re-entry.
- Stress allocation and release of cyclic graphs under a fixed low limit.
- Verify memory32 boundary behavior for every retained linear region.

Gate: cyclic stress stabilizes rather than growing until Store teardown, and
all T05 strong-root acceptance cases pass.

### Phase 6 — weak capability and finalization

- Replace the typed unavailable capability with the selected runtime
  weak/ephemeron facility.
- Implement weak collections, WeakRef and FinalizationRegistry against it.
- Wire `gc()` to a real collection request and cleanup jobs to the job queue.

Gate: focused real Test262 weak suites pass without direct table clearing,
strong substitutes, deterministic-finalization promises or expected failures.

## Completion boundary

T05 is complete only when phases 1–6 are implemented and verified. The schema
and anchor are foundations. Enabling `wasm_gc`, validating `struct.new`, or
passing acyclic allocation tests alone does not satisfy executable GC, cyclic
collection, side-storage reclamation or weak reachability.

## Segmenter retained graph

The Segmenter source batch uses the current linear-memory object layout. Its
16-byte Segmenter record stores locale; the 32-byte Segments record stores
the Segmenter, original String and private boundary Array; the 24-byte
iterator record stores Segments while retaining scalar cursor/done state.
These integer payload links have pointer metadata; they are not strong
Wasm-GC fields. This graph must migrate in the atomic semantic switch above.
The private partition is inaccessible through mutable public properties.
Fresh `containing` and `next` results slice the retained original UTF-16
string without performing another native segmentation request. Required
runtime capabilities remain the experimental Wasmtime GC lower bound.
Focused ownership and allocation-pressure execution remains pending. See
the [Segmenter contract](contracts/intl-segmenter-wasm.md).


### 2026-10-03 consumed static signature authority proposal

The queued source-only foundation replaces the module's seventeen handwritten
function-type declarations and parallel type-index constants with one closed
`StaticSignature` row domain. Each row supplies its existing ordinal, parameter
shape and result shape; it emits the ordered domain and exhaustive definition.
`ModuleTypeRegistry` registers those definitions before the existing runtime
GC anchor declarations. Main, prepared Script, helper and host-import function
sections and indirect calls consume the same domain. Function parameter counts
are derived from those definitions. All seventeen ordinals and shapes remain
unchanged, including equal shapes registered at distinct ordinals.

This is a consumed phase-3 foundation for the atomic semantic switch. It does
not migrate JavaScript values, closures, completions, suspended frames, jobs or
host decoding to GC references, and it does not reclaim the current linear
semantic heap. T05 and the URI allocation-pressure failure remain open. A new
signature row without a complete shape fails the macro contract, and an ordinal
that disagrees with declaration order fails the constant assertion. The
existing package-ownership and function-state structural assertions retain
their original lifecycle and capability purposes with signature expectations
updated to the new authority.

The proposal is authored against the approved batch-2 composition as a virtual
future base plus unchanged protected MAIN inputs. No actual future Source
manifest exists at preparation, and actual-base binding is deferred. Rustfmt
and source-only checks are preparation evidence; compilation, those existing
Rust tests, focused execution and fresh broad verification remain UNRUN.
There is no inherited compile or runtime pass and no change to published
conformance counts.
