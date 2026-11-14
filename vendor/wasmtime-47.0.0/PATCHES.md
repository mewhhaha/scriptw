# Lila collector patch

This directory vendors the crates.io Wasmtime 47.0.0 release under its original
Apache-2.0 WITH LLVM-exception license. The workspace selects it through
`[patch.crates-io]`; the version and other Wasmtime dependency pins remain 47.0.0.

The local changes add Store-scoped weak-target and ephemeron handles. Their
generational slots are outside the ordinary strong-root census. The copying
collector processes conditional values to a fixed point after strong tracing,
then updates or clears weak edges before sweeping ExternRef host data. A value
requires both a live owner and a live key. Other collectors reject registration.
Upgrades create a scoped strong root while collection is forbidden; stale or
foreign-Store handles cannot access another registration. Conditional marking
uses sorted owner/key dependencies and first-copy notifications to activate
each edge once, including chains through boxed values. All queue and retirement
storage is reserved before flipping the heap. Store-scoped, epoch-scoped opaque
key tokens permit non-rooting host lookup; every copying cycle invalidates them
before movement, even when no weak edges exist. Other collectors reject the
token capability by default.

The collector changes are confined to `src/runtime/gc/enabled{.rs,/weakref.rs}`,
`src/runtime/vm/gc{.rs,/weak.rs,/gc_runtime.rs,/enabled/copying.rs,
/enabled/copying/{weak.rs,marking.rs}}`, collection controls in
`src/runtime/store/gc.rs` and a shared GC-store accessor in
`src/runtime/store.rs`. They do not change the JIT heap-header or VMContext ABI.
The manifest adds an independent unit-test workspace, the existing local cache
patch, the vendored translator (the runtime's typed funcref read expects the
engine type that translator passes) and matching dependency optimization
profiles. Its separate lockfile pins the upstream test dependencies; it does not
replace the root workspace lockfile. On this Linux host the five upstream
`pagemap` tests fail in that workspace with and without these patches; the
`PAGEMAP_SCAN` results they assert are kernel-dependent.

The copying collector's allocation-failure policy uses the current active
semi-space rather than the physical backing-memory size. It still tries
collection first, then prefers growth when the previous collection and pending
allocation leave less than one eighth of that semi-space as headroom. This
avoids repeatedly copying an almost full retained graph while allowing live
sets with more headroom to reuse the existing heap. The public capacity metric
and initial heap size are unchanged. Other collectors retain their previous
growth policy. Native controls cover retained and temporary allocations, both
semi-space orientations, denied growth, real OOM and reuse after full reclamation.

`ArrayRef::new` fills a packed `i8` array's body with one byte fill instead of
type-checking and writing each element through a `Val`
(`src/runtime/gc/enabled/arrayref.rs`). Allocation, collection retry, the
out-of-memory result and the stored bytes are unchanged; other element types
keep the per-element path. ArrayBuffer backing allocation depends on this.

`Store::interrupt_handle` stops compiled code without checks in that code
(`src/runtime/vm/interrupt.rs`, with hooks in `src/runtime/{store.rs,func.rs}`,
`src/runtime/vm/traphandlers.rs` and `src/runtime/vm/sys/unix/signals.rs`). A
request removes access to the store's GC heap and to linear memories given to
`Store::interrupt_on_memory`. The fault handler restores access and, for a
faulting instruction in compiled code with a request pending, unwinds with
`Trap::Interrupt` whether or not that instruction is a registered trap site;
host code retries its access. Ranges are published in a fixed table of atomics
before access is removed and cleared before the store frees the memory. Only
memory that cannot relocate is watched. A request still pending at the next
entry into compiled code is delivered there. A fault may interrupt an object's
initialisation; this relies on the copying collector reaching objects from
roots only and keeping its allocation pointer outside the heap. Nothing in the compiled-code or
VMContext ABI changes. `tests/interrupt_handle.rs` covers GC-heap and
linear-memory loops, host code reaching protected memory, idle requests,
cancellation, store drop and concurrent stores. See the
[deadline contract](../../docs/rust-rewrite/contracts/execution-deadlines.md).

