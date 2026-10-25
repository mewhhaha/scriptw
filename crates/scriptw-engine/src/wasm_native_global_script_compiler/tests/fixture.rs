//! Isolated consumed projections; these are not emitted/admitted R artifacts.
use super::*;
use scriptw_aot_wasm::{GcHostField, GcHostLayout};
use wasmtime::{
    ArrayRef, ArrayRefPre, ArrayType, FieldType, HeapType, Mutability, RefType, StorageType,
    StructRef, StructRefPre, StructType, ValType,
};

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
    store.limiter(|s| &mut s.limits);
    store.set_epoch_deadline(u64::MAX / 2);
    store
}
fn reference(ty: StructType, nullable: bool) -> ValType {
    ValType::Ref(RefType::new(nullable, HeapType::ConcreteStruct(ty)))
}
pub(super) fn signature(engine: &WasmtimeEngine) -> NativeGlobalScriptSignature {
    let units = ArrayType::new(engine, FieldType::new(Mutability::Var, StorageType::I16));
    let string = StructType::new(
        engine,
        [
            FieldType::new(
                Mutability::Const,
                StorageType::ValType(ValType::Ref(RefType::new(
                    false,
                    HeapType::ConcreteArray(units),
                ))),
            ),
            FieldType::new(Mutability::Var, StorageType::ValType(ValType::I64)),
        ],
    )
    .unwrap();
    let opaque = |layout: GcHostLayout| {
        StructType::new(
            engine,
            (0..layout.field_count().unwrap())
                .map(|_| FieldType::new(Mutability::Const, StorageType::ValType(ValType::I32))),
        )
        .unwrap()
    };
    let realm = opaque(GcHostLayout::RealmRecord);
    let program = opaque(GcHostLayout::ProgramInstance);
    NativeGlobalScriptSignature::validate(wasmtime::FuncType::new(
        engine,
        [
            ValType::I32,
            reference(string.clone(), false),
            reference(realm, false),
            reference(program, false),
        ],
        [
            ValType::I32,
            ValType::I64,
            ValType::Ref(RefType::new(true, HeapType::Eq)),
            ValType::I32,
            ValType::I32,
            ValType::I32,
            reference(string, true),
        ],
    ))
    .unwrap()
}
pub(super) fn string(
    store: &mut WasmtimeStore<WasmHostState>,
    signature: &NativeGlobalScriptSignature,
    units: &[u16],
) -> Rooted<StructRef> {
    let values = units
        .iter()
        .map(|u| Val::I32(i32::from(*u)))
        .collect::<Vec<_>>();
    let pre = ArrayRefPre::new(&mut *store, signature.units.clone());
    let units = ArrayRef::new_fixed(&mut *store, &pre, &values).unwrap();
    let mut fields = vec![Val::I32(0); signature.string.fields().len()];
    fields[GcHostField::StringCodeUnits.definition().index() as usize] = Val::from(units);
    fields[GcHostField::StringRawKeyHash.definition().index() as usize] = Val::I64(0);
    let pre = StructRefPre::new(&mut *store, signature.string.clone());
    StructRef::new(&mut *store, &pre, &fields).unwrap()
}
pub(super) fn operands(
    store: &mut WasmtimeStore<WasmHostState>,
    signature: &NativeGlobalScriptSignature,
    units: &[u16],
) -> [Val; 4] {
    let source = string(store, signature, units);
    let mut allocate = |ty: &StructType| {
        let pre = StructRefPre::new(&mut *store, ty.clone());
        StructRef::new(&mut *store, &pre, &vec![Val::I32(0); ty.fields().len()]).unwrap()
    };
    let realm = allocate(&signature.realm);
    let program = allocate(&signature.program);
    [
        Val::I32(0),
        Val::from(source),
        Val::from(realm),
        Val::from(program),
    ]
}
