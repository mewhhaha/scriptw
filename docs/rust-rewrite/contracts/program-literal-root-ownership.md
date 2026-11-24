# Program literal root ownership

This is an unapplied ABI6 component based on frozen EB and the sealed
ProgramInstance type/API packet. It is separate from the ABI5 repair checkpoint.
It authors literal/data producers and checked literal lowering, with unit
regressions. Entry, registry and bootstrap consumers are coordinated separately.
No Cargo, Wasm validation, JavaScript or native test execution was run.

## Ownership enforced here

The shared R pool contains exactly the compiler-owned string prefix. Its
PooledStringsInitialize helper takes no program string count, allocates exactly
that prefix, and initializes the genuine R strings, well-known symbols and
Symbol registry. The helper's compile-time source pool must be runtime-only.
Only R initialization is guarded by the shared pooled-string root. This guard
cannot suppress a P producer or let one P resize another's table.

StringPool::pooled_string_domain checks the full collected-index range and
returns the closed Runtime(PooledStringIndex) or Program(ProgramLiteralIndex)
domain. ProgramLiteralIndex::for_pool subtracts the actual compiler prefix with
checked arithmetic and rejects R indexes and indexes beyond the P suffix. Its
integer constructor remains private. A P-local index is not a cross-P identity:
the entry integration must provide that code's actual effective ProgramInstance.

FunctionBuilder::emit_pooled_string_reference is the sole new lowering seam.
The R branch reads the stable shared pool; the P branch reads the current
ProgramInstance's immutable literal-table field. The builder's current program
comes from explicit entry/captured ownership, never from a Store or Realm
selector. The coordinated emit.rs consumers must route source literals and
runtime/source diagnostic strings through this checked seam.

Every P producer allocates a new P-only PooledStringTable of its exact literal
count, with every slot null. ProgramStringDescriptorImage holds one row per
P-local ordinal, in ordinal order, each a P-relative UTF-16 span. Its compiler
validation retains checked row lengths, aligned offsets, complete source spans
and one row per slot. Descriptor serialization does not assume that the sorted
string map is in insertion/ordinal order.

ProgramStringDescriptorTable copies the private passive descriptor image into
the existing ImmutableByteArray GC owner, which the ProgramInstance keeps as
LITERAL_DESCRIPTORS. No String is built at publication. A literal is built at
its first use by the P-owned `program_literal_resolve` hook body, the only
writer of the table: it reads that ordinal's row, builds an actual GC
CodeUnitArray from that P's passive UTF-16 segment and publishes the existing
StringValue representation in the slot, or returns the String a previous call
published. Embedded NUL, astral pairs and lone surrogate code units retain their
actual UTF-16 values; no text re-encoding or transient linear buffer supplies
the persistent string bytes. Every reader of a P literal, whatever its route,
gets the one String of its slot, so identity is per ProgramInstance as before.

P code reads a slot in place and calls its own resolver body directly only on a
null slot. R cannot reach P's passive segments; it calls the same body through
the owner's LITERAL_RESOLVE hook field. R needs this only for a source
function's text: a JavaScript FunctionObject whose text is one of its defining
P's literals stores a null TO_STRING and that literal's ordinal in
TO_STRING_LITERAL, and `Function.prototype.toString` resolves it through the
context's DEFINING_PROGRAM, which is the P whose code allocated the function and
owns the ordinal. Native callables and R-domain texts keep an eager TO_STRING.
The resolver allocates but runs no JavaScript and polls no deadline before its
write, and the new String is rooted by the table, so an interrupt or collection
can only find a slot null or published.

ProgramLiteralRoots::unresolved consumes the sole construction table and
descriptor roots. The producer retains no table alias. An empty P suffix still
publishes a real empty table, descriptor and metadata owner and installs no
resolver; it does not return early or inherit an earlier P's roots.

