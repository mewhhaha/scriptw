//! Small owned R/P fixtures exercise actual native linking, not Cargo R admission.
use super::*;

fn leb(mut value: usize, bytes: &mut Vec<u8>) {
    loop {
        let next = (value & 127) as u8;
        value >>= 7;
        bytes.push(next | if value == 0 { 0 } else { 128 });
        if value == 0 {
            break;
        }
    }
}
fn name(value: &str, bytes: &mut Vec<u8>) {
    leb(value.len(), bytes);
    bytes.extend_from_slice(value.as_bytes());
}
fn section(module: &mut Vec<u8>, id: u8, bytes: &[u8]) {
    module.push(id);
    leb(bytes.len(), module);
    module.extend_from_slice(bytes);
}
fn prefix() -> Vec<u8> {
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    // () -> i32, (i32) -> (). The second type also describes the tag.
    section(&mut bytes, 1, &[2, 0x60, 0, 1, 0x7f, 0x60, 1, 0x7f, 0]);
    bytes
}
fn runtime() -> Vec<u8> {
    let mut bytes = prefix();
    section(&mut bytes, 3, &[1, 0]);
    section(&mut bytes, 5, &[1, 0, 1]);
    section(&mut bytes, 13, &[1, 0, 1]);
    section(&mut bytes, 6, &[1, 0x7f, 0, 0x41, 7, 0x0b]);
    let mut exports = vec![6];
    for (label, kind) in [
        ("f", 0),
        ("g", 3),
        ("m", 2),
        ("t", 4),
        ("unused", 0),
        ("unused_global", 3),
    ] {
        name(label, &mut exports);
        exports.extend_from_slice(&[kind, 0]);
    }
    section(&mut bytes, 7, &exports);
    section(&mut bytes, 10, &[1, 4, 0, 0x41, 42, 0x0b]);
    bytes
}
// Import descriptors are exact Wasm wire descriptors: function/global/memory/tag.
fn program(imports: &[(&str, &str, &[u8])], exports: &[(&str, u8, u8)]) -> Vec<u8> {
    let mut bytes = prefix();
    let mut declarations = Vec::new();
    leb(imports.len(), &mut declarations);
    for (namespace, label, ty) in imports {
        name(namespace, &mut declarations);
        name(label, &mut declarations);
        declarations.extend_from_slice(ty);
    }
    section(&mut bytes, 2, &declarations);
    let mut declarations = Vec::new();
    leb(exports.len(), &mut declarations);
    for (label, kind, index) in exports {
        name(label, &mut declarations);
        declarations.extend_from_slice(&[*kind, *index]);
    }
    section(&mut bytes, 7, &declarations);
    bytes
}
fn state() -> WasmHostState {
    let realm = RealmBuilder::new().build();
    WasmHostState {
        user_data: None,
        embedding: None,
        weak: Default::default(),
        native_source_compiler: None,
        native_template_instances: Default::default(),
        monotonic_clock_origin: realm.host_clock().monotonic_instant(),
        realm,
        output_events: WasmOutputEvents::DelegateOnly,
        intl_kernel: intl_data_images::WasmIntlKernel::Absent,
        can_block: false,
        async_waiters: WasmStoreAsyncWaiters::new(None),
        shared_memory_backing: None,
        agent_group: None,
        agent_commands: None,
        agent_leaving: None,
        limits: WasmtimeStoreLimitsBuilder::new().build(),
    }
}
fn engine() -> WasmtimeEngine {
    let mut config = wasmtime::Config::new();
    configure_wasm_runtime(&mut config);
    WasmtimeEngine::new(&config).unwrap()
}
fn modules(engine: &WasmtimeEngine, program: &[u8]) -> WasmModules {
    WasmModules {
        program: WasmtimeModule::new(engine, program).unwrap(),
        runtime: Some(WasmtimeModule::new(engine, runtime()).unwrap()),
        memory_cache_outcome: WasmModuleMemoryCacheOutcome::Bypassed,
        matched_runtime_image_cache: WasmRuntimeMatchedImageCacheOutcome::NotAccessed,
        loading: Default::default(),
    }
}