## Per-operation host costs

These changes remove per-call work from host paths the retained requests run
on every operation ([measurements](../../docs/rust-rewrite/benchmarks.md#host-side-costs-2026-11-13)).

- Weak readers take a shared Store borrow, which cannot collect:
  `GcWeakKeyFingerprint::new`, `GcWeakHeapEpoch::current` and
  `GcEphemeron::{is_live, matches_key, key_fingerprint}` accept
  `impl AsContext` and enter no no-GC scope (`StoreOpaque::optional_gc_store`
  is the shared accessor). `WeakGcRegistry::live` names a live slot without
  allocating an error on a miss; upgrades and values use it once instead of
  `contains` and then `get`. A fingerprint hashes only its heap index, which
  agrees with `Eq`. `GcEphemeron::matches_fingerprint` checks a hit's key
  against the lookup's own token (same Store and epoch, so the same raw index
  as `matches_key` compares) without resolving the key's root again. Handles,
  epochs, identity checks and rooting are unchanged.
- `GcHeap::drop_gc_ref_is_noop` (true for the copying collector, whose drop
  is a plain local overwrite) lets a LIFO root scope release its roots at once
  instead of calling the drop barrier per root, the upstream TODO in
  `RootSet::exit_lifo_scope_slow`; other collectors keep the per-root drop.
- `get_interned_func_ref`'s third argument is now the engine-level expected
  type, which compiled code loads from its `VMContext` type-id array (the
  vendored `wasmtime-internal-cranelift` translator passes it; the reserved
  index still means an untyped `funcref`). The interned function's registered
  type is still checked against it. The builtin's signature is unchanged and
  only that argument's meaning changes; nothing else calls the builtin.
- `get_interned_func_ref` and `intern_func_ref_for_gc_heap` cannot collect and
  skip the no-GC scope bookkeeping in release builds. The intern table hashes
  `VMFuncRef` addresses with a murmur3 finalizer instead of SipHash.
- Each instance keeps one bit per escaping defined function whose `VMFuncRef`
  it has written. The written values are fixed for the instance's lifetime, so
  a repeated `ref.func` returns the entry instead of recomputing it.

These are in `src/runtime/vm/{libcalls.rs,instance.rs,gc/func_ref.rs}`.

- Ephemeron marking keeps one bit per `ALIGN` granule of every dependency's
  original index, built before the flip with the rest of its storage. A first
  copy tests that bit before its binary search, so copying an object that no
  edge depends on no longer searches the sorted dependencies. The filter is
  conservative: every recorded index has its bit set, and the search still
  decides.
- `copy` moves survivors of up to 128 bytes with constant-size copies chosen
  by size instead of a `memmove` call; larger objects still call it. The
  semi-spaces are disjoint, and an overlap keeps `memmove` semantics.

## Inline funcref reads and plain references

- The interned funcref table is a dense vector of nullable `VMFuncRef`
  pointers (ids are indices; entries are never removed) instead of a slab.
  `GcStore::intern_func_ref` interns and, when the table grew, publishes its
  base and length to the heap (`GcHeap::set_func_ref_table_view`); a new
  `GcStore` publishes its empty table and `detach` clears the view, so a
  pooled heap never describes another Store's table.
- The copying collector keeps that view in `VMCopyingHeapData` after
  wasmtime-environ's two fields: a `u32` length at offset 8 and the first
  entry's address at the next pointer-aligned offset (16 on 64-bit hosts).
  The vendored translator's `copying_heap_data` module defines the same
  offsets and the layout tests compare them. Compiled code reads an entry
  inline when the id is below the length: a null entry is the null funcref,
  and a non-null entry whose registered type equals the expected engine type
  (or any entry, for an untyped `funcref`) is the result. Those are exactly
  the values `get_interned_func_ref` returns without a subtype query; every
  other case still calls it with its complete check and traps.
- `GcHeap::refs_are_plain_copies` (true for the copying collector) is read
  once into `GcStore`, whose `clone_gc_ref`, `drop_gc_ref` and
  `expose_gc_ref_to_wasm` then copy or do nothing without a virtual call.
  `ExternRef::from_cloned_gc_ref` checks the externref-or-anyref kind with one
  header read instead of two.

These are in `src/runtime/vm/{gc.rs,libcalls.rs}`,
`src/runtime/vm/gc/{func_ref.rs,gc_ref.rs,gc_runtime.rs,enabled/copying.rs}`
and `src/runtime/gc/enabled/externref.rs`.

## GC heap null guard

The vendored translator lets a GC access through a nullable reference be its
own null check ([its notes](../wasmtime-internal-cranelift-47.0.0/PATCHES.md)):
through reference 0 the access lands in the heap's first bytes. The runtime
keeps those bytes faulting
([measurements](../../docs/rust-rewrite/benchmarks.md#gc-access-code-2026-11-14)).

- `src/runtime/vm/gc/enabled/copying.rs`: when the engine uses signal-based
  traps and the GC heap cannot move, the first 4 KiB of the heap (the
  translator's `copying_heap_data::NULL_GUARD_BYTES`, rounded up to whole host
  pages) are a null guard. They belong to neither semi-space: the two split the
  rest equally, so each loses 2,032 bytes (0.02% of the product's 8 MiB
  semi-spaces; collections come that much earlier). Neither semi-space then
  needs its `ALIGN` skip, which the guard replaces at reference 0. The heap
  makes the guard inaccessible once it is attached (or once growth from a
  smaller heap reaches it) and accessible again before it is detached.
  Allocation of the heap fails when its memory cannot be protected, and no
  heap is ever used without its guard.
- `src/runtime/store/gc.rs`: GC heap allocation protects the guard
  (`GcHeap::protect_null_guard`); `GcRuntime::null_guard_bytes` and
  `GcHeap::null_guard_len` report it (none for other collectors).
  `src/runtime/vm/{memory.rs,mmap.rs,sys/unix/mmap.rs}`:
  `Memory::set_prefix_access` changes the access of an mmap-backed memory's
  first pages; other backings report that they cannot.
- `src/runtime/store.rs`: `wasm_fault` accepts a fault inside the GC heap's
  guard as a null access and, like an explicit check, attaches no memory
  address to the trap.
- `src/runtime/vm/interrupt.rs`, `src/runtime/store.rs`: the Store watches its
  GC heap without the guard. A request never publishes, removes or restores
  access to it, so a fault there is never an interrupt and a withdrawn request
  leaves it inaccessible. Compiled code that reaches a null access before its
  next watched access traps on the null reference, as the explicit check
  (which accessed no memory) did; the request is then delivered at the next
  entry.
- Tests: `null_guard_tests` in `copying.rs` (trap codes for null struct reads
  and writes, null array lengths and accesses, and out-of-range indices, with
  the guard and without; the guard is never allocated, in either semi-space;
  a corrupted array length cannot address outside the reservation; a
  withdrawn interrupt keeps the guard), the growth tests in `store/gc.rs`
  count the guard out of a semi-space, and `tests/interrupt_handle.rs` tells a
  null access and a request apart.

## Global roots

`src/runtime/store/gc.rs`: a collection traces an instance's globals from
the module's own global types, reading each reference-typed definition
directly, instead of building a typed `Global` handle (and its registered
type) for every defined global. P defines 1,691 globals, so root tracing took
0.17–0.22 ms of every collection; it now takes about 0.02 ms. Host-created
globals keep the typed path. `global_root_tests` in `copying.rs` covers a
mutable and an immutable reference global, an `i31ref` global and a bottom-typed
global across collections.

## Young generation

The copying collector is generational when the GC heap starts at 2 MiB or more
([measurements](../../docs/rust-rewrite/benchmarks.md#young-generation-2026-11-14)).

- `src/runtime/vm/gc/enabled/copying.rs`: the heap's first eighth of its
  initial size (in 64 KiB units; 2 MiB of the product's 16 MiB) is a nursery
  after the null guard, and the two old semi-spaces split the rest. The
  mutator, inline and through `gc_alloc_raw`, bump-allocates in the nursery;
  an object larger than a quarter of it goes to the old space and is
  remembered whole until the next collection, since its initializing stores
  pass no barrier. The nursery's allocation window never exceeds the old
  space's free bytes, so a minor collection can always promote everything in
  it and a full one can always copy the old space and the nursery into the
  other semi-space.
- A full nursery with a whole nursery's room in the old space asks for a minor
  collection: roots, the remembered old-space slots, the objects allocated old
  since the last collection (or, after the slot buffer overflowed, every old
  object) are scanned, nursery survivors are copied into the old space and
  scanned there, and ephemerons, weak edges and `externref`s are processed
  with every old object counting as live. Without that room the failure goes
  to the existing collect-or-grow policy on the old space, which a full
  collection (nursery included) or growth answers. Explicit collections are
  full. The policy's post-collection live size comes from full collections
  only (`GcHeap::last_collection_was_minor`). Each collection advances the
  weak epoch, since young keys move.
- The remembered-slot buffer (4,096 slots, doubling after an overflow up to
  1 Mi) is published in `VMCopyingHeapData` after the funcref view, at the
  offsets the vendored translator's `copying_heap_data` module defines.
  Compiled stores record into it (see the translator's notes); host stores of
  references into existing objects record through `GcHeap::write_barrier`
  (`src/runtime/vm/gc/gc_ref.rs`).
- The nursery's end is a function of `gc_heap_initial_size`, which the module
  compatibility check already compares, so a module compiled for another
  boundary is rejected. Below a 2 MiB initial heap there is no nursery and the
  collector behaves as before.
- A minor collection's work follows what it moves: roots, remembered slots,
  survivors and the weak edges set since the last collection. On the
  retained request it went from 0.95M to 0.47M instructions, and a full
  collection from about 7.4M to about 5M.
  - `src/runtime/vm/gc/weak.rs`: the weak registry lists the slots whose
    edges were set since the last collection (`insert`, `get_mut`). After
    any collection every edge names only old objects, so a minor collection
    marks, processes and updates just the listed slots; a full collection
    still walks every slot. Both clear the list.
  - `copying/marking.rs`: a first copy finds the conditional edges waiting
    for its object through an open-addressing table keyed by the original
    index, instead of a sorted dependency array, a granule filter and a
    binary search.
  - `copying.rs`: an `Evacuation` holds one phase's copying state: the heap's
    base and length, every access checked against the length as a slice
    index would be; the bounds of what the collection moves; and the copies'
    bump pointer and `externref` list, written back when it is dropped. An
    object is bounds-checked once and its edges against its size, and
    forwarding reads a header once.
  - `src/runtime/vm/gc/gc_ref.rs`: the small `VMGcRef` and `VMGcHeader`
    accessors are `#[inline]`; they were calls across codegen units in the
    collector's loop. `raw_kind_matches` tests kind bits without validating
    the whole word.
  - `src/runtime/store/gc.rs`: each instance's reference-typed global
    definitions are listed when a collection first meets the instance; their
    addresses are fixed for its life.
- Tests: `generational_tests` in `copying.rs` (old-to-young stores through
  compiled `struct.set`, `array.set`, `array.fill`, `array.copy` and
  `array.init_elem`, and through the host API, survive minor collections; an
  overflowing slot buffer; large objects allocated old with young
  initializers; weak references to young targets; a young value set on an
  ephemeron no minor collection visits any more; rejection of a module
  compiled for another nursery).

Three upstream test feature guards are corrected so the collector control can
compile with the product's narrow feature set plus `wat`: Linux copy-on-write
tests in `src/runtime/vm/cow.rs` require `pooling-allocator`, and the hash-key
controls in `src/runtime/{memory.rs,externals/global.rs}` require `coredump`,
matching the methods they call. Those controls remain present and compile when
their dependencies are enabled. This is not a full upstream Wasmtime test run.

Collector tests exercise actual reachability and moving references. JavaScript
weak builtins require additional compiler, host and job integration before the
product capability can be enabled. See the
[collector contract](../../docs/rust-rewrite/contracts/collector-weak-edges.md).
