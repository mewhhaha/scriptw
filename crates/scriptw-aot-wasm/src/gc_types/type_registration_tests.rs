#![cfg(test)]
use super::*;
use crate::module::StaticSignature;
use crate::runtime_artifact::ModuleKind;
use crate::runtime_helpers::{HelperOwner, HelperTypeRegion, ProgramHook, RuntimeHelperId};
use wasmparser::{CompositeInnerType, Parser, Payload, SubType, Validator, WasmFeatures};

mod compact_records;

fn encoded(kind: ModuleKind<'_>) -> (FinalizedRuntimeModule, Vec<u8>) {
    let runtime =
        RuntimeModuleTypes::register(kind).finalize_globals(GlobalLedger::new(), false, 0);
    let mut module = wasm_encoder::Module::new();
    module.section(runtime.types());
    module.section(&runtime.globals().defined_section(0));
    let bytes = module.finish();
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(&bytes)
        .expect("registered concrete signatures and typed ProgramInstance graph validate");
    (runtime, bytes)
}

fn declarations(bytes: &[u8]) -> (Vec<SubType>, Vec<usize>, Vec<wasmparser::GlobalType>) {
    let mut types = Vec::new();
    let mut groups = Vec::new();
    let mut globals = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("registered module decodes") {
            Payload::TypeSection(reader) => {
                for group in reader {
                    let group = group.expect("type group decodes");
                    groups.push(group.types().len());
                    types.extend(group.types().cloned());
                }
            }
            Payload::GlobalSection(reader) => {
                globals.extend(reader.into_iter().map(|global| global.unwrap().ty));
            }
            _ => {}
        }
    }
    (types, groups, globals)
}

