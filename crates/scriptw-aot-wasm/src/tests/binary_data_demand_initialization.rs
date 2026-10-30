//! Physical canonical binary-data demand ownership and the shared R/P ABI.
use super::*;
use crate::functions::NonArrayRealmIntrinsicSlot as Slot;
use crate::gc_types::{
    DeferredBinaryDataConstructor, GcHeapType, GcNullability, GcSnapshotField, GlobalLedger,
    IntrinsicTable, PropertyValue, RealmRecord, RuntimeModuleTypes, RuntimeSchema,
};
use crate::heap::BinaryDataInitializationState;

const BINARY_DATA_SLOTS: [Slot; 32] = [
    Slot::ArrayBufferPrototype,
    Slot::SharedArrayBufferPrototype,
    Slot::DataViewPrototype,
    Slot::TypedArrayPrototype,
    Slot::Float64ArrayPrototype,
    Slot::Float32ArrayPrototype,
    Slot::Float16ArrayPrototype,
    Slot::Int32ArrayPrototype,
    Slot::Int16ArrayPrototype,
    Slot::Int8ArrayPrototype,
    Slot::Uint32ArrayPrototype,
    Slot::Uint16ArrayPrototype,
    Slot::Uint8ArrayPrototype,
    Slot::Uint8ClampedArrayPrototype,
    Slot::BigInt64ArrayPrototype,
    Slot::BigUint64ArrayPrototype,
    Slot::ArrayBufferConstructor,
    Slot::SharedArrayBufferConstructor,
    Slot::DataViewConstructor,
    Slot::TypedArrayConstructor,
    Slot::Float64ArrayConstructor,
    Slot::Float32ArrayConstructor,
    Slot::Float16ArrayConstructor,
    Slot::Int32ArrayConstructor,
    Slot::Int16ArrayConstructor,
    Slot::Int8ArrayConstructor,
    Slot::Uint32ArrayConstructor,
    Slot::Uint16ArrayConstructor,
    Slot::Uint8ArrayConstructor,
    Slot::Uint8ClampedArrayConstructor,
    Slot::BigInt64ArrayConstructor,
    Slot::BigUint64ArrayConstructor,
];

fn type_index<T: GcHeapType>(schema: &RuntimeSchema) -> u32 {
    let wasm_encoder::HeapType::Concrete(index) = schema
        .reference_type::<T>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("registered GC type has a concrete index");
    };
    index
}

fn reference(index: u32, nullable: bool) -> wasmparser::ValType {
    wasmparser::ValType::Ref(
        wasmparser::RefType::new(
            nullable,
            wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index)),
        )
        .unwrap(),
    )
}

fn body(bytes: &[u8], target: u32) -> Vec<Operator<'_>> {
    let mut index = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if index == target {
                return body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect();
            }
            index += 1;
        }
    }
    panic!("named R helper has a physical body");
}

fn inventory(bytes: &[u8]) -> (Vec<wasmparser::SubType>, Vec<u32>, BTreeSet<u32>) {
    let (mut types, mut declarations, mut exports) = (Vec::new(), Vec::new(), BTreeSet::new());
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::TypeSection(groups) => {
                for group in groups {
                    types.extend(group.unwrap().into_types());
                }
            }
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    if let wasmparser::TypeRef::Func(index)
                    | wasmparser::TypeRef::FuncExact(index) = import.unwrap().ty
                    {
                        declarations.push(index);
                    }
                }
            }
            Payload::FunctionSection(functions) => {
                declarations.extend(functions.into_iter().map(Result::unwrap))
            }
            Payload::ExportSection(entries) => {
                for entry in entries {
                    let entry = entry.unwrap();
                    if entry.kind == wasmparser::ExternalKind::Func {
                        exports.insert(entry.index);
                    }
                }
            }
            _ => {}
        }
    }
    (types, declarations, exports)
}

fn named(names: &BTreeMap<u32, String>, requested: &str) -> u32 {
    let found: Vec<_> = names
        .iter()
        .filter_map(|(&index, name)| (name == requested).then_some(index))
        .collect();
    assert_eq!(found.len(), 1, "one actual R body for {requested}");
    found[0]
}

