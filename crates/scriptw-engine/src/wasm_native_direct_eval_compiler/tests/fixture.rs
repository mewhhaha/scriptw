//! Isolated reply String projection, not a parallel runtime or caller model.
use super::*;
use wasmtime::{
    ArrayType, FieldType, HeapType, Mutability, RefType, StorageType, StructType, ValType,
};

pub(super) fn string_types(engine: &WasmtimeEngine) -> (StructType, ArrayType) {
    let units = ArrayType::new(engine, FieldType::new(Mutability::Var, StorageType::I16));
    let string = StructType::new(
        engine,
        [
            FieldType::new(
                Mutability::Const,
                StorageType::ValType(ValType::Ref(RefType::new(
                    false,
                    HeapType::ConcreteArray(units.clone()),
                ))),
            ),
            FieldType::new(Mutability::Var, StorageType::ValType(ValType::I64)),
        ],
    )
    .unwrap();
    (string, units)
}
pub(super) fn store() -> WasmtimeStore<WasmHostState> {
    let engine = shared_wasm_engine().unwrap();
    let realm = RealmBuilder::new().build();
    let mut store = WasmtimeStore::new(
        &engine,
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
            can_block: false,
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
    store.limiter(|state| &mut state.limits);
    store.set_epoch_deadline(u64::MAX / 2);
    store
}
