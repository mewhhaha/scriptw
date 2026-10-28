use super::*;
use scriptw_aot_wasm::GcHostImport;
use scriptw_intl::ConfiguredSystemTimeZone;
use wasmtime::{ArrayRef, ArrayType, ExternType, Rooted, Val};

const SYSTEM_ZONE_GC_TYPES: &[u8] = &[2, 0x5e, 0x78, 1, 0x60, 0, 1, 0x64, 0];

fn probe_module_with_types(
    types: &[u8],
    export_call: bool,
    identity_sections: &[&[u8]],
) -> Vec<u8> {
    fn u32_leb(mut value: u32, output: &mut Vec<u8>) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            output.push(byte | if value == 0 { 0 } else { 0x80 });
            if value == 0 {
                break;
            }
        }
    }
    fn name(value: &str, output: &mut Vec<u8>) {
        u32_leb(value.len() as u32, output);
        output.extend_from_slice(value.as_bytes());
    }
    fn section(id: u8, payload: &[u8], output: &mut Vec<u8>) {
        output.push(id);
        u32_leb(payload.len() as u32, output);
        output.extend_from_slice(payload);
    }
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    // The emitted host domain is a mutable packed i8 array and a no-input
    // function returning a non-null reference to that exact canonical type.
    section(1, types, &mut bytes);
    let row = GcHostImport::SystemTimeZoneSnapshot;
    let mut import = vec![1];
    name(row.module(), &mut import);
    name(row.name(), &mut import);
    import.extend_from_slice(&[0, 1]);
    section(2, &import, &mut bytes);
    if export_call {
        section(3, &[1, 1], &mut bytes);
        section(7, b"\x01\x04call\x00\x01", &mut bytes);
        section(10, &[1, 4, 0, 0x10, 0, 0x0b], &mut bytes);
    }
    for identity in identity_sections {
        let mut custom = Vec::new();
        name(INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION, &mut custom);
        custom.extend_from_slice(identity);
        section(0, &custom, &mut bytes);
    }
    bytes
}

fn probe_module(identity_sections: &[&[u8]]) -> Vec<u8> {
    probe_module_with_types(SYSTEM_ZONE_GC_TYPES, true, identity_sections)
}

