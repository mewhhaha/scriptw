//! The copying (semi-space) garbage collector.
//!
//! This implements a Cheney-style semi-space copying collector. The GC heap is
//! divided into two halves: the "active" semi-space where new objects are
//! allocated, and the "idle" semi-space. During collection, live objects are
//! copied from the idle space (which was the previous active space) to the new
//! active space, and all roots are updated to point to the new locations.
//!
//! Allocation is a simple bump pointer within the active semi-space.
//!
//! This collector does not require any read or write barriers.

mod marking;
use marking::EphemeronMarking;

use super::VMArrayRef;
use super::trace_infos::TraceInfos;
use crate::runtime::vm::{
    ExternRefHostDataId, GarbageCollection, GcHeap, GcHeapObject, GcProgress, GcRootsIter,
    GcRuntime, GcStoreTraceState, SendSyncUnsafeCell, TraceInfo, TypedGcRef, VMExternRef,
    VMGcHeader, VMGcRef, VMMemoryDefinition,
};
use crate::{Engine, bail_bug, prelude::*};
use core::{
    alloc::Layout, any::Any, mem, num::NonZeroU32, ptr::NonNull, sync::atomic::AtomicUsize,
};
use wasmtime_environ::copying::{
    ALIGN, ARRAY_LENGTH_OFFSET, CopyingTypeLayouts, HEADER_COPIED_BIT, HEADER_SIZE, InlineTraceInfo,
};
use wasmtime_environ::{
    GcArrayLayout, GcLayout, GcStructLayout, GcTypeLayouts, MemoryKind, MemoryTunables, POISON,
    Tunables, VMGcKind, VMSharedTypeIndex, gc_assert,
};

#[expect(clippy::cast_possible_truncation, reason = "known to not overflow")]
const GC_REF_ARRAY_ELEMS_OFFSET: u32 = ARRAY_LENGTH_OFFSET + (mem::size_of::<u32>() as u32);

/// The copying collector.
#[derive(Default)]
pub struct CopyingCollector {
    layouts: CopyingTypeLayouts,
}

unsafe impl GcRuntime for CopyingCollector {
    fn layouts(&self) -> &dyn GcTypeLayouts {
        &self.layouts
    }

    fn needs_out_of_line_trace_info(&self, layout: &GcLayout) -> bool {
        matches!(InlineTraceInfo::new(layout), InlineTraceInfo::OutOfLine)
    }

    fn new_gc_heap(&self, engine: &Engine) -> Result<Box<dyn GcHeap>> {
        let tunables = engine.tunables();
        let heap = CopyingHeap::new(null_guard_bytes(tunables), nursery_end(tunables));
        Ok(Box::new(heap) as _)
    }

    fn null_guard_bytes(&self, engine: &Engine) -> usize {
        usize::try_from(null_guard_bytes(engine.tunables())).unwrap()
    }
}

/// The bytes from the start of the heap within which compiled code may let an
/// access through a null reference (index 0) fault instead of testing the
/// reference: `wasmtime_cranelift::copying_heap_data::NULL_GUARD_BYTES`.
const COMPILED_NULL_GUARD_BYTES: u32 = 4096;

/// The bytes at the start of a copying heap that are never allocated and kept
/// inaccessible: the span compiled code relies on, in whole host pages. Zero
/// when the engine does not use signal-based traps, or when the heap may move
/// (growth would copy the protected page); compiled code then tests every
/// reference explicitly.
fn null_guard_bytes(tunables: &Tunables) -> u32 {
    let memory_tunables = MemoryTunables::new(tunables, MemoryKind::GcHeap);
    if !tunables.signals_based_traps
        || tunables
            .gc_heap_memory_type()
            .memory_may_move(&memory_tunables)
    {
        return 0;
    }
    let page = u32::try_from(crate::runtime::vm::host_page_size()).unwrap();
    COMPILED_NULL_GUARD_BYTES.next_multiple_of(page)
}

/// The end of the young generation: the nursery is the heap below this index,
/// after the null guard, and the old semi-spaces lie above it. Compiled code
/// tests `slot >= base + nursery_end` in its write barrier, so this derives
/// from tunables alone (`gc_heap_initial_size`, which module compatibility
/// checks) and is the same function as the vendored translator's
/// `copying_heap_data::nursery_end`. Zero means no nursery.
fn nursery_end(tunables: &Tunables) -> u32 {
    nursery_end_for_initial_size(tunables.gc_heap_initial_size)
}

/// One eighth of the initial heap, in whole 64 KiB units; no nursery below
/// 256 KiB.
const fn nursery_end_for_initial_size(initial_size: u64) -> u32 {
    let end = (initial_size / 8) & !(64 * 1024 - 1);
    if end < 256 * 1024 || end > (u32::MAX / 2) as u64 {
        0
    } else {
        end as u32
    }
}

/// The most a heap's pretenure window allocates in the old space (see
/// `YoungAllocation`).
const PRETENURE_WINDOW: u32 = 4 << 20;

/// Where the mutator's JIT-visible bump pointer and limit allocate, in a heap
/// with a nursery.
///
/// A heap that has not collected yet has nothing in its old space to keep
/// apart from young objects. When its nursery first fills, the mutator goes
/// on allocating in a window of the empty old semi-space instead of
/// collecting: a Store that is dropped before the window fills too never
/// collects, where it used to copy the first nursery's survivors. Window
/// objects are old by address, which is how compiled barriers, the host
/// barrier and the `externref` lists classify them.
///
/// Compiled code initializes a window object without barriers while the
/// nursery holds objects, and a fault that interrupts it mid-initialization
/// leaves a headerless gap, so the window's references into the nursery are
/// neither all in the remembered slots nor findable by walking it. No
/// collection has to find them: until one has evacuated the window's objects
/// the next collection is a full one, which reaches objects from roots alone
/// and copies the nursery's and the window's survivors into the other
/// semi-space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum YoungAllocation {
    /// The JIT-visible pair bumps the nursery of a heap that has not
    /// collected since it got its memory. A window opens when it fills.
    FirstNursery,
    /// The JIT-visible pair bumps the active old semi-space, up to the
    /// window's end; `old_bump` is stale and the old space's allocation
    /// pointer is the JIT-visible one. The nursery's objects end at
    /// `nursery_bump`.
    Pretenuring { nursery_bump: u32 },
    /// The JIT-visible pair bumps the nursery again, and the old space holds
    /// the closed window's objects: the next collection is full.
    NurseryAfterWindow,
    /// The JIT-visible pair bumps the nursery, and every old object was
    /// copied by a collection or allocated by the runtime
    /// (`allocate_with_nursery`).
    Nursery,
}

/// Initial capacity of the remembered-slot buffer, in slots.
const REMEMBERED_INITIAL: usize = 4096;
/// Largest capacity the buffer grows to after overflowing.
const REMEMBERED_MAX: usize = 1 << 20;

/// The common header for all objects in the copying collector.
#[repr(C)]
struct VMCopyingHeader {
    header: VMGcHeader,
    object_size: u32,
}

// Safety: All copying collector objects have a `VMCopyingHeader`.
unsafe impl GcHeapObject for VMCopyingHeader {
    #[inline]
    fn is(_header: &VMGcHeader) -> bool {
        true
    }
}

impl VMCopyingHeader {
    /// Get the size of this object in the GC heap.
    #[inline]
    fn object_size(&self) -> u32 {
        self.object_size
    }

    /// Check whether this object has been copied to the new semi-space during
    /// a collection.
    #[inline]
    fn copied(&self) -> bool {
        self.header.reserved_u26() & HEADER_COPIED_BIT != 0
    }

    /// Mark this object as having been copied to the new semi-space.
    #[inline]
    fn set_copied(&mut self) {
        let reserved = self.header.reserved_u26();
        self.header.set_reserved_u26(reserved | HEADER_COPIED_BIT);
    }

    /// Decode the inline trace info from this header's reserved bits.
    #[inline]
    fn inline_trace_info(&self) -> InlineTraceInfo {
        InlineTraceInfo::decode(self.header.reserved_u26())
    }
}

/// A copying collector header together with a forwarding reference.
///
/// During collection, after an object has been copied to the new semi-space,
/// its old location is overwritten with the forwarding reference pointing to
/// the new location.
#[repr(C)]
struct VMCopyingHeaderAndForwardingRef {
    header: VMCopyingHeader,
    forwarding_ref: Option<VMGcRef>,
}

// Safety: All copying collector objects have a `VMCopyingHeader` and space for
// the forwarding reference.
unsafe impl GcHeapObject for VMCopyingHeaderAndForwardingRef {
    #[inline]
    fn is(_header: &VMGcHeader) -> bool {
        true
    }
}

impl VMCopyingHeaderAndForwardingRef {
    /// Get the forwarding reference for this object, if it has been copied
    /// during the current collection.
    fn forwarding_ref(&self) -> Result<Option<VMGcRef>> {
        debug_assert!(
            self.header.object_size()
                >= u32::try_from(mem::size_of::<VMCopyingHeaderAndForwardingRef>()).unwrap()
        );
        Ok(if self.header.copied() {
            match self.forwarding_ref.as_ref() {
                Some(r) => Some(r.unchecked_copy()),
                None => bail_bug!("should always have a forwarding ref if the copied bit is set"),
            }
        } else {
            None
        })
    }

    /// Set the forwarding reference for this object and mark it as copied.
    fn set_forwarding_ref(&mut self, forwarding_ref: VMGcRef) {
        debug_assert!(!self.header.copied());
        debug_assert!(
            self.header.object_size()
                >= u32::try_from(mem::size_of::<VMCopyingHeaderAndForwardingRef>()).unwrap()
        );
        self.header.set_copied();
        self.forwarding_ref = Some(forwarding_ref);
    }
}

/// The header for an array in the copying collector.
#[repr(C)]
struct VMCopyingArrayHeader {
    header: VMCopyingHeader,
    length: u32,
}

unsafe impl GcHeapObject for VMCopyingArrayHeader {
    #[inline]
    fn is(header: &VMGcHeader) -> bool {
        header.kind() == VMGcKind::ArrayRef
    }
}

/// The representation of an `externref` in the copying collector.
#[repr(C)]
struct VMCopyingExternRef {
    /// NB: Explicitly leave room for the forwarding ref so that our other
    /// fields aren't overwritten after copying to the new semi-space.
    header: VMCopyingHeaderAndForwardingRef,

    /// The ID of this ref's data in the `ExternRefHostDataTable`.
    host_data: ExternRefHostDataId,

    /// Link to the next `externref` in this semi-space.
    next_extern_ref: Option<VMExternRef>,
}

unsafe impl GcHeapObject for VMCopyingExternRef {
    #[inline]
    fn is(header: &VMGcHeader) -> bool {
        header.kind() == VMGcKind::ExternRef
    }
}

/// JIT-accessible bump-allocation state for the copying collector.
///
/// NB: Layout is defined by constants in `wasmtime_environ::copying`. Keep in
/// sync!
#[repr(C)]
struct VMCopyingHeapDataInner {
    /// Current bump pointer (index into the GC heap).
    bump_ptr: u32,
    /// End of the active semi-space.
    active_space_end: u32,
    /// Number of entries in the Store's interned funcref table
    /// (`FuncRefTable::view`). The two fields above are wasmtime-environ's
    /// layout; this one and `func_refs` follow it, at the offsets the vendored
    /// translator's `copying_heap_data` module defines.
    func_refs_len: u32,
    /// Address of the first funcref table entry, a nullable `*mut VMFuncRef`.
    /// Compiled code reads an entry inline when the id is below
    /// `func_refs_len`.
    func_refs: usize,
    /// Old-space slots that write barriers recorded since the last collection
    /// as holding a nursery reference: `remembered[..remembered_len]`.
    remembered_len: u32,
    /// Capacity of `remembered`. A barrier that finds the buffer full sets
    /// `remembered_overflow` instead, and the next minor collection scans the
    /// whole old space.
    remembered_cap: u32,
    /// Nonzero once a barrier found `remembered` full.
    remembered_overflow: u32,
    /// Address of the first remembered slot, a `*mut u32`.
    remembered: usize,
    /// Address of the Store's funcref intern cache
    /// (`FuncRefTableView::cache`), which compiled code consults before it
    /// calls `intern_func_ref_for_gc_heap`. Never null: a heap without a
    /// published table points at the empty cache.
    func_ref_cache: usize,
}

impl Default for VMCopyingHeapDataInner {
    fn default() -> Self {
        Self {
            bump_ptr: 0,
            active_space_end: 0,
            func_refs_len: 0,
            func_refs: 0,
            remembered_len: 0,
            remembered_cap: 0,
            remembered_overflow: 0,
            remembered: 0,
            func_ref_cache: crate::vm::FuncRefTableView::unpublished().cache,
        }
    }
}

#[derive(Default)]
#[repr(transparent)]
struct VMCopyingHeapData {
    inner: SendSyncUnsafeCell<VMCopyingHeapDataInner>,
}

impl VMCopyingHeapData {
    fn bump_ptr(&self) -> u32 {
        // Safety: `inner` is valid to read from.
        unsafe { (*self.inner.get()).bump_ptr }
    }

    fn set_bump_ptr(&mut self, val: u32) {
        self.inner.get_mut().bump_ptr = val;
    }

    fn active_space_end(&self) -> u32 {
        // Safety: `inner` is valid to read from.
        unsafe { (*self.inner.get()).active_space_end }
    }

    fn set_active_space_end(&mut self, val: u32) {
        self.inner.get_mut().active_space_end = val;
    }

    fn set_func_refs(&mut self, view: crate::vm::FuncRefTableView) {
        let inner = self.inner.get_mut();
        inner.func_refs_len = view.len;
        inner.func_refs = view.base;
        inner.func_ref_cache = view.cache;
    }
}

/// Get a typed reference to a copying-collector object from a raw `VMGcRef`.
fn copying_ref(gc_ref: &VMGcRef) -> &TypedGcRef<VMCopyingHeader> {
    debug_assert!(!gc_ref.is_i31());
    gc_ref.as_typed_unchecked()
}

fn externref_to_copying(externref: &VMExternRef) -> &TypedGcRef<VMCopyingExternRef> {
    let gc_ref = externref.as_gc_ref();
    debug_assert!(!gc_ref.is_i31());
    gc_ref.as_typed_unchecked()
}

/// Copy one object's bytes from the idle into the active semi-space. The two
/// ranges are already bounds-checked and lie in disjoint semi-spaces; an
/// overlap would be heap corruption and keeps `memmove` semantics.
///
/// Most survivors are a few `ALIGN` chunks, where a `memmove` call costs more
/// than the copy. A constant-size copy compiles to inline moves, and a copy
/// loop would be turned back into a call, so small sizes dispatch on size.
fn copy_object(heap: &mut [u8], from: usize, to: usize, size: usize) {
    let (source, destination) = if from + size <= to {
        let (low, high) = heap.split_at_mut(to);
        (&low[from..from + size], &mut high[..size])
    } else if to + size <= from {
        let (low, high) = heap.split_at_mut(from);
        (&high[..size], &mut low[to..to + size])
    } else {
        heap.copy_within(from..from + size, to);
        return;
    };
    fn fixed<const N: usize>(destination: &mut [u8], source: &[u8]) {
        match (destination.first_chunk_mut::<N>(), source.first_chunk::<N>()) {
            (Some(destination), Some(source)) => *destination = *source,
            // Both slices are exactly `size` bytes, the arm's `N`.
            _ => unreachable!("fixed-size object copy of the wrong length"),
        }
    }
    match size {
        16 => fixed::<16>(destination, source),
        32 => fixed::<32>(destination, source),
        48 => fixed::<48>(destination, source),
        64 => fixed::<64>(destination, source),
        80 => fixed::<80>(destination, source),
        96 => fixed::<96>(destination, source),
        112 => fixed::<112>(destination, source),
        128 => fixed::<128>(destination, source),
        _ => destination.copy_from_slice(source),
    }
}

/// Round a raw GC-heap byte length down to a usable capacity.
///
/// The result is a multiple of `2 * ALIGN` so that each semi-space (half the
/// capacity) is itself a multiple of `ALIGN`. (Rounding only to `ALIGN` is not
/// enough: e.g. `0xffff_fff0` is an odd multiple of `ALIGN`, and halving it
/// yields the misaligned `0x7fff_fff8`).
fn round_down_heap_capacity(current_length: usize) -> u32 {
    let len = u32::try_from(current_length).unwrap_or(u32::MAX);
    len & !(2 * ALIGN - 1)
}

/// A copying (semi-space) heap.
mod weak;

struct CopyingHeap {
    /// For every type that we have allocated in this heap, how do we trace it?
    trace_infos: TraceInfos,

