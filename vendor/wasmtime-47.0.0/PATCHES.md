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
