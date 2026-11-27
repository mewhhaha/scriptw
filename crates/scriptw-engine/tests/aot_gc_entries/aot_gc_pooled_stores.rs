//! A pooled Engine hands each Store a GC heap slot another Store returned.
//! The reused slot keeps its null guard, deadlines and Store limits, and
//! shows the next Store nothing of the previous one.
use scriptw_engine::{
    configure_wasm_runtime, NativeCompilationPolicy, PooledStoreReset, PooledStores,
};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use wasmtime::{
    ArrayRef, ArrayRefPre, ArrayType, Config, Engine, FieldType, Func, Instance, InterruptHandle,
    Module, Mutability, PoolConcurrencyLimitError, StorageType, Store, StoreLimits,
    StoreLimitsBuilder, StructRef, StructRefPre, Trap, Val,
};

const KEEP_RESIDENT: usize = scriptw_engine::MAX_POOLED_KEEP_RESIDENT_BYTES;

fn runtime_config() -> Config {
    let mut config = Config::new();
    configure_wasm_runtime(
        &mut config,
        NativeCompilationPolicy::Fast,
        scriptw_engine::CompilationJobs::current(),
    );
    config
}

fn pooled_engine_for(pool: PooledStores) -> (Engine, PooledStoreReset) {
    let (engine, reset) = pool.engine(&runtime_config()).unwrap();
    if wasmtime::PoolingAllocationConfig::is_pagemap_scan_available() {
        assert_eq!(
            reset,
            PooledStoreReset::KeepDirtyPagesResident {
                bytes: KEEP_RESIDENT
            }
        );
    } else {
        assert_eq!(reset, PooledStoreReset::ReleaseAll);
    }
    (engine, reset)
}

fn pooled_engine() -> Engine {
    pooled_engine_for(PooledStores::sequential()).0
}

fn module(engine: &Engine, sections: &[(u8, &[u8])]) -> Module {
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    for (id, section) in sections {
        assert!(section.len() < 128);
        bytes.extend_from_slice(&[*id, section.len() as u8]);
        bytes.extend_from_slice(section);
    }
    Module::new(engine, bytes).unwrap()
}

// (module
//   (type $bytes (array (mut i8)))
//   (type $cell (struct (field (mut i32))))
//   (func (export "field") (param (ref null $cell)) (result i32)
//     local.get 0 struct.get $cell 0)
//   ;; Runs until interrupted; every iteration loads and stores the GC heap.
//   (func (export "spin") (param (ref null $cell))
//     (loop (struct.set $cell 0 (local.get 0)
//       (i32.add (struct.get $cell 0 (local.get 0)) (i32.const 1))) (br 0))))
fn heap_module(engine: &Engine) -> Module {
    module(
        engine,
        &[
            (
                1,
                &[
                    4, 0x5e, 0x78, 1, // mutable byte array
                    0x5f, 1, 0x7f, 1, // mutable i32 struct
                    0x60, 1, 0x63, 1, 1, 0x7f, // field: (ref null $cell) -> i32
                    0x60, 1, 0x63, 1, 0, // spin: (ref null $cell) -> ()
                ],
            ),
            (3, &[2, 2, 3]),
            (7, b"\x02\x05field\x00\x00\x04spin\x00\x01"),
            (
                10,
                &[
                    2, // two bodies
                    8, 0, 0x20, 0, 0xfb, 2, 1, 0, 0x0b, // field
                    22, 0, 0x03, 0x40, 0x20, 0, 0x20, 0, 0xfb, 2, 1, 0, 0x41, 1, 0x6a, 0xfb, 5, 1,
                    0, 0x0c, 0, 0x0b, 0x0b, // spin
                ],
            ),
        ],
    )
}

// (module
//   (memory 1)
//   (func (export "load") (param i32) (result i32) local.get 0 i32.load)
//   (func (export "store") (param i32 i32) local.get 0 local.get 1 i32.store))
fn memory_module(engine: &Engine) -> Module {
    module(
        engine,
        &[
            (1, &[2, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 2, 0x7f, 0x7f, 0]),
            (3, &[2, 0, 1]),
            (5, &[1, 0, 1]),
            (7, b"\x02\x04load\x00\x00\x05store\x00\x01"),
            (
                10,
                &[
                    2, 7, 0, 0x20, 0, 0x28, 2, 0, 0x0b, // load
                    9, 0, 0x20, 0, 0x20, 1, 0x36, 2, 0, 0x0b, // store
                ],
            ),
        ],
    )
}