    /// Vacant scratch buffers reused by successive collections.
    ephemeron_marking: EphemeronMarking,

    /// Count of how many no-gc scopes we are currently within.
    no_gc_count: u64,

    /// The storage for the GC heap itself.
    memory: Option<crate::vm::Memory>,

    /// The cached `VMMemoryDefinition` for `self.memory` so that we don't have
    /// to make indirect calls through a `dyn RuntimeLinearMemory` object.
    ///
    /// Must be updated and kept in sync with `self.memory`, cleared when the
    /// memory is taken and updated when the memory is replaced.
    vmmemory: Option<VMMemoryDefinition>,

    /// JIT-accessible allocation state: bump pointer and active-space end.
    ///
    /// NB: The bump pointer is written to by compiled Wasm code via the pointer
    /// returned by `vmctx_gc_heap_data`.
    vmctx_data: VMCopyingHeapData,

    /// The start of the active semi-space.
    active_space_start: u32,

    /// The start of the idle semi-space.
    idle_space_start: u32,

    /// The end of the idle semi-space.
    idle_space_end: u32,

    /// "Pointer" (really an index) to the start of the worklist.
    ///
    /// This is always within the active semi-space and is always less than or
    /// equal to `bump_ptr`.
    ///
    /// This is used to implement a Cheney-style worklist: grey objects (the set
    /// of objects that have been copied to the new semi-space but have not yet
    /// been scanned) are always within `worklist_ptr..bump_ptr`. The worklist
    /// is empty when `worklist_ptr == bump_ptr` and we can pop from the
    /// worklist by advancing `worklist_ptr`.
    worklist_ptr: u32,

    /// The set of `externref`s in the active semi-space.
    ///
    /// The set is implemented as an intrusive linked-list, and this is the
    /// head of the list.
    active_extern_ref_set_head: Option<VMExternRef>,

    /// Like `active_extern_ref_set_head` but for the idle semi-space.
    idle_extern_ref_set_head: Option<VMExternRef>,

    /// The bytes at the start of the heap that belong to neither semi-space
    /// and are kept inaccessible (see `null_guard_bytes`).
    null_guard: u32,

    /// Whether the first `null_guard` bytes of the memory are inaccessible.
    null_guard_protected: bool,

    /// The young generation's end (see `nursery_end`). With a nursery the
    /// JIT-visible bump pointer and limit allocate in
    /// `nursery_start()..nursery_end`, the old semi-spaces lie above it and
    /// `old_bump`/`old_end` allocate in the active one. Zero without a
    /// nursery, when the JIT-visible pair is the active semi-space's.
    nursery_end: u32,

    /// The active old semi-space's allocation pointer, with a nursery that
    /// the mutator allocates in (stale while `young` is `Pretenuring`).
    old_bump: u32,

    /// What the JIT-visible pair allocates, with a nursery.
    young: YoungAllocation,

    /// The active old semi-space's end.
    old_end: u32,

    /// The mutator found the nursery full while the old space had room for
    /// everything in it: the next collection may be a minor one.
    minor_requested: bool,

    /// The heap grew since the mutator last found the nursery full; the
    /// allocation retried after growth goes to the old space.
    grew: bool,

    /// Whether the collection in progress is a minor one.
    collecting_minor: bool,

    /// Whether the last completed collection was a minor one.
    last_minor: bool,

    /// Objects allocated directly in the old space since the last collection.
    /// Their initializing stores pass no barrier, so a minor collection scans
    /// them whole.
    direct_old: Vec<u32>,

    /// Backing store of the remembered-slot buffer that `vmctx_data`
    /// publishes to compiled code. The published capacity is this vector's
    /// initialized length, and the published pointer is null while empty.
    remembered: Vec<u32>,

    /// The set of `externref`s in the nursery.
    young_extern_ref_set_head: Option<VMExternRef>,
}

impl CopyingHeap {
    fn new(null_guard: u32, nursery_end: u32) -> Self {
        log::trace!("allocating new copying heap");
        Self {
            trace_infos: TraceInfos::new(),
            ephemeron_marking: EphemeronMarking::default(),
            no_gc_count: 0,
            memory: None,
            vmmemory: None,
            vmctx_data: VMCopyingHeapData::default(),
            active_space_start: 0,
            idle_space_start: 0,
            idle_space_end: 0,
            worklist_ptr: 0,
            active_extern_ref_set_head: None,
            idle_extern_ref_set_head: None,
            null_guard,
            null_guard_protected: false,
            nursery_end,
            old_bump: 0,
            young: YoungAllocation::Nursery,
            old_end: 0,
            minor_requested: false,
            grew: false,
            collecting_minor: false,
            last_minor: false,
            direct_old: Vec::new(),
            remembered: Vec::new(),
            young_extern_ref_set_head: None,
        }
    }

    /// Replaces the (empty) remembered-slot buffer by one of `capacity` slots.
    fn resize_remembered(&mut self, capacity: usize) -> Result<()> {
        let mut slots = Vec::new();
        slots.try_reserve_exact(capacity)?;
        slots.resize(capacity, 0);
        self.remembered = slots;
        let inner = self.vmctx_data.inner.get_mut();
        assert_eq!(inner.remembered_len, 0);
        inner.remembered = self.remembered.as_mut_ptr() as usize;
        inner.remembered_cap = u32::try_from(capacity).unwrap();
        Ok(())
    }

    /// Records that the old-space slot at heap index `slot` holds a nursery
    /// reference, as a write barrier does.
    fn remember_slot(&mut self, slot: u32) {
        if self.remembered.is_empty() {
            // A compiled store may already have flagged overflow. Preserve it
            // even when this allocation succeeds: its edge is still missing.
            let _ = self.resize_remembered(REMEMBERED_INITIAL);
        }
        let inner = self.vmctx_data.inner.get_mut();
        let len = usize::try_from(inner.remembered_len).unwrap();
        if len < self.remembered.len() {
            // SAFETY: the index is in bounds. Write through the vector's
            // raw pointer without invalidating the pointer compiled code has.
            unsafe { self.remembered.as_mut_ptr().add(len).write(slot) };
            inner.remembered_len += 1;
        } else {
            inner.remembered_overflow = 1;
        }
    }

    /// Whether `index` is a nursery object's index.
    fn is_young(&self, index: u32) -> bool {
        index < self.nursery_end
    }

    /// The first index the nursery allocates: past the null guard, and never 0.
    fn nursery_start(&self) -> u32 {
        self.null_guard.max(ALIGN)
    }

    /// The end of the nursery's backed bytes.
    fn nursery_limit(&self) -> u32 {
        self.nursery_end.min(self.capacity())
    }

    /// The nursery's size.
    fn nursery_capacity(&self) -> u32 {
        self.nursery_limit().saturating_sub(self.nursery_start())
    }

    /// Bytes not yet allocated in the active old semi-space.
    fn old_free(&self) -> u32 {
        self.active_space_end() - self.bump_ptr()
    }

    /// The end of the part of the nursery the mutator may fill. Its size
    /// never exceeds the old space's free bytes, so that a minor collection
    /// can promote everything in it and a full one can copy the old space's
    /// objects and everything in it into the other semi-space.
    fn nursery_window_end(&self) -> u32 {
        self.nursery_start() + self.nursery_capacity().min(self.old_free())
    }

    /// After either collection: the nursery is empty, and with it every
    /// old-to-young edge the barriers recorded.
    fn finish_young_generation(&mut self) -> Result<()> {
        self.last_minor = self.collecting_minor;
        self.collecting_minor = false;
        self.minor_requested = false;
        self.grew = false;
        self.direct_old.clear();
        if self.nursery_end == 0 {
            return Ok(());
        }
        // `gc` closed any window, and a collection that followed one was
        // full: the window's survivors are copies in the other semi-space.
        assert!(!matches!(self.young, YoungAllocation::Pretenuring { .. }));
        assert!(!self.last_minor || self.young != YoungAllocation::NurseryAfterWindow);
        self.young = YoungAllocation::Nursery;
        if cfg!(gc_zeal) {
            let start = usize::try_from(self.nursery_start()).unwrap();
            let end = usize::try_from(self.vmctx_data.bump_ptr()).unwrap();
            self.heap_slice_mut()[start..end].fill(POISON);
        }
        self.reset_nursery();
        let inner = self.vmctx_data.inner.get_mut();
        let overflowed = inner.remembered_overflow != 0;
        inner.remembered_len = 0;
        inner.remembered_overflow = 0;
        if overflowed && self.remembered.len() < REMEMBERED_MAX {
            // Compiled barriers with zero capacity only set the overflow
            // flag. This collection covered their edges; install the first
            // buffer or grow the previous one for the next collection.
            // Reallocation failure keeps the previous capacity.
            let capacity = (self.remembered.len() * 2).clamp(REMEMBERED_INITIAL, REMEMBERED_MAX);
            let _ = self.resize_remembered(capacity);
        }
        Ok(())
    }

    /// Empties the nursery and publishes it as the mutator's allocation window.
    fn reset_nursery(&mut self) {
        if self.nursery_end == 0 {
            return;
        }
        assert!(!matches!(self.young, YoungAllocation::Pretenuring { .. }));
        let start = self.nursery_start();
        let end = self.nursery_window_end();
        self.vmctx_data.set_bump_ptr(start);
        self.vmctx_data.set_active_space_end(end);
        self.young_extern_ref_set_head = None;
    }

    /// Makes the first `null_guard` bytes of the memory inaccessible, once
    /// they exist. Returns `false` where the backing cannot protect them.
    fn protect_null_guard_pages(&mut self) -> Result<bool> {
        let len = usize::try_from(self.null_guard).unwrap();
        if len == 0 || self.null_guard_protected {
            return Ok(true);
        }
        // A memory without these bytes yet only reports whether its backing
        // supports this; they are still inaccessible reservation.
        let memory = self.memory.as_mut().unwrap();
        let len = if memory.byte_size() >= len { len } else { 0 };
        // SAFETY: nothing is ever allocated in these bytes, so no reference
        // into them exists; host code reaches heap bytes only through
        // allocated objects.
        if !unsafe { memory.set_prefix_access(len, false)? } {
            return Ok(false);
        }
        self.null_guard_protected = len != 0;
        Ok(true)
    }

    /// Whether the JIT-visible pair is the active old semi-space's
    /// allocation pointer: always without a nursery, and with one while the
    /// pretenure window is open.
    fn mutator_bumps_old_space(&self) -> bool {
        self.nursery_end == 0
            || match self.young {
                YoungAllocation::Pretenuring { .. } => true,
                YoungAllocation::FirstNursery
                | YoungAllocation::NurseryAfterWindow
                | YoungAllocation::Nursery => false,
            }
    }

    /// The active old semi-space's allocation pointer.
    fn bump_ptr(&self) -> u32 {
        if self.mutator_bumps_old_space() {
            self.vmctx_data.bump_ptr()
        } else {
            self.old_bump
        }
    }

    fn set_bump_ptr(&mut self, val: u32) {
        if self.mutator_bumps_old_space() {
            self.vmctx_data.set_bump_ptr(val);
        } else {
            self.old_bump = val;
        }
    }

    /// The first full nursery of a heap that has not collected: lets the
    /// mutator go on in the old space, if a window there has room for
    /// `size` (see `YoungAllocation`). The window leaves the old space two
    /// nurseries' room: one so that the allocation that fills the window asks
    /// for a collection instead of growth, and that collection, a full one,
    /// has the other semi-space for the old space's objects and the nursery.
    fn open_pretenure_window(&mut self, size: u32) -> bool {
        assert_eq!(self.young, YoungAllocation::FirstNursery);
        let reserve = self.nursery_capacity().saturating_mul(2);
        let window = PRETENURE_WINDOW.min(self.old_free().saturating_sub(reserve)) & !(ALIGN - 1);
        if window < size {
            return false;
        }
        let nursery_bump = self.vmctx_data.bump_ptr();
        let start = self.old_bump;
        self.young = YoungAllocation::Pretenuring { nursery_bump };
        self.vmctx_data.set_bump_ptr(start);
        self.vmctx_data.set_active_space_end(start + window);
        true
    }

    /// Ends the pretenure window, if it is open: the old space's allocation
    /// pointer becomes the runtime's again and the mutator is back at the
    /// nursery's end, where its next allocation asks for a collection. A
    /// no-op in every other state.
    fn close_pretenure_window(&mut self) {
        let nursery_bump = match self.young {
            YoungAllocation::Pretenuring { nursery_bump } => nursery_bump,
            YoungAllocation::FirstNursery
            | YoungAllocation::NurseryAfterWindow
            | YoungAllocation::Nursery => return,
        };
        let bump = self.vmctx_data.bump_ptr();
        assert!(bump >= self.active_space_start);
        assert!(bump <= self.vmctx_data.active_space_end());
        assert!(self.vmctx_data.active_space_end() <= self.old_end);
        self.old_bump = bump;
        self.young = YoungAllocation::NurseryAfterWindow;
        self.vmctx_data.set_bump_ptr(nursery_bump);
        self.vmctx_data.set_active_space_end(nursery_bump);
    }

    /// The end of the active old semi-space.
    fn active_space_end(&self) -> u32 {
        self.old_end
    }

    fn set_active_space_end(&mut self, val: u32) {
        self.old_end = val;
        if self.nursery_end == 0 {
            self.vmctx_data.set_active_space_end(val);
        }
    }

    fn capacity(&self) -> u32 {
        let len = self.vmmemory.as_ref().unwrap().current_length();
        round_down_heap_capacity(len)
    }

    /// Initialize the semi-spaces for a heap of the given capacity.
    fn initialize_semi_spaces(&mut self) {
        debug_assert_eq!(self.active_space_start, 0);
        debug_assert_eq!(self.active_space_end(), 0);
        debug_assert_eq!(self.idle_space_start, 0);
        debug_assert_eq!(self.idle_space_end, 0);
        debug_assert_eq!(self.bump_ptr(), 0);

        self.resize_semi_spaces();
        self.reset_bump_ptr();
        self.reset_nursery();
        self.young = YoungAllocation::FirstNursery;
    }

    fn resize_semi_spaces(&mut self) {
        debug_assert_eq!(
            self.active_space_end() - self.active_space_start,
            self.idle_space_end - self.idle_space_start,
            "the active and idle spaces should be the same size"
        );

        // We only adjust the semi-space regions for new memory capacity if the
        // active semi-space is the first half of the GC heap. Else, when the
        // the second half of the GC heap is the active semi-space, wait until
        // the next collection to automatically update the regions.
        if self.idle_space_start < self.active_space_start {
            return;
        }

        // The null guard and the nursery at the start of the heap belong to
        // neither semi-space: they split the rest equally. (Both multiples of
        // `2 * ALIGN`, so each half is a multiple of `ALIGN`.)
        let capacity = self.capacity();
        let floor = self.null_guard.max(self.nursery_end).min(capacity);
        let halfway = floor + (capacity - floor) / 2;

        debug_assert!(self.bump_ptr() <= halfway);
        debug_assert!(self.idle_space_start <= halfway);
        debug_assert!(self.active_space_end() <= halfway);

        // The first semi-space is empty whenever its start moves: it moves
        // only while the heap is smaller than the guard.
        self.active_space_start = floor;
        if self.bump_ptr() < floor {
            self.set_bump_ptr(floor);
        }
        self.set_active_space_end(halfway);
        self.idle_space_start = halfway;
        self.idle_space_end = capacity;

        debug_assert_eq!(
            self.active_space_end() - self.active_space_start,
            self.idle_space_end - self.idle_space_start,
            "the active and idle spaces should be the same size"
        );
    }

    fn reset_bump_ptr(&mut self) {
        // We always need to keep `bump_ptr` aligned to `ALIGN`.
        //
        // When the active space is the first half of the GC heap, we need to
        // skip past index 0, since `VMGcRef` is a `NonZeroU32`.
        //
        // When the active space is the second half of the GC heap, we *also*
        // skip the first `ALIGN` bytes. This ensures that the active and idle
        // spaces are always equally sized, which is required to guarantee that
        // evacuating objects from one to the other will succeed.
        //
        // A null guard or a nursery already keeps the first semi-space away
        // from index 0, so then neither skips.
        let mut bp = self.active_space_start;
        if self.null_guard == 0
            && self.nursery_end == 0
            && self.active_space_end() - self.active_space_start >= ALIGN
        {
            bp += ALIGN;
        }
        self.set_bump_ptr(bp);
        debug_assert!(self.bump_ptr().is_multiple_of(ALIGN));
    }