#[test]
fn requested_exports_link_once_and_runtime_keeps_unpublished_roots() {
    let engine = engine();
    let ns = scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE;
    let p = program(
        &[
            (ns, "f", &[0, 0]),
            (ns, "f", &[0, 0]),
            (ns, "g", &[3, 0x7f, 0]),
            (ns, "m", &[2, 0, 1]),
            (ns, "t", &[4, 0, 1]),
        ],
        &[("call", 0, 1), ("g", 3, 0), ("m", 2, 0), ("t", 4, 0)],
    );
    let modules = modules(&engine, &p);
    let mut store = WasmtimeStore::new(&engine, state());
    store.set_epoch_deadline(u64::MAX / 2);
    let mut linker = WasmtimeLinker::new(&engine);
    let (program, owner) = instantiate(&mut linker, &mut store, &modules).unwrap();
    assert_eq!(
        program
            .get_typed_func::<(), i32>(&mut store, "call")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        42
    );
    assert!(program.get_global(&mut store, "g").is_some());
    assert!(program.get_memory(&mut store, "m").is_some());
    assert!(program.get_tag(&mut store, "t").is_some());
    assert!(linker.get(&mut store, ns, "unused").is_none());
    assert!(linker.get(&mut store, ns, "unused_global").is_none());
    assert!(
        owner.get_func(&mut store, "unused").is_some(),
        "R remains the complete diagnostic/root owner"
    );
}

#[test]
fn missing_export_and_incompatible_repeated_import_are_errors() {
    let engine = engine();
    let ns = scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE;
    for (imports, expected) in [
        (vec![(ns, "missing", &[0, 0][..])], "runtime export missing"),
        (
            vec![(ns, "f", &[0, 0][..]), (ns, "f", &[0, 1][..])],
            "wasmtime instantiate failed",
        ),
    ] {
        let modules = modules(&engine, &program(&imports, &[]));
        let mut store = WasmtimeStore::new(&engine, state());
        let mut linker = WasmtimeLinker::new(&engine);
        let error = match instantiate(&mut linker, &mut store, &modules) {
            Ok(_) => panic!("invalid runtime import unexpectedly linked"),
            Err(error) => error,
        };
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn existing_runtime_definition_collision_is_not_masked() {
    let engine = engine();
    let ns = scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE;
    let modules = modules(&engine, &program(&[(ns, "f", &[0, 0])], &[]));
    let mut store = WasmtimeStore::new(&engine, state());
    let mut linker = WasmtimeLinker::new(&engine);
    linker.func_wrap(ns, "f", || 99i32).unwrap();
    assert!(instantiate(&mut linker, &mut store, &modules).is_err());
}

#[test]
fn foreign_namespace_is_untouched_and_other_store_definitions_still_fail() {
    let engine = engine();
    let p = program(&[("host", "f", &[0, 0])], &[("call", 0, 0)]);
    let modules = modules(&engine, &p);
    let mut first = WasmtimeStore::new(&engine, state());
    let function = wasmtime::Func::wrap(&mut first, || 88i32);
    let mut linker = WasmtimeLinker::new(&engine);
    linker.define(&first, "host", "f", function).unwrap();
    let mut other = WasmtimeStore::new(&engine, state());
    assert!(instantiate(&mut linker, &mut other, &modules).is_err());
    first.set_epoch_deadline(u64::MAX / 2);
    let (program, _) = instantiate(&mut linker, &mut first, &modules).unwrap();
    assert_eq!(
        program
            .get_typed_func::<(), i32>(&mut first, "call")
            .unwrap()
            .call(&mut first, ())
            .unwrap(),
        88
    );
    assert!(linker
        .get(&mut first, scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE, "f")
        .is_none());
}
