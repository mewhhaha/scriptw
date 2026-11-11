//! Protected GC address space retains logical traps, roots and Store limits.
use scriptw_engine::{
    configure_precompiled_wasm_runtime, configure_wasm_runtime, CompileOptions, EmbeddedRuntime,
    Engine, RealmBuilder,
};
use wasmtime::{
    ArrayRef, ArrayRefPre, Config, Instance, Module, Store, StoreLimitsBuilder, Trap, Val,
};

fn engine() -> wasmtime::Engine {
    let mut config = Config::new();
    configure_wasm_runtime(&mut config);
    wasmtime::Engine::new(&config).unwrap()
}

// The virtual-reservation policy is intended for the current 64-bit baseline.
#[cfg(target_pointer_width = "64")]
#[test]
fn public_runtime_configures_gc_reservation_independently_of_linear_memory() {
    let engine = engine();
    assert_eq!(engine.get_gc_heap_reservation(), 1_u64 << 32);
    assert_eq!(engine.get_gc_heap_initial_size(), 16 * 1024 * 1024);
    assert_eq!(engine.get_gc_heap_guard_size(), 32 * 1024 * 1024);
    assert_eq!(engine.get_gc_heap_reservation_for_growth(), 0);
    assert!(!engine.get_gc_heap_may_move());
    assert_eq!(engine.get_memory_reservation(), 1024 * 1024 * 1024);
    assert_eq!(engine.get_memory_guard_size(), 32 * 1024 * 1024);
    assert_eq!(engine.get_memory_reservation_for_growth(), 0);
    // A deadline watches the private linear memory, which must not relocate.
    assert!(!engine.get_memory_may_move());
    assert!(engine.get_guard_before_linear_memory());
}

// (module
//   (type $bytes (array (mut i8)))
//   (type $record (struct (field (mut i32))))
//   (func (export "byte") (param (ref null $bytes) i32) (result i32)
//     local.get 0 local.get 1 array.get_u $bytes)
//   (func (export "field") (param (ref null $record)) (result i32)
//     local.get 0 struct.get $record 0))
// Binary fixture avoids adding a second encoder/parser dependency to Engine.
fn probe_bytes() -> Vec<u8> {
    fn section(module: &mut Vec<u8>, id: u8, bytes: &[u8]) {
        assert!(bytes.len() < 128);
        module.extend_from_slice(&[id, bytes.len() as u8]);
        module.extend_from_slice(bytes);
    }
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut module,
        1,
        &[
            4, 0x5e, 0x78, 1, // mutable byte array
            0x5f, 1, 0x7f, 1, // mutable i32 struct
            0x60, 2, 0x63, 0, 0x7f, 1, 0x7f, // byte function
            0x60, 1, 0x63, 1, 1, 0x7f, // field function
        ],
    );
    section(&mut module, 3, &[2, 2, 3]);
    section(&mut module, 7, b"\x02\x04byte\x00\x00\x05field\x00\x01");
    section(
        &mut module,
        10,
        &[
            2, 9, 0, 0x20, 0, 0x20, 1, 0xfb, 0x0d, 0, 0x0b, 8, 0, 0x20, 0, 0xfb, 2, 1, 0, 0x0b,
        ],
    );
    module
}

