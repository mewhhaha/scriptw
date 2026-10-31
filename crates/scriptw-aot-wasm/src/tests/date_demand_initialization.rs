//! Inspect the physical Date demand owner, its publications and canonical reads.
use super::error_demand_initialization::{
    body, encode_realm_probe, inventory, named, reference, type_index,
};
use super::*;
use crate::functions::NonArrayRealmIntrinsicSlot as Slot;
use crate::gc_types::{
    DeferredDateConstructor, DeferredIntrinsicValue, FinalizedRuntimeModule, GcNullability,
    GcSnapshotField, GlobalLedger, IntrinsicTable, NonNullable, RealmRecord, RealmRecordSchema,
    RuntimeModuleTypes, StoredValue,
};
use crate::heap::DateInitializationState;

const DATE_SLOTS: [Slot; 2] = [Slot::DatePrototype, Slot::DateConstructor];

fn calls(ops: &[Operator<'_>], target: u32) -> Vec<usize> {
    ops.iter()
        .enumerate()
        .filter_map(|(index, op)| {
            matches!(op, Operator::Call { function_index } if *function_index == target)
                .then_some(index)
        })
        .collect()
}

fn date_state_field(runtime: &FinalizedRuntimeModule) -> u32 {
    let schema = runtime.globals().schema();
    let mut function = crate::code_sink::Function::with_parameters(
        crate::code_sink::LocalDeclarations::from_types([]),
        vec![wasm_encoder::ValType::Ref(
            schema.reference_type::<RealmRecord>(GcNullability::NonNullable),
        )],
    );
    let realm = schema.parameter_gc::<RealmRecord, NonNullable>(0, &mut function);
    let state = schema.reserve_i32_local(&mut function);
    schema
        .field(RealmRecordSchema::DATE_STATE)
        .read(&realm, schema, &mut function)
        .store(state, &mut function);
    realm.clear(&mut function);
    schema.release_i32_local(state, &mut function);
    function.instruction(&wasm_encoder::Instruction::End);
    let bytes = encode_realm_probe(runtime, function);
    let reads: Vec<_> = body(&bytes, 0)
        .into_iter()
        .filter_map(|op| match op {
            Operator::StructGet {
                struct_type_index,
                field_index,
            } if struct_type_index == type_index::<RealmRecord>(schema) => Some(field_index),
            _ => None,
        })
        .collect();
    assert_eq!(
        reads.len(),
        1,
        "the typed DATE_STATE declaration owns its ordinal"
    );
    reads[0]
}

// The helper ABI emits its already rooted operands directly, with only nullable
// refinements between them. Inspect those operands without evaluating the body.
fn arguments<'a, 'b>(
    ops: &'a [Operator<'b>],
    position: usize,
    count: usize,
) -> Vec<&'a Operator<'b>> {
    let mut args: Vec<_> = ops[..position]
        .iter()
        .rev()
        .filter(|op| !matches!(op, Operator::RefAsNonNull))
        .take(count)
        .collect();
    assert!(
        args.iter().all(|op| matches!(
            op,
            Operator::LocalGet { .. }
                | Operator::I32Const { .. }
                | Operator::I64Const { .. }
                | Operator::RefFunc { .. }
                | Operator::GlobalGet { .. }
        )),
        "actual helper operands are rooted leaves"
    );
    args.reverse();
    args
}

fn local(op: &Operator<'_>) -> u32 {
    let Operator::LocalGet { local_index } = op else {
        panic!("reference operand is a typed GC root")
    };
    *local_index
}