#[test]
fn direct_program_signatures_follow_the_exact_core_graph_and_hooks_keep_core_types() {
    let (runtime, runtime_bytes) = encoded(ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ));
    let (_, native_bytes) = encoded(ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::NativeHost,
    ));
    assert_eq!(
        runtime_bytes, native_bytes,
        "source compiler authority cannot change the core GC schema"
    );
    let (_, program_bytes) = encoded(ModuleKind::Standalone);
    let (core, core_groups, core_globals) = declarations(&runtime_bytes);
    let (program, program_groups, program_globals) = declarations(&program_bytes);
    let core_end = StaticSignature::ALL.len()
        + RuntimeHelperId::type_count(HelperTypeRegion::Core)
        + GcLayout::ALL.len();
    let program_count = RuntimeHelperId::type_count(HelperTypeRegion::Program);
    assert_eq!(core.len(), core_end, "R has no direct program signatures");
    assert_eq!(program.len(), core_end + program_count);
    assert_eq!(
        &program[..core_end],
        &core,
        "every exact core type is identical"
    );
    assert_eq!(&program_groups[..core_groups.len()], &core_groups);
    assert_eq!(program_groups.len(), core_groups.len() + program_count);
    assert!(
        program_groups[core_groups.len()..]
            .iter()
            .all(|size| *size == 1),
        "direct program signatures cannot enlarge the core recursion group"
    );
    assert_eq!(
        program_globals, core_globals,
        "the suffix cannot renumber core roots"
    );

    let mut core_indexes = std::collections::BTreeSet::new();
    let mut program_indexes = std::collections::BTreeSet::new();
    for helper in RuntimeHelperId::ALL {
        let index = helper.type_index() as usize;
        let expected = helper.definition(runtime.globals().schema().layouts());
        let CompositeInnerType::Func(actual) = &program[index].composite_type.inner else {
            panic!("{helper:?} must name its actual function type");
        };
        let convert = |values: &[wasmparser::ValType]| {
            values
                .iter()
                .copied()
                .map(|value| ValType::try_from(value).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            convert(actual.params()).as_slice(),
            expected.parameters(),
            "{helper:?} parameters"
        );
        assert_eq!(
            convert(actual.results()).as_slice(),
            expected.results(),
            "{helper:?} results"
        );
        match helper.owner() {
            HelperOwner::Runtime | HelperOwner::Hook(_) | HelperOwner::ContextHook => {
                assert!(index < core_end && core_indexes.insert(index));
                assert_eq!(core[index], program[index]);
            }
            HelperOwner::Program => {
                assert!(index >= core_end && program_indexes.insert(index));
            }
        }
    }
    assert_eq!(
        core_indexes.into_iter().collect::<Vec<_>>(),
        (StaticSignature::ALL.len()
            ..StaticSignature::ALL.len() + RuntimeHelperId::recursive_type_count())
            .chain(
                StaticSignature::ALL.len()
                    + RuntimeHelperId::recursive_type_count()
                    + GcLayout::ALL.len()..core_end
            )
            .collect::<Vec<_>>()
    );
    assert_eq!(
        program_indexes.into_iter().collect::<Vec<_>>(),
        (core_end..program.len()).collect::<Vec<_>>()
    );

    let instance_index = runtime
        .globals()
        .schema()
        .layouts()
        .layout_index(GcLayout::ProgramInstance) as usize;
    let CompositeInnerType::Struct(instance) = &core[instance_index].composite_type.inner else {
        panic!("ProgramInstance must be an actual GC struct");
    };
    assert_eq!(instance.fields.len(), 13);
    let payload_index = ProgramInstanceSchema::NATIVE_SOURCE_PAYLOAD.ordinal().raw() as usize;
    assert_eq!(payload_index, 10);
    assert!(!instance.fields[payload_index].mutable);
    let payload_type = runtime
        .globals()
        .schema()
        .array_type::<NativeSourcePayloadArray>()
        .type_index()
        .raw();
    assert_eq!(
        instance.fields[payload_index].element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                false,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(payload_type))
            )
            .unwrap()
        ))
    );
    let CompositeInnerType::Array(payload) = &core[payload_type as usize].composite_type.inner
    else {
        panic!("native payload is a packed GC array");
    };
    assert_eq!(payload.0.element_type, wasmparser::StorageType::I8);
    assert!(
        !payload.0.mutable,
        "native host payloads require physical Wasm immutability"
    );

    let policy_index = ProgramInstanceSchema::LANGUAGE_FEATURE_POLICY
        .ordinal()
        .raw() as usize;
    assert_eq!(policy_index, 9);
    assert_eq!(
        instance.fields[policy_index].element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::I32)
    );
    assert!(!instance.fields[policy_index].mutable);
    let mut next_group_start = 0;
    let instance_group = core_groups
        .iter()
        .find_map(|size| {
            let range = next_group_start..next_group_start + *size;
            next_group_start = range.end;
            range.contains(&instance_index).then_some(range)
        })
        .expect("the actual ProgramInstance belongs to one emitted recursion group");
    assert!(instance_group.len() > 1);
    for helper in RuntimeHelperId::ALL {
        let index = helper.type_index() as usize;
        assert_eq!(
            instance_group.contains(&index),
            helper.requires_runtime_group(),
            "{helper:?}: only actual typed GC fields require recursive signatures"
        );
        if helper.type_region() == HelperTypeRegion::Core && !helper.requires_runtime_group() {
            assert!(index >= instance_group.end);
            let mut first = 0;
            let size = core_groups
                .iter()
                .find_map(|size| {
                    let contains = (first..first + *size).contains(&index);
                    first += *size;
                    contains.then_some(*size)
                })
                .unwrap();
            assert_eq!(size, 1, "ordinary runtime signatures must stand alone");
        }
    }

    for hook in ProgramHook::ALL {
        let helper = hook.helper();
        assert!(
            helper.is_program_owned(),
            "the hook body remains program-owned"
        );
        assert_eq!(helper.type_region(), HelperTypeRegion::Core);
        assert!(
            instance_group.contains(&(helper.type_index() as usize)),
            "{hook:?} and ProgramInstance must be in the same actual recursion group"
        );
        let CompositeInnerType::Func(signature) =
            &core[helper.type_index() as usize].composite_type.inner
        else {
            panic!("the typed hook must reference its actual registered function type");
        };
        assert_eq!(
            ValType::try_from(signature.params()[0]).unwrap(),
            ValType::Ref(RefType {
                nullable: false,
                heap_type: HeapType::Concrete(instance_index as u32),
            }),
            "every typed hook receives the actual non-null ProgramInstance first"
        );

        let field = &instance.fields[hook as usize];
        assert!(!field.mutable, "a published P hook cannot be replaced");
        let wasmparser::StorageType::Val(actual) = field.element_type else {
            panic!("a ProgramInstance hook must be a typed function reference");
        };
        assert_eq!(
            ValType::try_from(actual).unwrap(),
            ValType::Ref(RefType {
                nullable: true,
                heap_type: HeapType::Concrete(helper.type_index()),
            }),
            "P's {hook:?} field consumes the exact registered core hook signature"
        );
        assert!(
            core_globals.iter().all(|global| {
                ValType::try_from(global.content_type).unwrap()
                    != ValType::Ref(RefType {
                        nullable: true,
                        heap_type: HeapType::Concrete(helper.type_index()),
                    })
            }),
            "the shared runtime must not retain a hook global"
        );
    }
}

