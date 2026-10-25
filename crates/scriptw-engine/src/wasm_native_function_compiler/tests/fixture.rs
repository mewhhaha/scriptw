use super::*;
use scriptw_aot_wasm::{GcHostField, GcHostLayout, GcHostStorage};
use wasmtime::{
    ArrayRef, ArrayRefPre, ArrayType, AsContextMut, FieldType, FuncType, HeapType, Mutability,
    RefType, StorageType, StructRef, StructRefPre, StructType, ValType,
};

pub(super) fn store() -> WasmtimeStore<WasmHostState> {
    let engine = shared_wasm_engine().expect("required native GC engine");
    let realm = RealmBuilder::new().build();
    let mut store = WasmtimeStore::new(
        &engine,
        WasmHostState {
            user_data: None,
            embedding: None,
            weak: wasm_gc_weak_host::WeakGcState::default(),
            monotonic_clock_origin: realm.host_clock().monotonic_instant(),
            realm,
            output_events: WasmOutputEvents::DelegateOnly,
            intl_kernel: shared_embedded_intl_kernel().expect("embedded Intl").into(),
            can_block: false,
            shared_memory_backing: None,
            async_waiters: WasmStoreAsyncWaiters::new(None),
            agent_group: None,
            agent_commands: None,
            agent_leaving: None,
            native_source_compiler: None,
            native_template_instances: Default::default(),
            limits: WasmtimeStoreLimitsBuilder::new()
                .memory_size(WASM_STORE_MEMORY_CAP_BYTES)
                .build(),
        },
    );
    store.limiter(|state| &mut state.limits);
    store.set_epoch_deadline(u64::MAX / 2);
    store
}

fn structure(ty: StructType, nullable: bool) -> ValType {
    ValType::Ref(RefType::new(nullable, HeapType::ConcreteStruct(ty)))
}

/// Isolated consumed-field projections, not an emitted/admitted R artifact.
/// Opaque fields are deliberately not a parallel model of the runtime graph.
pub(super) fn signature(engine: &WasmtimeEngine) -> NativeFunctionSignature {
    let units = ArrayType::new(engine, FieldType::new(Mutability::Var, StorageType::I16));
    let units_reference = ValType::Ref(RefType::new(false, HeapType::ConcreteArray(units)));
    let string = StructType::new(
        engine,
        [
            FieldType::new(Mutability::Const, StorageType::ValType(units_reference)),
            FieldType::new(Mutability::Var, StorageType::ValType(ValType::I64)),
        ],
    )
    .unwrap();
    let stored_fields = [
        GcHostField::StoredTag,
        GcHostField::StoredScalar,
        GcHostField::StoredReference,
    ];
    assert_eq!(
        GcHostLayout::StoredValue.field_count(),
        Some(stored_fields.len() as u32)
    );
    let stored = StructType::new(
        engine,
        stored_fields.into_iter().map(|field| {
            let definition = field.definition();
            let storage = match definition.storage() {
                GcHostStorage::I32 => StorageType::ValType(ValType::I32),
                GcHostStorage::I64 => StorageType::ValType(ValType::I64),
                GcHostStorage::NullableEqRef => {
                    StorageType::ValType(ValType::Ref(RefType::new(true, HeapType::Eq)))
                }
                storage => panic!("unexpected StoredValue field projection: {storage:?}"),
            };
            FieldType::new(
                if definition.mutable() {
                    Mutability::Var
                } else {
                    Mutability::Const
                },
                storage,
            )
        }),
    )
    .unwrap();
    let sources = ArrayType::new(
        engine,
        FieldType::new(
            Mutability::Var,
            StorageType::ValType(structure(stored, false)),
        ),
    );
    let opaque = |layout: GcHostLayout| {
        StructType::new(
            engine,
            (0..layout.field_count().unwrap())
                .map(|_| FieldType::new(Mutability::Const, StorageType::ValType(ValType::I32))),
        )
        .unwrap()
    };
    let program = opaque(GcHostLayout::ProgramInstance);
    let mut context = opaque(GcHostLayout::FunctionContext)
        .fields()
        .collect::<Vec<_>>();
    let owner = GcHostField::ContextDefiningProgram.definition();
    context[owner.index() as usize] = FieldType::new(
        Mutability::Const,
        StorageType::ValType(structure(program, true)),
    );
    // This unconsumed field remains opaque, but its storage is a typed nullable
    // function reference under the same declaration-owned slot projection.
    let initializer = GcHostField::ContextInstanceElementsInitializer.definition();
    context[initializer.index() as usize] = FieldType::new(
        Mutability::Const,
        StorageType::ValType(ValType::Ref(RefType::new(
            true,
            HeapType::ConcreteFunc(FuncType::new(engine, [], [])),
        ))),
    );
    let context = StructType::new(engine, context).unwrap();
    let environment = opaque(GcHostLayout::Environment);
    let eq = ValType::Ref(RefType::new(true, HeapType::Eq));
    NativeFunctionSignature::validate(FuncType::new(
        engine,
        [
            ValType::I32,
            ValType::I32,
            ValType::I64,
            eq.clone(),
            ValType::Ref(RefType::new(false, HeapType::ConcreteArray(sources))),
            structure(context, false),
            structure(environment, true),
        ],
        [
            ValType::I32,
            ValType::I64,
            eq,
            ValType::I32,
            ValType::I32,
            structure(string, true),
        ],
    ))
    .expect("exact consumed host projections")
}