struct SystemZoneProbe {
    store: WasmtimeStore<WasmHostState>,
    array: ArrayType,
    call: wasmtime::Func,
}
impl SystemZoneProbe {
    fn new(zone: ConfiguredSystemTimeZone) -> Self {
        let wasm_engine = shared_wasm_engine().unwrap();
        let module = WasmtimeModule::new(&wasm_engine, probe_module(&[])).unwrap();
        let row = GcHostImport::SystemTimeZoneSnapshot;
        let import = module
            .imports()
            .find(|import| import.module() == row.module() && import.name() == row.name())
            .unwrap();
        let ExternType::Func(signature) = import.ty() else {
            panic!("system-zone function import")
        };
        let result = signature.results().next().unwrap();
        let array = result
            .as_ref()
            .unwrap()
            .heap_type()
            .as_concrete_array()
            .unwrap()
            .clone();
        let realm = RealmBuilder::new().with_system_time_zone(zone).build();
        let mut store = WasmtimeStore::new(
            &wasm_engine,
            WasmHostState {
                user_data: None,
                embedding: None,
                weak: wasm_gc_weak_host::WeakGcState::default(),
                #[cfg(feature = "native-source-compiler")]
                native_source_compiler: None,
                #[cfg(feature = "native-source-compiler")]
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
        let mut linker = WasmtimeLinker::new(&wasm_engine);
        wasm_gc_intl_host::link(&mut linker, &module).unwrap();
        let instance = linker.instantiate(&mut store, &module).unwrap();
        let call = instance.get_func(&mut store, "call").unwrap();
        Self { store, array, call }
    }
    fn invoke_array(&mut self) -> wasmtime::Result<Rooted<ArrayRef>> {
        let mut output = [Val::AnyRef(None)];
        self.call.call(&mut self.store, &[], &mut output)?;
        let [Val::AnyRef(Some(reference))] = output else {
            return Err(wasmtime::Error::msg("system-zone result has no GC root"));
        };
        let response = reference
            .as_array(&self.store)?
            .ok_or_else(|| wasmtime::Error::msg("system-zone result is not an array"))?;
        assert!(response.matches_ty(&self.store, &self.array)?);
        Ok(response)
    }
    fn read_snapshot(&mut self, response: &Rooted<ArrayRef>) -> wasmtime::Result<Vec<u8>> {
        let mut bytes = vec![0; response.len(&self.store)? as usize];
        response.copy_to_i8_slice(&mut self.store, &mut bytes)?;
        Ok(bytes)
    }
    fn invoke(&mut self) -> wasmtime::Result<Vec<u8>> {
        let response = self.invoke_array()?;
        self.read_snapshot(&response)
    }
}

#[test]
fn system_zone_gc_snapshot_publishes_primary_and_retains_each_root() {
    let zone = ConfiguredSystemTimeZone::resolve("US/Eastern").unwrap();
    let expected = zone.encode();
    let mut probe = SystemZoneProbe::new(zone);
    let first = probe.invoke_array().unwrap();
    let second = probe.invoke_array().unwrap();
    assert!(!Rooted::ref_eq(&probe.store, &first, &second).unwrap());
    assert_eq!(probe.read_snapshot(&first).unwrap(), expected);
    assert_eq!(probe.read_snapshot(&second).unwrap(), expected);
    assert_eq!(&expected[32..], b"America/New_York");
}

#[test]
fn system_zone_fixed_zero_keeps_the_numeric_response_association() {
    let mut probe = SystemZoneProbe::new(ConfiguredSystemTimeZone::resolve("-00:00").unwrap());
    let bytes = probe.invoke().unwrap();
    assert_eq!(bytes.len(), 38);
    assert_eq!(i64::from_le_bytes(bytes[8..16].try_into().unwrap()), 1);
    assert_eq!(i64::from_le_bytes(bytes[16..24].try_into().unwrap()), 0);
    assert_eq!(&bytes[32..], b"+00:00");
}

#[test]
fn system_zone_import_rejects_invalid_gc_result_and_call_domains() {
    let engine = shared_wasm_engine().unwrap();
    for types in [
        // The result cannot be null.
        &[2, 0x5e, 0x78, 1, 0x60, 0, 1, 0x63, 0][..],
        // The result array must use packed i8 storage and be mutable.
        &[2, 0x5e, 0x77, 1, 0x60, 0, 1, 0x64, 0][..],
        &[2, 0x5e, 0x78, 0, 0x60, 0, 1, 0x64, 0][..],
        // The host takes no arguments and returns exactly one reference.
        &[2, 0x5e, 0x78, 1, 0x60, 1, 0x7f, 1, 0x64, 0][..],
        &[2, 0x5e, 0x78, 1, 0x60, 0, 0][..],
        &[2, 0x5e, 0x78, 1, 0x60, 0, 2, 0x64, 0, 0x64, 0][..],
        &[2, 0x5e, 0x78, 1, 0x60, 0, 1, 0x7e][..],
        // A concrete struct reference cannot stand in for the byte array.
        &[2, 0x5f, 0, 0x60, 0, 1, 0x64, 0][..],
    ] {
        let module =
            WasmtimeModule::new(&engine, probe_module_with_types(types, false, &[])).unwrap();
        let mut linker = WasmtimeLinker::new(&engine);
        assert!(wasm_gc_intl_host::link(&mut linker, &module).is_err());
    }
}

#[test]
fn system_zone_import_requires_the_existing_pinned_artifact_identity() {
    let expected = scriptw_intl::embedded_intl_data_identity().unwrap();
    let identity = expected.artifact_identity();
    validate_wasm_intl_artifact_identity(
        &probe_module(&[identity.as_bytes()]),
        &expected,
        IntlArtifactIdentityRequirement::HostImports,
    )
    .unwrap();
    for (sections, error) in [
        (
            Vec::<&[u8]>::new(),
            IntlArtifactIdentityError::MissingSection,
        ),
        (
            vec![identity.as_bytes(), identity.as_bytes()],
            IntlArtifactIdentityError::DuplicateSections,
        ),
        (
            vec![b"different".as_slice()],
            IntlArtifactIdentityError::IdentityMismatch,
        ),
    ] {
        assert_eq!(
            validate_wasm_intl_artifact_identity(
                &probe_module(&sections),
                &expected,
                IntlArtifactIdentityRequirement::HostImports,
            )
            .unwrap_err()
            .intl_artifact_identity_error(),
            Some(error)
        );
    }
}