#[test]
fn empty_storage_globals_are_nonnull_immutable_zero_length_arrays() {
    for kind in [
        ModuleKind::Runtime(crate::RuntimeSourceCompilationProfile::NativeHost),
        ModuleKind::Runtime(crate::RuntimeSourceCompilationProfile::Portable),
        ModuleKind::Standalone,
    ] {
        let (module, bytes) = encoded(kind);
        let schema = module.globals().schema();
        let expected = [
            (
                schema.empty_arrays.values.index(),
                schema.array_type::<ValueArray>().type_index().raw(),
            ),
            (
                schema.empty_arrays.properties.index(),
                schema.array_type::<PropertyTable>().type_index().raw(),
            ),
            (
                schema.empty_arrays.property_index.index(),
                schema
                    .array_type::<OrdinaryPropertyIndex>()
                    .type_index()
                    .raw(),
            ),
            (
                schema.empty_arrays.private_elements.index(),
                schema
                    .array_type::<PrivateElementTable>()
                    .type_index()
                    .raw(),
            ),
            (
                schema.empty_arrays.array_buckets.index(),
                schema
                    .array_type::<ArrayIndexBucketTable>()
                    .type_index()
                    .raw(),
            ),
            (
                schema.empty_arrays.array_elements.index(),
                schema.array_type::<IndexedTable>().type_index().raw(),
            ),
            (
                schema.empty_arrays.number_bits.index(),
                schema.array_type::<NumberBitsI64Array>().type_index().raw(),
            ),
            (
                schema.empty_arrays.number_presence.index(),
                schema
                    .array_type::<NumberPresenceU64Array>()
                    .type_index()
                    .raw(),
            ),
        ];
        let mut checked = 0;
        for payload in Parser::new(0).parse_all(&bytes) {
            if let Payload::GlobalSection(globals) = payload.unwrap() {
                for (index, global) in globals.into_iter().enumerate() {
                    let Some((_, array)) = expected.iter().find(|(slot, _)| *slot == index as u32)
                    else {
                        continue;
                    };
                    let global = global.unwrap();
                    assert!(!global.ty.mutable && !global.ty.shared);
                    assert_eq!(
                        global.ty.content_type,
                        wasmparser::ValType::Ref(
                            wasmparser::RefType::new(
                                false,
                                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(
                                    *array
                                ))
                            )
                            .unwrap()
                        )
                    );
                    let mut operators = global.init_expr.get_operators_reader();
                    assert!(matches!(operators.read().unwrap(),
                        wasmparser::Operator::ArrayNewFixed { array_type_index, array_size: 0 }
                        if array_type_index == *array));
                    assert!(matches!(
                        operators.read().unwrap(),
                        wasmparser::Operator::End
                    ));
                    assert!(operators.eof());
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, expected.len());
        assert_eq!(
            module.globals().runtime_global_types().len(),
            module.globals().schema().module_guards.first as usize
        );
    }
}

#[test]
fn ordinary_storage_subtypes_and_cache_metadata_are_encoded_in_the_single_core_graph() {
    let (runtime, bytes) = encoded(ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ));
    let (types, _, _) = declarations(&bytes);
    let layouts = runtime.globals().schema().layouts();
    let index = |layout| layouts.layout_index(layout);
    let base = index(GcLayout::ObjectPropertyStorage);
    assert!(!types[base as usize].is_final);
    let CompositeInnerType::Struct(fields) = &types[base as usize].composite_type.inner else {
        panic!("storage base is a struct");
    };
    assert!(
        fields.fields.is_empty(),
        "the base has no dictionary-shaped fields"
    );
    for layout in [
        GcLayout::OrdinaryPropertyStorage,
        GcLayout::CompactDataStorage,
    ] {
        let subtype = &types[index(layout) as usize];
        assert!(subtype.is_final);
        assert_eq!(
            subtype.supertype_idx.map(|parent| parent.unpack()),
            Some(wasmparser::UnpackedIndex::Module(base))
        );
    }
    let fields = |layout| match &types[index(layout) as usize].composite_type.inner {
        CompositeInnerType::Struct(record) => record.fields.as_ref(),
        _ => panic!("metadata owner is a struct"),
    };
    let reference = |layout, nullable| {
        wasmparser::StorageType::Val(wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                nullable,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index(layout))),
            )
            .unwrap(),
        ))
    };
    let shape = fields(GcLayout::CompactDataShape);
    assert_eq!(shape.len(), 2);
    assert!(shape.iter().all(|field| !field.mutable));
    assert_eq!(
        shape[0].element_type,
        reference(GcLayout::CompactShapeKeys, false)
    );
    assert_eq!(
        shape[1].element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::I32)
    );
    let CompositeInnerType::Array(keys) = &types[index(GcLayout::CompactShapeKeys) as usize]
        .composite_type
        .inner
    else {
        panic!("shape keys are a primitive string array");
    };
    assert!(!keys.0.mutable);
    assert_eq!(keys.0.element_type, reference(GcLayout::StringValue, false));
    let cache = fields(GcLayout::OwnDataReadCache);
    assert_eq!(cache.len(), 3);
    assert_eq!(
        cache[0].element_type,
        reference(GcLayout::StringValue, false)
    );
    assert!(!cache[0].mutable);
    assert_eq!(
        cache[1].element_type,
        reference(GcLayout::CompactDataShape, true)
    );
    for (table, element) in [
        (GcLayout::CompactShapeTable, GcLayout::CompactDataShape),
        (GcLayout::OwnDataReadCacheTable, GcLayout::OwnDataReadCache),
        (GcLayout::CompactShapePageTable, GcLayout::CompactShapeTable),
        (
            GcLayout::OwnDataReadCachePageTable,
            GcLayout::OwnDataReadCacheTable,
        ),
    ] {
        let CompositeInnerType::Array(table) = &types[index(table) as usize].composite_type.inner
        else {
            panic!("program cache sites require direct indexed GC tables");
        };
        assert!(table.0.mutable);
        assert_eq!(table.0.element_type, reference(element, true));
    }
    // The encoded metadata has no eqref/StoredValue/receiver edge. Only the
    // actual per-object reference SoA can retain arbitrary JavaScript values.
    for layout in [GcLayout::CompactDataShape, GcLayout::OwnDataReadCache] {
        for field in fields(layout) {
            if let wasmparser::StorageType::Val(wasmparser::ValType::Ref(reference)) =
                field.element_type
            {
                assert!(matches!(
                    reference.heap_type(),
                    wasmparser::HeapType::Concrete(_)
                ));
            }
        }
    }
}