struct HeapStore {
    store: Store<StoreLimits>,
    field: Func,
    spin: Func,
    cells: StructRefPre,
}

impl HeapStore {
    fn new(engine: &Engine, module: &Module, limit: usize) -> Result<Self, wasmtime::Error> {
        let mut store = Store::new(engine, StoreLimitsBuilder::new().memory_size(limit).build());
        store.limiter(|limits| limits);
        let instance = Instance::new(&mut store, module, &[])?;
        let field = instance.get_func(&mut store, "field").unwrap();
        let spin = instance.get_func(&mut store, "spin").unwrap();
        let cell_type = field
            .ty(&store)
            .params()
            .next()
            .unwrap()
            .as_ref()
            .unwrap()
            .heap_type()
            .as_concrete_struct()
            .unwrap()
            .clone();
        let cells = StructRefPre::new(&mut store, cell_type);
        Ok(Self {
            store,
            field,
            spin,
            cells,
        })
    }

    fn cell(&mut self, value: i32) -> wasmtime::Rooted<StructRef> {
        StructRef::new(&mut self.store, &self.cells, &[Val::I32(value)]).unwrap()
    }

    fn read(&mut self, cell: Option<wasmtime::Rooted<StructRef>>) -> Result<i32, wasmtime::Error> {
        let mut output = [Val::I32(0)];
        self.field.call(
            &mut self.store,
            &[Val::AnyRef(cell.map(|cell| cell.to_anyref()))],
            &mut output,
        )?;
        Ok(output[0].i32().unwrap())
    }

    fn assert_null_access_traps(&mut self) {
        let error = self.read(None).unwrap_err();
        assert_eq!(error.downcast_ref::<Trap>(), Some(&Trap::NullReference));
    }

    /// Runs `spin` until a requester thread stops it, as a deadline does.
    fn assert_interrupted(&mut self, cell: wasmtime::Rooted<StructRef>) {
        let handle = self.store.interrupt_handle();
        let (stop, requester) = requester(handle.clone());
        let error = self
            .spin
            .call(
                &mut self.store,
                &[Val::AnyRef(Some(cell.to_anyref()))],
                &mut [],
            )
            .unwrap_err();
        stop.store(true, Ordering::SeqCst);
        requester.join().unwrap();
        handle.cancel();
        assert_eq!(
            error.downcast_ref::<Trap>(),
            Some(&Trap::Interrupt),
            "{error:?}"
        );
    }
}