    /// Allocate `size` bytes from the active semi-space bump pointer.
    ///
    /// Returns `None` if there isn't enough room.
    fn allocate(&mut self, size: u32) -> Option<u32> {
        debug_assert!(size.is_multiple_of(ALIGN));
        debug_assert!(self.bump_ptr().is_multiple_of(ALIGN));
        debug_assert!(self.bump_ptr() >= self.active_space_start);
        debug_assert!(self.bump_ptr() <= self.active_space_end());

        let result = self.bump_ptr();
        let new_bump_ptr = result.checked_add(size)?;
        if new_bump_ptr > self.active_space_end() {
            return None;
        }

        self.set_bump_ptr(new_bump_ptr);
        debug_assert!(self.bump_ptr().is_multiple_of(ALIGN));
        debug_assert!(self.bump_ptr() >= self.active_space_start);
        debug_assert!(self.bump_ptr() <= self.active_space_end());

        Some(result)
    }

    /// The mutator's allocation when there is a nursery. Objects up to a
    /// quarter of the nursery bump it. Larger ones, and the one retried right
    /// after the heap grew (the store then collects no more before it), go
    /// to the old space and are remembered whole until the next collection.
    ///
    /// While the pretenure window is open every size bumps it, as compiled
    /// code does; the allocation that does not fit closes it. A heap's first
    /// full nursery opens the window instead of asking for a collection.
    fn allocate_with_nursery(&mut self, size: u32) -> Option<u32> {
        if let YoungAllocation::Pretenuring { .. } = self.young {
            let bump = self.vmctx_data.bump_ptr();
            if let Some(end) = bump.checked_add(size)
                && end <= self.vmctx_data.active_space_end()
            {
                self.vmctx_data.set_bump_ptr(end);
                return Some(bump);
            }
            self.close_pretenure_window();
        }
        let capacity = self.nursery_capacity();
        let window_end = self.vmctx_data.active_space_end();
        let large = size > capacity / 4;
        if !large && !self.grew {
            let bump = self.vmctx_data.bump_ptr();
            if let Some(end) = bump.checked_add(size)
                && end <= window_end
            {
                self.vmctx_data.set_bump_ptr(end);
                return Some(bump);
            }
            if self.young == YoungAllocation::FirstNursery {
                if self.open_pretenure_window(size) {
                    let index = self.vmctx_data.bump_ptr();
                    self.vmctx_data.set_bump_ptr(index + size);
                    return Some(index);
                }
                // No room for a window: an ordinary nursery from here on.
                self.young = YoungAllocation::Nursery;
            }
            // The nursery is full. With room in the old space for a whole
            // nursery, a minor collection follows. Otherwise the failure
            // reaches the old space's own collect-or-grow policy, as an old
            // allocation that failed would.
            self.minor_requested = self.old_free() >= capacity;
            return None;
        }
        // Keep room in the old space for everything the nursery's window
        // may hold.
        let window = window_end.saturating_sub(self.nursery_start());
        if self.old_free() < size.checked_add(window)? {
            return None;
        }
        self.direct_old.try_reserve(1).ok()?;
        let index = self.allocate(size)?;
        self.grew = false;
        self.direct_old.push(index);
        Some(index)
    }

    /// Check whether an index is within the active semi-space.
    fn is_in_active_space(&self, index: u32) -> bool {
        index >= self.active_space_start && index < self.active_space_end()
    }

    /// Check whether an index is within the idle semi-space.
    fn is_in_idle_space(&self, index: u32) -> bool {
        index >= self.idle_space_start && index < self.idle_space_end
    }

    /// Whether the collection in progress moves the object at `index`: one
    /// in the nursery, and for a full collection one in the idle semi-space
    /// it evacuates too. A minor collection leaves old objects in place.
    fn is_in_from_space(&self, index: u32) -> bool {
        self.is_young(index) || (!self.collecting_minor && self.is_in_idle_space(index))
    }

    /// Swap the active and idle semi-spaces.
    fn flip(&mut self) {
        debug_assert_eq!(
            self.active_space_end() - self.active_space_start,
            self.idle_space_end - self.idle_space_start,
            "the active and idle spaces should be the same size"
        );

        mem::swap(&mut self.active_space_start, &mut self.idle_space_start);
        let new_active_space_end = self.idle_space_end;
        self.idle_space_end = self.active_space_end();
        self.set_active_space_end(new_active_space_end);
        self.reset_bump_ptr();

        // The active idle list becomes the old active list; the active list is
        // cleared because we are starting fresh in the new space.
        self.idle_extern_ref_set_head = self.active_extern_ref_set_head.take();
    }

    /// Initialize the worklist at the start of a collection.
    fn initialize_worklist(&mut self) {
        self.worklist_ptr = self.bump_ptr();
    }
}

/// One phase's evacuation of the objects a collection moves. The heap's bytes
/// are read and written through `base`, each access checked against `len` as
/// a slice index would be, so the inner loop neither re-derives the heap slice
/// nor re-validates an object for each of its fields. The copies' allocation
/// pointer and externref list stay in locals and are written back to the heap
/// when the evacuation is dropped.
struct Evacuation<'a> {
    heap: &'a mut CopyingHeap,
    base: *mut u8,
    len: usize,
    /// The collection moves the nursery's objects, `[.., nursery_end)`, and
    /// for a full collection also the idle semi-space's,
    /// `[idle_start, idle_start + idle_len)`.
    nursery_end: u32,
    idle_start: u32,
    idle_len: u32,
    /// Copies are allocated at `bump`, up to `end`.
    bump: u32,
    end: u32,
    externrefs: Option<VMExternRef>,
}

impl<'a> Evacuation<'a> {
    fn new(heap: &'a mut CopyingHeap) -> Self {
        let memory = heap.vmmemory();
        let (idle_start, idle_len) = if heap.collecting_minor {
            (0, 0)
        } else {
            (
                heap.idle_space_start,
                heap.idle_space_end - heap.idle_space_start,
            )
        };
        Self {
            base: memory.base.as_ptr(),
            len: memory.current_length(),
            nursery_end: heap.nursery_end,
            idle_start,
            idle_len,
            bump: heap.bump_ptr(),
            end: heap.active_space_end(),
            externrefs: heap.active_extern_ref_set_head.take(),
            heap,
        }
    }

    /// Whether the collection moves the object `raw` refers to: false for
    /// null, `i31` and objects it leaves in place.
    #[inline(always)]
    fn moves(&self, raw: u32) -> bool {
        raw & 1 == 0
            && raw != 0
            && (raw < self.nursery_end || raw.wrapping_sub(self.idle_start) < self.idle_len)
    }

    /// The `T` at heap byte `index`, after the bounds and alignment checks
    /// `GcHeap::index` makes.
    #[inline(always)]
    fn object<T: GcHeapObject>(&self, index: usize) -> Result<&T> {
        if index
            .checked_add(mem::size_of::<T>())
            .is_none_or(|end| end > self.len)
            || index % mem::align_of::<T>() != 0
        {
            bail_bug!("gc object out of bounds or misaligned");
        }
        // SAFETY: in bounds and aligned; no other reference to these heap
        // bytes is live during the evacuation.
        Ok(unsafe { &*self.base.add(index).cast::<T>() })
    }

    #[inline(always)]
    fn object_mut<T: GcHeapObject>(&mut self, index: usize) -> Result<&mut T> {
        if index
            .checked_add(mem::size_of::<T>())
            .is_none_or(|end| end > self.len)
            || index % mem::align_of::<T>() != 0
        {
            bail_bug!("gc object out of bounds or misaligned");
        }
        // SAFETY: as for `object`, and `&mut self` makes this the only one.
        Ok(unsafe { &mut *self.base.add(index).cast::<T>() })
    }

    /// Where the object `from`, which the collection moves, now is: its
    /// forwarding reference, or a new copy.
    #[inline(always)]
    fn forward(&mut self, from: u32, marking: &mut EphemeronMarking) -> Result<u32> {
        debug_assert!(self.moves(from));
        let index = usize::try_from(from)?;
        let object = self.object::<VMCopyingHeaderAndForwardingRef>(index)?;
        if object.header.copied() {
            return match object.forwarding_ref.as_ref() {
                Some(to_ref) => Ok(to_ref.as_raw_u32()),
                None => bail_bug!("should always have a forwarding ref if the copied bit is set"),
            };
        }
        let size = object.header.object_size();
        // The kind's bits, without `kind()`'s validation of the whole word.
        let is_externref = object.header.header.raw_kind_matches(VMGcKind::ExternRef);
        self.copy(from, size, is_externref, marking)
    }

    /// Copy the object `from`, of `size` bytes, to the allocation pointer and
    /// leave its forwarding reference behind.
    #[inline]
    fn copy(
        &mut self,
        from: u32,
        size: u32,
        is_externref: bool,
        marking: &mut EphemeronMarking,
    ) -> Result<u32> {
        let to = self.bump;
        self.bump = match to.checked_add(size) {
            Some(bump) if bump <= self.end => bump,
            _ => bail_bug!(
                "\
there should always be enough room in the active semi-space for objects that \
survived collection, since the active space is the same size as the idle space",
            ),
        };
        debug_assert!(self.heap.is_in_active_space(to));
        let Some(to_ref) = VMGcRef::from_raw_u32(to) else {
            bail_bug!("invalid to_index");
        };
        let (from_start, to_start) = (usize::try_from(from)?, usize::try_from(to)?);
        let size = usize::try_from(size)?;
        if from_start.checked_add(size).is_none_or(|end| end > self.len)
            || to_start.checked_add(size).is_none_or(|end| end > self.len)
        {
            bail_bug!("corrupt gc heap");
        }
        {
            // SAFETY: the whole heap, while no other reference to its bytes
            // is live.
            let heap = unsafe { core::slice::from_raw_parts_mut(self.base, self.len) };
            copy_object(heap, from_start, to_start, size);
        }
        // `forward` checked the bounds and alignment of this header.
        debug_assert!(self.object::<VMCopyingHeaderAndForwardingRef>(from_start).is_ok());
        // SAFETY: as for `object_mut`.
        unsafe { &mut *self.base.add(from_start).cast::<VMCopyingHeaderAndForwardingRef>() }
            .set_forwarding_ref(to_ref.unchecked_copy());
        if is_externref {
            let next = self.externrefs.take();
            self.object_mut::<VMCopyingExternRef>(to_start)?.next_extern_ref = next;
            self.externrefs = Some(to_ref.unchecked_copy().into_externref_unchecked());
        }
        let Some(from_ref) = VMGcRef::from_raw_u32(from) else {
            bail_bug!("invalid from_index");
        };
        marking.copied(&from_ref);
        Ok(to)
    }

    /// The location of `raw` after this collection, if it has one yet. Objects
    /// the collection does not move are live: copies in the active semi-space,
    /// and old objects during a minor collection.
    #[inline]
    fn forwarded(&self, raw: u32) -> Result<Option<u32>> {
        if !self.moves(raw) {
            return Ok(Some(raw));
        }
        let object = self.object::<VMCopyingHeaderAndForwardingRef>(usize::try_from(raw)?)?;
        Ok(object.forwarding_ref()?.map(|to_ref| to_ref.as_raw_u32()))
    }

    /// Forward the reference in a remembered slot, which is not covered by the
    /// object bounds checked during `scan`.
    #[inline(always)]
    fn scan_remembered_slot(&mut self, field: usize, marking: &mut EphemeronMarking) -> Result<()> {
        if field.checked_add(4).is_none_or(|end| end > self.len) {
            bail_bug!("corrupt gc heap");
        }
        // SAFETY: the write barriers remember slots in allocated objects,
        // and the range is in bounds in the heap, which cannot grow or move
        // during evacuation.
        unsafe { self.scan_field_in_bounds(field, marking) }
    }

    /// Forward the reference in the four bytes at a validated heap offset.
    ///
    /// # Safety
    ///
    /// The four bytes starting at `field` must be within the heap allocation
    /// and readable and writable during evacuation.
    #[inline(always)]
    unsafe fn scan_field_in_bounds(
        &mut self,
        field: usize,
        marking: &mut EphemeronMarking,
    ) -> Result<()> {
        // SAFETY: the caller checked the range, and unaligned access is valid.
        let raw = u32::from_le(unsafe { self.base.add(field).cast::<u32>().read_unaligned() });
        if self.moves(raw) {
            let to = self.forward(raw, marking)?;
            // SAFETY: forwarding cannot resize or relocate the heap.
            unsafe {
                self.base
                    .add(field)
                    .cast::<u32>()
                    .write_unaligned(to.to_le())
            };
        }
        Ok(())
    }

    /// Trace the outgoing edges of the object at `index`, forwarding the
    /// referents the collection moves. Returns the object's size.
    fn scan(
        &mut self,
        index: u32,
        trace_infos: &mut TraceInfos,
        trace_state: &mut GcStoreTraceState<'_>,
        marking: &mut EphemeronMarking,
    ) -> Result<u32> {
        debug_assert!(self.heap.is_in_active_space(index));
        let start = usize::try_from(index)?;
        let header = self.object::<VMCopyingHeader>(start)?;
        let size = header.object_size();
        let inline_info = header.inline_trace_info();
        let ty = header.header.ty();
        // Every edge lies inside the object: check its extent once, and each
        // edge's offset against its size.
        if start
            .checked_add(usize::try_from(size)?)
            .is_none_or(|end| end > self.len)
        {
            bail_bug!("gc object out of bounds");
        }
        let inside = |offset: u32| -> Result<()> {
            match offset.checked_add(4) {
                Some(end) if end <= size => Ok(()),
                _ => bail_bug!("gc reference outside its object"),
            }
        };

        match inline_info {
            InlineTraceInfo::Struct { gc_ref_bitmap } => {
                // Every set bit names a slot at or below the highest set bit.
                if gc_ref_bitmap != 0 {
                    inside(HEADER_SIZE + (31 - gc_ref_bitmap.leading_zeros()) * 4)?;
                }
                let mut remaining = gc_ref_bitmap;
                while remaining != 0 {
                    let slot = remaining.trailing_zeros();
                    let field = start + usize::try_from(HEADER_SIZE + slot * 4)?;
                    // SAFETY: the highest slot fits in the checked object range.
                    unsafe { self.scan_field_in_bounds(field, marking)? };
                    remaining &= remaining - 1;
                }
            }
            InlineTraceInfo::Array { elems_are_gc_refs } => {
                if elems_are_gc_refs && !header.has_weak_array_elements() {
                    let len = self.object::<VMCopyingArrayHeader>(start)?.length;
                    if len == 0 {
                        return Ok(size);
                    }
                    let Some(end) = len
                        .checked_mul(4)
                        .and_then(|bytes| GC_REF_ARRAY_ELEMS_OFFSET.checked_add(bytes))
                    else {
                        bail_bug!("array element offset overflow");
                    };
                    if end > size {
                        bail_bug!("gc reference outside its object");
                    }
                    let elems = start + usize::try_from(GC_REF_ARRAY_ELEMS_OFFSET)?;
                    let end = start + usize::try_from(end)?;
                    for field in (elems..end).step_by(mem::size_of::<u32>()) {
                        // SAFETY: every element fits in the checked object range.
                        unsafe { self.scan_field_in_bounds(field, marking)? };
                    }
                }
            }
            InlineTraceInfo::OutOfLine => {
                let Some(ty) = ty else {
                    bail_bug!("out-of-line trace info but no type index");
                };
                match trace_infos.trace_info(&ty, trace_state) {
                    TraceInfo::Struct { gc_ref_offsets } => {
                        for &offset in gc_ref_offsets {
                            inside(offset)?;
                            let field = start + usize::try_from(offset)?;
                            // SAFETY: this field fits in the checked object range.
                            unsafe { self.scan_field_in_bounds(field, marking)? };
                        }
                    }
                    TraceInfo::Array { .. } => {
                        bail_bug!("arrays should always have inline trace info");
                    }
                }
            }
        }
        Ok(size)
    }
}

impl Drop for Evacuation<'_> {
    fn drop(&mut self) {
        debug_assert!(self.bump <= self.end);
        self.heap.set_bump_ptr(self.bump);
        self.heap.active_extern_ref_set_head = self.externrefs.take();
    }
}

unsafe impl GcHeap for CopyingHeap {
    fn supports_weak_refs(&self) -> bool {
        true
    }

    fn supports_weak_key_fingerprints(&self) -> bool {
        true
    }

    fn is_attached(&self) -> bool {
        debug_assert_eq!(self.memory.is_some(), self.vmmemory.is_some());
        self.memory.is_some()
    }

