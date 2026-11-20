//! A pooled Engine hands each Store a GC heap slot another Store returned.
//! The reused slot keeps its null guard, deadlines and Store limits, and
//! shows the next Store nothing of the previous one.
use scriptw_engine::{
    configure_pooled_stores, configure_wasm_runtime, NativeCompilationPolicy, PooledStoreReset,
    PooledStores,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use wasmtime::{
    ArrayRef, ArrayRefPre, ArrayType, Config, Engine, FieldType, Func, Instance, InterruptHandle,
    Module, Mutability, PoolConcurrencyLimitError, StorageType, Store, StoreLimits,
    StoreLimitsBuilder, StructRef, StructRefPre, Trap, Val,
};

fn pooled_engine() -> Engine {
    let mut config = Config::new();
    configure_wasm_runtime(
        &mut config,
        NativeCompilationPolicy::Fast,
        scriptw_engine::CompilationJobs::current(),
    );
    let reset = configure_pooled_stores(&mut config, &PooledStores::sequential());
    if wasmtime::PoolingAllocationConfig::is_pagemap_scan_available() {
        assert_eq!(
            reset,
            PooledStoreReset::KeepDirtyPagesResident {
                bytes: 16 * 1024 * 1024
            }
        );
    } else {
        assert_eq!(reset, PooledStoreReset::ReleaseAll);
    }
    Engine::new(&config).unwrap()
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