#[test]
fn packed_array_storage_uses_distinct_mutable_raw_bits_and_presence_arrays() {
    let (runtime, bytes) = encoded(ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ));
    let (types, _, globals) = declarations(&bytes);
    let schema = runtime.globals().schema();
    let layouts = schema.layouts();
    let index = |layout| layouts.layout_index(layout);
    let CompositeInnerType::Struct(storage) = &types[index(GcLayout::ArrayIndexStorage) as usize]
        .composite_type
        .inner
    else {
        panic!("indexed storage is a real GC struct");
    };
    assert_eq!(ArrayIndexStorageSchema::BUCKETS.ordinal().raw(), 0);
    assert_eq!(ArrayIndexStorageSchema::COUNT.ordinal().raw(), 1);
    assert_eq!(ArrayIndexStorageSchema::DENSE.ordinal().raw(), 2);
    assert_eq!(ArrayIndexStorageSchema::MODE.ordinal().raw(), 3);
    assert_eq!(ArrayIndexStorageSchema::NUMBERS.ordinal().raw(), 4);
    assert_eq!(ArrayIndexStorageSchema::NUMBER_PRESENT.ordinal().raw(), 5);
    assert_eq!(storage.fields.len(), 7);
    assert_eq!(ArrayIndexStorageSchema::VALUES.ordinal().raw(), 6);
    let values = &storage.fields[6];
    assert!(values.mutable);
    assert_eq!(
        values.element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                true,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index(
                    GcLayout::PackedValueTable
                )))
            )
            .unwrap()
        ))
    );
    let CompositeInnerType::Array(value_slots) = &types[index(GcLayout::PackedValueTable) as usize]
        .composite_type
        .inner
    else {
        panic!("general packed slots are a real GC array")
    };
    assert!(value_slots.0.mutable);
    assert_eq!(
        value_slots.0.element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                true,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index(
                    GcLayout::StoredValue
                )))
            )
            .unwrap()
        ))
    );
    let mode = ArrayIndexStorageSchema::MODE.ordinal().raw() as usize;
    assert!(storage.fields[mode].mutable);
    assert_eq!(
        storage.fields[mode].element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::I32)
    );
    assert_ne!(
        index(GcLayout::NumberBitsI64Array),
        index(GcLayout::NumberPresenceU64Array)
    );
    for (field, layout) in [
        (
            ArrayIndexStorageSchema::NUMBERS.ordinal().raw(),
            GcLayout::NumberBitsI64Array,
        ),
        (
            ArrayIndexStorageSchema::NUMBER_PRESENT.ordinal().raw(),
            GcLayout::NumberPresenceU64Array,
        ),
    ] {
        assert!(storage.fields[field as usize].mutable);
        assert_eq!(
            storage.fields[field as usize].element_type,
            wasmparser::StorageType::Val(wasmparser::ValType::Ref(
                wasmparser::RefType::new(
                    false,
                    wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index(
                        layout
                    )))
                )
                .unwrap()
            ))
        );
        let CompositeInnerType::Array(array) = &types[index(layout) as usize].composite_type.inner
        else {
            panic!("packed backing is a GC array");
        };
        assert!(array.0.mutable);
        assert_eq!(
            array.0.element_type,
            wasmparser::StorageType::Val(wasmparser::ValType::I64)
        );
    }
    let mut constructor = Function::new(crate::code_sink::LocalDeclarations::from_types([]));
    let _ = schema
        .struct_type::<ArrayIndexStorage>()
        .empty(schema, &mut constructor);
    constructor.instruction(&Instruction::Drop);
    constructor.instruction(&Instruction::End);
    let constructor = constructor.into_body();
    // Validate the actual constructor with its real local declarations and
    // globals. In particular, a nullable physical local must be refined before
    // it is consumed by any non-null backing field.
    let mut module = wasm_encoder::Module::new();
    module.section(runtime.types());
    let mut functions = wasm_encoder::FunctionSection::new();
    functions.function(StaticSignature::HostCollectGc.type_index()); // () -> ()
    module.section(&functions);
    module.section(&runtime.globals().defined_section(0));
    let mut code = wasm_encoder::CodeSection::new();
    code.function(&constructor);
    module.section(&code);
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(&module.finish())
        .expect("the complete seven-field constructor has valid Wasm stack types");
    let mut body = Vec::new();
    wasm_encoder::Encode::encode(&constructor, &mut body);
    let mut reader = wasmparser::BinaryReader::new(&body, 0);
    let length = reader.read_var_u32().unwrap() as usize;
    let body = reader.read_bytes(length).unwrap();
    let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(body, 0));
    let operators = body
        .get_operators_reader()
        .unwrap()
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let allocation = operators.iter().position(|op| matches!(op, wasmparser::Operator::StructNew { struct_type_index } if *struct_type_index == index(GcLayout::ArrayIndexStorage))).unwrap();
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Operand {
        I32(i32),
        I64(i64),
        Reference { global: Option<u32>, non_null: bool },
    }
    let local_types = body
        .get_locals_reader()
        .unwrap()
        .into_iter()
        .flat_map(|local| {
            let (count, ty) = local.unwrap();
            std::iter::repeat(ty).take(count as usize)
        })
        .collect::<Vec<_>>();
    let mut locals = std::collections::BTreeMap::<u32, Operand>::new();
    let mut operands = Vec::new();
    // Follow stack values, not instruction offsets: a LocalGet followed by
    // RefAsNonNull is one constructor operand, and neither local numbers nor
    // the order used to root the four globals is part of the contract.
    for operator in &operators[..allocation] {
        match operator {
            wasmparser::Operator::GlobalGet { global_index } => {
                let wasmparser::ValType::Ref(ty) = globals[*global_index as usize].content_type
                else {
                    panic!("canonical empty backing is a reference global");
                };
                operands.push(Operand::Reference {
                    global: Some(*global_index),
                    non_null: !ty.is_nullable(),
                });
            }
            wasmparser::Operator::I32Const { value } => operands.push(Operand::I32(*value)),
            wasmparser::Operator::I64Const { value } => operands.push(Operand::I64(*value)),
            wasmparser::Operator::RefNull { .. } => operands.push(Operand::Reference {
                global: None,
                non_null: false,
            }),
            wasmparser::Operator::LocalSet { local_index } => {
                locals.insert(
                    *local_index,
                    operands
                        .pop()
                        .expect("local initialization consumes a value"),
                );
            }
            wasmparser::Operator::LocalGet { local_index } => {
                let mut value = locals[local_index].clone();
                if let Operand::Reference { non_null, .. } = &mut value {
                    let wasmparser::ValType::Ref(ty) = local_types[*local_index as usize] else {
                        panic!("reference provenance must have a reference local declaration");
                    };
                    *non_null = !ty.is_nullable();
                }
                operands.push(value);
            }
            wasmparser::Operator::RefAsNonNull => {
                let Some(Operand::Reference { non_null, .. }) = operands.last_mut() else {
                    panic!("reference refinement consumes exactly one reference value");
                };
                *non_null = true;
            }
            unexpected => panic!("unexpected constructor operand producer: {unexpected:?}"),
        }
    }
    let empty = |global| Operand::Reference {
        global: Some(global),
        non_null: true,
    };
    assert_eq!(
        operands,
        vec![
            empty(schema.empty_arrays.array_buckets.index()),
            Operand::I64(0),
            empty(schema.empty_arrays.array_elements.index()),
            Operand::I32(ArrayIndexedStorageMode::PackedNumbers.encode()),
            empty(schema.empty_arrays.number_bits.index()),
            empty(schema.empty_arrays.number_presence.index()),
            Operand::Reference {global:None,non_null:false},
        ],
        "StructNew consumes exactly the declared BUCKETS/COUNT/DENSE/MODE/NUMBERS/NUMBER_PRESENT/VALUES values"
    );
}