    fn attach(&mut self, memory: crate::vm::Memory) {
        assert!(!self.is_attached());
        assert!(!memory.is_shared_memory());
        self.vmmemory = Some(memory.vmmemory());
        self.memory = Some(memory);
        // A pooled heap object is reused with another slot or Store. Its
        // memory arrives accessible from offset 0 (the previous owner's
        // `detach` restored the guard; a fresh slot never had one), so the
        // guard is not protected until `protect_null_guard` runs for it.
        self.null_guard_protected = false;
        self.initialize_semi_spaces();

        if cfg!(gc_zeal) {
            self.heap_slice_mut().fill(POISON);
        }
    }

    fn detach(&mut self) -> crate::vm::Memory {
        assert!(self.is_attached());

        // Any open window is abandoned with the memory.
        self.young = YoungAllocation::Nursery;
        self.set_bump_ptr(0);
        self.set_active_space_end(0);
        self.vmctx_data.set_bump_ptr(0);
        self.vmctx_data.set_active_space_end(0);
        {
            let inner = self.vmctx_data.inner.get_mut();
            inner.remembered_len = 0;
            inner.remembered_overflow = 0;
        }
        // The next Store publishes its own table.
        self.vmctx_data.set_func_refs(crate::vm::FuncRefTableView::unpublished());

        // The memory's next owner may access every byte again: a pooling
        // allocator resets a returned slot from offset 0 and may hand it to
        // another Store's heap or linear memory. Nothing may reach the
        // deallocation with the guard still inaccessible.
        if self.null_guard_protected {
            let len = usize::try_from(self.null_guard).unwrap();
            // SAFETY: restoring access cannot invalidate any reference.
            let restored = unsafe { self.memory.as_mut().unwrap().set_prefix_access(len, true) };
            assert!(
                matches!(restored, Ok(true)),
                "restoring access to the GC heap's null guard: {restored:?}"
            );
            self.null_guard_protected = false;
        }

        let CopyingHeap {
            no_gc_count,
            memory,
            vmmemory,
            vmctx_data: _,
            active_space_start,
            idle_space_start,
            idle_space_end,
            worklist_ptr,
            active_extern_ref_set_head,
            idle_extern_ref_set_head,
            trace_infos,
            null_guard: _,
            null_guard_protected: _,
            nursery_end: _,
            old_bump,
            young: _,
            old_end,
            minor_requested,
            grew,
            collecting_minor,
            last_minor,
            direct_old,
            remembered: _,
            ephemeron_marking,
            young_extern_ref_set_head,
        } = self;

        *old_bump = 0;
        *old_end = 0;
        *minor_requested = false;
        *grew = false;
        *collecting_minor = false;
        *last_minor = false;
        direct_old.clear();
        *ephemeron_marking = EphemeronMarking::default();
        *young_extern_ref_set_head = None;
        *no_gc_count = 0;
        *vmmemory = None;
        *active_space_start = 0;
        *idle_space_start = 0;
        *idle_space_end = 0;
        *worklist_ptr = 0;
        *active_extern_ref_set_head = None;
        *idle_extern_ref_set_head = None;
        trace_infos.clear();

        memory.take().unwrap()
    }

    fn as_any(&self) -> &dyn Any {
        self as _
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self as _
    }

    fn enter_no_gc_scope(&mut self) {
        self.no_gc_count += 1;
    }

    fn exit_no_gc_scope(&mut self) {
        self.no_gc_count -= 1;
    }

    fn clone_gc_ref(&mut self, gc_ref: &VMGcRef) -> VMGcRef {
        // The copying collector doesn't use reference counting; cloning is a
        // simple copy.
        gc_ref.unchecked_copy()
    }

    fn refs_are_plain_copies(&self) -> bool {
        // Cloning is `unchecked_copy`, dropping overwrites a local and
        // exposing has no over-approximated root list.
        true
    }

    unsafe fn set_func_ref_table_view(&mut self, view: crate::vm::FuncRefTableView) {
        self.vmctx_data.set_func_refs(view);
    }

    fn drop_gc_ref_is_noop(&self) -> bool {
        // `drop_gc_ref` is the default `write_gc_ref(dest, None)`, a plain
        // overwrite of a local below.
        true
    }

    fn write_gc_ref(
        &mut self,
        destination: &mut Option<VMGcRef>,
        source: Option<&VMGcRef>,
    ) -> Result<()> {
        // The copying collector doesn't use reference counting; writes are
        // simple overwrites.
        *destination = source.map(|s| s.unchecked_copy());
        Ok(())
    }

    fn expose_gc_ref_to_wasm(&mut self, _gc_ref: VMGcRef) -> Result<()> {
        // The copying collector doesn't need any special handling when exposing
        // a GC ref to Wasm. There is no over-approximated-stack-roots list.
        Ok(())
    }

    fn alloc_externref(
        &mut self,
        host_data: ExternRefHostDataId,
    ) -> Result<Result<VMExternRef, u64>> {
        let align = usize::try_from(ALIGN).unwrap();
        let size = core::mem::size_of::<VMCopyingExternRef>();
        let size = (size + align - 1) & !(align - 1);
        let mut header = VMGcHeader::externref();
        header.set_reserved_u26(InlineTraceInfo::NO_GC_EDGES.encode());
        let gc_ref = match self.alloc_raw(header, Layout::from_size_align(size, align)?)? {
            Err(n) => return Ok(Err(n)),
            Ok(gc_ref) => gc_ref,
        };
        // Take the old list head before borrowing self mutably through
        // index_mut. A nursery externref joins the nursery's list.
        let young = self.is_young(gc_ref.heap_index()?.get());
        let old_head = if young {
            self.young_extern_ref_set_head.take()
        } else {
            self.active_extern_ref_set_head.take()
        };
        let externref_obj = self.index_mut::<VMCopyingExternRef>(gc_ref.as_typed_unchecked())?;
        externref_obj.host_data = host_data;
        externref_obj.next_extern_ref = old_head;
        let externref = gc_ref.into_externref_unchecked();
        if young {
            self.young_extern_ref_set_head = Some(externref.unchecked_copy());
        } else {
            self.active_extern_ref_set_head = Some(externref.unchecked_copy());
        }
        Ok(Ok(externref))
    }

    fn externref_host_data(&self, externref: &VMExternRef) -> Result<ExternRefHostDataId> {
        let typed_ref = externref_to_copying(externref);
        Ok(self.index(typed_ref)?.host_data)
    }

    fn header(&self, gc_ref: &VMGcRef) -> Result<&VMGcHeader> {
        let header: &VMGcHeader = self.index(gc_ref.as_typed_unchecked())?;

        debug_assert!(
            VMGcKind::try_from_u32(header.kind().as_u32()).is_some(),
            "header: invalid VMGcKind {:#010x} at gc_ref {gc_ref:#p}",
            header.kind().as_u32(),
        );

        Ok(header)
    }

    fn header_mut(&mut self, gc_ref: &VMGcRef) -> Result<&mut VMGcHeader> {
        let header: &mut VMGcHeader = self.index_mut(gc_ref.as_typed_unchecked())?;

        debug_assert!(
            VMGcKind::try_from_u32(header.kind().as_u32()).is_some(),
            "header_mut: invalid VMGcKind {:#010x} at gc_ref {gc_ref:#p}",
            header.kind().as_u32(),
        );

        Ok(header)
    }

    fn object_size(&self, gc_ref: &VMGcRef) -> Result<usize> {
        Ok(usize::try_from(
            self.index(copying_ref(gc_ref))?.object_size(),
        )?)
    }

    fn alloc_raw(&mut self, header: VMGcHeader, layout: Layout) -> Result<Result<VMGcRef, u64>> {
        let align = u32::try_from(layout.align()).unwrap();
        ensure!(
            align == ALIGN,
            "copying collector requires all allocations to have alignment {ALIGN}, \
             but got alignment {align}",
        );

        debug_assert!(layout.size() >= core::mem::size_of::<VMCopyingHeader>());
        debug_assert_eq!(
            self.bump_ptr() % ALIGN,
            0,
            "bump_ptr is not aligned to ALIGN"
        );
        debug_assert_eq!(header.reserved_u26() & HEADER_COPIED_BIT, 0);

        let size = u32::try_from(layout.size()).unwrap();
        // Round up the allocation size to ALIGN so that the next bump-pointer
        // allocation is also aligned.
        let size = size
            .checked_next_multiple_of(ALIGN)
            .ok_or_else(|| crate::Trap::AllocationTooLarge)?;

        let index = if self.nursery_end == 0 {
            self.allocate(size)
        } else {
            self.allocate_with_nursery(size)
        };
        let gc_ref = match index {
            None => {
                // Multiply by two because we need capacity in both semi-spaces.
                let bytes_needed = u64::try_from(layout.size()).unwrap().saturating_mul(2);
                return Ok(Err(bytes_needed));
            }
            Some(index) => {
                debug_assert_ne!(index, 0, "index 0 is reserved; bump_ptr should skip it");
                VMGcRef::from_heap_index(NonZeroU32::new(index).unwrap()).unwrap()
            }
        };

        // Assert that the newly-allocated memory is still filled with the
        // poison pattern.
        if cfg!(gc_zeal) {
            let start = usize::try_from(gc_ref.heap_index()?.get()).unwrap();
            let slice = &self.heap_slice()[start..][..layout.size()];
            gc_assert!(
                slice.iter().all(|&b| b == POISON),
                "newly allocated GC object at index {start} is not fully poisoned; \
                 freed memory was corrupted",
            );
        }

        let object_size = size;
        *self.index_mut(copying_ref(&gc_ref))? = VMCopyingHeader {
            header,
            object_size,
        };
        Ok(Ok(gc_ref))
    }

    fn alloc_uninit_struct_or_exn(
        &mut self,
        ty: VMSharedTypeIndex,
        layout: &GcStructLayout,
    ) -> Result<Result<VMGcRef, u64>> {
        let kind = if layout.is_exception {
            VMGcKind::ExnRef
        } else {
            VMGcKind::StructRef
        };
        let mut header = VMGcHeader::from_kind_and_index(kind, ty);
        header.set_reserved_u26(InlineTraceInfo::r#struct(layout).encode());
        let gc_ref = match self.alloc_raw(header, layout.layout())? {
            Err(n) => return Ok(Err(n)),
            Ok(gc_ref) => gc_ref,
        };

        Ok(Ok(gc_ref))
    }

    fn dealloc_uninit_struct_or_exn(&mut self, _gcref: VMGcRef) -> Result<()> {
        // The copying collector doesn't support individual deallocation; memory
        // is reclaimed during collection.
        Ok(())
    }

    fn alloc_uninit_array(
        &mut self,
        ty: VMSharedTypeIndex,
        length: u32,
        layout: &GcArrayLayout,
    ) -> Result<Result<VMArrayRef, u64>> {
        let mut header = VMGcHeader::from_kind_and_index(VMGcKind::ArrayRef, ty);
        header.set_reserved_u26(InlineTraceInfo::array(layout).encode());
        let layout = layout
            .layout(length)
            .ok_or_else(|| crate::Trap::AllocationTooLarge)?;

        let gc_ref = match self.alloc_raw(header, layout)? {
            Err(n) => return Ok(Err(n)),
            Ok(gc_ref) => gc_ref,
        };

        self.index_mut(gc_ref.as_typed_unchecked::<VMCopyingArrayHeader>())?
            .length = length;

        Ok(Ok(gc_ref.into_arrayref_unchecked()))
    }

    fn dealloc_uninit_array(&mut self, _arrayref: VMArrayRef) -> Result<()> {
        // The copying collector doesn't support individual deallocation; memory
        // is reclaimed during collection.
        Ok(())
    }

    fn array_len(&self, arrayref: &VMArrayRef) -> Result<u32> {
        debug_assert!(arrayref.as_gc_ref().is_typed::<VMCopyingArrayHeader>(self));
        Ok(self
            .index::<VMCopyingArrayHeader>(arrayref.as_gc_ref().as_typed_unchecked())?
            .length)
    }

    fn allocated_bytes(&self) -> usize {
        usize::try_from(self.bump_ptr() - self.active_space_start).unwrap()
    }

    fn should_collect_before_growth(
        &self,
        bytes_needed: u64,
        last_post_gc_allocated_bytes: usize,
    ) -> bool {
        // A full nursery whose objects all fit the old space is collected.
        if self.minor_requested {
            return true;
        }
        let capacity = usize::try_from(self.active_space_end() - self.active_space_start).unwrap();

        // Only the active semi-space can satisfy allocations. Wait for a
        // collection to leave less than one eighth of that space as headroom
        // before preferring growth: a half-full live set can keep reusing the
        // existing heap, while a nearly full retained graph cannot cause a
        // long sequence of ever-less-productive copying collections.
        // Use the current regions, since growth can defer their resizing until
        // the next flip. alloc_raw reports twice the allocation's size to
        // reserve room in both semi-spaces; convert back to allocation bytes.
        super::super::should_collect_first(
            bytes_needed.div_ceil(2),
            capacity,
            last_post_gc_allocated_bytes,
            capacity - capacity / 8,
        )
    }

    fn gc<'a, 'b>(
        &'a mut self,
        roots: GcRootsIter<'a>,
        trace_state: &'a mut GcStoreTraceState<'b>,
    ) -> Box<dyn GarbageCollection + 'a>
    where
        'b: 'a,
    {
        assert_eq!(self.no_gc_count, 0, "Cannot GC inside a no-GC scope!");
        self.close_pretenure_window();
        // Only a full nursery asks for a minor collection, and only while the
        // old space can take everything in it; any other collection, explicit
        // ones included, is full. So is the one that follows a pretenure
        // window: a minor collection would have to find the nursery
        // references in the window's objects, which no barrier recorded and
        // no scan may look for (see `YoungAllocation`).
        self.collecting_minor = self.nursery_end != 0
            && self.minor_requested
            && match self.young {
                YoungAllocation::FirstNursery | YoungAllocation::Nursery => true,
                YoungAllocation::NurseryAfterWindow => false,
                YoungAllocation::Pretenuring { .. } => {
                    unreachable!("the window was just closed")
                }
            };
        let marking = mem::take(&mut self.ephemeron_marking);
        Box::new(CopyingCollection {
            roots: Some(roots),
            trace_state,
            heap: self,
            phase: CopyingCollectionPhase::ProcessRoots,
            weak_cursor: 0,
            marking,
        })
    }

    unsafe fn vmctx_gc_heap_data(&self) -> NonNull<u8> {
        NonNull::from(&self.vmctx_data).cast::<u8>()
    }

    fn take_memory(&mut self) -> crate::vm::Memory {
        debug_assert!(self.is_attached());
        self.vmmemory.take();
        self.memory.take().unwrap()
    }

    fn needs_gc_before_next_growth(&self) -> bool {
        // We need a GC before growth when the active space is the second half
        // of the GC heap, because we cannot safely extend the active space in
        // that configuration without making it larger than the idle space.
        self.idle_space_start < self.active_space_start
    }

    fn min_growth_bytes(&self, bytes_needed: u64) -> u64 {
        let alloc_size = bytes_needed
            .div_ceil(2)
            .checked_next_multiple_of(u64::from(ALIGN))
            .unwrap_or(u64::MAX);
        let floor = u64::from(self.null_guard.max(self.nursery_end));
        let nursery_window = if self.nursery_end != 0 {
            u64::from(self.nursery_end.saturating_sub(self.nursery_start()))
        } else {
            0
        };
        let old_used = if self.active_space_end() == 0 && self.idle_space_end == 0 {
            0
        } else {
            u64::from(self.bump_ptr().saturating_sub(self.active_space_start))
        };
        let required_per_space = old_used
            .saturating_add(alloc_size)
            .saturating_add(nursery_window);
        let required_total = floor.saturating_add(required_per_space.saturating_mul(2));
        required_total
            .saturating_sub(u64::from(self.capacity()))
            .max(1)
    }

    unsafe fn replace_memory(&mut self, memory: crate::vm::Memory, delta_bytes_grown: u64) {
        debug_assert!(self.memory.is_none());
        debug_assert!(!memory.is_shared_memory());
        self.vmmemory = Some(memory.vmmemory());
        self.memory = Some(memory);

        // The semi-spaces below are resized around the runtime's own old
        // allocation pointer, and the nursery's limit is recomputed.
        self.close_pretenure_window();

        // If the heap was previously empty, reinitialize the semi-spaces from
        // scratch.
        if self.active_space_end() == 0 && self.idle_space_end == 0 {
            self.initialize_semi_spaces();
        } else {
            // Otherwise the memory was grown: try to resize the semi-spaces
            // accordingly.
            self.resize_semi_spaces();
            if self.nursery_end != 0 {
                // More of the nursery may be backed, and the old space may
                // have grown; the window only widens.
                let end = self
                    .nursery_window_end()
                    .max(self.vmctx_data.active_space_end());
                self.vmctx_data.set_active_space_end(end);
                self.grew |= delta_bytes_grown > 0;
            }
        }

        // Poison the newly-grown region.
        if cfg!(gc_zeal) && delta_bytes_grown > 0 {
            let slice = self.heap_slice_mut();
            let len = slice.len();
            let delta_bytes_grown = usize::try_from(delta_bytes_grown).unwrap();
            slice[len - delta_bytes_grown..].fill(POISON);
        }

        // Growth from a heap smaller than the null guard made it accessible.
        // `protect_null_guard` already established that the backing supports
        // protection, so only an OS failure can fail here, and compiled code
        // must not run without the guard.
        let protected = self.protect_null_guard_pages();
        assert!(
            matches!(protected, Ok(true)),
            "protecting the GC heap's null guard after growth: {protected:?}"
        );
    }

    fn last_collection_was_minor(&self) -> bool {
        self.last_minor
    }

    fn write_barrier(&mut self, object: &VMGcRef, offset: u32, value: Option<&VMGcRef>) {
        if self.nursery_end == 0 || object.is_i31() {
            return;
        }
        let Some(value) = value.filter(|v| !v.is_i31()) else {
            return;
        };
        let (Ok(object), Ok(value)) = (object.heap_index(), value.heap_index()) else {
            return;
        };
        if !self.is_young(object.get()) && self.is_young(value.get()) {
            self.remember_slot(object.get() + offset);
        }
    }

    fn protect_null_guard(&mut self) -> Result<()> {
        if !self.protect_null_guard_pages()? {
            bail!(
                "the GC heap's memory cannot keep its first {:#x} bytes inaccessible, \
                 which compiled code relies on to detect null references",
                self.null_guard
            );
        }
        Ok(())
    }

    fn null_guard_len(&self) -> usize {
        // Protected, or (in a heap still without memory there) not yet
        // accessible: `protect_null_guard` succeeded for every attached heap.
        usize::try_from(self.null_guard).unwrap()
    }

    #[inline]
    fn vmmemory(&self) -> VMMemoryDefinition {
        debug_assert!(self.is_attached());
        debug_assert!(!self.memory.as_ref().unwrap().is_shared_memory());
        let vmmemory = self.vmmemory.as_ref().unwrap();
        VMMemoryDefinition {
            base: vmmemory.base,
            current_length: AtomicUsize::new(vmmemory.current_length()),
        }
    }
}

const OBJECTS_PER_INCREMENT: usize = 16_384;

struct CopyingCollection<'a, 'b> {
    roots: Option<GcRootsIter<'a>>,
    trace_state: &'a mut GcStoreTraceState<'b>,
    heap: &'a mut CopyingHeap,
    phase: CopyingCollectionPhase,
    weak_cursor: usize,
    marking: EphemeronMarking,
}

enum CopyingCollectionPhase {
    ProcessRoots,
    ProcessWorklist,
    ProcessEphemerons,
    UpdateWeak,
    SweepExternRefs,
    Done,
}

impl CopyingCollection<'_, '_> {
    /// Forward all GC roots from the idle space to the active space.
    fn process_roots(&mut self) -> Result<()> {
        log::trace!("Begin processing GC roots");
        let roots = self.roots.take().unwrap();
        let mut evacuation = Evacuation::new(self.heap);
        for mut root in roots {
            let raw = root.get()?.as_raw_u32();
            if !evacuation.moves(raw) {
                // An `i31`, or an old object a minor collection leaves in place.
                debug_assert!(raw & 1 == 1 || evacuation.heap.collecting_minor);
                continue;
            }
            let to = evacuation.forward(raw, &mut self.marking)?;
            let Some(to_ref) = VMGcRef::from_raw_u32(to) else {
                bail_bug!("forwarded root is null");
            };
            root.set(to_ref);
        }
        log::trace!("End processing GC roots");
        Ok(())
    }