pub(super) fn string(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    signature: &NativeFunctionSignature,
    units: &[u16],
) -> wasmtime::Rooted<StructRef> {
    let pre = ArrayRefPre::new(&mut *store, signature.units.clone());
    let values = units
        .iter()
        .map(|unit| Val::I32(i32::from(*unit)))
        .collect::<Vec<_>>();
    let units = ArrayRef::new_fixed(&mut *store, &pre, &values).unwrap();
    let mut fields = vec![Val::I32(0); signature.string.fields().len()];
    fields[GcHostField::StringCodeUnits.definition().index() as usize] = Val::from(units);
    fields[GcHostField::StringRawKeyHash.definition().index() as usize] = Val::I64(0);
    let pre = StructRefPre::new(&mut *store, signature.string.clone());
    StructRef::new(&mut *store, &pre, &fields).unwrap()
}

pub(super) fn operands(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    signature: &NativeFunctionSignature,
    sources: &[&[u16]],
) -> [Val; 7] {
    let mut entries = Vec::new();
    for units in sources {
        let string = string(store, signature, units);
        let mut fields = vec![Val::I32(0); signature.stored.fields().len()];
        fields[GcHostField::StoredTag.definition().index() as usize] =
            Val::I32(WasmRuntimeValueTag::String.tag());
        fields[GcHostField::StoredScalar.definition().index() as usize] = Val::I64(0);
        fields[GcHostField::StoredReference.definition().index() as usize] = Val::from(string);
        let pre = StructRefPre::new(&mut *store, signature.stored.clone());
        entries.push(Val::from(
            StructRef::new(&mut *store, &pre, &fields).unwrap(),
        ));
    }
    let pre = ArrayRefPre::new(&mut *store, signature.sources.clone());
    let sources = ArrayRef::new_fixed(&mut *store, &pre, &entries).unwrap();
    let constructor = signature.arguments[5]
        .unwrap_ref()
        .heap_type()
        .unwrap_concrete_struct()
        .clone();
    let mut fields = vec![Val::I32(0); constructor.fields().len()];
    fields[GcHostField::ContextDefiningProgram.definition().index() as usize] = Val::AnyRef(None);
    fields[GcHostField::ContextInstanceElementsInitializer
        .definition()
        .index() as usize] = Val::FuncRef(None);
    let pre = StructRefPre::new(&mut *store, constructor);
    let constructor = StructRef::new(&mut *store, &pre, &fields).unwrap();
    [
        Val::I32(0),
        Val::I32(WasmRuntimeValueTag::Undefined.tag()),
        Val::I64(0),
        Val::AnyRef(None),
        Val::from(sources),
        Val::from(constructor),
        Val::AnyRef(None),
    ]
}