#[cfg(target_pointer_width = "64")]
#[test]
fn gc_traps_and_live_roots_survive_forced_collection_and_heap_growth() {
    let engine = engine();
    let module = Module::new(&engine, probe_bytes()).unwrap();
    let heap_limit = 128 * 1024 * 1024;
    let mut store = Store::new(
        &engine,
        StoreLimitsBuilder::new().memory_size(heap_limit).build(),
    );
    store.limiter(|limits| limits);
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let byte = instance.get_func(&mut store, "byte").unwrap();
    let field = instance.get_func(&mut store, "field").unwrap();
    let parameter = byte.ty(&store).params().next().unwrap();
    let array_type = parameter
        .as_ref()
        .unwrap()
        .heap_type()
        .as_concrete_array()
        .unwrap()
        .clone();
    let allocator = ArrayRefPre::new(&mut store, array_type);
    let saved = ArrayRef::new(&mut store, &allocator, &Val::I32(0xa5), 2).unwrap();
    let mut output = [Val::I32(0)];
    for (function, arguments, expected) in [
        (
            byte,
            vec![Val::AnyRef(None), Val::I32(0)],
            Trap::NullReference,
        ),
        (field, vec![Val::AnyRef(None)], Trap::NullReference),
        (
            byte,
            vec![Val::AnyRef(Some(saved.to_anyref())), Val::I32(2)],
            Trap::ArrayOutOfBounds,
        ),
        (
            byte,
            vec![Val::AnyRef(Some(saved.to_anyref())), Val::I32(-1)],
            Trap::ArrayOutOfBounds,
        ),
    ] {
        let error = function
            .call(&mut store, &arguments, &mut output)
            .unwrap_err();
        assert_eq!(error.downcast_ref::<Trap>(), Some(&expected));
    }
    let initial_capacity = store.gc_heap_capacity();
    let mut live = Vec::new();
    for marker in 0..4 {
        live.push(
            ArrayRef::new(&mut store, &allocator, &Val::I32(marker), 8 * 1024 * 1024).unwrap(),
        );
        store.gc(None).unwrap();
        byte.call(
            &mut store,
            &[Val::AnyRef(Some(saved.to_anyref())), Val::I32(1)],
            &mut output,
        )
        .unwrap();
        assert_eq!(output[0].i32(), Some(0xa5));
        for (index, array) in live.iter().enumerate() {
            byte.call(
                &mut store,
                &[
                    Val::AnyRef(Some(array.to_anyref())),
                    Val::I32(8 * 1024 * 1024 - 1),
                ],
                &mut output,
            )
            .unwrap();
            assert_eq!(output[0].i32(), Some(index as i32));
        }
    }
    assert!(store.gc_heap_capacity() > initial_capacity);
    assert!(ArrayRef::new(&mut store, &allocator, &Val::I32(0), 256 * 1024 * 1024).is_err());
    assert!(store.gc_heap_capacity() <= heap_limit);
    byte.call(
        &mut store,
        &[Val::AnyRef(Some(saved.to_anyref())), Val::I32(1)],
        &mut output,
    )
    .unwrap();
    assert_eq!(output[0].i32(), Some(0xa5));
}

#[cfg(target_pointer_width = "64")]
#[test]
fn precompiled_admission_rejects_old_gc_policy_in_both_directions() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_script("42;", CompileOptions::default())
        .unwrap();
    let artifact = compiler.emit_wasm(&unit).unwrap();
    assert!(artifact.runtime.is_none());
    let producer = engine();
    let image = EmbeddedRuntime::precompile(&producer, &artifact).unwrap();

    let mut old_config = Config::new();
    configure_wasm_runtime(&mut old_config);
    old_config
        .gc_heap_reservation(1_u64 << 30)
        .gc_heap_may_move(true);
    let old_producer = wasmtime::Engine::new(&old_config).unwrap();
    let old_image = EmbeddedRuntime::precompile(&old_producer, &artifact).unwrap();

    let mut consumer_config = Config::new();
    configure_precompiled_wasm_runtime(&mut consumer_config);
    let consumer = wasmtime::Engine::new(&consumer_config).unwrap();
    assert!(consumer.get_cranelift_opt_level().is_none());
    EmbeddedRuntime::from_precompiled(&consumer, &image).unwrap();
    let error = EmbeddedRuntime::from_precompiled(&consumer, &old_image)
        .err()
        .unwrap();
    assert!(error.to_string().contains("native program is incompatible"));
    assert!(error.to_string().contains("GC heap reservation"));

    consumer_config
        .gc_heap_reservation(1_u64 << 30)
        .gc_heap_may_move(true);
    let old_consumer = wasmtime::Engine::new(&consumer_config).unwrap();
    EmbeddedRuntime::from_precompiled(&old_consumer, &old_image).unwrap();
    let error = EmbeddedRuntime::from_precompiled(&old_consumer, &image)
        .err()
        .unwrap();
    assert!(error.to_string().contains("native program is incompatible"));
    assert!(error.to_string().contains("GC heap reservation"));
}