    /// The minor collection's other roots: old-space slots the write barriers
    /// recorded, objects allocated directly old since the last collection,
    /// and, when the recorded slots overflowed, every old object.
    fn process_remembered(&mut self, old_end: u32) -> Result<()> {
        let mut trace_infos = mem::take(&mut self.heap.trace_infos);
        let inner = self.heap.vmctx_data.inner.get_mut();
        let len = usize::try_from(inner.remembered_len)?;
        let overflowed = inner.remembered_overflow != 0;
        if len > self.heap.remembered.len() {
            bail_bug!("remembered slot count exceeds the buffer");
        }
        let start = self.heap.active_space_start;
        let mut evacuation = Evacuation::new(self.heap);
        if overflowed {
            // Old objects are contiguous from the space's first object.
            let mut index = start;
            while index < old_end {
                let size =
                    evacuation.scan(index, &mut trace_infos, self.trace_state, &mut self.marking)?;
                index = match index.checked_add(size) {
                    Some(next) if size != 0 => next,
                    _ => bail_bug!("corrupt old space object size"),
                };
            }
        } else {
            for i in 0..len {
                let slot = evacuation.heap.remembered[i];
                if !self.marking.arrays.should_skip_remembered(slot) {
                    evacuation.scan_remembered_slot(usize::try_from(slot)?, &mut self.marking)?;
                }
            }
            for i in 0..evacuation.heap.direct_old.len() {
                let index = evacuation.heap.direct_old[i];
                evacuation.scan(index, &mut trace_infos, self.trace_state, &mut self.marking)?;
            }
        }
        drop(evacuation);
        self.heap.trace_infos = trace_infos;
        Ok(())
    }

    /// Handle the `ProcessRoots` phase: flip semi-spaces, initialize the
    /// worklist, forward all roots, then transition to `ProcessWorklist`.
    fn begin_collection(&mut self) -> Result<GcProgress> {
        log::trace!("Begin copying collection: processing roots");

        assert!(self.heap.active_space_start <= self.heap.bump_ptr());
        assert!(self.heap.bump_ptr() <= self.heap.active_space_end());
        assert!(self.heap.idle_space_start <= self.heap.idle_space_end);
        assert!(
            self.heap.active_space_end() <= self.heap.idle_space_start
                || self.heap.idle_space_end <= self.heap.active_space_start
        );

        // Complete every allocation and the checked cache epoch change before
        // moving any object. Collection-local indices do not retain referents.
        self.prepare_weak_marking()?;
        if self.heap.collecting_minor {
            // Promoted objects are copied after the old space's objects and
            // scanned from there.
            let old_end = self.heap.bump_ptr();
            self.heap.initialize_worklist();
            self.process_roots()?;
            self.process_remembered(old_end)?;
        } else {
            self.heap.flip();
            self.heap.initialize_worklist();
            self.process_roots()?;
        }

        self.phase = CopyingCollectionPhase::ProcessWorklist;
        Ok(GcProgress::Continue)
    }

    /// Scan up to `OBJECTS_PER_INCREMENT` grey objects from the worklist.
    ///
    /// When drained, process conditional edges before any weak target clears.
    /// Always returns `GcProgress::Continue` so the caller yields after each
    /// increment regardless of whether the worklist still has items.
    fn process_worklist_increment(&mut self) -> Result<GcProgress> {
        log::trace!("Begin processing worklist increment");
        let mut trace_infos = mem::take(&mut self.heap.trace_infos);
        let mut worklist = self.heap.worklist_ptr;
        let mut evacuation = Evacuation::new(self.heap);
        let mut count = 0;
        let has_more = loop {
            if count >= OBJECTS_PER_INCREMENT {
                break true;
            }
            // Grey objects are the copies between the worklist pointer and the
            // bump pointer, in allocation order.
            let index = worklist;
            if index == evacuation.bump {
                break false;
            }
            let size =
                evacuation.scan(index, &mut trace_infos, self.trace_state, &mut self.marking)?;
            worklist = match index.checked_add(size) {
                Some(next) if size != 0 && next <= evacuation.bump => next,
                _ => bail_bug!("corrupt worklist object size"),
            };
            count += 1;
        };
        drop(evacuation);
        self.heap.worklist_ptr = worklist;
        self.heap.trace_infos = trace_infos;
        if !has_more {
            self.weak_cursor = 0;
            self.phase = CopyingCollectionPhase::ProcessEphemerons;
        }
        log::trace!("End processing worklist increment (has_more={has_more})");
        Ok(GcProgress::Continue)
    }

    /// Clean up dead externrefs by iterating the idle semi-space's externref
    /// linked list and deallocating host data for any that were not forwarded.
    fn sweep_extern_refs(&mut self) -> Result<()> {
        log::trace!("Begin sweeping `externref`s");
        let young = self.heap.young_extern_ref_set_head.take();
        self.sweep_extern_ref_list(young)?;
        if !self.heap.collecting_minor {
            let old = self.heap.idle_extern_ref_set_head.take();
            self.sweep_extern_ref_list(old)?;
        }
        log::trace!("End sweeping `externref`s");
        Ok(())
    }

    /// Deallocates the host data of every uncopied `externref` on a list of
    /// evacuated ones.
    fn sweep_extern_ref_list(&mut self, mut link: Option<VMExternRef>) -> Result<()> {
        while let Some(externref) = link {
            let gc_ref = externref.as_gc_ref();
            debug_assert!(self.heap.is_in_from_space(gc_ref.heap_index()?.get()));
            let header = self.heap.index(copying_ref(gc_ref))?;
            if !header.copied() {
                let typed: &TypedGcRef<VMCopyingExternRef> = gc_ref.as_typed_unchecked();
                let host_data_id = self.heap.index(typed)?.host_data;
                self.trace_state.host_data_table.dealloc(host_data_id)?;
            }
            link = self
                .heap
                .index::<VMCopyingExternRef>(gc_ref.as_typed_unchecked())?
                .next_extern_ref
                .as_ref()
                .map(|e| e.unchecked_copy());
        }
        Ok(())
    }