ProgramInstanceImage::require_pool checks that the producer uses its exact
checked source pool. produce_program_literal_roots returns the literal wrapper
and ProgramMetadataBytes together, for root bootstrap to consume with the
ProgramInstanceHooks in ProgramInstanceLocals::publish. The metadata owner uses
the existing passive P static segment and ImmutableByteArray publication API.

## Required consumer joins

The root bootstrap must create one checked image for its actual linked P and
follow this order before Realm/source-function initialization:

1. Call initialize_runtime_literal_roots. It may skip a fully initialized R.
2. Always call produce_program_literal_roots(image), including an empty suffix.
3. Construct the exact P hook references and consume all resources in
   ProgramInstanceLocals::publish; bind that owner as main's current program.
4. Retain and forward the explicit owner through calls, host reentry, stored
   functions, activations, jobs and module/eval continuations.

The checked reader returns Result<GcStackReference<StringValue>, EmitError>.
Entry-lane changes must replace the heap branch in
emit.rs::emit_interned_string_reference and the direct shared-pool read in
emit.rs::emit_runtime_error_message_reference. The heap-free inline literal
construction path stays available without a P owner. Root owns the renamed
body_compilation.rs initializer call, current_program binding, publication and
helper signature join. The helper-registry row is exactly:

```text
PooledStringsInitialize / PooledStringsInitializeArguments /
PooledStringsInitializeParameters / "pooled_strings_initialize" {} => Void;
```

There is no unused compatibility count. This pure R bootstrap helper has no
calling Program operand; demanding one would make P construction circular.
Hook signatures and P-dependent runtime helpers have their separately required
explicit program ownership.

The ModuleKind policy follows the actual EB variants. Runtime emits only R
resources. Program carries a real linked RuntimeArtifact and publishes one P.
Standalone is heap-free: it emits inline GC string primitives, initializes no
shared GC pool and needs no ProgramInstance. Any feature requiring source
functions, hooks or persistent P metadata must select linked Program emission,
or reject a violated emission invariant. Standalone is not a fake R bootstrap
with an ownerless heap P.

## Metadata still pending

This component migrates literal descriptor bytes and UTF-16 ownership only.
Copying the P static bytes into ProgramMetadataBytes does not relocate embedded
absolute-pointer records. Existing RegExp descriptor/candidate tables and
Unicode metadata, NumericCallTable rows, PooledModuleBytes and remaining static
StringPool byte consumers still need bounded reads through the actual immutable
P metadata owner. Compact-array recipe ownership also needs its own audit.

The old program MemoryInit and transient-byte-cursor reset are intentionally
not removed here. Retained-P static data is still unsafe until those consumers
join their owner-relative readers and root removes the old installation/reset.
This packet is not independently mergeable and does not claim the complete
retained-hook/static-data defect fixed.

## Verification and remaining acceptance

New unit controls exercise checked R/P classification and invalid indexes,
runtime deduplication, stable R initializers after P growth, P-local descriptor
coverage independent of sorted map order, an empty P suffix, and two independent
P byte images with colliding local slots. They preserve exact NUL, astral and
lone-surrogate units. These tests are authored but unexecuted.

The integrated module-boundary guard must register the new private modules and
the moved R helper owner; its old initializer name/call assertions must be
updated to pin the new bootstrap sequence and actual current-program reader.
Formatting and selected-copy patch application cannot establish those joins.

Native acceptance must retain the exact prior same-Store transport control:
old P Function returns 17, reentrant P2 returns 42 across forced GC, and outer P
resumes 17. Its genuine conflict control must change from old P returning 23
after P2 main to returning 17. Add both-P literal strings with identical local
ordinals but distinct NUL/astral/lone-surrogate values, retain source functions
and diagnostic paths from each across GC and reverse call order, and verify
native R constant reads and well-known Symbol identity remain stable. Empty-P
and no-prepared-hook controls must still use the existing unprepared behavior.
Run the complete typed ABI6 schema/admission and ordinary/Realm/async/module
regressions only after all coordinated components have compiled together.
