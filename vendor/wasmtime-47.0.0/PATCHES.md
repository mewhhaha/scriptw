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
host code retries its access. Ranges are published in a list of fixed chunks
of 64 atomic slots before access is removed and cleared before the store frees
the memory. A requester that finds every slot taken appends a chunk; chunks
are never freed, so the fault handler walks the list without a lock, and no
watched memory is ever left unpublished (one table of 64 slots used to leave
the 33rd concurrently interrupted store, with its two ranges, running until
another one was dropped; a pool serves up to 1,024 stores). Only
memory that cannot relocate is watched. A request still pending at the next
entry into compiled code is delivered there. A fault may interrupt an object's
initialisation; this relies on the copying collector reaching objects from
roots only and keeping its allocation pointer outside the heap. Nothing in the compiled-code or
VMContext ABI changes. `tests/interrupt_handle.rs` covers GC-heap and
linear-memory loops, host code reaching protected memory, idle requests,
cancellation, store drop and concurrent stores, and `interrupt::tests`
claims three chunks' worth of slots; no repository command runs these
vendored tests. The engine's
`every_one_of_many_concurrently_interrupted_stores_stops` stops 80 spinning
stores at once and does run. The fault handler walks every chunk ever
appended, so after a burst of N concurrently interrupted watched memories
each later fault (null-guard traps included) reads about N atomics more, for
the life of the process. See the
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

## Raw weak operands

`src/runtime/gc/enabled/weakref.rs`: a host function made with
`Func::new_unchecked` receives each GC reference as the raw `u32` of a
`ValRaw`. Until that call returns or the Store collects, the raw value is the
reference itself, so an operation that cannot collect needs no root for it.
The product's WeakMap and WeakSet imports are such functions.

- `GcEphemeron::new_raw` and `set_value_raw` register and replace from raw
  operands. They are `unsafe`: the caller promises references that Wasm passed
  to the running host call, with no collection since. Null and `i31` owners
  and keys are rejected as `new` rejects them; a zero value is no value.
  Registration and replacement go through the same `WeakGcRegistry::insert`
  and `get_mut` as the rooted forms, so the slot is listed for the next minor
  collection.
- `GcEphemeron::matches_raw` says whether a live entry's owner and key are
  both exactly the objects two raw references name. It compares identities
  only and follows no reference, so it is safe; a retired entry matches
  nothing.
- `GcEphemeron::value_raw` returns the surviving value as a raw reference for
  a host function to return to Wasm at once, through `clone_gc_ref` and
  `expose_gc_ref_to_wasm` as a rooted result is stored; zero for no value or
  a retired entry.
- None of these enters a no-GC scope: registration, replacement and reads
  allocate no GC object and cannot collect. `insert` takes the `StoreOpaque`
  for that reason; the rooted constructors still hold their `AutoAssertNoGc`
  guard around it.
- `WeakHandle::{check, live}`, `WeakGcRegistry::live`, `GcEphemeron::is_live`
  and `StoreOpaque::id` are `#[inline]`: an index that probes several handles
  per lookup pays a call per accessor without it (`src/runtime/store.rs`,
  `src/runtime/vm/gc/weak.rs`).

Handles, generations, the foreign-Store check, the collector's processing of
weak edges and every rooted operation are unchanged. The engine's
`weak_gc_facility` test `raw_operands_name_the_same_entries_as_rooted_ones_until_the_next_collection`
covers the raw forms against the rooted ones across collections.
`GcWeakHeapEpoch`, `GcWeakKeyFingerprint`, `GcEphemeron::matches_fingerprint`
and `key_fingerprint` no longer have a product user: the product finds weak
collection entries by stable identities that compiled code mints, not by heap
indices that change with every collection.

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

## Funcref stores without a call

`src/runtime/vm/gc/{func_ref.rs,enabled/copying.rs}`. A store of a `funcref`
into the GC heap stores the id `intern_func_ref_for_gc_heap` returns. A
function object of the JavaScript runtime holds eight such fields, so one
closure cost eight calls of about a hundred instructions each. Two facts
let compiled code find the id itself.

- The null funcref is id zero in every table. `FuncRefTable::default`
  interns it first (`FuncRefTableId::NULL`); a table used to give it
  whichever id its first store reached.