    /// Handle the `SweepExternRefs` phase: sweep dead externrefs, resize
    /// semi-spaces, and transition to `Done`.
    fn finish_collection(&mut self) -> Result<GcProgress> {
        log::trace!("Copying collection: sweeping externrefs");

        assert!(self.heap.active_space_start <= self.heap.bump_ptr());
        assert!(self.heap.bump_ptr() <= self.heap.active_space_end());
        assert!(self.heap.idle_space_start <= self.heap.idle_space_end);
        assert!(
            self.heap.active_space_end() <= self.heap.idle_space_start
                || self.heap.idle_space_end <= self.heap.active_space_start
        );

        self.sweep_extern_refs()?;
        if self.heap.collecting_minor {
            self.heap.finish_young_generation()?;
            log::trace!("End minor copying collection");
            self.phase = CopyingCollectionPhase::Done;
            return Ok(GcProgress::Complete);
        }
        self.heap.resize_semi_spaces();

        debug_assert_eq!(
            self.heap.active_space_end() - self.heap.active_space_start,
            self.heap.idle_space_end - self.heap.idle_space_start,
            "the active and idle spaces should be the same size"
        );

        if cfg!(gc_zeal) {
            let idle_start = usize::try_from(self.heap.idle_space_start).unwrap();
            let idle_end = usize::try_from(self.heap.idle_space_end).unwrap();
            self.heap.heap_slice_mut()[idle_start..idle_end].fill(POISON);
        } else {
            let range = self.heap.idle_space_start as usize..self.heap.idle_space_end as usize;
            // Weak/ephemeron processing and the externref sweep have consumed
            // every old header/forwarding link. Resizing has finalized the
            // boundaries; only the wholly dead idle semi-space is discarded.
            // The owning allocation keeps it RW for the next copying flip.
            if let Some(memory) = self.heap.memory.as_mut() {
                // SAFETY: no live objects or outstanding collector references
                // remain in idle space after all collection phases above.
                if let Err(error) = unsafe { memory.discard_unused_pages(range) } {
                    // Reclamation is optional; an OS refusal changes neither
                    // collection semantics nor the allocator's capacity.
                    log::debug!("idle semi-space backing reclamation failed: {error:#}");
                }
            }
        }
        // With the semi-spaces final, the nursery's window can be sized.
        self.heap.finish_young_generation()?;

        log::trace!("End copying collection");
        self.phase = CopyingCollectionPhase::Done;
        Ok(GcProgress::Complete)
    }
}

impl GarbageCollection for CopyingCollection<'_, '_> {
    fn collect_increment(&mut self) -> Result<GcProgress> {
        match self.phase {
            CopyingCollectionPhase::ProcessRoots => self.begin_collection(),
            CopyingCollectionPhase::ProcessWorklist => self.process_worklist_increment(),
            CopyingCollectionPhase::ProcessEphemerons => self.process_ephemerons_increment(),
            CopyingCollectionPhase::UpdateWeak => self.update_weak_increment(),
            CopyingCollectionPhase::SweepExternRefs => self.finish_collection(),
            CopyingCollectionPhase::Done => Ok(GcProgress::Complete),
        }
    }
}

impl Drop for CopyingCollection<'_, '_> {
    fn drop(&mut self) {
        // A collection is logically atomic with respect to the mutator: the
        // mutator must never run while the heap is in a half-collected
        // (half-flipped, half-forwarded) state. The collection only yields
        // between increments to cooperate with the async runtime and
        // `Future::poll`; it does not interleave with the mutator.
        //
        // However, the future driving an incremental collection can be dropped
        // before the collection completes (e.g. when a `call_async` future is
        // cancelled while a GC is in progress). If we did nothing here, the
        // store would be left with a half-collected GC heap and reusing it
        // (e.g. starting another `call_async`) would corrupt the GC heap. So,
        // to uphold the logical atomicity of collection, we finish any
        // in-progress collection here synchronously when dropped.
        match self.phase {
            CopyingCollectionPhase::ProcessWorklist
            | CopyingCollectionPhase::ProcessEphemerons
            | CopyingCollectionPhase::UpdateWeak
            | CopyingCollectionPhase::SweepExternRefs => {
                if let Err(e) = self.collect() {
                    log::error!("error finishing an interrupted copying collection: {e:?}");
                }
            }
            CopyingCollectionPhase::ProcessRoots | CopyingCollectionPhase::Done => {}
        }
        // Even a failed preparation may have reserved part of the scratch.
        self.marking.reset();
        self.heap.ephemeron_marking = mem::take(&mut self.marking);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime_environ::{HostPtr, PtrSize};

    #[test]
    fn vm_copying_header_size_align() {
        assert_eq!(
            wasmtime_environ::copying::HEADER_SIZE as usize,
            core::mem::size_of::<VMCopyingHeader>(),
        );
        // The struct's natural alignment must not exceed ALIGN, which is
        // enforced by the bump allocator.
        assert!(
            core::mem::align_of::<VMCopyingHeader>() <= wasmtime_environ::copying::ALIGN as usize,
        );
    }

    #[test]
    fn vm_copying_array_header_length_offset() {
        assert_eq!(
            wasmtime_environ::copying::ARRAY_LENGTH_OFFSET,
            u32::try_from(core::mem::offset_of!(VMCopyingArrayHeader, length)).unwrap(),
        );
    }

    #[test]
    fn vm_copying_header_object_size_offset() {
        assert_eq!(
            // object_size comes right after the VMGcHeader
            wasmtime_environ::VM_GC_HEADER_SIZE,
            u32::try_from(core::mem::offset_of!(VMCopyingHeader, object_size)).unwrap(),
        );
    }

    #[test]
    fn vm_copying_forwarding_ref_offset() {
        assert_eq!(
            wasmtime_environ::copying::FORWARDING_REF_OFFSET as usize,
            core::mem::offset_of!(VMCopyingHeaderAndForwardingRef, forwarding_ref),
        );
    }

    #[test]
    fn vm_copying_header_and_forwarding_ref_size() {
        // The forwarding ref data (at FORWARDING_REF_OFFSET) plus its size
        // must fit within MIN_OBJECT_SIZE, so every object has room for
        // the forwarding pointer during collection.
        assert!(
            wasmtime_environ::copying::FORWARDING_REF_OFFSET as usize
                + core::mem::size_of::<Option<VMGcRef>>()
                <= wasmtime_environ::copying::MIN_OBJECT_SIZE as usize,
        );
    }

    #[test]
    fn vm_copying_heap_data_bump_ptr_offset() {
        assert_eq!(
            HostPtr.vmcopying_heap_data_bump_ptr() as usize,
            core::mem::offset_of!(VMCopyingHeapDataInner, bump_ptr),
        );
    }

    #[test]
    fn vm_copying_heap_data_active_space_end_offset() {
        assert_eq!(
            HostPtr.vmcopying_heap_data_active_space_end() as usize,
            core::mem::offset_of!(VMCopyingHeapDataInner, active_space_end),
        );
    }

    #[test]
    fn vm_copying_heap_data_size() {
        // wasmtime-environ describes the two-field prefix; the funcref view
        // fields follow it.
        assert_eq!(
            HostPtr.size_of_vmcopying_heap_data() as usize,
            core::mem::offset_of!(VMCopyingHeapDataInner, func_refs_len),
        );
        assert_eq!(
            core::mem::size_of::<VMCopyingHeapData>(),
            core::mem::size_of::<VMCopyingHeapDataInner>(),
        );
    }

    #[test]
    fn vm_copying_heap_data_align() {
        assert!(
            core::mem::align_of::<VMCopyingHeapDataInner>()
                >= HostPtr.align_of_vmcopying_heap_data() as usize
        );
        assert_eq!(
            core::mem::align_of::<VMCopyingHeapData>(),
            core::mem::align_of::<VMCopyingHeapDataInner>(),
        );
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn vm_copying_heap_data_func_refs_offsets() {
        use wasmtime_cranelift::copying_heap_data;
        assert_eq!(
            copying_heap_data::FUNC_REFS_LEN as usize,
            core::mem::offset_of!(VMCopyingHeapDataInner, func_refs_len),
        );
        assert_eq!(
            copying_heap_data::func_refs(core::mem::size_of::<usize>() as u32) as usize,
            core::mem::offset_of!(VMCopyingHeapDataInner, func_refs),
        );
        let pointer = core::mem::size_of::<usize>() as u32;
        assert_eq!(
            copying_heap_data::func_ref_cache(pointer) as usize,
            core::mem::offset_of!(VMCopyingHeapDataInner, func_ref_cache),
        );
        assert_eq!(
            copying_heap_data::FUNC_REF_CACHE_ENTRIES as usize,
            crate::vm::FUNC_REF_CACHE_ENTRIES,
        );
        assert_eq!(
            copying_heap_data::func_ref_cache_entry_bytes(pointer) as usize,
            core::mem::size_of::<crate::vm::FuncRefCacheEntry>(),
        );
        assert_eq!(
            copying_heap_data::remembered_len(pointer) as usize,
            core::mem::offset_of!(VMCopyingHeapDataInner, remembered_len),
        );
        assert_eq!(
            copying_heap_data::remembered_cap(pointer) as usize,
            core::mem::offset_of!(VMCopyingHeapDataInner, remembered_cap),
        );
        assert_eq!(
            copying_heap_data::remembered_overflow(pointer) as usize,
            core::mem::offset_of!(VMCopyingHeapDataInner, remembered_overflow),
        );
        assert_eq!(
            copying_heap_data::remembered(pointer) as usize,
            core::mem::offset_of!(VMCopyingHeapDataInner, remembered),
        );
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn compiled_code_and_runtime_agree_on_the_nursery() {
        for size in [
            0,
            64 << 10,
            1 << 20,
            2 << 20,
            16 << 20,
            32 << 20,
            64 << 20,
            1 << 32,
        ] {
            assert_eq!(
                super::nursery_end_for_initial_size(size),
                wasmtime_cranelift::copying_heap_data::nursery_end(size),
                "{size}"
            );
        }
        assert_eq!(super::nursery_end_for_initial_size(16 << 20), 2 << 20);
        assert_eq!(super::nursery_end_for_initial_size(32 << 20), 4 << 20);
        assert_eq!(super::nursery_end_for_initial_size(1 << 20), 0);
    }

    #[test]
    fn semi_space_split_stays_aligned() {
        fn check(len: usize) {
            let capacity = round_down_heap_capacity(len);
            assert_eq!(
                capacity % (2 * ALIGN),
                0,
                "capacity {capacity:#x} for len {len:#x} is not a multiple of 2*ALIGN",
            );
            assert_eq!(
                (capacity / 2) % ALIGN,
                0,
                "halfway {:#x} for len {len:#x} is not a multiple of ALIGN",
                capacity / 2,
            );
        }

        for len in [
            0usize,
            usize::try_from(ALIGN).unwrap(),
            usize::try_from(2 * ALIGN).unwrap(),
            4096,
            1 << 16,
            // Largest page-aligned value below 2^32.
            0xffff_0000,
            usize::try_from(u32::MAX).unwrap(),
        ] {
            check(len);
        }

        #[cfg(target_pointer_width = "64")]
        for len in [1usize << 32, 3usize << 32] {
            check(len);
        }
    }

    #[test]
    fn allocation_growth_policy_uses_active_space_and_allocation_bytes() {
        let mut heap = CopyingHeap::new(0, 0);
        assert!(!heap.should_collect_before_growth(0, 0));
        assert!(!heap.should_collect_before_growth(32, 0));

        let capacity = 8 * 1024 * 1024;
        // Both semi-space orientations, including an old region size when a
        // larger backing memory has not yet been incorporated by a flip.
        for start in [0, capacity] {
            heap.active_space_start = start;
            heap.set_active_space_end(start + capacity);

            assert!(heap.should_collect_before_growth(0, capacity as usize));
            assert!(heap.should_collect_before_growth(32, 0));
            assert!(heap.should_collect_before_growth(32, capacity as usize * 3 / 4));
            assert!(!heap.should_collect_before_growth(32, capacity as usize * 15 / 16));

            // A 1 MiB object needs 2 MiB of backing growth. Treating that as
            // 2 MiB of active allocation would incorrectly grow this heap.
            assert!(heap.should_collect_before_growth(2 * 1024 * 1024, 11 * 1024 * 1024 / 2));
            assert!(!heap.should_collect_before_growth(2 * u64::from(capacity), 0));
        }
    }
}

#[cfg(all(test, feature = "std", target_os = "linux", not(miri), not(gc_zeal)))]
mod reclamation_tests {
    use crate::{
        AnyRef, Collector, Config, Engine, ExternRef, FieldType, GcEphemeron, Mutability,
        RootScope, Rooted, StorageType, Store, StructRef, StructRefPre, StructType, Val, ValType,
        WeakAnyRef,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct Dropped(Arc<AtomicUsize>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn repeated_flips_and_growth_preserve_survivors_weak_edges_and_externref_order() {
        let mut config = Config::new();
        config
            .enable_compiler(false)
            .wasm_gc(true)
            .wasm_function_references(true)
            .collector(Collector::Copying)
            .gc_heap_initial_size(64 * 1024);
        let engine = Engine::new(&config).unwrap();
        let ty = StructType::new(
            &engine,
            [FieldType::new(
                Mutability::Const,
                StorageType::ValType(ValType::I32),
            )],
        )
        .unwrap();
        let mut store = Store::new(&engine, ());
        let pre = StructRefPre::new(&mut store, ty);
        let owner = StructRef::new(&mut store, &pre, &[Val::I32(1)])
            .unwrap()
            .to_anyref();
        let drops = Arc::new(AtomicUsize::new(0));
        let (key, entry, weak) = {
            let mut roots = RootScope::new(&mut store);
            let key = StructRef::new(&mut roots, &pre, &[Val::I32(42)])
                .unwrap()
                .to_anyref();
            let native = ExternRef::new(&mut roots, Dropped(Arc::clone(&drops))).unwrap();
            let value = AnyRef::convert_extern(&mut roots, native).unwrap();
            let entry = GcEphemeron::new(&mut roots, &owner, &key, Some(&value)).unwrap();
            let weak = WeakAnyRef::new(&mut roots, &owner, &value).unwrap();
            (key.to_owned_rooted(&mut roots).unwrap(), entry, weak)
        };
        for _ in 0..8 {
            {
                let mut roots = RootScope::new(&mut store);
                // Root enough objects simultaneously to require real growth,
                // then release them before the next copying flip.
                for marker in 0..4096 {
                    StructRef::new(&mut roots, &pre, &[Val::I32(marker)]).unwrap();
                }
            }
            store.gc(None).unwrap();
            assert!(store.gc_heap_capacity() > 64 * 1024);
            let mut roots = RootScope::new(&mut store);
            assert!(entry.matches_key(&mut roots, &key).unwrap());
            let value = weak.upgrade(&mut roots).unwrap().unwrap();
            let native = ExternRef::convert_any(&mut roots, value).unwrap();
            assert!(native.data(&roots).unwrap().unwrap().is::<Dropped>());
            assert_eq!(drops.load(Ordering::SeqCst), 0);
            // Keep the owned key as the only root during collection. Create
            // a scoped view afterwards: AnyRef::as_struct returns a LIFO root.
            let key_view = key.to_rooted(&mut roots);
            assert_eq!(
                key_view
                    .as_struct(&roots)
                    .unwrap()
                    .unwrap()
                    .field(&mut roots, 0)
                    .unwrap()
                    .unwrap_i32(),
                42
            );
            let through_entry = entry.value(&mut roots).unwrap().unwrap();
            assert!(Rooted::ref_eq(&roots, &value, &through_entry).unwrap());
        }
        drop(key);
        store.gc(None).unwrap();
        assert!(!entry.is_live(&mut store).unwrap());
        assert!(weak.upgrade(&mut store).unwrap().is_none());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        store.gc(None).unwrap();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}

#[cfg(all(
    test,
    feature = "std",
    feature = "cranelift",
    feature = "wat",
    target_os = "linux",
    not(miri),
    not(gc_zeal)
))]
mod null_guard_tests {
    use crate::{
        AsContext, AsContextMut, Collector, Config, Engine, FieldType, Instance, Module,
        Mutability, RootScope, StorageType, Store, StructRef, StructRefPre, StructType, Trap, Val,
        ValType,
    };

    const WAT: &str = r#"
        (module
          (type $s (struct (field (mut i32)) (field (mut anyref))))
          (type $a (array (mut i32)))
          (func (export "struct_get") (param (ref null $s)) (result i32)
            (struct.get $s 0 (local.get 0)))
          (func (export "struct_get_ref") (param (ref null $s)) (result anyref)
            (struct.get $s 1 (local.get 0)))
          (func (export "struct_set") (param (ref null $s))
            (struct.set $s 0 (local.get 0) (i32.const 1)))
          (func (export "array_len") (param (ref null $a)) (result i32)
            (array.len (local.get 0)))
          (func (export "array_get") (param (ref null $a) i32) (result i32)
            (array.get $a (local.get 0) (local.get 1)))
          (func (export "array_set") (param (ref null $a) i32)
            (array.set $a (local.get 0) (local.get 1) (i32.const 7)))
          (func (export "new_array") (param i32) (result (ref $a))
            (array.new_default $a (local.get 0)))
          (global $g (ref $s) (struct.new $s (i32.const 5) (ref.null any)))
          (func (export "global_read") (result i32)
            (struct.get $s 0 (global.get $g)))
          (func (export "null_read") (result i32)
            (struct.get $s 0 (ref.null $s)))
        )
    "#;

    /// The product's GC heap configuration: a non-moving heap whose 32-bit
    /// reservation plus guard covers every reference, so compiled code leaves
    /// null checks to the heap's null guard. `guard: false` gives the heap a
    /// small reservation it may move out of, which turns the guard off and
    /// the explicit null and heap-bound checks back on.
    fn engine(guard: bool) -> Engine {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .collector(Collector::Copying)
            .gc_heap_reservation(if guard { 1 << 32 } else { 64 << 20 })
            .gc_heap_guard_size(32 << 20)
            .gc_heap_reservation_for_growth(0)
            .gc_heap_may_move(!guard);
        Engine::new(&config).unwrap()
    }

    fn trap_of(error: crate::Error) -> Trap {
        // An access that faults in the guard reports no memory address,
        // exactly like an explicit check.
        assert!(error.downcast_ref::<crate::runtime::vm::WasmFault>().is_none());
        *error.downcast_ref::<Trap>().unwrap()
    }

    #[test]
    fn compiled_code_and_runtime_agree_on_the_null_guard() {
        assert_eq!(
            super::COMPILED_NULL_GUARD_BYTES,
            wasmtime_cranelift::copying_heap_data::NULL_GUARD_BYTES
        );
        assert!(super::null_guard_bytes(engine(true).tunables()) >= super::COMPILED_NULL_GUARD_BYTES);
        assert_eq!(super::null_guard_bytes(engine(false).tunables()), 0);
    }

    #[test]
    fn null_and_out_of_range_accesses_keep_their_trap_codes() {
        for guard in [true, false] {
            let engine = engine(guard);
            let module = Module::new(&engine, WAT).unwrap();
            let mut store = Store::new(&engine, ());
            let instance = Instance::new(&mut store, &module, &[]).unwrap();
            let null = Val::AnyRef(None);
            let mut call = |name: &str, args: &[Val], results: usize| {
                let mut out = vec![Val::I32(0); results];
                instance
                    .get_func(&mut store, name)
                    .unwrap()
                    .call(&mut store, args, &mut out)
                    .map(|()| out)
            };
            for (name, results) in [
                ("struct_get", 1),
                ("struct_get_ref", 1),
                ("struct_set", 0),
                ("array_len", 1),
            ] {
                let error = call(name, &[null.clone()], results).unwrap_err();
                assert_eq!(trap_of(error), Trap::NullReference, "{name}, guard: {guard}");
            }
            for name in ["array_get", "array_set"] {
                let results = usize::from(name == "array_get");
                let error = call(name, &[null.clone(), Val::I32(0)], results).unwrap_err();
                assert_eq!(trap_of(error), Trap::NullReference, "{name}, guard: {guard}");
                let array = call("new_array", &[Val::I32(3)], 1).unwrap().remove(0);
                for index in [3, -1, i32::MAX] {
                    let args = [array.clone(), Val::I32(index)];
                    let error = call(name, &args, results).unwrap_err();
                    assert_eq!(trap_of(error), Trap::ArrayOutOfBounds, "{name}[{index}]");
                }
                assert!(call(name, &[array, Val::I32(2)], results).is_ok());
            }
            let heap = store.as_context().0.optional_gc_store().unwrap();
            let expected = if guard {
                super::null_guard_bytes(engine.tunables())
            } else {
                0
            };
            assert_eq!(heap.gc_heap.null_guard_len(), expected as usize);
        }
    }

    /// An access through the 32-bit element offset wraps at 2^32: with the
    /// stored length corrupted to `u32::MAX`, index 2^30 of an `i32` array
    /// addresses element 0 again (4 * 2^30 = 2^32), not 4 GiB past it. (The
    /// other configuration keeps the original object-size check, which
    /// reports the corruption as a bug.)
    #[test]
    fn a_corrupt_array_length_cannot_leave_the_reservation() {
        let engine = engine(true);
        let module = Module::new(&engine, WAT).unwrap();
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let call = |store: &mut Store<()>, name: &str, args: &[Val], results: usize| {
            let mut out = vec![Val::I32(0); results];
            instance
                .get_func(&mut *store, name)
                .unwrap()
                .call(&mut *store, args, &mut out)
                .map(|()| out)
        };
        let array = call(&mut store, "new_array", &[Val::I32(3)], 1).unwrap().remove(0);
        call(&mut store, "array_set", &[array.clone(), Val::I32(0)], 0).unwrap();
        let index = array
            .unwrap_anyref()
            .unwrap()
            .try_gc_ref(store.as_context().0)
            .unwrap()
            .as_raw_u32();
        let length = usize::try_from(index + wasmtime_environ::copying::ARRAY_LENGTH_OFFSET)
            .unwrap();
        store
            .as_context_mut()
            .0
            .unwrap_gc_store_mut()
            .gc_heap
            .heap_slice_mut()[length..length + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        let read = |store: &mut Store<()>, index: u32| {
            let args = [array.clone(), Val::I32(index.cast_signed())];
            call(store, "array_get", &args, 1).map(|mut out| out.remove(0).unwrap_i32())
        };
        assert_eq!(read(&mut store, 1 << 30).unwrap(), 7);
        assert_eq!(read(&mut store, (1 << 30) + 2).unwrap(), 0);
        assert_eq!(read(&mut store, 3 << 30).unwrap(), 7);
    }

    /// Withdrawing an interrupt request restores access to the heap but not
    /// to its null guard (see also `tests/interrupt_handle.rs`).
    #[test]
    fn a_withdrawn_interrupt_leaves_the_null_guard_inaccessible() {
        let engine = engine(true);
        let module = Module::new(&engine, WAT).unwrap();
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let null_read = instance
            .get_typed_func::<(), i32>(&mut store, "null_read")
            .unwrap();
        let global_read = instance
            .get_typed_func::<(), i32>(&mut store, "global_read")
            .unwrap();
        let handle = store.interrupt_handle();
        handle.interrupt();
        handle.cancel();
        let error = null_read.call(&mut store, ()).unwrap_err();
        assert_eq!(trap_of(error), Trap::NullReference);
        assert_eq!(global_read.call(&mut store, ()).unwrap(), 5);
    }

    #[test]
    fn the_null_guard_is_never_handed_out() {
        let engine = engine(true);
        let guard = super::null_guard_bytes(engine.tunables());
        assert!(guard >= super::COMPILED_NULL_GUARD_BYTES);
        let ty = StructType::new(
            &engine,
            [FieldType::new(
                Mutability::Var,
                StorageType::ValType(ValType::I32),
            )],
        )
        .unwrap();
        let mut store = Store::new(&engine, ());
        let pre = StructRefPre::new(&mut store, ty);
        // The first object after the heap is attached, and the first after
        // each flip, in both semi-spaces, lies past the guard.
        for _ in 0..4 {
            {
                let mut roots = RootScope::new(&mut store);
                let first = StructRef::new(&mut roots, &pre, &[Val::I32(1)]).unwrap();
                let index = first
                    .try_gc_ref(roots.as_context().0)
                    .unwrap()
                    .as_raw_u32();
                assert!(index >= guard, "object at {index:#x} inside the guard");
            }
            store.gc(None).unwrap();
        }
        let heap = store.as_context().0.optional_gc_store().unwrap();
        assert_eq!(heap.gc_heap.null_guard_len(), guard as usize);
    }
}

#[cfg(all(
    test,
    feature = "std",
    feature = "cranelift",
    feature = "wat",
    not(miri),
    not(gc_zeal)
))]
mod global_root_tests {
    use crate::{Collector, Config, Engine, Instance, Module, Store};

