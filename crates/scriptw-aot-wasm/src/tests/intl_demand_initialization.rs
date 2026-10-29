//! Check the physical demand boundary, complete graph and shared R/P GC ABI.
use super::*;
use crate::functions::NonArrayRealmIntrinsicSlot as Slot;
use crate::gc_types::{
    DeferredIntlNamespace, GcHeapType, GcNullability, GcSnapshotField, GcStructHeapType,
    GlobalLedger, IntrinsicTable, OrdinaryObject, PropertyValue, RealmRecord, RuntimeModuleTypes,
    RuntimeSchema, StoredValue,
};
use crate::heap::IntlInitializationState;

const INTL_SLOTS: [Slot; 22] = [
    Slot::IntlLocalePrototype,
    Slot::IntlDateTimeFormatPrototype,
    Slot::IntlNumberFormatPrototype,
    Slot::IntlPluralRulesPrototype,
    Slot::IntlListFormatPrototype,
    Slot::IntlCollatorPrototype,
    Slot::IntlDisplayNamesPrototype,
    Slot::IntlRelativeTimeFormatPrototype,
    Slot::IntlSegmenterPrototype,
    Slot::IntlDurationFormatPrototype,
    Slot::IntlSegmentsPrototype,
    Slot::IntlSegmentIteratorPrototype,
    Slot::IntlLocaleConstructor,
    Slot::IntlDateTimeFormatConstructor,
    Slot::IntlNumberFormatConstructor,
    Slot::IntlPluralRulesConstructor,
    Slot::IntlListFormatConstructor,
    Slot::IntlCollatorConstructor,
    Slot::IntlDisplayNamesConstructor,
    Slot::IntlRelativeTimeFormatConstructor,
    Slot::IntlSegmenterConstructor,
    Slot::IntlDurationFormatConstructor,
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
fn canonical_runtime_defers_the_complete_intl_graph_and_preserves_its_typed_abi() {
    // A program with no Intl reference still retains its complete canonical R.
    let artifact = emit_script("this;").unwrap();
    expect_valid_module(&artifact, 0);
    let bytes = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(bytes);
    let ensure_index = named(&names, "helper::realm_ensure_intl");
    let initial_index = named(&names, "helper::realm_initialize_intrinsics");
    let initial = body(bytes, initial_index);
    let ensure = body(bytes, ensure_index);
    let resolve = body(bytes, named(&names, "helper::property_value_resolve"));
    let calls = |ops: &[Operator<'_>], target| {
        ops.iter()
            .filter(|op| {
                matches!(op,
        Operator::Call { function_index } if *function_index == target)
            })
            .count()
    };
    assert_eq!(
        calls(&initial, ensure_index),
        0,
        "eager bootstrap cannot force Intl"
    );
    assert_eq!(
        calls(&ensure, ensure_index),
        0,
        "Building reads use their private capability"
    );
    assert_eq!(
        calls(&resolve, ensure_index),
        1,
        "property resolution forces the defining Realm once"
    );
    let mut constructors = 0;
    let mut getters = 0;
    for builtin in StandardBuiltinId::all_functions()
        .iter()
        .copied()
        .filter(|builtin| {
            builtin
                .debug_name()
                .strip_prefix("get ")
                .unwrap_or(builtin.debug_name())
                .starts_with("Intl.")
        })
    {
        let entry = named(&names, &format!("builtin::{}", builtin.debug_name()));
        assert!(
            !initial.iter().any(
                |op| matches!(op, Operator::RefFunc { function_index } if *function_index == entry)
            ),
            "eager bootstrap still allocates {}",
            builtin.debug_name()
        );
        if builtin.constructable()
            || builtin
                .native_function_name()
                .is_some_and(|name| name.starts_with("get "))
        {
            constructors += usize::from(builtin.constructable());
            getters += usize::from(!builtin.constructable());
            assert_eq!(
                ensure
                    .iter()
                    .filter(|op| matches!(op,
                Operator::RefFunc { function_index } if *function_index == entry))
                    .count(),
                1,
                "ensure owns the one eager callable for {}",
                builtin.debug_name()
            );
        }
    }
    assert_eq!((constructors, getters), (10, 15));
    for (helper, expected) in [
        ("helper::native_constructor_header_allocate", 10),
        ("helper::intrinsic_constructor_allocate", 10),
        ("helper::intrinsic_accessor_allocate", 15),
    ] {
        assert_eq!(
            calls(&ensure, named(&names, helper)),
            expected,
            "the actual demand owner allocates the complete original callable family"
        );
    }
    assert_eq!(
        calls(&ensure, named(&names, "helper::deferred_intrinsic_install")),
        49,
        "all original Intl method recipes belong to demand initialization"
    );

    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let realm_index = type_index::<RealmRecord>(schema);
    // ABI 38 appends the state and retained namespace to RealmRecord. Check
    // their physical types below without exposing private encoder ordinals.
    let state_field = RealmRecord::FIELD_COUNT - 3;
    let namespace_field = RealmRecord::FIELD_COUNT - 2;
    assert!(resolve.windows(5).any(|ops| matches!(ops,
        [Operator::StructGet { struct_type_index, field_index }, Operator::LocalSet { local_index: retained_realm },
         Operator::LocalGet { local_index: argument_realm }, Operator::RefAsNonNull, Operator::Call { function_index }]
        if *struct_type_index == type_index::<DeferredIntlNamespace>(schema)
            && *field_index == 0
            && retained_realm == argument_realm && *function_index == ensure_index)),
        "property resolution passes the recipe's retained defining Realm directly to ensure");
    let (types, declarations, exports) = inventory(bytes);
    assert!(exports.contains(&ensure_index));
    let wasmparser::CompositeInnerType::Func(signature) = &types
        [declarations[ensure_index as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("Intl ensure has an actual registered signature");
    };
    assert_eq!(signature.params(), [reference(realm_index, false)]);
    assert_eq!(
        signature.results(),
        [reference(type_index::<StoredValue>(schema), false)]
    );
    let wasmparser::CompositeInnerType::Func(signature) = &types
        [declarations[initial_index as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("eager bootstrap has an actual registered signature");
    };
    assert_eq!(
        signature.results(),
        [reference(type_index::<OrdinaryObject>(schema), true); 4]
    );

    for module in [bytes, artifact.bytes.as_slice()] {
        let (types, _, _) = inventory(module);
        let wasmparser::CompositeInnerType::Struct(recipe) = &types
            [type_index::<DeferredIntlNamespace>(schema) as usize]
            .composite_type
            .inner
        else {
            panic!("the real Intl recipe is a GC struct");
        };
        assert_eq!(
            recipe
                .fields
                .iter()
                .map(|field| (field.element_type, field.mutable))
                .collect::<Vec<_>>(),
            [(
                wasmparser::StorageType::Val(reference(realm_index, false)),
                false
            )]
        );
        assert_eq!(
            types[type_index::<DeferredIntlNamespace>(schema) as usize]
                .supertype_idx
                .unwrap()
                .unpack(),
            wasmparser::UnpackedIndex::Module(type_index::<PropertyValue>(schema))
        );
        let wasmparser::CompositeInnerType::Struct(realm) =
            &types[realm_index as usize].composite_type.inner
        else {
            panic!("the real Realm is a GC struct");
        };
        assert_eq!(
            (
                realm.fields[state_field as usize].element_type,
                realm.fields[state_field as usize].mutable
            ),
            (wasmparser::StorageType::Val(wasmparser::ValType::I32), true)
        );
        assert_eq!(
            (
                realm.fields[namespace_field as usize].element_type,
                realm.fields[namespace_field as usize].mutable
            ),
            (
                wasmparser::StorageType::Val(reference(type_index::<StoredValue>(schema), true)),
                true
            )
        );
    }

    let state_writes: Vec<_> = ensure
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
        state_writes
            .iter()
            .map(|(_, value)| *value)
            .collect::<Vec<_>>(),
        [
            IntlInitializationState::Building.word() as i32,
            IntlInitializationState::Ready.word() as i32
        ]
    );
    let building = state_writes[0].0;
    let ready = state_writes[1].0;
    let publication = ensure
        .iter()
        .position(|op| {
            matches!(op, Operator::StructSet { struct_type_index, field_index }
        if *struct_type_index == realm_index && *field_index == namespace_field)
        })
        .unwrap();
    assert!(
        building < publication && publication < ready,
        "publish only the complete rooted graph"
    );
    assert!(
        ensure[..building].windows(6).any(|ops| matches!(ops,
        [Operator::LocalGet { .. }, Operator::I32Const { value }, Operator::I32Ne,
         Operator::If { .. }, Operator::Unreachable, Operator::End]
        if *value == IntlInitializationState::Dormant.word() as i32)),
        "Building reentry traps before allocation"
    );
    let ready_return = ensure[..building]
        .iter()
        .position(|op| matches!(op, Operator::Return))
        .unwrap();
    assert!(ensure[..ready_return].windows(4).any(|ops| matches!(ops,
        [Operator::LocalGet { .. }, Operator::I32Const { value }, Operator::I32Eq, Operator::If { .. }]
        if *value == IntlInitializationState::Ready.word() as i32)));
    assert!(ensure[..ready_return].iter().any(|op| matches!(op,
        Operator::StructGet { struct_type_index, field_index }
        if *struct_type_index == realm_index && *field_index == namespace_field)));
    assert!(
        !ensure[..building].iter().any(|op| matches!(
            op,
            Operator::StructNew { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewFixed { .. }
                | Operator::ArrayNewDefault { .. }
        )),
        "Ready reuse and the reentry check allocate nothing"
    );

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
                panic!("actual intrinsic-table write names its scalar slot local");
            };
            let slot = constants[&local_index];
            assert!(building < index && index < publication);
            *slots.entry(slot).or_insert(0) += 1;
        }
    }
    assert_eq!(
        slots,
        INTL_SLOTS
            .into_iter()
            .map(|slot| (slot.gc_index(), 1))
            .collect(),
        "the complete 22 canonical slots are each published exactly once"
    );
}