#[test]
fn dictionary_publication_refines_the_nullable_local_before_nonnull_base_storage() {
    fn module(unrefined_control: bool) -> Vec<u8> {
        let runtime = RuntimeModuleTypes::register(ModuleKind::Runtime(
            crate::RuntimeSourceCompilationProfile::Portable,
        ))
        .finalize_globals(GlobalLedger::new(), false, 0);
        let schema = runtime.globals().schema();
        let signature = RuntimeHelperId::OrdinaryPropertyStorageEnsure.definition(schema.layouts());
        let mut body = Function::with_parameters(
            crate::code_sink::LocalDeclarations::from_types([]),
            signature.parameters().to_vec(),
        );
        let object = schema.parameter_gc::<OrdinaryObject, NonNullable>(0, &mut body);
        let entries = schema
            .reserve_gc_local(&mut body)
            .initialize(schema.empty_property_table(&mut body), &mut body);
        let buckets = schema
            .reserve_gc_local(&mut body)
            .initialize(schema.empty_property_index(&mut body), &mut body);
        let dictionary = schema.reserve_gc_local(&mut body).initialize(
            schema.struct_type::<OrdinaryPropertyStorage>().construct(
                (
                    GcOperand::reference(&entries, schema),
                    GcOperand::i32(0),
                    GcOperand::reference(&buckets, schema),
                ),
                &mut body,
            ),
            &mut body,
        );
        if unrefined_control {
            // Deliberately emit the old invalid wire operand. The typed field
            // writer cannot accept a nullable-reference operand for this field.
            let nullable = schema
                .reserve_gc_local::<OrdinaryPropertyStorage, Nullable>(&mut body)
                .initialize(dictionary.load(schema, &mut body).nullable(), &mut body);
            object.load(schema, &mut body);
            nullable.load(schema, &mut body);
            body.instruction(&Instruction::StructSet {
                struct_type_index: schema.layouts().layout_index(GcLayout::OrdinaryObject),
                field_index: OrdinaryObjectSchema::PROPERTIES.ordinal().raw(),
            });
            nullable.clear(&mut body);
        } else {
            schema.field(OrdinaryObjectSchema::PROPERTIES).write(
                &object,
                GcOperand::subtype_reference(&dictionary, schema),
                schema,
                &mut body,
            );
        }
        dictionary.load(schema, &mut body);
        dictionary.clear(&mut body);
        buckets.clear(&mut body);
        entries.clear(&mut body);
        object.clear(&mut body);
        body.instruction(&Instruction::End);
        let mut module = wasm_encoder::Module::new();
        module.section(runtime.types());
        let mut functions = wasm_encoder::FunctionSection::new();
        functions.function(RuntimeHelperId::OrdinaryPropertyStorageEnsure.type_index());
        module.section(&functions);
        module.section(&runtime.globals().defined_section(0));
        let mut code = wasm_encoder::CodeSection::new();
        code.function(&body.into_body());
        module.section(&code);
        module.finish()
    }
    Validator::new_with_features(WasmFeatures::all()).validate_all(&module(false))
        .expect("the actual subtype operand publishes a non-null dictionary into its non-null base field");
    assert!(
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(&module(true))
            .is_err(),
        "the unrefined nullable-local control must fail the real Wasm validator"
    );
}