#[test]
fn canonical_runtime_defers_the_complete_binary_data_graph_with_a_void_realm_abi() {
    let artifact = emit_script("this;").unwrap();
    expect_valid_module(&artifact, 0);
    let bytes = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(bytes);
    let ensure_index = named(&names, "helper::realm_ensure_binary_data");
    let initial = body(bytes, named(&names, "helper::realm_initialize_intrinsics"));
    let ensure = body(bytes, ensure_index);
    let resolve = body(bytes, named(&names, "helper::property_value_resolve"));
    let calls =
        |ops: &[Operator<'_>], target| {
            ops.iter().filter(|op|
        matches!(op, Operator::Call { function_index } if *function_index == target)).count()
        };
    assert_eq!(
        calls(&initial, ensure_index),
        0,
        "bootstrap cannot force binary data"
    );
    assert_eq!(
        calls(&ensure, ensure_index),
        0,
        "Building reads use their private capability"
    );
    assert_eq!(
        calls(&resolve, ensure_index),
        15,
        "one gated branch for each closed public constructor recipe"
    );
    let mut constructors = 0;
    let mut getters = 0;
    for builtin in StandardBuiltinId::all_functions()
        .iter()
        .copied()
        .filter(|builtin| {
            let name = builtin
                .debug_name()
                .strip_prefix("get ")
                .unwrap_or(builtin.debug_name());
            crate::module::standard_builtin_constructor_realm_slot(*builtin)
                .is_some_and(Slot::is_binary_data)
                || [
                    "ArrayBuffer.",
                    "ArrayBuffer ",
                    "SharedArrayBuffer.",
                    "DataView.",
                    "TypedArray.",
                    "TypedArray ",
                    "Uint8Array.",
                ]
                .into_iter()
                .any(|prefix| name.starts_with(prefix))
        })
    {
        let entry = named(&names, &format!("builtin::{}", builtin.debug_name()));
        let references = |ops: &[Operator<'_>]| {
            ops.iter().filter(|op|
            matches!(op, Operator::RefFunc { function_index } if *function_index == entry)).count()
        };
        if builtin == StandardBuiltinId::TypedArrayPrototypeToString {
            assert_eq!(
                references(&initial),
                1,
                "Array.prototype.toString owns the shared eager callable"
            );
            assert_eq!(
                references(&ensure),
                0,
                "binary initialization reuses Array's canonical callable"
            );
            continue;
        }
        assert_eq!(
            references(&initial),
            0,
            "eager allocation of {}",
            builtin.debug_name()
        );
        if builtin.constructable()
            || builtin
                .native_function_name()
                .is_some_and(|name| name.starts_with("get "))
        {
            constructors += usize::from(builtin.constructable());
            let expected_references = if builtin == StandardBuiltinId::ArrayBufferSpeciesGetter {
                2
            } else {
                1
            };
            getters += if builtin.constructable() {
                0
            } else {
                expected_references
            };
            assert_eq!(
                references(&ensure),
                expected_references,
                "one demand-owned callable for {}",
                builtin.debug_name()
            );
        }
    }
    assert_eq!((constructors, getters), (16, 19));
    for (helper, expected) in [
        ("helper::native_constructor_header_allocate", 16),
        ("helper::intrinsic_constructor_allocate", 16),
        ("helper::intrinsic_accessor_allocate", 19),
    ] {
        assert_eq!(
            calls(&ensure, named(&names, helper)),
            expected,
            "complete binary-data allocation family: {helper}"
        );
    }
    let batched = named(&names, "helper::deferred_intrinsic_install_run");
    let rows: usize = ensure.windows(3).filter_map(|ops| match ops {
        [Operator::I32Const { .. }, Operator::I32Const { value: count }, Operator::Call { function_index }]
            if *function_index == batched => Some(*count as usize),
        _ => None,
    }).sum();
    assert_eq!(
        rows + calls(&ensure, named(&names, "helper::deferred_intrinsic_install")),
        69,
        "all original binary-data method recipes belong to the demand owner"
    );
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let realm_index = type_index::<RealmRecord>(schema);
    let state_field = crate::gc_types::RealmRecordSchema::BINARY_DATA_STATE
        .ordinal()
        .raw();
    let (types, declarations, exports) = inventory(bytes);
    assert!(exports.contains(&ensure_index));
    let wasmparser::CompositeInnerType::Func(signature) = &types
        [declarations[ensure_index as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("binary-data ensure has a registered function signature");
    };
    assert_eq!(signature.params(), [reference(realm_index, false)]);
    assert!(
        signature.results().is_empty(),
        "canonical slots retain the complete graph without an extra namespace root"
    );
    for module in [bytes, artifact.bytes.as_slice()] {
        let (types, _, _) = inventory(module);
        let recipe_index = type_index::<DeferredBinaryDataConstructor>(schema);
        let wasmparser::CompositeInnerType::Struct(recipe) =
            &types[recipe_index as usize].composite_type.inner
        else {
            panic!("public constructor recipe is an actual GC struct");
        };
        assert_eq!(
            recipe
                .fields
                .iter()
                .map(|field| (field.element_type, field.mutable))
                .collect::<Vec<_>>(),
            [
                (
                    wasmparser::StorageType::Val(reference(realm_index, false)),
                    false
                ),
                (
                    wasmparser::StorageType::Val(wasmparser::ValType::I32),
                    false
                ),
            ]
        );
        assert_eq!(
            types[recipe_index as usize].supertype_idx.unwrap().unpack(),
            wasmparser::UnpackedIndex::Module(type_index::<PropertyValue>(schema))
        );
        let wasmparser::CompositeInnerType::Struct(realm) =
            &types[realm_index as usize].composite_type.inner
        else {
            panic!("actual Realm GC struct");
        };
        assert_eq!(
            (
                realm.fields[state_field as usize].element_type,
                realm.fields[state_field as usize].mutable
            ),
            (wasmparser::StorageType::Val(wasmparser::ValType::I32), true)
        );
    }
    let writes: Vec<_> = ensure
        .windows(2)
        .enumerate()
        .filter_map(|(index, ops)| match ops {
            [Operator::I32Const { value }, Operator::StructSet {
                struct_type_index,
                field_index,
            }] if *struct_type_index == realm_index && *field_index == state_field => {
                Some((index + 1, *value))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        writes.iter().map(|(_, value)| *value).collect::<Vec<_>>(),
        [
            BinaryDataInitializationState::Building.word() as i32,
            BinaryDataInitializationState::Ready.word() as i32,
        ]
    );
    let building = writes[0].0;
    let ready = writes[1].0;
    assert!(ensure[..building].windows(6).any(|ops| matches!(ops,
        [Operator::LocalGet { .. }, Operator::I32Const { value }, Operator::I32Ne,
         Operator::If { .. }, Operator::Unreachable, Operator::End]
            if *value == BinaryDataInitializationState::Dormant.word() as i32)));
    let ready_return = ensure[..building]
        .iter()
        .position(|op| matches!(op, Operator::Return))
        .unwrap();
    assert!(ensure[..ready_return].windows(4).any(|ops| matches!(ops,
        [Operator::LocalGet { .. }, Operator::I32Const { value }, Operator::I32Eq, Operator::If { .. }]
            if *value == BinaryDataInitializationState::Ready.word() as i32)));
    assert!(!ensure[..building].iter().any(|op| matches!(
        op,
        Operator::StructNew { .. }
            | Operator::ArrayNew { .. }
            | Operator::ArrayNewFixed { .. }
            | Operator::ArrayNewDefault { .. }
    )));
    let table_index = type_index::<IntrinsicTable>(schema);
    let mut constants = BTreeMap::new();
    let mut slots = BTreeMap::new();
    for (index, op) in ensure.iter().enumerate() {
        if let Operator::LocalSet { local_index } | Operator::LocalTee { local_index } = op {
            constants.remove(local_index);
            if let Some(Operator::I32Const { value }) = index.checked_sub(1).map(|i| &ensure[i]) {
                constants.insert(*local_index, *value as u32);
            }
        }
        if matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == table_index)
        {
            let Operator::LocalGet { local_index } = ensure[index - 3] else {
                panic!("actual intrinsic write names its slot");
            };
            assert!(
                building < index && index < ready,
                "complete the graph before Ready"
            );
            *slots.entry(constants[&local_index]).or_insert(0) += 1;
        }
    }
    assert_eq!(
        slots,
        BINARY_DATA_SLOTS
            .into_iter()
            .map(|slot| (slot.gc_index(), 1))
            .collect(),
        "all 32 canonical slots published exactly once"
    );
}

#[test]
fn physical_canonical_binary_data_reads_ensure_the_same_realm_before_table_access() {
    let artifact =
        emit_script("new Uint8Array(4).buffer; new DataView(new ArrayBuffer(8));").unwrap();
    expect_valid_module(&artifact, 0);
    let runtime = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(runtime);
    let ensure = named(&names, "helper::realm_ensure_binary_data");
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let table = type_index::<IntrinsicTable>(schema);
    let realm = type_index::<RealmRecord>(schema);
    let field = GcSnapshotField::RealmIntrinsics.index();
    let slots: BTreeSet<_> = BINARY_DATA_SLOTS.into_iter().map(Slot::gc_index).collect();
    let export = Parser::new(0)
        .parse_all(runtime)
        .find_map(|payload| {
            if let Payload::ExportSection(entries) = payload.unwrap() {
                return entries
                    .into_iter()
                    .map(Result::unwrap)
                    .find(|entry| {
                        entry.kind == wasmparser::ExternalKind::Func && entry.index == ensure
                    })
                    .map(|entry| entry.name);
            }
            None
        })
        .unwrap();
    let mut program_ensure = None;
    let mut import = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(entries) = payload.unwrap() {
            for entry in entries.into_imports() {
                let entry = entry.unwrap();
                if matches!(
                    entry.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if entry.module == crate::RUNTIME_IMPORT_NAMESPACE && entry.name == export {
                        program_ensure = Some(import);
                    }
                    import += 1;
                }
            }
        }
    }
    let mut checked = 0;
    for (bytes, guarded, builder) in [
        (runtime, Some(ensure), Some(ensure)),
        (artifact.bytes.as_slice(), program_ensure, None),
    ] {
        let mut function = imported_function_count_in_bytes(bytes);
        for payload in Parser::new(0).parse_all(bytes) {
            let Payload::CodeSectionEntry(body) = payload.unwrap() else {
                continue;
            };
            let index = function;
            function += 1;
            if Some(index) == builder {
                continue;
            }
            let ops: Vec<_> = body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .map(Result::unwrap)
                .collect();
            let mut constants = BTreeMap::new();
            for (position, op) in ops.iter().enumerate() {
                if let Operator::LocalSet { local_index } | Operator::LocalTee { local_index } = op
                {
                    constants.remove(local_index);
                    if let Some(Operator::I32Const { value }) =
                        position.checked_sub(1).map(|i| &ops[i])
                    {
                        constants.insert(*local_index, *value as u32);
                    }
                }
                if !matches!(op, Operator::ArrayGet { array_type_index } if *array_type_index == table)
                {
                    continue;
                }
                let Operator::LocalGet { local_index } = ops[position - 1] else {
                    panic!("intrinsic read names its slot");
                };
                if !constants
                    .get(&local_index)
                    .is_some_and(|slot| slots.contains(slot))
                {
                    continue;
                }
                let guard =
                    guarded.expect("a canonical binary-data read must import its ensure owner");
                let call = ops[..position]
                    .iter()
                    .rposition(|op| {
                        matches!(op,
                    Operator::Call { function_index } if *function_index == guard)
                    })
                    .expect("binary-data read crosses ensure");
                assert!(
                    position - call <= 20,
                    "guard belongs to this exact table read"
                );
                assert!(
                    !ops[call + 1..position].iter().any(|op| matches!(
                        op,
                        Operator::Block { .. }
                            | Operator::Loop { .. }
                            | Operator::If { .. }
                            | Operator::Else
                            | Operator::End
                            | Operator::Br { .. }
                            | Operator::BrIf { .. }
                            | Operator::BrTable { .. }
                            | Operator::Return
                            | Operator::Call { .. }
                            | Operator::CallRef { .. }
                            | Operator::CallIndirect { .. }
                    )),
                    "the ensure call dominates the raw read on one straight-line path"
                );
                let projection = ops[call + 1..position].iter().position(|op| matches!(op,
                    Operator::StructGet { struct_type_index, field_index } if *struct_type_index == realm && *field_index == field))
                    .map(|offset| call + 1 + offset).expect("read the ensured Realm's table");
                let [Operator::LocalGet {
                    local_index: ensured_realm,
                }, Operator::RefAsNonNull] = &ops[call - 2..call]
                else {
                    panic!("ensure receives its actual defining Realm");
                };
                let [Operator::LocalGet {
                    local_index: read_realm,
                }, Operator::RefAsNonNull] = &ops[projection - 2..projection]
                else {
                    panic!("table projection receives its actual defining Realm");
                };
                assert_eq!(
                    ensured_realm, read_realm,
                    "another Realm's ensure cannot authorize this read"
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 0,
        "inspect actual canonical readers rather than a source-shaped guard"
    );
}
