use super::*;
use scriptw_aot_wasm::GcHostImport;
use wasmtime::{ArrayRef, ArrayRefPre, ArrayType, ExternType, Val};

pub(super) struct IntlHostProbe {
    store: WasmtimeStore<WasmHostState>,
    array: ArrayType,
    allocator: ArrayRefPre,
    call: wasmtime::Func,
}

/// A real Wasm forwarding body with the emitted canonical ByteArray import.
/// The compact binary uses the same GC encodings as wasm-encoder: mutable i8
/// array, non-null input reference and nullable reference result.
fn module_bytes(types: &[u8], function_type: u8, export_call: bool) -> Vec<u8> {
    fn section(module: &mut Vec<u8>, id: u8, bytes: &[u8]) {
        assert!(bytes.len() < 128);
        module.extend_from_slice(&[id, bytes.len() as u8]);
        module.extend_from_slice(bytes);
    }
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    section(&mut bytes, 1, types);
    let row = GcHostImport::IntlProviderCall;
    let mut import = vec![1, row.module().len().try_into().unwrap()];
    import.extend_from_slice(row.module().as_bytes());
    import.push(row.name().len().try_into().unwrap());
    import.extend_from_slice(row.name().as_bytes());
    import.extend_from_slice(&[0, function_type]);
    section(&mut bytes, 2, &import);
    if export_call {
        section(&mut bytes, 3, &[1, function_type]);
        section(&mut bytes, 7, b"\x01\x04call\x00\x01");
        section(&mut bytes, 10, &[1, 6, 0, 0x20, 0, 0x10, 0, 0x0b]);
    }
    bytes
}

const BYTE_ARRAY_CALL_TYPES: &[u8] = &[2, 0x5e, 0x78, 1, 0x60, 1, 0x64, 0, 1, 0x63, 0];

impl IntlHostProbe {
    pub(super) fn new() -> Self {
        let wasm_engine = shared_wasm_engine().unwrap();
        let module =
            WasmtimeModule::new(&wasm_engine, module_bytes(BYTE_ARRAY_CALL_TYPES, 1, true))
                .unwrap();
        let row = GcHostImport::IntlProviderCall;
        let import = module
            .imports()
            .find(|import| import.module() == row.module() && import.name() == row.name())
            .unwrap();
        let ExternType::Func(signature) = import.ty() else {
            panic!("Intl function import")
        };
        let parameter = signature.params().next().unwrap();
        let array = parameter
            .as_ref()
            .unwrap()
            .heap_type()
            .as_concrete_array()
            .unwrap()
            .clone();
        let realm = RealmBuilder::new().build();
        let mut store = WasmtimeStore::new(
            &wasm_engine,
            WasmHostState {
                user_data: None,
                embedding: None,
                weak: wasm_gc_weak_host::WeakGcState::default(),
                native_source_compiler: None,
                native_template_instances: Default::default(),
                monotonic_clock_origin: realm.host_clock().monotonic_instant(),
                realm,
                output_events: WasmOutputEvents::DelegateOnly,
                intl_kernel: shared_embedded_intl_kernel().unwrap().into(),
                can_block: true,
                shared_memory_backing: None,
                async_waiters: WasmStoreAsyncWaiters::new(None),
                agent_group: None,
                agent_commands: None,
                agent_leaving: None,
                limits: WasmtimeStoreLimitsBuilder::new()
                    .memory_size(WASM_STORE_MEMORY_CAP_BYTES)
                    .build(),
            },
        );
        store.set_epoch_deadline(u64::MAX / 2);
        let allocator = ArrayRefPre::new(&mut store, array.clone());
        let mut linker = WasmtimeLinker::new(&wasm_engine);
        wasm_gc_intl_host::link(&mut linker, &module).unwrap();
        let instance = linker.instantiate(&mut store, &module).unwrap();
        let call = instance.get_func(&mut store, "call").unwrap();
        Self {
            store,
            array,
            allocator,
            call,
        }
    }

    pub(super) fn invoke(
        &mut self,
        operation: IntlHostOp,
        payload: &[u8],
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        let mut request = operation.wire().to_le_bytes().to_vec();
        request.extend_from_slice(payload);
        self.invoke_request(&request)
    }

    fn invoke_request(&mut self, bytes: &[u8]) -> wasmtime::Result<Option<Vec<u8>>> {
        let request = ArrayRef::new_from_i8_slice(&mut self.store, &self.allocator, bytes)?;
        let mut output = [Val::AnyRef(None)];
        let result = self.call.call(
            &mut self.store,
            &[Val::AnyRef(Some(request.to_anyref()))],
            &mut output,
        );
        // The callback must preserve its rooted input even when native decoding
        // or semantic validation rejects it.
        let mut unchanged = vec![0; bytes.len()];
        request.copy_to_i8_slice(&mut self.store, &mut unchanged)?;
        assert_eq!(unchanged, bytes);
        result?;
        let [Val::AnyRef(reference)] = output else {
            panic!("Intl reference result")
        };
        let Some(reference) = reference else {
            return Ok(None);
        };
        let response = reference
            .as_array(&self.store)?
            .expect("Intl byte-array response");
        assert!(response.matches_ty(&self.store, &self.array)?);
        assert!(!wasmtime::Rooted::ref_eq(&self.store, &request, &response)?);
        let mut bytes = vec![0; response.len(&self.store)? as usize];
        response.copy_to_i8_slice(&mut self.store, &mut bytes)?;
        Ok(Some(bytes))
    }
}

#[test]
fn gc_import_rejects_nullable_wrong_storage_and_wrong_canonical_result() {
    let engine = shared_wasm_engine().unwrap();
    for (types, function_type) in [
        // The request must be non-null.
        (&[2, 0x5e, 0x78, 1, 0x60, 1, 0x63, 0, 1, 0x63, 0][..], 1),
        // The concrete array must have the schema's packed byte storage.
        (&[2, 0x5e, 0x77, 1, 0x60, 1, 0x64, 0, 1, 0x63, 0][..], 1),
        // The request array must have the schema's mutability.
        (&[2, 0x5e, 0x78, 0, 0x60, 1, 0x64, 0, 1, 0x63, 0][..], 1),
        // Different concrete input/output array types cannot share the bridge.
        (
            &[
                3, 0x5e, 0x78, 1, 0x5e, 0x77, 1, 0x60, 1, 0x64, 0, 1, 0x63, 1,
            ][..],
            2,
        ),
        // The response must admit semantic rejection as a null root.
        (&[2, 0x5e, 0x78, 1, 0x60, 1, 0x64, 0, 1, 0x64, 0][..], 1),
    ] {
        let module =
            WasmtimeModule::new(&engine, module_bytes(types, function_type, false)).unwrap();
        let mut linker = WasmtimeLinker::new(&engine);
        assert!(wasm_gc_intl_host::link(&mut linker, &module).is_err());
    }
}

#[test]
fn gc_request_requires_a_non_null_root_and_a_complete_known_operation() {
    let mut probe = IntlHostProbe::new();
    let mut output = [Val::AnyRef(None)];
    assert!(probe
        .call
        .call(&mut probe.store, &[Val::AnyRef(None)], &mut output)
        .is_err());
    assert!(probe.invoke_request(&[]).is_err());
    assert!(probe.invoke_request(&[0; 7]).is_err());
    assert!(probe.invoke_request(&u64::MAX.to_le_bytes()).is_err());
    let response = probe
        .invoke(IntlHostOp::LookupNamedTimeZone, b"UTC")
        .unwrap()
        .expect("valid request still works after faults");
    assert_eq!(&response[16..], b"UTCUTC");
}