    /// Globals of reference type are roots, read from each module's global
    /// types; globals of other types, `i31ref` and bottom types are not.
    #[test]
    fn reference_globals_keep_their_objects_across_collections() {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .collector(Collector::Copying);
        let engine = Engine::new(&config).unwrap();
        let module = Module::new(
            &engine,
            r#"
            (module
              (type $s (struct (field i32)))
              (global $n (mut i32) (i32.const 3))
              (global $o (mut (ref null $s)) (ref.null $s))
              (global $i (mut i31ref) (ref.i31 (i32.const 9)))
              (global $z (mut (ref null none)) (ref.null none))
              (global $c (ref $s) (struct.new $s (i32.const 11)))
              (func (export "fill") (param i32)
                (global.set $o (struct.new $s (local.get 0))))
              (func (export "churn")
                (local i32)
                (loop $l
                  (drop (struct.new $s (local.get 0)))
                  (local.set 0 (i32.add (local.get 0) (i32.const 1)))
                  (br_if $l (i32.lt_u (local.get 0) (i32.const 100000)))))
              (func (export "read") (result i32)
                (i32.add
                  (i32.add (struct.get $s 0 (global.get $o)) (struct.get $s 0 (global.get $c)))
                  (i32.add (global.get $n) (i31.get_s (global.get $i)))))
            )
            "#,
        )
        .unwrap();
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let fill = instance
            .get_typed_func::<i32, ()>(&mut store, "fill")
            .unwrap();
        let churn = instance.get_typed_func::<(), ()>(&mut store, "churn").unwrap();
        let read = instance.get_typed_func::<(), i32>(&mut store, "read").unwrap();
        fill.call(&mut store, 20).unwrap();
        for _ in 0..3 {
            churn.call(&mut store, ()).unwrap();
            store.gc(None).unwrap();
            assert_eq!(read.call(&mut store, ()).unwrap(), 20 + 11 + 3 + 9);
        }
    }
}

#[cfg(all(
    test,
    feature = "std",
    feature = "cranelift",
    feature = "wat",
    not(miri),
    not(gc_zeal)
))]
mod generational_tests {
    use super::YoungAllocation;
    use crate::prelude::*;
    use crate::{
        AsContext, Collector, Config, Engine, GcEphemeron, GcEphemeronUpdate, Instance, Module,
        RootScope, Store, StructRef, StructRefPre, Val, WeakAnyRef,
    };

    /// A 2 MiB initial heap: a 256 KiB nursery and two 896 KiB semi-spaces.
    fn engine(initial: u64) -> Engine {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .collector(Collector::Copying)
            .gc_heap_initial_size(initial);
        Engine::new(&config).unwrap()
    }

    const WAT: &str = r#"
        (module
          (type $s (struct (field (mut anyref)) (field (mut i32))))
          (type $a (array (mut anyref)))
          (elem $e anyref (item (ref.i31 (i32.const 5))))
          (global $old (mut (ref null $s)) (ref.null $s))
          (global $arr (mut (ref null $a)) (ref.null $a))
          (global $big (mut (ref null $a)) (ref.null $a))
          (func (export "make_old") (param i32)
            (global.set $old (struct.new $s (ref.null any) (i32.const 0)))
            (global.set $arr (array.new_default $a (local.get 0))))
          ;; Allocate about 4 MiB of garbage: many minor collections.
          (func (export "churn")
            (local i32)
            (loop $l
              (drop (struct.new $s (ref.null any) (local.get 0)))
              (local.set 0 (i32.add (local.get 0) (i32.const 1)))
              (br_if $l (i32.lt_u (local.get 0) (i32.const 200000)))))
          ;; Allocate the given number of garbage structs.
          (func (export "alloc") (param i32)
            (loop $l
              (drop (struct.new $s (ref.null any) (local.get 0)))
              (local.set 0 (i32.sub (local.get 0) (i32.const 1)))
              (br_if $l (local.get 0))))
          (func $child (param i32) (result (ref $s))
            (struct.new $s (ref.null any) (local.get 0)))
          (func (export "struct_set") (param i32)
            (struct.set $s 0 (global.get $old) (call $child (local.get 0))))
          ;; A new struct that holds the old one.
          (func (export "wrap")
            (global.set $old (struct.new $s (global.get $old) (i32.const 0))))
          (func (export "inner_read") (result i32)
            (struct.get $s 1 (ref.cast (ref $s)
              (struct.get $s 0 (ref.cast (ref $s) (struct.get $s 0 (global.get $old)))))))
          (func (export "struct_read") (result i32)
            (struct.get $s 1
              (ref.cast (ref $s) (struct.get $s 0 (global.get $old)))))
          (func (export "array_set_all") (param i32)
            (local $i i32)
            (loop $l
              (array.set $a (global.get $arr) (local.get $i)
                (call $child (i32.add (local.get 0) (local.get $i))))
              (local.set $i (i32.add (local.get $i) (i32.const 1)))
              (br_if $l (i32.lt_u (local.get $i) (array.len (global.get $arr))))))
          (func (export "array_fill") (param i32)
            (array.fill $a (global.get $arr) (i32.const 0) (call $child (local.get 0))
              (array.len (global.get $arr))))
          (func (export "array_copy") (param i32)
            (local $src (ref $a))
            (local.set $src (array.new $a (call $child (local.get 0))
              (array.len (global.get $arr))))
            (array.copy $a $a (global.get $arr) (i32.const 0) (local.get $src) (i32.const 0)
              (array.len (global.get $arr))))
          (func (export "array_init_elem")
            (array.init_elem $a $e (global.get $arr) (i32.const 0) (i32.const 0) (i32.const 1)))
          (func (export "array_sum") (result i32)
            (local $i i32) (local $sum i32)
            (loop $l
              (local.set $sum (i32.add (local.get $sum)
                (struct.get $s 1 (ref.cast (ref $s)
                  (array.get $a (global.get $arr) (local.get $i))))))
              (local.set $i (i32.add (local.get $i) (i32.const 1)))
              (br_if $l (i32.lt_u (local.get $i) (array.len (global.get $arr)))))
            (local.get $sum))
          (func (export "array_first_i31") (result i32)
            (i31.get_s (ref.cast (ref i31) (array.get $a (global.get $arr) (i32.const 0)))))
          ;; A large array is allocated in the old space and initialized
          ;; there without barriers.
          (func (export "make_big") (param i32) (param i32)
            (global.set $big (array.new $a (call $child (local.get 1)) (local.get 0))))
          (func (export "big_sum") (result i32)
            (local $i i32) (local $sum i32)
            (loop $l
              (local.set $sum (i32.add (local.get $sum)
                (struct.get $s 1 (ref.cast (ref $s)
                  (array.get $a (global.get $big) (local.get $i))))))
              (local.set $i (i32.add (local.get $i) (i32.const 1)))
              (br_if $l (i32.lt_u (local.get $i) (array.len (global.get $big)))))
            (local.get $sum))
          (func (export "old") (result (ref null $s)) (global.get $old))
        )
    "#;

    struct Fixture {
        store: Store<()>,
        instance: Instance,
    }

    impl Fixture {
        fn new(initial: u64) -> Self {
            let engine = engine(initial);
            let module = Module::new(&engine, WAT).unwrap();
            let mut store = Store::new(&engine, ());
            let instance = Instance::new(&mut store, &module, &[]).unwrap();
            Fixture { store, instance }
        }

        fn call(&mut self, name: &str, args: &[Val], results: usize) -> Vec<Val> {
            let mut out = vec![Val::I32(0); results];
            self.instance
                .get_func(&mut self.store, name)
                .unwrap()
                .call(&mut self.store, args, &mut out)
                .unwrap();
            out
        }

        fn i32(&mut self, name: &str) -> i32 {
            self.call(name, &[], 1)[0].unwrap_i32()
        }

        /// Collects garbage until a minor collection has run.
        fn churn(&mut self) {
            self.call("churn", &[], 0);
            assert!(self.last_minor(), "churn should end in minor collections");
        }

        fn last_minor(&mut self) -> bool {
            self.store
                .as_context()
                .0
                .optional_gc_store()
                .unwrap()
                .gc_heap
                .last_collection_was_minor()
        }

        fn heap(&self) -> &super::CopyingHeap {
            let gc_store = self.store.as_context().0.optional_gc_store().unwrap();
            gc_store.gc_heap.as_any().downcast_ref().unwrap()
        }

        /// Allocates garbage until the first nursery is full and the
        /// pretenure window has opened.
        fn enter_window(&mut self) {
            while self.heap().young == YoungAllocation::FirstNursery {
                self.call("alloc", &[Val::I32(64)], 0);
            }
            assert!(self.in_window());
        }

        fn in_window(&self) -> bool {
            matches!(self.heap().young, YoungAllocation::Pretenuring { .. })
        }

        /// Allocates garbage until the pretenure window is full. The
        /// allocation that closes it finds the nursery full too and asks
        /// for the heap's first collection, a full one.
        fn leave_window(&mut self) {
            self.enter_window();
            while self.in_window() {
                self.call("alloc", &[Val::I32(1)], 0);
            }
            assert_eq!(self.heap().young, YoungAllocation::Nursery);
            assert!(!self.last_minor());
        }
    }