- Each table owns a direct-mapped cache of 256 entries, a `VMFuncRef`
  address and the id the table gave it (`FuncRefCacheEntry`, two
  pointer-sized words; the address word of an empty entry is zero).
  `FuncRefTable::intern` writes the entry of every non-null address it is
  asked for. The cache is a boxed array, so its address is fixed while the
  table lives. `FuncRefTableView::cache` carries it to the heap with the
  entries, and the copying collector publishes it in `VMCopyingHeapData`
  after the remembered-slot buffer's address
  (`copying_heap_data::func_ref_cache` in the vendored translator). A heap
  that no Store has published to, at creation and after `detach`, points at
  one static cache whose entries are all empty, so the field is never null
  and a pooled heap never describes another Store's cache.

Why it is exact. An id is assigned once and never removed or reassigned,
so an entry that names an address holds that address's id for as long as
the table lives, whatever was interned since. Compiled code accepts an
entry only when its address word equals the address being stored, which no
empty entry and no entry for another address can; everything else calls
the libcall as before, and that call fills the entry. The cache and the
table are host memory that only host code writes, between calls; the GC
heap never holds either. Reads are unchanged: id zero is an entry like any
other and holds null.

Tests: `func_ref::tests::` (the null funcref is id zero before and after
other addresses and takes no entry; with three times as many addresses as
entries every entry names an address with its own id, the cache does not
move as the table grows, and the entry layout is the one compiled code
reads), `vm_copying_heap_data_func_refs_offsets` (the field offset, entry
count and entry size against the translator's), and the engine probe
`a_stored_funcref_is_the_function_that_was_stored`.

## Funcref code addresses

An instance writes a defined function's `VMFuncRef` into its `VMContext` the
first time the function is asked for (`ref.func`, a table element, an export).
Each write looked the module up in the store's registry (two B-tree searches)
and located the array-call trampoline and the Wasm body in the compiled
function table (a binary search each): about 760 Callgrind instructions per
function, 459K of the 14.1M of a fresh React SSR entry (605 functions).

- `src/runtime/instantiate.rs`: `CompiledModule::func_ref_text` returns the
  two text-section offsets from a per-module table indexed by `FuncRefIndex`,
  filled from the function table (`locate_func_ref_text`) on the first
  request. The offsets depend on the immutable module alone, so the entries
  are plain atomics: racing instances store the same values, and a half-read
  entry is located again.
- `src/runtime/vm/instance.rs`: an instance finds its store's copy of the
  module's text in the registry once (`func_ref_text`) and adds the offsets,
  bounds-checked against that text. The registry keeps the code at that
  address for the store's life, which the `VMFuncRef`s already written rely
  on. The module startup function, asked for once per instance, is still
  located in the function table.
- Nothing in the `VMFuncRef`, `VMContext` or compiled-code ABI changes.
- Test: `func_ref_text_tests` in `src/runtime/instance.rs` compares every
  escaping function's `VMFuncRef` with the registry and function-table
  answer, on the first and on a repeated request, for two modules and three
  instances in each of two stores.

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

## Pooled GC heaps

A pooled Engine takes GC heaps from its memory pool, and the pool hands a
returned slot to the next Store with its dirty pages kept resident. The null
guard above must hold on such a slot too.

- `src/runtime/vm/memory.rs`: `Memory::set_prefix_access` protects the first
  pages of a memory whose `MemoryImageSlot` maps no copy-on-write image,
  which is every pooled GC heap; it used to report that it could not, so a
  pooled Engine failed at its first GC heap. The slot records only that its
  first `accessible` bytes are read-write and never touches the prefix while
  the memory is in use. Its reset writes from offset 0, so the caller must
  restore the prefix before deallocation (a slot with a copy-on-write image
  still reports that it cannot).
- `src/runtime/vm/gc/enabled/copying.rs`: `detach` restores the guard before
  the Store returns the memory, and asserts that the restore happened, so
  neither the pool's reset nor the slot's next owner (another heap or a
  linear memory) finds it inaccessible. `attach` clears the protected flag:
  a pooled heap object is reused with a memory that arrives accessible from
  offset 0, and its own guard is then protected again by
  `protect_null_guard`. `StoreOpaque::drop` already retires the Store's
  interrupt state, restoring any access a request removed, before
  `detach`.
- `src/config.rs`: `PoolingAllocationConfig::total_stacks` no longer requires
  the `async` feature (without it there is no stack pool and the setting has
  no effect). The product enables no `async`, but a host that links
  `wasmtime-wasi` does through feature unification, and the default of
  1,000 64-MiB fiber stacks would map 64 GiB and protect 1,000 guard pages at
  every Engine creation.
- `src/runtime/vm/sys/unix/pagemap.rs`: the `PAGEMAP_SCAN` reset that keeps
  dirty pages resident searched for pages that are written *and present*. A
  dirty page that reclaim had swapped out, or that compaction, NUMA balancing
  or a THP collapse was migrating, is a swap entry at that moment (`SWAPPED`,
  `WRITTEN`): it matched nothing, lay before the scan's end, and so was
  neither zeroed nor decommitted, and the next Store on the slot read the
  previous Store's bytes. The scan now takes written pages that are present
  *or* swapped (`category_anyof_mask`), returns the `SWAPPED` category, zeroes
  present regions in place and decommits swapped ones (no swap-in to
  overwrite them). Dropping `PRESENT` alone would be wrong: this 6.18 kernel
  reports never-populated pages as `WRITTEN` too, so every page of the range
  would be zeroed and populated. Which kernels do that is also why the five
  upstream `pagemap::tests` that expect holes to be unwritten fail here. The
  scan's filter and its region handling are `DirtyPageScan` and
  `reset_regions`; `wasmtime::_internal::model_dirty_page_reset` runs that
  handling on given page categories, with the kernel's selection modelled. The
  model and its export exist only under the vendored `scriptw-pagemap-model`
  feature, which only scriptw-engine's Linux dev-dependencies enable, so
  product builds do not contain it, and
  the engine's `the_reset_releases_swapped_dirty_pages_and_zeroes_present_ones`
  drives it with swapped, migrating, present, zero-page, file and
  never-populated pages. The engine's
  `a_swapped_out_dirty_page_reads_zero_in_the_next_store` pages real dirty
  memory out with `MADV_PAGEOUT` before the drop; it needs an active swap
  device, which the verification wrapper's swap-free cgroups never have, so
  it is `#[ignore]`d with that reason and fails, rather than passing, when run
  without one.
- Tests: the engine's `aot_gc_pooled_stores` (a null access traps on each
  reused slot and after a collection; an interrupted Store and one dropped
  with a request pending hand the next Store an accessible heap with its
  guard and working interrupts; Store limits cap a pooled heap; a reused slot
  starts zeroed; a full pool refuses a Store until one is dropped) and the
  embedding test that reports a full pool as a typed error.

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
  initial size (in 64 KiB units; 4 MiB of the product's 32 MiB) is a nursery
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
- Scans of inline struct fields and reference arrays check the field range
  against the validated object extent before visiting individual slots.
  Remembered slots still check their own heap range. Each heap reuses four
  ephemeron scratch buffers, retaining up to 256 KiB per buffer between
  collections. Ephemerons reach their fixed point before weak slots clear.
- A heap's first full nursery does not collect
  ([measurements](../../docs/rust-rewrite/benchmarks.md#pretenure-window-2026-11-24)).
  While no collection has run since the heap got its memory, the old space
  holds only large objects allocated there directly, and the mutator's bump
  pointer and limit move to a *pretenure window* at the old space's bump: the
  smaller of 4 MiB and the old space's free bytes less two nurseries (4 MiB
  with an empty old space at the product's default size). A Store that is dropped
  before the window fills never collects. `YoungAllocation` holds the state:
  `FirstNursery`, `Pretenuring { nursery_bump }`, `NurseryAfterWindow`,
  `Nursery`.
  - Window objects are old by address, so compiled barriers, the host barrier
    and the `externref` lists treat them as old with no change to compiled
    code, tunables or the module compatibility check.
  - Compiled code initializes a window object without barriers while the
    nursery holds objects, and an
    [interrupt](../../docs/rust-rewrite/contracts/execution-deadlines.md) can
    unwind out of an initialization and leave a gap without a header. So the
    window's references into the nursery are not all remembered, and the
    window cannot be walked object by object. No collection needs either:
    while the old space holds a window's objects (`Pretenuring`,
    `NurseryAfterWindow`) the next collection is full. It reaches objects
    from roots only, evacuates the nursery and the window's semi-space
    together, and leaves an ordinary heap (`Nursery`). The window's size
    leaves the other semi-space room for both.
  - The window closes at the allocation that does not fit it (the nursery is
    still full, so that allocation asks for the collection), at the start of
    any collection, when the heap grows, and with `detach`. A pooled heap
    starts each Store in `FirstNursery`. An explicit collection before the
    first nursery fills ends `FirstNursery`: no window opens afterwards.
  - Until that first collection the window's objects are kept, as promoted
    objects are until a full collection. A Store that allocates between one
    nursery and the window's end therefore holds up to about 3 MiB more.
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
  compiled for another nursery; the pretenure window's bounds and its full
  collection; a nursery object reachable only through a window object's
  initializing store, and host and compiled stores between the two; an
  explicit collection in the first nursery and in the window; growth with the
  window open; weak references and `externref`s of window objects; a pooled
  heap dropped in each state). The engine's `aot_gc_pretenure_window` runs a
  6 MiB graph and weak collections through the same states from JavaScript.
  No repository command runs the vendored tests; they were run from a copy of
  this manifest with the workspace's `[patch]` paths and the features
  `cranelift,gc,gc-copying,runtime,std,wat,pooling-allocator,threads,async,component-model,cache`.

## Growth to a limit

`src/runtime/store/gc.rs`: GC heap growth aims to double the heap; when a
limit refuses the doubled size, it now grows by what the allocation needs
instead of failing. A nursery keeps part of the heap out of both semi-spaces,
so a live set that fit a heap just under a Store's memory limit before the
young generation needs this to fit at the limit (`aot_gc_virtual_reservation`
in the engine's tests; `a_heap_near_its_limit_still_grows_to_the_limit` in
`copying.rs`).

## File-range deserialization

`Module::deserialize_open_file_range(engine, file, offset, len)`
(`src/runtime/module.rs`, with `Engine::load_code_file_range`,
`MmapVec::from_file_range`, `Mmap::from_file_range` and the Unix
`sys::unix::mmap::Mmap::from_file_range`) is `deserialize_open_file` for `len`
bytes of an open file starting at `offset`. The range must be non-empty, lie
inside the file and start on a host page boundary; otherwise it returns an
error before mapping anything. The mapping is private and copy-on-write like a
whole-file one. `UnalignedLength` records the mapping's offset in its file so
that copy-on-write memory images (`src/runtime/vm/cow.rs`) map their data from
`offset + position in the image` instead of from the position alone, and a
`debug`-feature deep clone copies a range mapping instead of remapping the whole
file. Whole-file mappings keep offset zero and their previous behavior. Unix
hosts only (`cfg(all(feature = "std", unix, not(miri)))`).

Scriptw maps its embedded native runtime images from the running executable
through this entry
([contract](../../docs/rust-rewrite/contracts/runtime-byte-ownership.md#native-backing-storage)).
Test: `embedded_runtime::native_backing::tests` in `scriptw-engine` runs code
and reads a data segment from a module mapped at a nonzero offset of a larger
file, and checks the rejected ranges.

## Copy-on-write image beside an imported memory

`ModuleMemoryImages::new` (`src/runtime/vm/cow.rs`) skips an imported
memory whose static-initialization entry is empty instead of giving the
whole module no image. Static initialization (wasmtime-environ's
`try_static_init`) already refuses a module with any data segment for an
imported memory, so every imported memory's entry is empty; a module with
data for one keeps segmented initialization and no image, as before. Before
the patch a module that imports one memory and defines another, as
Scriptw's R imports its shared memory and defines its private one, got
static initialization without an image, so every instantiation copied the
defined memory's whole dense image (1.7 MiB for R) instead of mapping it.
Instance initialization already skipped imported memories in the static map
(`src/runtime/vm/instance.rs`), and an imported memory's contents are never
touched. `module::tests::a_defined_memory_beside_an_imported_one_gets_an_image`
instantiates such a module on demand and from a reused, kept-resident pooled
slot, and checks the image's bytes, zeroes around them and the imported
memory after the previous instance dirtied the image's page and others.

## Type registry hashing

`TypeRegistryInner::hash_consing_map` (`src/runtime/type_registry.rs`) is keyed
by `RecGroupHasher`, a multiply-and-add word hasher in the style of
`rustc-hash` 2, instead of the standard library's keyed SipHash-1-3, and
`register_module_types` reserves the map for a module's rec groups before it
inserts them. Every rec group of every loaded module is hashed by its complete
structural definition when the module is registered, deserialized modules
included, so this hashing is paid by every process start. Type identity is
unchanged: the map still compares complete keys.

With an empty registry, the first module uses the reserved arenas and hashes
each owned canonical key once before insertion. Duplicate groups reuse the
registered type IDs and increment the existing registration count. A
populated registry can still find shared groups through a borrowed key.

The hasher is not keyed. A module whose type section is built to collide can
make registering its own rec groups quadratic; upstream's SipHash prevents
that. Scriptw only loads modules its own compiler emitted, so it gives this up.
An embedder that loads untrusted Wasm through this vendored copy does not have
that protection.

Before the patch, registering the types of the runtime and program modules was
10.1M of the 41.1M Callgrind instructions of a warm `scriptw run` of a
print-only program (2026-11-24): 6.5M in SipHash and the derived `Hash`
implementations feeding it, and 2.0M rehashing registered groups while the
map grew. The measured result is in
[current work](../../docs/rust-rewrite/current-inventory.md) under process
startup. No test is added for the hasher: the engine suites register and
canonicalize every module they load through it.

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