fn constant(ops: &[Operator<'_>], before: usize, operand: &Operator<'_>) -> i32 {
    match operand {
        Operator::I32Const { value } => *value,
        Operator::LocalGet { local_index } => {
            let position = ops[..before].iter().rposition(|op| matches!(op,
                Operator::LocalSet { local_index: target } | Operator::LocalTee { local_index: target }
                    if target == local_index)).expect("constant local is initialized");
            constant(ops, position, &ops[position - 1])
        }
        _ => panic!("expected a physical constant operand"),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Origin {
    Parameter(u32),
    Operation(usize),
}

fn origin(ops: &[Operator<'_>], before: usize, root: u32) -> Origin {
    let Some(position) = ops[..before].iter().rposition(|op| matches!(op,
        Operator::LocalSet { local_index } | Operator::LocalTee { local_index } if *local_index == root)) else {
        return Origin::Parameter(root);
    };
    let source = (0..position)
        .rev()
        .find(|&index| !matches!(ops[index], Operator::RefAsNonNull))
        .unwrap();
    match ops[source] {
        Operator::LocalGet { local_index } => origin(ops, source, local_index),
        _ => Origin::Operation(source),
    }
}

fn installation_rows(
    bytes: &[u8],
    ops: &[Operator<'_>],
    install_run: u32,
) -> Vec<(usize, Vec<[u32; 4]>)> {
    let positions = calls(ops, install_run);
    if positions.is_empty() {
        return Vec::new();
    }
    let mut segments = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::DataSection(entries) = payload.unwrap() {
            for entry in entries {
                let entry = entry.unwrap();
                if let wasmparser::DataKind::Active {
                    memory_index: 0,
                    offset_expr,
                } = entry.kind
                {
                    let Operator::I32Const { value } =
                        offset_expr.get_operators_reader().read().unwrap()
                    else {
                        panic!("runtime data has a constant base");
                    };
                    segments.push((value as u32, entry.data));
                }
            }
        }
    }
    positions
        .into_iter()
        .map(|position| {
            let [Operator::I32Const { value: start }, Operator::I32Const { value: count }] =
                ops[position - 2..position]
            else {
                panic!("actual run operands name the sealed rows");
            };
            assert!(count > 0);
            let (start, length) = (start as u32, count as u32 * 16);
            let (base, data) = segments
                .iter()
                .find(|(base, data)| {
                    start >= *base
                        && start as u64 + length as u64 <= *base as u64 + data.len() as u64
                })
                .expect("the complete run belongs to active runtime data");
            let offset = (start - base) as usize;
            (
                position,
                data[offset..offset + length as usize]
                    .chunks_exact(16)
                    .map(|row| {
                        std::array::from_fn(|i| {
                            u32::from_le_bytes(row[i * 4..i * 4 + 4].try_into().unwrap())
                        })
                    })
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn canonical_runtime_moves_all_date_rows_and_roots_inside_the_void_demand_owner() {
    let artifact = emit_script("this;").unwrap();
    expect_valid_module(&artifact, 0);
    let bytes = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(bytes);
    let ensure_index = named(&names, "helper::realm_ensure_date");
    let initial = body(bytes, named(&names, "helper::realm_initialize_intrinsics"));
    let ensure = body(bytes, ensure_index);
    assert!(
        calls(&initial, ensure_index).is_empty(),
        "core bootstrap leaves Date dormant"
    );
    assert!(
        calls(&ensure, ensure_index).is_empty(),
        "only the private build capability bypasses demand"
    );
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let realm = type_index::<RealmRecord>(schema);
    let table = type_index::<IntrinsicTable>(schema);
    let state = date_state_field(&registered);
    let (types, declarations, exports) = inventory(bytes);
    assert!(exports.contains(&ensure_index));
    let wasmparser::CompositeInnerType::Func(signature) = &types
        [declarations[ensure_index as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("ensure has a registered physical signature");
    };
    assert_eq!(signature.params(), [reference(realm, false)]);
    assert!(
        signature.results().is_empty(),
        "the canonical table owns both completed roots"
    );
    for module in [bytes, artifact.bytes.as_slice()] {
        let (types, _, _) = inventory(module);
        let wasmparser::CompositeInnerType::Array(elements) =
            &types[table as usize].composite_type.inner
        else {
            panic!("canonical intrinsics remain an array");
        };
        assert_eq!(
            elements.0.element_type,
            wasmparser::StorageType::Val(reference(type_index::<StoredValue>(schema), false))
        );
    }
    assert!(
        !initial.iter().any(
            |op| matches!(op, Operator::StructGet { struct_type_index, field_index }
        if *struct_type_index == realm && *field_index == state)
        ),
        "startup never probes Date state"
    );
    let writes: Vec<_> = ensure
        .iter()
        .enumerate()
        .filter_map(|(position, op)| {
            if matches!(op, Operator::StructSet { struct_type_index, field_index }
            if *struct_type_index == realm && *field_index == state)
            {
                let args = arguments(&ensure, position, 2);
                assert_eq!(
                    origin(&ensure, position, local(args[0])),
                    Origin::Parameter(0)
                );
                Some((position, constant(&ensure, position, args[1])))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        writes.iter().map(|(_, value)| *value).collect::<Vec<_>>(),
        [
            DateInitializationState::Building.word() as i32,
            DateInitializationState::Ready.word() as i32,
        ]
    );
    let (building, ready) = (writes[0].0, writes[1].0);
    let guards: Vec<_> = ensure[..building].windows(6).enumerate().filter_map(|(position, ops)| match ops {
        [Operator::LocalGet { local_index }, Operator::I32Const { value }, Operator::I32Eq,
            Operator::If { .. }, Operator::Return, Operator::End] => Some((position, *local_index, *value, true)),
        [Operator::LocalGet { local_index }, Operator::I32Const { value }, Operator::I32Ne,
            Operator::If { .. }, Operator::Unreachable, Operator::End] => Some((position, *local_index, *value, false)),
        _ => None,
    }).collect();
    assert_eq!(
        guards.len(),
        2,
        "Ready returns and non-Dormant traps in complete control frames"
    );
    assert_eq!(
        (guards[0].2, guards[0].3),
        (DateInitializationState::Ready.word() as i32, true)
    );
    assert_eq!(
        (guards[1].2, guards[1].3),
        (DateInitializationState::Dormant.word() as i32, false)
    );
    assert_eq!(
        guards[0].1, guards[1].1,
        "both branches inspect the same state read"
    );
    let state_read = ensure[..guards[0].0].iter().position(|op| matches!(op,
        Operator::StructGet { struct_type_index, field_index } if *struct_type_index == realm && *field_index == state)).unwrap();
    assert_eq!(
        origin(
            &ensure,
            state_read,
            local(arguments(&ensure, state_read, 1)[0])
        ),
        Origin::Parameter(0)
    );
    assert!(
        matches!(ensure[state_read + 1], Operator::LocalSet { local_index } if local_index == guards[0].1)
    );
    let mut depth = 0;
    let mut slot_writes = BTreeMap::new();
    for (position, op) in ensure.iter().enumerate() {
        match op {
            Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::If { .. }
            | Operator::TryTable { .. } => depth += 1,
            Operator::End => {
                if position + 1 < ensure.len() {
                    depth -= 1;
                }
            }
            _ => {}
        }
        if position == building || position == ready {
            assert_eq!(depth, 0, "state commits are unconditional");
        }
        if matches!(
            op,
            Operator::StructNew { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewFixed { .. }
                | Operator::ArrayNewDefault { .. }
                | Operator::Call { .. }
        ) {
            assert!(
                building < position && position < ready,
                "all graph work is inside Building"
            );
        }
        assert!(
            !matches!(op, Operator::CallRef { .. } | Operator::CallIndirect { .. }),
            "initialization executes no JavaScript callable"
        );
        if matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == table) {
            let args = arguments(&ensure, position, 3);
            let slot = constant(&ensure, position, args[1]) as u32;
            assert!(
                slot_writes.insert(slot, position).is_none(),
                "each canonical root is published once"
            );
            let Origin::Operation(projection) = origin(&ensure, position, local(args[0])) else {
                panic!("table is projected from the supplied Realm")
            };
            assert!(
                matches!(ensure[projection], Operator::StructGet { struct_type_index, field_index }
                if struct_type_index == realm && field_index == GcSnapshotField::RealmIntrinsics.index())
            );
            assert_eq!(
                origin(
                    &ensure,
                    projection,
                    local(arguments(&ensure, projection, 1)[0])
                ),
                Origin::Parameter(0)
            );
            assert!(building < position && position < ready);
        }
    }
    assert_eq!(
        slot_writes.keys().copied().collect::<Vec<_>>(),
        DATE_SLOTS.map(Slot::gc_index)
    );
    for (position, op) in initial.iter().enumerate() {
        if matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == table) {
            let args = arguments(&initial, position, 3);
            assert!(
                !DATE_SLOTS
                    .map(Slot::gc_index)
                    .contains(&(constant(&initial, position, args[1]) as u32)),
                "startup preserves both undefined placeholders"
            );
        }
    }
    let constructor = named(&names, "builtin::Date");
    assert!(!initial.iter().any(
        |op| matches!(op, Operator::RefFunc { function_index } if *function_index == constructor)
    ));
    assert_eq!(ensure.iter().filter(|op| matches!(op, Operator::RefFunc { function_index } if *function_index == constructor)).count(), 1);
    let constructors = calls(
        &ensure,
        named(&names, "helper::intrinsic_constructor_allocate"),
    );
    assert_eq!(constructors.len(), 1);
    assert!(slot_writes[&Slot::DatePrototype.gc_index()] < constructors[0]);
    let run = named(&names, "helper::deferred_intrinsic_install_run");
    let rows = installation_rows(bytes, &ensure, run);
    assert_eq!(
        rows.iter().map(|(_, rows)| rows.len()).collect::<Vec<_>>(),
        [3, 46]
    );
    assert!(
        constructors[0] < slot_writes[&Slot::DateConstructor.gc_index()]
            && slot_writes[&Slot::DateConstructor.gc_index()] < rows[0].0,
        "root the complete constructor cycle before method publications"
    );
    let expected: Vec<_> = [
        crate::intrinsics::IntrinsicMethodPlan::DateConstructor,
        crate::intrinsics::IntrinsicMethodPlan::DatePrototype,
    ]
    .into_iter()
    .flat_map(|plan| {
        plan.methods()
            .unwrap()
            .iter()
            .map(|method| method.builtin as u32)
            .collect::<Vec<_>>()
    })
    .collect();
    assert_eq!(
        rows.iter()
            .flat_map(|(_, rows)| rows.iter().map(|row| row[0]))
            .collect::<Vec<_>>(),
        expected
    );
    for (_, rows) in &rows {
        for row in rows {
            assert!(row[1] <= 1 && row[3] & !3 == 0);
        }
    }
    let mut owner = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(encoded) = payload.unwrap() {
            let ops: Vec<_> = encoded
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .map(Result::unwrap)
                .collect();
            for (_, rows) in installation_rows(bytes, &ops, run) {
                if rows.iter().any(|row| expected.contains(&row[0])) {
                    assert_eq!(
                        owner, ensure_index,
                        "every physical Date row span belongs exclusively to ensure"
                    );
                }
            }
            owner += 1;
        }
    }
    let date_builtins: Vec<_> = StandardBuiltinId::all_functions()
        .iter()
        .copied()
        .filter(|builtin| {
            builtin.debug_name() == "Date" || builtin.debug_name().starts_with("Date.")
        })
        .collect();
    assert_eq!(
        date_builtins.len(),
        51,
        "Date demand preserves the complete native root inventory"
    );
    let mut declared = BTreeSet::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ElementSection(entries) = payload.unwrap() {
            for entry in entries {
                if let wasmparser::ElementItems::Functions(functions) = entry.unwrap().items {
                    declared.extend(functions.into_iter().map(Result::unwrap));
                }
            }
        }
    }
    for builtin in date_builtins {
        let target = named(&names, &format!("builtin::{}", builtin.debug_name()));
        assert!(
            declared.contains(&target),
            "non-Date programs retain {} as a reflective native root",
            builtin.debug_name()
        );
        assert!(!body(bytes, target).is_empty());
    }
}

#[test]
fn date_global_and_resolver_use_the_defining_realm_recipe_and_utc_alias_keeps_one_cache() {
    let artifact = emit_script("this;").unwrap();
    let bytes = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(bytes);
    let ensure_index = named(&names, "helper::realm_ensure_date");
    let ensure = body(bytes, ensure_index);
    let resolve = body(bytes, named(&names, "helper::property_value_resolve"));
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let recipe_type = type_index::<DeferredDateConstructor>(schema);
    let publish = named(&names, "helper::intrinsic_data_publish");
    // Intrinsic initialization and global publication have separate physical
    // owners. Both runtime Realm producers publish their own unread Date recipe.
    for publisher in [
        format!("host::{}", scriptw_ir::HostBuiltinId::CreateRealm.as_str()),
        format!(
            "builtin::{}",
            StandardBuiltinId::ShadowRealmConstructor.debug_name()
        ),
    ] {
        let initial = body(bytes, named(&names, &publisher));
        let markers: Vec<_> = initial.iter().enumerate().filter_map(|(position, op)|
            matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == recipe_type).then_some(position)).collect();
        assert_eq!(
            markers.len(),
            1,
            "{publisher} publishes one ordinary Date data recipe"
        );
        let marker_publications: Vec<_> = calls(&initial, publish)
            .into_iter()
            .filter(|&position| {
                let args = arguments(&initial, position, 8);
                origin(&initial, position, local(args[4])) == Origin::Operation(markers[0])
            })
            .collect();
        assert_eq!(marker_publications.len(), 1);
        let publication = marker_publications[0];
        let args = arguments(&initial, publication, 8);
        assert_eq!(
            args[5..]
                .iter()
                .map(|arg| constant(&initial, publication, arg))
                .collect::<Vec<_>>(),
            [1, 0, 1]
        );
        assert!(
            calls(&initial, ensure_index).is_empty(),
            "{publisher} publishes the marker without observing it"
        );
    }
    let heap = wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(recipe_type));
    let branch = resolve
        .windows(2)
        .position(|ops| {
            matches!(ops,
        [Operator::RefTestNonNull { hty }, Operator::If { .. }] if *hty == heap)
        })
        .expect("the real generic resolver tests the Date recipe");
    let fallback_heap =
        wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(type_index::<
            DeferredIntrinsicValue,
        >(schema)));
    let fallback = resolve
        .iter()
        .position(|op| matches!(op, Operator::RefCastNonNull { hty } if *hty == fallback_heap))
        .unwrap();
    let ensure_calls = calls(&resolve, ensure_index);
    assert_eq!(ensure_calls.len(), 1);
    let demand = ensure_calls[0];
    assert!(
        branch < demand && demand < fallback,
        "Date is resolved before the generic method cast"
    );
    let mut depth = 1;
    let end = (branch + 2..resolve.len())
        .find(|&position| {
            match resolve[position] {
                Operator::If { .. } | Operator::Block { .. } | Operator::Loop { .. } => depth += 1,
                Operator::End => depth -= 1,
                _ => {}
            }
            depth == 0
        })
        .unwrap();
    assert!(demand < end && end < fallback);
    let Origin::Operation(realm_read) =
        origin(&resolve, demand, local(arguments(&resolve, demand, 1)[0]))
    else {
        panic!("recipe provides its Realm")
    };
    assert!(
        matches!(resolve[realm_read], Operator::StructGet { struct_type_index, field_index: 0 } if struct_type_index == recipe_type)
    );
    let table_read = (demand..end)
        .find(|&position| {
            matches!(resolve[position], Operator::ArrayGet { array_type_index }
        if array_type_index == type_index::<IntrinsicTable>(schema))
        })
        .expect("Date resolution reads the canonical StoredValue");
    assert_eq!(
        constant(&resolve, table_read, &resolve[table_read - 1]) as u32,
        Slot::DateConstructor.gc_index()
    );
    let Operator::LocalSet {
        local_index: canonical,
    } = resolve[table_read + 1]
    else {
        panic!("canonical StoredValue remains rooted")
    };
    let returned = (table_read + 2..end)
        .find(|&position| matches!(resolve[position], Operator::Return))
        .unwrap();
    let mut operand = returned;
    // Root retirement writes null to the local while retaining its loaded
    // nonnull reference on the operand stack for Return.
    while matches!(
        &resolve[operand - 2..operand],
        [Operator::RefNull { .. }, Operator::LocalSet { .. }]
    ) {
        operand -= 2;
    }
    if matches!(resolve[operand - 1], Operator::RefAsNonNull) {
        operand -= 1;
    }
    assert!(
        matches!(resolve[operand - 1], Operator::LocalGet { local_index } if local_index == canonical),
        "return the exact canonical record without another wrapper or descriptor lookup"
    );
    assert!(
        !resolve[branch..end]
            .iter()
            .any(|op| matches!(op, Operator::StructNew { .. } | Operator::StructSet { .. })),
        "resolution neither rewrites the descriptor nor allocates another identity record"
    );
    let install = named(&names, "helper::deferred_intrinsic_install");
    let alias_calls = calls(&ensure, install);
    assert_eq!(
        alias_calls.len(),
        1,
        "the only direct Date recipe is the shared UTC/GMT alias"
    );
    let alias = alias_calls[0];
    let args = arguments(&ensure, alias, 9);
    assert_eq!(
        constant(&ensure, alias, args[4]) as u32,
        StandardBuiltinId::DatePrototypeToUtcString as u32
    );
    let alias_publications: Vec<_> = calls(&ensure, publish)
        .into_iter()
        .filter(|&position| {
            origin(&ensure, position, local(arguments(&ensure, position, 8)[4]))
                == Origin::Operation(alias)
        })
        .collect();
    assert_eq!(
        alias_publications.len(),
        1,
        "the second descriptor receives the first recipe's exact GC root"
    );
    assert_eq!(
        calls(&body(bytes, install), publish).len(),
        1,
        "the first alias publication belongs to recipe creation"
    );
    let install_body = body(bytes, install);
    assert_eq!(
        install_body
            .iter()
            .filter(|op| matches!(op, Operator::StructNew { struct_type_index }
        if *struct_type_index == type_index::<DeferredIntrinsicValue>(schema)))
            .count(),
        1,
        "both descriptors share one nullable function cache"
    );
}

#[test]
fn both_physical_date_slot_readers_ensure_the_same_realm_before_table_access() {
    let artifact =
        emit_script("Date; new Date(0); Reflect.construct(Date, [], function Target() {});")
            .unwrap();
    expect_valid_module(&artifact, 1);
    let runtime = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(runtime);
    let ensure = named(&names, "helper::realm_ensure_date");
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let table = type_index::<IntrinsicTable>(schema);
    let realm = type_index::<RealmRecord>(schema);
    let export = Parser::new(0)
        .parse_all(runtime)
        .find_map(|payload| {
            if let Payload::ExportSection(entries) = payload.unwrap() {
                entries
                    .into_iter()
                    .map(Result::unwrap)
                    .find(|entry| {
                        entry.kind == wasmparser::ExternalKind::Func && entry.index == ensure
                    })
                    .map(|entry| entry.name)
            } else {
                None
            }
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
    let mut checked = BTreeSet::new();
    for (bytes, guard, builder) in [
        (runtime, Some(ensure), Some(ensure)),
        (artifact.bytes.as_slice(), program_ensure, None),
    ] {
        let mut function = imported_function_count_in_bytes(bytes);
        for payload in Parser::new(0).parse_all(bytes) {
            let Payload::CodeSectionEntry(encoded) = payload.unwrap() else {
                continue;
            };
            let index = function;
            function += 1;
            if Some(index) == builder {
                continue;
            }
            let ops: Vec<_> = encoded
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
                let slot = constants.get(&local(&ops[position - 1]));
                let Some(&slot) = slot.filter(|slot| DATE_SLOTS.map(Slot::gc_index).contains(slot))
                else {
                    continue;
                };
                let guard = guard.expect("every Date read has its ensure import");
                let call = calls(&ops[..position], guard)
                    .last()
                    .copied()
                    .expect("Date reads cross demand");
                assert!(
                    position - call <= 20,
                    "guard belongs to this exact canonical read"
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
                    "ensure dominates the read on one straight-line path"
                );
                let projection = (call + 1..position).find(|&i| matches!(ops[i], Operator::StructGet { struct_type_index, field_index }
                    if struct_type_index == realm && field_index == GcSnapshotField::RealmIntrinsics.index())).unwrap();
                assert_eq!(
                    origin(&ops, call, local(arguments(&ops, call, 1)[0])),
                    origin(&ops, projection, local(arguments(&ops, projection, 1)[0])),
                    "ensure and table projection use the same defining Realm root"
                );
                checked.insert(slot);
            }
        }
    }
    assert_eq!(
        checked,
        DATE_SLOTS.into_iter().map(Slot::gc_index).collect(),
        "both constructor StoredValue and prototype value reads are covered"
    );
}

#[test]
fn date_intrinsic_callable_returns_the_ensured_constructor_without_an_allocation_fallback() {
    use crate::planning::{
        FunctionMetaRegistry, HostImportFunctionIndices, RejectRuntimeSemanticsImportFunctionIndex,
        RuntimeBootstrapPlan, WasmFunctionMeta,
    };
    use crate::runtime_helpers::{HelperOwner, RuntimeHelperFunctionBase, RuntimeHelperId};

    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let builtin = StandardBuiltinId::DateConstructor;
    // This route needs the planned identity, but must never consume its code
    // entry or native metadata to build a second constructor.
    let meta = WasmFunctionMeta {
        constant_leaf: None,
        numeric_leaf: None,
        async_generator_resume_environment_plan: None,
        template_source: None,
        name: "Date".into(),
        to_string_value: "function Date() { [native code] }".into(),
        standard_builtin: Some(builtin),
        host_builtin: None,
        length: 7,
        length_name_configurable: true,
        entry: crate::function_entry::PlannedFunctionEntry::standard(builtin, 0),
        strict: true,
        is_named_expression: false,
        class_element_execution_kind: scriptw_ir::ClassElementExecutionKind::None,
        class_heritage_kind: scriptw_ir::ClassHeritageKind::None,
        is_static_class_member: false,
        is_derived_constructor: false,
        is_synthetic_default_derived_constructor: false,
        class_instance_element_plan: None,
        uses_super: false,
        this_before_super: false,
        captures_private_environment: false,
        needs_active_function_identity: false,
    };
    let imports = HostImportFunctionIndices::new(
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        RejectRuntimeSemanticsImportFunctionIndex::new(0),
    );
    let functions = FunctionMetaRegistry::new(
        BTreeMap::from([(builtin.function_id(), meta)]),
        BTreeSet::new(),
        imports,
        Vec::new(),
        Vec::new(),
        None,
    );
    let strings = crate::data::StringPool::default();
    let helper_base = RuntimeHelperFunctionBase::at(0, 0);
    let mut builder = crate::FunctionBuilder::new_program_initialize(
        schema,
        &strings,
        &functions,
        RuntimeBootstrapPlan::default(),
        Some(helper_base),
    );
    let mut function = crate::code_sink::Function::with_parameters(
        crate::code_sink::LocalDeclarations::from_types([]),
        vec![wasm_encoder::ValType::Ref(
            schema.reference_type::<RealmRecord>(GcNullability::NonNullable),
        )],
    );
    let realm = schema.parameter_gc::<RealmRecord, NonNullable>(0, &mut function);
    let context =
        builder.emit_realm_function_materialization_context_from_realm(&realm, &mut function);
    let callable = builder
        .emit_intrinsic_callable(builtin, &context, &mut function)
        .unwrap();
    callable.clear(&mut function);
    builder.release_realm_function_materialization_context(context, &mut function);
    realm.clear(&mut function);
    function.instruction(&wasm_encoder::Instruction::End);
    let mut code = wasm_encoder::CodeSection::new();
    code.function(&function.into_body());
    let mut imports = wasm_encoder::ImportSection::new();
    for &helper in RuntimeHelperId::ALL {
        if helper.owner() == HelperOwner::Runtime {
            imports.import(
                "test-runtime",
                helper.debug_name(),
                wasm_encoder::EntityType::Function(helper.type_index()),
            );
        }
    }
    let mut functions = wasm_encoder::FunctionSection::new();
    functions.function(RuntimeHelperId::RealmEnsureDate.type_index());
    let mut module = wasm_encoder::Module::new();
    module.section(registered.types());
    module.section(&imports);
    module.section(&functions);
    module.section(&code);
    let bytes = module.finish();
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(&bytes)
        .expect(
            "the actual Date callable probe validates with its helper imports and registered types",
        );
    let ops = body(&bytes, imported_function_count_in_bytes(&bytes));
    let ensure = RuntimeHelperId::RealmEnsureDate.index(helper_base);
    let demand = calls(&ops, ensure);
    assert_eq!(
        demand.len(),
        1,
        "the callable producer crosses the ordinary Date demand gate"
    );
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(op, Operator::Call { .. }))
            .count(),
        1,
        "no constructor, native-code or metadata allocation helper is reachable"
    );
    assert!(!ops.iter().any(|op| matches!(op,
        Operator::StructNew { .. } | Operator::ArrayNew { .. } | Operator::ArrayNewFixed { .. }
        | Operator::ArrayNewDefault { .. } | Operator::RefFunc { .. })),
        "the callable route allocates neither FunctionObject nor ExecutableCode, even behind an Undefined branch");
    let table = type_index::<IntrinsicTable>(schema);
    let read = (demand[0] + 1..ops.len())
        .find(|&position| {
            matches!(ops[position], Operator::ArrayGet { array_type_index }
        if array_type_index == table)
        })
        .expect("the callable is read from its canonical table");
    assert_eq!(
        constant(&ops, read, &ops[read - 1]) as u32,
        Slot::DateConstructor.gc_index()
    );
    let callable_heap =
        wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(type_index::<
            crate::gc_types::FunctionObject,
        >(schema)));
    assert!(
        ops[read + 1..]
            .iter()
            .any(|op| matches!(op, Operator::RefCastNonNull { hty } if *hty == callable_heap)),
        "the returned callable binds the completed canonical FunctionObject"
    );
}