    #[test]
    fn module_struct_fields_beyond_the_inline_trace_bitmap_are_traced() {
        let engine = engine(2 << 20);
        let wat = format!(
            r#"(module
              (type $child (struct (field i32)))
              (type $wide (struct {} (field (mut (ref null $child)))))
              (global $root (mut (ref null $wide)) (ref.null $wide))
              (func (export "create")
                (global.set $root (struct.new_default $wide))
                (struct.set $wide 23 (global.get $root)
                  (struct.new $child (i32.const 42))))
              (func (export "read") (result i32)
                (struct.get $child 0 (struct.get $wide 23 (global.get $root)))))"#,
            "(field (ref null $child)) ".repeat(23)
        );
        let module = Module::new(&engine, wat).unwrap();
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let create = instance
            .get_typed_func::<(), ()>(&mut store, "create")
            .unwrap();
        let read = instance
            .get_typed_func::<(), i32>(&mut store, "read")
            .unwrap();
        create.call(&mut store, ()).unwrap();
        store.gc(None).unwrap();
        assert_eq!(read.call(&mut store, ()).unwrap(), 42);
    }

    #[test]
    fn a_first_full_nursery_continues_in_the_old_space() {
        // 16 MiB: a 2 MiB nursery and 7 MiB semi-spaces, whose window leaves
        // room for two nurseries (each less the null guard): about 3 MiB.
        let mut f = Fixture::new(16 << 20);
        assert_eq!(f.heap().young, YoungAllocation::FirstNursery);
        f.enter_window();
        let heap = f.heap();
        let window = heap.vmctx_data.active_space_end() - heap.active_space_start;
        assert_eq!(window, (7 << 20) - 2 * heap.nursery_capacity());
        assert!((3 << 20..(3 << 20) + (64 << 10)).contains(&window));

        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(64)], 0);
        let heap = f.heap();
        assert_eq!(heap.young, YoungAllocation::FirstNursery);
        assert!(heap.is_young(heap.vmctx_data.bump_ptr()));
        assert_eq!(heap.bump_ptr(), heap.active_space_start);

        f.enter_window();
        let heap = f.heap();
        let YoungAllocation::Pretenuring { nursery_bump } = heap.young else {
            unreachable!()
        };
        assert!(heap.nursery_limit() - nursery_bump < 64, "the nursery was filled");
        let (start, end) = (heap.active_space_start, heap.vmctx_data.active_space_end());
        assert_eq!(start, heap.nursery_end);
        assert_eq!(end - start, (heap.old_end - start) - 2 * heap.nursery_capacity());
        let allocated = heap.vmctx_data.bump_ptr();
        assert!(start < allocated && allocated <= end);
        assert_eq!(heap.bump_ptr(), allocated);

        // The window fills to its end.
        while f.heap().vmctx_data.active_space_end() - f.heap().vmctx_data.bump_ptr() >= 64 {
            assert!(f.in_window());
            f.call("alloc", &[Val::I32(1)], 0);
        }
        assert!(f.heap().old_end - f.heap().bump_ptr() >= 2 * f.heap().nursery_capacity());

        // The collection that follows a window is full and evacuates both
        // the nursery and the window's semi-space; later ones are minor and
        // no second window opens.
        f.leave_window();
        let heap = f.heap();
        assert!(heap.active_space_start > start, "the semi-spaces flipped");
        assert_eq!(heap.vmctx_data.bump_ptr(), heap.nursery_start() + 32);
        f.churn();
        assert_eq!(f.heap().young, YoungAllocation::Nursery);
    }

    #[test]
    fn edges_between_the_nursery_and_the_window_survive_the_first_collection() {
        let mut f = Fixture::new(2 << 20);
        // Nursery objects.
        f.call("make_old", &[Val::I32(64)], 0);
        f.call("struct_set", &[Val::I32(7)], 0);
        f.enter_window();
        // A window object whose initializing store, which has no barrier,
        // holds the only reference to a nursery object.
        f.call("wrap", &[], 0);
        assert_eq!(f.i32("inner_read"), 7);
        // Window objects stored into a nursery array, and a window array
        // copied into it.
        f.call("array_set_all", &[Val::I32(100)], 0);
        assert!(f.in_window());
        f.leave_window();
        assert_eq!(f.i32("inner_read"), 7);
        assert_eq!(f.i32("array_sum"), (100..164).sum::<i32>());
        // The survivors are ordinary old objects now.
        f.call("struct_set", &[Val::I32(11)], 0);
        f.call("array_copy", &[Val::I32(2)], 0);
        f.churn();
        assert_eq!(f.i32("struct_read"), 11);
        assert_eq!(f.i32("array_sum"), 2 * 64);

        // A host store of a window object into a nursery object, and of a
        // nursery object into a window object.
        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(4)], 0);
        let nursery_object = f.call("old", &[], 1).remove(0);
        f.enter_window();
        f.call("wrap", &[], 0);
        let window_object = f.call("old", &[], 1).remove(0);
        {
            let mut scope = RootScope::new(&mut f.store);
            let inner = *nursery_object.unwrap_anyref().unwrap();
            let outer = window_object.unwrap_anyref().unwrap().unwrap_struct(&scope).unwrap();
            let in_window = young_struct(&mut scope, &outer, 9);
            inner
                .unwrap_struct(&scope)
                .unwrap()
                .set_field(&mut scope, 0, Val::AnyRef(Some(in_window.to_anyref())))
                .unwrap();
            // Rewrites the edge the initializing store made, with a barrier.
            outer.set_field(&mut scope, 0, Val::AnyRef(Some(inner))).unwrap();
        }
        assert!(f.in_window());
        assert_eq!(f.i32("inner_read"), 9);
        f.leave_window();
        f.churn();
        assert_eq!(f.i32("inner_read"), 9);
    }

    #[test]
    fn an_explicit_collection_ends_the_first_nursery_and_the_window() {
        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(8)], 0);
        f.call("struct_set", &[Val::I32(5)], 0);
        f.store.gc(None).unwrap();
        assert_eq!(f.heap().young, YoungAllocation::Nursery);
        assert_eq!(f.i32("struct_read"), 5);
        // No window opens after a collection.
        f.churn();
        assert_eq!(f.heap().young, YoungAllocation::Nursery);
        assert_eq!(f.i32("struct_read"), 5);

        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(8)], 0);
        f.call("struct_set", &[Val::I32(5)], 0);
        f.enter_window();
        f.call("wrap", &[], 0);
        f.call("array_set_all", &[Val::I32(1)], 0);
        assert!(f.in_window());
        f.store.gc(None).unwrap();
        assert_eq!(f.heap().young, YoungAllocation::Nursery);
        assert!(!f.last_minor());
        assert_eq!(f.i32("inner_read"), 5);
        assert_eq!(f.i32("array_sum"), (1..9).sum::<i32>());
        f.call("struct_set", &[Val::I32(6)], 0);
        f.churn();
        assert_eq!(f.i32("struct_read"), 6);
        assert_eq!(f.i32("array_sum"), (1..9).sum::<i32>());
    }

    #[test]
    fn a_heap_that_grows_during_its_window_keeps_both_generations() {
        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(8)], 0);
        f.call("struct_set", &[Val::I32(5)], 0);
        f.enter_window();
        f.call("wrap", &[], 0);
        f.call("array_set_all", &[Val::I32(1)], 0);
        assert!(f.in_window());
        let before = f.store.gc_heap_capacity();
        // 4 MiB of elements: more than the whole initial heap.
        f.call("make_big", &[Val::I32(1 << 20), Val::I32(1)], 0);
        assert!(f.store.gc_heap_capacity() > before);
        assert!(!f.in_window());
        assert_eq!(f.i32("inner_read"), 5);
        assert_eq!(f.i32("array_sum"), (1..9).sum::<i32>());
        assert_eq!(f.i32("big_sum"), 1 << 20);
        f.call("struct_set", &[Val::I32(6)], 0);
        f.churn();
        assert_eq!(f.i32("struct_read"), 6);
        assert_eq!(f.i32("array_sum"), (1..9).sum::<i32>());
        assert_eq!(f.i32("big_sum"), 1 << 20);
    }

    #[test]
    fn weak_references_and_externrefs_of_window_objects_follow_the_first_collection() {
        use crate::ExternRef;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Flag(Arc<AtomicBool>);
        impl Drop for Flag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }
        let (dropped_young, dropped_old) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );

        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(1)], 0);
        {
            let mut scope = RootScope::new(&mut f.store);
            ExternRef::new(&mut scope, Flag(dropped_young.clone())).unwrap();
        }
        f.enter_window();
        let old = f.call("old", &[], 1).remove(0);
        let (weak_dead, weak_kept) = {
            let mut scope = RootScope::new(&mut f.store);
            let owner = *old.unwrap_anyref().unwrap();
            let old = owner.unwrap_struct(&scope).unwrap();
            let dead = young_struct(&mut scope, &old, 1).to_anyref();
            let kept = young_struct(&mut scope, &old, 2).to_anyref();
            old.set_field(&mut scope, 0, Val::AnyRef(Some(kept))).unwrap();
            ExternRef::new(&mut scope, Flag(dropped_old.clone())).unwrap();
            (
                WeakAnyRef::new(&mut scope, &owner, &dead).unwrap(),
                WeakAnyRef::new(&mut scope, &owner, &kept).unwrap(),
            )
        };
        // The targets and the second `externref` are window objects.
        assert!(f.in_window());
        assert!(!dropped_young.load(Ordering::Relaxed) && !dropped_old.load(Ordering::Relaxed));
        f.leave_window();
        assert!(dropped_young.load(Ordering::Relaxed) && dropped_old.load(Ordering::Relaxed));
        assert!(weak_dead.upgrade(&mut f.store).unwrap().is_none());
        f.churn();
        let mut scope = RootScope::new(&mut f.store);
        let kept = weak_kept.upgrade(&mut scope).unwrap().unwrap();
        let kept = kept.unwrap_struct(&scope).unwrap();
        assert_eq!(kept.field(&mut scope, 1).unwrap().unwrap_i32(), 2);
    }

    #[cfg(feature = "pooling-allocator")]
    #[test]
    fn a_pooled_heap_starts_each_store_with_a_first_nursery() {
        use crate::{InstanceAllocationStrategy, PoolingAllocationConfig};
        let mut pooling = PoolingAllocationConfig::new();
        pooling
            .total_gc_heaps(1)
            .total_memories(1)
            .total_core_instances(1)
            .total_tables(1)
            .total_stacks(1);
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .wasm_function_references(true)
            .collector(Collector::Copying)
            .gc_heap_initial_size(2 << 20)
            .allocation_strategy(InstanceAllocationStrategy::Pooling(pooling));
        let engine = Engine::new(&config).unwrap();
        let module = Module::new(&engine, WAT).unwrap();
        // Stores dropped in their first nursery, with the window open, after
        // the first collection and after later ones: the next starts afresh.
        for round in 0..5 {
            let mut store = Store::new(&engine, ());
            let instance = Instance::new(&mut store, &module, &[]).unwrap();
            let mut f = Fixture { store, instance };
            assert_eq!(f.heap().young, YoungAllocation::FirstNursery);
            assert_eq!(f.heap().vmctx_data.bump_ptr(), f.heap().nursery_start());
            assert_eq!(f.heap().bump_ptr(), f.heap().active_space_start);
            f.call("make_old", &[Val::I32(64)], 0);
            f.call("struct_set", &[Val::I32(round)], 0);
            if round == 0 {
                continue;
            }
            f.enter_window();
            f.call("wrap", &[], 0);
            f.call("array_set_all", &[Val::I32(round)], 0);
            if round == 1 {
                continue;
            }
            f.leave_window();
            if round == 2 {
                continue;
            }
            f.churn();
            assert_eq!(f.i32("inner_read"), round);
            assert_eq!(f.i32("array_sum"), (round..round + 64).sum::<i32>());
        }
    }

    #[test]
    fn the_nursery_is_an_eighth_of_the_initial_heap() {
        assert_eq!(super::nursery_end(engine(2 << 20).tunables()), 256 << 10);
        assert_eq!(super::nursery_end(engine(16 << 20).tunables()), 2 << 20);
        assert_eq!(super::nursery_end(engine(1 << 20).tunables()), 0);
    }

    #[test]
    fn old_to_young_stores_survive_minor_collections() {
        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(64)], 0);
        f.churn();
        assert!(f.heap().remembered.is_empty());
        // The first compiled store takes the overflow path at zero capacity.
        f.call("struct_set", &[Val::I32(7)], 0);
        assert!(f.heap().remembered.is_empty());
        f.churn();
        assert_eq!(f.i32("struct_read"), 7);
        assert_eq!(f.heap().remembered.len(), super::REMEMBERED_INITIAL);
        // Compiled array.set into an old array.
        f.call("array_set_all", &[Val::I32(100)], 0);
        f.churn();
        assert_eq!(f.i32("array_sum"), (100..164).sum::<i32>());
        // array.fill, array.copy and array.init_elem into an old array.
        f.call("array_fill", &[Val::I32(3)], 0);
        f.churn();
        assert_eq!(f.i32("array_sum"), 3 * 64);
        f.call("array_copy", &[Val::I32(2)], 0);
        f.churn();
        assert_eq!(f.i32("array_sum"), 2 * 64);
        f.call("array_init_elem", &[], 0);
        f.churn();
        assert_eq!(f.i32("array_first_i31"), 5);
    }

    /// The young value's struct type, from the old object.
    fn young_struct(
        scope: &mut RootScope<&mut Store<()>>,
        like: &crate::Rooted<StructRef>,
        value: i32,
    ) -> crate::Rooted<StructRef> {
        let ty = like.ty(&*scope).unwrap();
        let pre = StructRefPre::new(&mut *scope, ty);
        StructRef::new(&mut *scope, &pre, &[Val::AnyRef(None), Val::I32(value)]).unwrap()
    }

    #[test]
    fn host_stores_into_old_objects_survive_minor_collections() {
        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(4)], 0);
        f.churn();
        assert!(f.heap().remembered.is_empty());
        let old = f.call("old", &[], 1).remove(0);
        {
            let mut scope = RootScope::new(&mut f.store);
            let old = old.unwrap_anyref().unwrap().unwrap_struct(&scope).unwrap();
            let young = young_struct(&mut scope, &old, 9);
            old.set_field(&mut scope, 0, Val::AnyRef(Some(young.to_anyref())))
                .unwrap();
        }
        assert_eq!(f.heap().remembered.len(), super::REMEMBERED_INITIAL);
        f.churn();
        assert_eq!(f.i32("struct_read"), 9);
    }

    #[test]
    fn an_overflowing_remembered_buffer_scans_the_old_space() {
        // More old-to-young stores than the buffer's 4096 slots.
        let mut f = Fixture::new(4 << 20);
        f.call("make_old", &[Val::I32(10_000)], 0);
        f.churn();
        f.call("struct_set", &[Val::I32(1)], 0);
        f.churn();
        assert_eq!(f.heap().remembered.len(), super::REMEMBERED_INITIAL);
        f.store.gc(None).unwrap();
        f.call("array_set_all", &[Val::I32(1)], 0);
        f.churn();
        assert_eq!(f.i32("array_sum"), (1..10_001).sum::<i32>());
    }

    #[test]
    fn large_objects_allocated_old_keep_their_young_initializers() {
        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(1)], 0);
        // Past the heap's first, full, collection, so that the collection
        // after `make_big` is minor and scans the old-allocated array.
        f.churn();
        // 64 Ki elements of 4 bytes: larger than a quarter of the nursery.
        f.call("make_big", &[Val::I32(65_536), Val::I32(1)], 0);
        f.churn();
        assert_eq!(f.i32("big_sum"), 65_536);
    }

    #[test]
    fn weak_references_follow_young_targets_through_minor_collections() {
        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(1)], 0);
        f.churn();
        let old = f.call("old", &[], 1).remove(0);
        let (weak_dead, weak_kept) = {
            let mut scope = RootScope::new(&mut f.store);
            let owner = *old.unwrap_anyref().unwrap();
            let old = owner.unwrap_struct(&scope).unwrap();
            let dead = young_struct(&mut scope, &old, 1).to_anyref();
            let kept = young_struct(&mut scope, &old, 2).to_anyref();
            // Only the old owner reaches `kept`.
            old.set_field(&mut scope, 0, Val::AnyRef(Some(kept))).unwrap();
            (
                WeakAnyRef::new(&mut scope, &owner, &dead).unwrap(),
                WeakAnyRef::new(&mut scope, &owner, &kept).unwrap(),
            )
        };
        f.churn();
        assert!(weak_dead.upgrade(&mut f.store).unwrap().is_none());
        let mut scope = RootScope::new(&mut f.store);
        let kept = weak_kept.upgrade(&mut scope).unwrap().unwrap();
        let kept = kept.unwrap_struct(&scope).unwrap();
        assert_eq!(kept.field(&mut scope, 1).unwrap().unwrap_i32(), 2);
    }

    #[test]
    fn ephemeron_values_set_since_the_last_collection_survive_minor_collections() {
        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(1)], 0);
        f.churn();
        let old = f.call("old", &[], 1).remove(0);
        // Owner, key and value are one old object; once a collection has
        // processed the new edge, minor collections no longer visit it.
        let ephemeron = {
            let mut scope = RootScope::new(&mut f.store);
            let owner = *old.unwrap_anyref().unwrap();
            GcEphemeron::new(&mut scope, &owner, &owner, Some(&owner)).unwrap()
        };
        f.churn();
        // Setting a young value marks the edge again; only the edge reaches it.
        {
            let mut scope = RootScope::new(&mut f.store);
            let owner = *old.unwrap_anyref().unwrap();
            let old = owner.unwrap_struct(&scope).unwrap();
            let young = young_struct(&mut scope, &old, 7).to_anyref();
            ephemeron.set_value(&mut scope, Some(&young)).unwrap();
        }
        f.churn();
        let mut scope = RootScope::new(&mut f.store);
        let value = ephemeron.value(&mut scope).unwrap().unwrap();
        let value = value.unwrap_struct(&scope).unwrap();
        assert_eq!(value.field(&mut scope, 1).unwrap().unwrap_i32(), 7);
    }

    #[test]
    fn fused_raw_ephemeron_update_remembers_young_values() {
        let mut f = Fixture::new(2 << 20);
        f.call("make_old", &[Val::I32(1)], 0);
        f.churn();
        let old = f.call("old", &[], 1).remove(0);
        let owner = *old.unwrap_anyref().unwrap();
        let ephemeron = GcEphemeron::new(&mut f.store, &owner, &owner, None).unwrap();
        f.churn(); // Clears the insertion's touched bit.
        let nursery_end = f.heap().nursery_end;

        {
            let mut scope = RootScope::new(&mut f.store);
            let old = owner.unwrap_struct(&scope).unwrap();
            let young = young_struct(&mut scope, &old, 7).to_anyref();
            let owner = owner.to_raw(&mut scope).unwrap();
            let value = young.to_raw(&mut scope).unwrap();
            assert!(owner >= nursery_end && value < nursery_end);
            GcEphemeron::with_raw_access(&mut scope, |_, edges| {
                assert_eq!(
                    edges.value_if_matches_raw(ephemeron, owner, owner).unwrap(),
                    Some(0)
                );
                // SAFETY: The value is rooted in this Store and has not moved.
                let result = unsafe { edges.set_value_if_matches_raw(ephemeron, owner, 0, value) };
                assert_eq!(result.unwrap(), GcEphemeronUpdate::Mismatch);
                assert_eq!(
                    edges.value_if_matches_raw(ephemeron, owner, owner).unwrap(),
                    Some(0)
                );
                // SAFETY: The value is still rooted and the access cannot collect.
                let result =
                    unsafe { edges.set_value_if_matches_raw(ephemeron, owner, owner, value) };
                assert_eq!(result.unwrap(), GcEphemeronUpdate::Updated);
            });
        } // The young value is reachable only through the ephemeron.

        f.churn();
        let mut scope = RootScope::new(&mut f.store);
        let raw_owner = owner.to_raw(&mut scope).unwrap();
        let value = ephemeron.value(&mut scope).unwrap().unwrap();
        let raw_value = value.to_raw(&mut scope).unwrap();
        GcEphemeron::with_raw_access(&mut scope, |_, edges| {
            assert_eq!(
                edges
                    .value_if_matches_raw(ephemeron, raw_owner, raw_owner)
                    .unwrap(),
                Some(raw_value)
            );
        });
        let value = value.unwrap_struct(&scope).unwrap();
        assert_eq!(value.field(&mut scope, 1).unwrap().unwrap_i32(), 7);
    }

    #[test]
    fn a_heap_near_its_limit_still_grows_to_the_limit() {
        use crate::{
            ArrayRef, ArrayRefPre, ArrayType, FieldType, Mutability, StorageType,
            StoreLimitsBuilder,
        };
        // Doubling from about 64 MiB would pass the 128 MiB limit; the four
        // 8 MiB arrays still fit a heap grown only as far as they need.
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .collector(Collector::Copying)
            .gc_heap_initial_size(16 << 20)
            .gc_heap_reservation(1 << 32)
            .gc_heap_reservation_for_growth(0)
            .gc_heap_may_move(false);
        let engine = Engine::new(&config).unwrap();
        let limit = 128 << 20;
        let mut store = Store::new(&engine, StoreLimitsBuilder::new().memory_size(limit).build());
        store.limiter(|limits| limits);
        let ty = ArrayType::new(&engine, FieldType::new(Mutability::Var, StorageType::I8));
        let pre = ArrayRefPre::new(&mut store, ty);
        let mut live = Vec::new();
        for marker in 0..4 {
            live.push(ArrayRef::new(&mut store, &pre, &Val::I32(marker), 8 << 20).unwrap());
            store.gc(None).unwrap();
        }
        assert!(store.gc_heap_capacity() <= limit);
        for (marker, array) in live.iter().enumerate() {
            let last = array.get(&mut store, (8 << 20) - 1).unwrap().unwrap_i32();
            assert_eq!(last, marker as i32);
        }
        assert!(ArrayRef::new(&mut store, &pre, &Val::I32(0), 256 << 20).is_err());
    }

    #[test]
    fn a_module_compiled_for_another_nursery_is_rejected() {
        let small = engine(2 << 20);
        let large = engine(4 << 20);
        let bytes = Module::new(&small, WAT).unwrap().serialize().unwrap();
        let error = unsafe { Module::deserialize(&large, &bytes) }.unwrap_err();
        assert!(format!("{error:?}").contains("GC heap initial size"), "{error:?}");
    }
}