#[test]
fn physical_canonical_intl_reads_ensure_the_same_realm_before_table_access() {
    let artifact = emit_script(
        "new Intl.NumberFormat('en').format(1); new Intl.Segmenter('en').segment('a');",
    )
    .unwrap();
    expect_valid_module(&artifact, 0);
    let runtime = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(runtime);
    let ensure = named(&names, "helper::realm_ensure_intl");
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let table = type_index::<IntrinsicTable>(schema);
    let realm = type_index::<RealmRecord>(schema);
    let field = GcSnapshotField::RealmIntrinsics.index();
    let slots: BTreeSet<_> = INTL_SLOTS.into_iter().map(Slot::gc_index).collect();
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
                let guard = guarded.expect("a canonical Intl read must import its ensure owner");
                let call = ops[..position]
                    .iter()
                    .rposition(|op| {
                        matches!(op,
                    Operator::Call { function_index } if *function_index == guard)
                    })
                    .expect("Intl read crosses ensure");
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

#[test]
fn previous_layout_is_rejected_even_with_a_valid_resigned_runtime_package() {
    use sha2::{Digest, Sha256};
    let artifact = emit_script("this;").unwrap();
    let runtime = artifact.runtime().unwrap();
    let selection =
        scriptw_intl::IntlDataSelection::new(scriptw_intl::IntlCompilationProfile::Minimal);
    let source = [38; 32];
    let package = crate::encode_portable_runtime_package(runtime, &selection, &source).unwrap();
    assert_eq!(&package[..8], b"SCRWRB01");
    assert_eq!(u32::from_le_bytes(package[8..12].try_into().unwrap()), 39);
    let admitted = crate::admit_portable_runtime_package(&package, &selection, &source).unwrap();
    assert_eq!(admitted.key(), runtime.key());
    let mut old = package;
    old[8..12].copy_from_slice(&38u32.to_le_bytes());
    let end = old.len() - 32;
    let digest = Sha256::digest(&old[..end]);
    old[end..].copy_from_slice(&digest);
    assert!(
        crate::admit_portable_runtime_package(&old, &selection, &source).is_err(),
        "a checksum-valid ABI38 envelope must not grant ABI39 layout authority"
    );
}