fn requester(handle: InterruptHandle) -> (Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let thread = std::thread::spawn({
        let stop = stop.clone();
        move || {
            std::thread::sleep(Duration::from_millis(20));
            while !stop.load(Ordering::SeqCst) {
                handle.interrupt();
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });
    (stop, thread)
}

const STORE_LIMIT: usize = 1024 * 1024 * 1024;

#[cfg(target_pointer_width = "64")]
#[test]
fn a_null_access_traps_on_every_reused_pooled_gc_heap() {
    let engine = pooled_engine();
    let module = heap_module(&engine);
    for round in 0..4 {
        let mut heap = HeapStore::new(&engine, &module, STORE_LIMIT).unwrap();
        heap.assert_null_access_traps();
        let cell = heap.cell(round);
        assert_eq!(heap.read(Some(cell)).unwrap(), round);
        heap.store.gc(None).unwrap();
        assert_eq!(heap.read(Some(cell)).unwrap(), round);
        heap.assert_null_access_traps();
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn an_interrupted_pooled_store_returns_an_accessible_heap_to_the_next_store() {
    let engine = pooled_engine();
    let module = heap_module(&engine);
    // A delivered request, withdrawn before the Store is dropped.
    let mut heap = HeapStore::new(&engine, &module, STORE_LIMIT).unwrap();
    let cell = heap.cell(1);
    heap.assert_interrupted(cell);
    assert!(
        heap.read(Some(cell)).unwrap() > 1,
        "the loop ran before it was stopped"
    );
    drop(heap);
    // A request still pending when the Store is dropped: the drop restores
    // access before the pool resets and reuses the slot.
    let mut heap = HeapStore::new(&engine, &module, STORE_LIMIT).unwrap();
    let cell = heap.cell(2);
    let handle = heap.store.interrupt_handle();
    handle.interrupt();
    assert!(handle.is_pending());
    assert_eq!(cell.field(&mut heap.store, 0).unwrap().unwrap_i32(), 2);
    drop(heap);
    handle.interrupt();
    handle.cancel();
    // The next Store on that slot reaches its whole heap, keeps the null
    // guard, and is interrupted again.
    let mut heap = HeapStore::new(&engine, &module, STORE_LIMIT).unwrap();
    let cell = heap.cell(3);
    assert_eq!(heap.read(Some(cell)).unwrap(), 3);
    heap.assert_null_access_traps();
    heap.assert_interrupted(cell);
    heap.assert_null_access_traps();
    let fresh = heap.cell(4);
    assert_eq!(heap.read(Some(fresh)).unwrap(), 4);
}

#[cfg(target_pointer_width = "64")]
#[test]
fn store_limits_still_cap_a_pooled_gc_heap() {
    let engine = pooled_engine();
    let module = heap_module(&engine);
    let limit = 64 * 1024 * 1024;
    for _ in 0..2 {
        let mut heap = HeapStore::new(&engine, &module, limit).unwrap();
        // Structurally the module's `$bytes` type.
        let bytes_type = ArrayType::new(&engine, FieldType::new(Mutability::Var, StorageType::I8));
        let bytes = ArrayRefPre::new(&mut heap.store, bytes_type);
        let kept = ArrayRef::new(&mut heap.store, &bytes, &Val::I32(7), 16 * 1024 * 1024).unwrap();
        assert!(ArrayRef::new(&mut heap.store, &bytes, &Val::I32(0), 2 * limit as u32).is_err());
        assert!(heap.store.gc_heap_capacity() <= limit);
        assert_eq!(kept.get(&mut heap.store, 0).unwrap().unwrap_i32(), 7);
        heap.assert_null_access_traps();
    }
}

#[cfg(target_pointer_width = "64")]
#[test]
fn a_store_on_a_reused_slot_sees_nothing_of_the_previous_store() {
    let engine = pooled_engine();
    let heap_module = heap_module(&engine);
    let memory_module = memory_module(&engine);
    let offsets = [0, 0x100, 0x1000, 0xfffc];
    for round in 0..3 {
        let mut store = Store::new(
            &engine,
            StoreLimitsBuilder::new().memory_size(STORE_LIMIT).build(),
        );
        store.limiter(|limits| limits);
        // The GC heap and the defined linear memory both come from the pool,
        // and both are reset by the same slot reset.
        Instance::new(&mut store, &heap_module, &[]).unwrap();
        let instance = Instance::new(&mut store, &memory_module, &[]).unwrap();
        let load = instance
            .get_typed_func::<i32, i32>(&mut store, "load")
            .unwrap();
        let write = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "store")
            .unwrap();
        for offset in offsets {
            assert_eq!(load.call(&mut store, offset).unwrap(), 0, "round {round}");
            write.call(&mut store, (offset, -1 - round)).unwrap();
        }
    }
}

#[cfg(target_pointer_width = "64")]
#[test]
fn a_full_store_pool_refuses_another_store_until_one_is_dropped() {
    let engine = pooled_engine();
    let module = heap_module(&engine);
    let mut first = HeapStore::new(&engine, &module, STORE_LIMIT).unwrap();
    let error = HeapStore::new(&engine, &module, STORE_LIMIT)
        .err()
        .expect("a one-Store pool has no second GC heap");
    assert!(error.is::<PoolConcurrencyLimitError>(), "{error:#}");
    first.assert_null_access_traps();
    drop(first);
    let mut next = HeapStore::new(&engine, &module, STORE_LIMIT).unwrap();
    next.assert_null_access_traps();
}

// (module (memory (export "memory") PAGES))
fn exported_memory_module(engine: &Engine, pages: u32) -> Module {
    let mut limits = vec![1, 0];
    let mut remaining = pages;
    loop {
        let byte = (remaining & 0x7f) as u8;
        remaining >>= 7;
        if remaining == 0 {
            limits.push(byte);
            break;
        }
        limits.push(byte | 0x80);
    }
    module(engine, &[(5, &limits), (7, b"\x01\x06memory\x02\x00")])
}

const WASM_PAGE: usize = 64 * 1024;

/// Every byte of the Store's exported memory is zero.
fn assert_zeroed(store: &Store<StoreLimits>, memory: &wasmtime::Memory, context: &str) {
    let data = memory.data(store);
    if let Some(offset) = data.iter().position(|byte| *byte != 0) {
        panic!(
            "{context}: byte {offset:#x} of {:#x} is {:#04x}, left by an earlier Store",
            data.len(),
            data[offset]
        );
    }
}

fn limited_store(engine: &Engine) -> Store<StoreLimits> {
    let mut store = Store::new(
        engine,
        StoreLimitsBuilder::new().memory_size(STORE_LIMIT).build(),
    );
    store.limiter(|limits| limits);
    store
}

/// Whether each host page of `bytes` at `base` is present, from
/// `/proc/self/pagemap` (bit 63 present, bit 62 swapped).
#[cfg(target_os = "linux")]
fn page_states(base: *const u8, bytes: usize) -> Vec<(bool, bool)> {
    use std::io::{Read, Seek, SeekFrom};
    let page = rustix::param::page_size();
    let mut pagemap = std::fs::File::open("/proc/self/pagemap").unwrap();
    pagemap
        .seek(SeekFrom::Start((base as u64 / page as u64) * 8))
        .unwrap();
    let mut entries = vec![0u8; bytes.div_ceil(page) * 8];
    pagemap.read_exact(&mut entries).unwrap();
    entries
        .chunks_exact(8)
        .map(|entry| {
            let entry = u64::from_le_bytes(entry.try_into().unwrap());
            (entry >> 63 & 1 == 1, entry >> 62 & 1 == 1)
        })
        .collect()
}

/// More dirty memory than the pool keeps resident, and more separate dirty
/// regions than one `PAGEMAP_SCAN` reports (32): the scan stops early and the
/// rest of the slot is released, so the next Store on the same slot reads
/// zeros everywhere.
#[cfg(target_pointer_width = "64")]
#[test]
fn a_reused_slot_is_zero_past_keep_resident_and_past_the_scans_region_limit() {
    let engine = pooled_engine();
    let pages = (KEEP_RESIDENT / WASM_PAGE) as u32 + 64;
    let module = exported_memory_module(&engine, pages);
    let mut base = None;
    for round in 0..3 {
        let mut store = limited_store(&engine);
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        let address = memory.data_ptr(&store) as usize;
        assert_eq!(
            *base.get_or_insert(address),
            address,
            "the one-Store pool hands the module its slot again"
        );
        assert_zeroed(&store, &memory, &format!("round {round}"));
        let data = memory.data_mut(&mut store);
        match round {
            // Every other Wasm page, at a varying offset: 160 separate
            // dirty regions on any host page size up to 64 KiB.
            0 => {
                for page in (0..pages as usize).step_by(2) {
                    data[page * WASM_PAGE + (page % 7) * 4096 + 13] = 0xa5;
                }
            }
            // Every byte: 4 MiB past KEEP_RESIDENT dirty against KEEP_RESIDENT kept resident.
            1 => data.fill(0x5a),
            _ => {}
        }
    }
}

/// A GC heap's slot goes back to the shared memory pool, and the next Store
/// can get it as a linear memory: none of the heap's objects show through.
#[cfg(target_pointer_width = "64")]
#[test]
fn a_gc_heap_slot_reused_as_linear_memory_shows_none_of_the_heaps_bytes() {
    let engine = pooled_engine();
    let heap_module = heap_module(&engine);
    let memory_module = exported_memory_module(&engine, (KEEP_RESIDENT / WASM_PAGE) as u32);
    let mut heap = HeapStore::new(&engine, &heap_module, STORE_LIMIT).unwrap();
    let bytes_type = ArrayType::new(&engine, FieldType::new(Mutability::Var, StorageType::I8));
    let bytes = ArrayRefPre::new(&mut heap.store, bytes_type);
    let mut kept = Vec::new();
    for fill in [0xab, 0xcd, 0xef] {
        // Each array is larger than a quarter of the nursery, so it is
        // allocated in the old space: the heap's pages fill well past it.
        kept.push(ArrayRef::new(&mut heap.store, &bytes, &Val::I32(fill), 3 << 20).unwrap());
    }
    for _ in 0..1000 {
        heap.cell(0x7777_7777);
    }
    assert_eq!(
        kept[2].get(&mut heap.store, 0).unwrap().unwrap_i32() as u8,
        0xef
    );
    drop(kept);
    drop(heap);
    // The heap's slot is the pool's only warm slot, and a one-Store pool
    // keeps at most one warm: the memory gets that slot.
    let mut store = limited_store(&engine);
    let instance = Instance::new(&mut store, &memory_module, &[]).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    #[cfg(target_os = "linux")]
    if let PooledStoreReset::KeepDirtyPagesResident { .. } = PooledStores::sequential().reset() {
        // Before anything touches the memory: pages the heap dirtied are
        // still resident, so this is the heap's slot, zeroed in place.
        let resident = page_states(memory.data_ptr(&store), memory.data_size(&store))
            .iter()
            .filter(|(present, _)| *present)
            .count();
        assert!(resident > 0, "the memory reuses the GC heap's warm slot");
    }
    assert_zeroed(&store, &memory, "linear memory on a former GC heap slot");
}

/// Several pooled Stores alive at once on several threads, each dirtying
/// its memory and its GC heap: no Store ever sees another's bytes.
#[cfg(target_pointer_width = "64")]
#[test]
fn concurrent_pooled_stores_never_see_each_others_bytes() {
    const THREADS: u32 = 4;
    let (engine, _) = pooled_engine_for(
        PooledStores::new(NonZeroU32::new(THREADS).unwrap(), KEEP_RESIDENT).unwrap(),
    );
    let heap_module = heap_module(&engine);
    let memory_module = exported_memory_module(&engine, 64);
    let start = Arc::new(Barrier::new(THREADS as usize));
    let threads = (0..THREADS)
        .map(|thread| {
            let (engine, heap_module, memory_module, start) = (
                engine.clone(),
                heap_module.clone(),
                memory_module.clone(),
                start.clone(),
            );
            std::thread::spawn(move || {
                start.wait();
                for round in 0..12 {
                    let mut heap = HeapStore::new(&engine, &heap_module, STORE_LIMIT).unwrap();
                    let instance = Instance::new(&mut heap.store, &memory_module, &[]).unwrap();
                    let memory = instance.get_memory(&mut heap.store, "memory").unwrap();
                    assert_zeroed(
                        &heap.store,
                        &memory,
                        &format!("thread {thread} round {round}"),
                    );
                    let marker = (thread * 16 + round + 1) as u8;
                    memory.data_mut(&mut heap.store).fill(marker);
                    let value = i32::from_ne_bytes([marker; 4]);
                    let cells = (0..2000).map(|_| heap.cell(value)).collect::<Vec<_>>();
                    for cell in cells {
                        assert_eq!(heap.read(Some(cell)).unwrap(), value);
                    }
                    heap.assert_null_access_traps();
                    assert!(memory.data(&heap.store).iter().all(|byte| *byte == marker));
                }
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
}

/// The reset's classification of dirty pages, on `PAGEMAP_SCAN` categories
/// as the kernel reports them (`PAGE_IS_*`), run through the reset's own
/// region handling. This host cannot swap pages out, so this is what covers
/// the swapped and migrating cases here: a written page that is a swap or
/// migration entry when the Store drops must be released, not skipped.
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
#[test]
fn the_reset_releases_swapped_dirty_pages_and_zeroes_present_ones() {
    use wasmtime::_internal::{model_dirty_page_reset, DirtyPageReset::*};
    const WRITTEN: u64 = 1 << 1;
    const FILE: u64 = 1 << 2;
    const PRESENT: u64 = 1 << 3;
    const SWAPPED: u64 = 1 << 4;
    const PFNZERO: u64 = 1 << 5;
    const HUGE: u64 = 1 << 6;
    let pages = [
        // Never populated: 6.18 still says WRITTEN.
        (WRITTEN, Untouched),
        (0, Untouched),
        // Written and present (6.x sets WRITTEN; 7.x omits WRITTEN without uffd-wp).
        (PRESENT | WRITTEN, Zeroed),
        (PRESENT, Zeroed),
        // Written, then swapped out or being migrated.
        (SWAPPED | WRITTEN, Decommitted),
        (SWAPPED, Decommitted),
        (PRESENT | WRITTEN | HUGE, Zeroed),
        (PRESENT | HUGE, Zeroed),
        // Only read: the zero page, an image's page-cache page (present or
        // being migrated).
        (PRESENT | WRITTEN | PFNZERO, Untouched),
        (PRESENT | PFNZERO, Untouched),
        (PRESENT | WRITTEN | FILE, Untouched),
        (PRESENT | FILE, Untouched),
        (SWAPPED | WRITTEN | FILE, Untouched),
        (SWAPPED | FILE, Untouched),
        (SWAPPED | WRITTEN, Decommitted),
    ];
    let reset = model_dirty_page_reset(&pages.map(|(bits, _)| bits));
    assert_eq!(reset, pages.map(|(_, expected)| expected));
}

/// A dirty page that is swapped out when its Store drops is not present, so
/// a reset that zeroes only present dirty pages left it, and the next Store
/// read the previous Store's bytes. This pages dirty memory out with
/// `MADV_PAGEOUT` and needs an active swap device to do so.
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
#[test]
#[ignore = "needs an active swap device, which the cloud machine and the verification \
            wrapper (memory.swap.max = 0) never have; run with --ignored on a host with swap"]
fn a_swapped_out_dirty_page_reads_zero_in_the_next_store() {
    assert!(
        wasmtime::PoolingAllocationConfig::is_pagemap_scan_available(),
        "this kernel has no PAGEMAP_SCAN: the pool releases every page, and the \
         swapped-page reset cannot be exercised here"
    );
    let engine = pooled_engine();
    let pages = 64u32;
    let module = exported_memory_module(&engine, pages);
    let bytes = pages as usize * WASM_PAGE;
    let mut base = None;
    for round in 0..2 {
        let mut store = limited_store(&engine);
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        let address = memory.data_ptr(&store);
        assert_eq!(*base.get_or_insert(address as usize), address as usize);
        assert_zeroed(&store, &memory, &format!("round {round}"));
        if round == 1 {
            break;
        }
        memory.data_mut(&mut store).fill(0xc3);
        // SAFETY: the range is this Store's live, accessible linear memory;
        // MADV_PAGEOUT only asks the kernel to reclaim it.
        unsafe { rustix::mm::madvise(address.cast(), bytes, rustix::mm::Advice::LinuxPageOut) }
            .expect("MADV_PAGEOUT (Linux 5.4 and later)");
        let swapped = page_states(address, bytes)
            .iter()
            .filter(|(_, swapped)| *swapped)
            .count();
        assert!(
            swapped > 0,
            "no page of the dirty memory was swapped out: this host has no usable swap \
             device, so the swapped-page reset cannot be exercised here"
        );
    }
}

/// More Stores interrupted at once than a table of 64 published ranges holds
/// (each GC heap is one range): every one of them stops while all the others
/// still hold their ranges.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn every_one_of_many_concurrently_interrupted_stores_stops() {
    const STORES: usize = 80;
    let engine = Engine::new(&runtime_config()).unwrap();
    let module = heap_module(&engine);
    let ready = Arc::new(Barrier::new(STORES + 1));
    let release = Arc::new(Barrier::new(STORES + 1));
    let stopped = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = std::sync::mpsc::channel();
    let threads = (0..STORES)
        .map(|_| {
            let (engine, module, ready, release, stopped, sender) = (
                engine.clone(),
                module.clone(),
                ready.clone(),
                release.clone(),
                stopped.clone(),
                sender.clone(),
            );
            std::thread::spawn(move || {
                let mut heap = HeapStore::new(&engine, &module, STORE_LIMIT).unwrap();
                let cell = heap.cell(0);
                sender.send(heap.store.interrupt_handle()).unwrap();
                ready.wait();
                let error = heap
                    .spin
                    .call(
                        &mut heap.store,
                        &[Val::AnyRef(Some(cell.to_anyref()))],
                        &mut [],
                    )
                    .unwrap_err();
                assert_eq!(
                    error.downcast_ref::<Trap>(),
                    Some(&Trap::Interrupt),
                    "{error:?}"
                );
                stopped.fetch_add(1, Ordering::SeqCst);
                // Keep the Store, and so its published range, until every
                // Store has stopped.
                release.wait();
            })
        })
        .collect::<Vec<_>>();
    let handles = receiver.iter().take(STORES).collect::<Vec<_>>();
    ready.wait();
    std::thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    while stopped.load(Ordering::SeqCst) < STORES {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "{} of {STORES} interrupted Stores were still running",
            STORES - stopped.load(Ordering::SeqCst)
        );
        for handle in &handles {
            handle.interrupt();
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    release.wait();
    for thread in threads {
        thread.join().unwrap();
    }
    for handle in handles {
        handle.cancel();
    }
}
