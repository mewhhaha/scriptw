//! Inspect actual R callers, GC allocations, ordering and declared helper ABI.
use super::*;
use crate::gc_types::{
    DeferredIntrinsicValue, Environment, ExecutableCode, FunctionContext, FunctionObject,
    GcHeapType, GcNullability, GlobalLedger, OrdinaryObject, PropertyValue, RealmRecord,
    RuntimeModuleTypes, RuntimeSchema, StoredValue, StringValue,
};

fn type_index<T: GcHeapType>(schema: &RuntimeSchema) -> u32 {
    let wasm_encoder::HeapType::Concrete(index) = schema
        .reference_type::<T>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("registered GC type has a concrete index");
    };
    index
}

fn body(bytes: &[u8], index: u32) -> Vec<Operator<'_>> {
    let mut next = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if next == index {
                return body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect();
            }
            next += 1;
        }
    }
    panic!("named runtime helper has a physical body");
}

#[test]
fn ordered_realm_bootstrap_shares_real_recipe_and_constructor_allocation() {
    let artifact = emit_script("new Uint16Array(1); Set.prototype.keys;").unwrap();
    expect_valid_module(&artifact, 1);
    let bytes = artifact.runtime().unwrap().bytes();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let names = function_names_in_bytes(bytes);
    let named = |requested: &str| {
        let indices: Vec<_> = names
            .iter()
            .filter_map(|(&index, name)| (name == requested).then_some(index))
            .collect();
        assert_eq!(indices.len(), 1, "one physical R body for {requested}");
        indices[0]
    };
    let install = named("helper::deferred_intrinsic_install");
    let batch = named("helper::deferred_intrinsic_install_run");
    let constructor = named("helper::intrinsic_constructor_allocate");
    let constructor_header = named("helper::native_constructor_header_allocate");
    let publish = named("helper::intrinsic_data_publish");
    let key = named("helper::pooled_string_resolve");
    let header = named("helper::object_header_projection");
    let resolve = named("helper::property_value_resolve");
    let ensure_temporal = named("helper::realm_ensure_temporal");
    let initialize = named("helper::realm_initialize_intrinsics");
    let bootstrap = body(bytes, initialize);
    let installs = body(bytes, install);
    let constructors = body(bytes, constructor);
    let calls = |ops: &[Operator<'_>], target| {
        ops.iter()
            .enumerate()
            .filter_map(|(i, op)| {
                matches!(op, Operator::Call { function_index } if *function_index == target)
                    .then_some(i)
            })
            .collect::<Vec<_>>()
    };
    let allocations = |ops: &[Operator<'_>], target| {
        ops.iter().enumerate().filter_map(|(i, op)| {
        matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == target).then_some(i)
    }).collect::<Vec<_>>()
    };
    assert!(
        calls(&bootstrap, install).len() > 1 && calls(&bootstrap, constructor).len() > 1,
        "the executed Realm initializer must call both actual shared bodies"
    );
    let header_calls = calls(&bootstrap, constructor_header);
    let constructor_calls = calls(&bootstrap, constructor);
    assert_eq!(header_calls.len(), constructor_calls.len());
    for (position, (&header, &callable)) in header_calls.iter().zip(&constructor_calls).enumerate()
    {
        assert!(
            header < callable && (position == 0 || constructor_calls[position - 1] < header),
            "each eager constructor builds its own dictionary header before its context/callable tail"
        );
    }
    assert!(
        allocations(&bootstrap, type_index::<DeferredIntrinsicValue>(schema)).is_empty(),
        "repeated inline recipe setup has left the actual Realm initializer"
    );
    let recipes = allocations(&installs, type_index::<DeferredIntrinsicValue>(schema));
    assert_eq!(
        recipes.len(),
        1,
        "one fresh original recipe, no packaging allocation"
    );
    assert_eq!(
        installs
            .iter()
            .filter(|op| matches!(op, Operator::StructNew { .. }))
            .count(),
        1
    );
    assert!(
        !installs.iter().any(|op| matches!(
            op,
            Operator::ArrayNew { .. }
                | Operator::ArrayNewFixed { .. }
                | Operator::ArrayNewDefault { .. }
        )),
        "key plans do not allocate an alias or installation table"
    );
    let projections = calls(&installs, header);
    let keys = calls(&installs, key);
    let publications = calls(&installs, publish);
    assert_eq!(
        (projections.len(), keys.len(), publications.len()),
        (1, 1, 1)
    );
    assert!(
        recipes[0] < projections[0] && projections[0] < keys[0] && keys[0] < publications[0],
        "recipe allocation precedes header projection, lazy key resolution and publication"
    );
    assert!(
        calls(&installs, resolve).is_empty(),
        "installation must not observe the deferred function"
    );
    let contexts = allocations(&constructors, type_index::<FunctionContext>(schema));
    let callables = allocations(&constructors, type_index::<FunctionObject>(schema));
    let sources = calls(&constructors, key);
    assert_eq!((contexts.len(), callables.len(), sources.len()), (1, 1, 1));
    assert!(
        contexts[0] < sources[0] && sources[0] < callables[0],
        "constructor context exists before source resolution and callable allocation"
    );
    assert_eq!(
        constructors
            .iter()
            .filter(|op| matches!(op, Operator::StructNew { .. }))
            .count(),
        2,
        "constructor sharing adds no GC record to package its operands"
    );
    let close: Vec<_> = constructors
        .iter()
        .enumerate()
        .filter_map(|(i, op)| {
            matches!(op, Operator::StructSet { struct_type_index, field_index }
            if *struct_type_index == type_index::<FunctionContext>(schema)
            && *field_index == crate::GcHostField::ContextActiveFunction.definition().index())
            .then_some(i)
        })
        .collect();
    assert_eq!(close.len(), 1);
    assert!(
        callables[0] < close[0],
        "complete the active-function cycle before returning"
    );

    // Inspect exported real R signatures, rather than assuming helper indices
    // or trusting the Rust registration row in isolation.
    let mut types = Vec::new();
    let mut declarations = Vec::new();
    let mut exports = std::collections::BTreeSet::new();
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
    let reference = |index| {
        wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                false,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index)),
            )
            .unwrap(),
        )
    };
    for (helper, parameters, result) in [
        (
            constructor_header,
            vec![
                wasmparser::ValType::I32,
                wasmparser::ValType::I64,
                wasmparser::ValType::Ref(wasmparser::RefType::EQREF),
                reference(type_index::<StringValue>(schema)),
                wasmparser::ValType::I64,
                wasmparser::ValType::I32,
            ],
            reference(type_index::<OrdinaryObject>(schema)),
        ),
        (
            constructor,
            vec![
                reference(type_index::<OrdinaryObject>(schema)),
                reference(type_index::<ExecutableCode>(schema)),
                reference(type_index::<RealmRecord>(schema)),
                wasmparser::ValType::I32,
            ],
            reference(type_index::<FunctionObject>(schema)),
        ),
        (
            install,
            vec![
                wasmparser::ValType::I32,
                wasmparser::ValType::I64,
                wasmparser::ValType::Ref(wasmparser::RefType::EQREF),
                reference(type_index::<RealmRecord>(schema)),
                wasmparser::ValType::I32,
                wasmparser::ValType::I32,
                wasmparser::ValType::I32,
                wasmparser::ValType::I32,
                wasmparser::ValType::I32,
            ],
            reference(type_index::<PropertyValue>(schema)),
        ),
        (
            ensure_temporal,
            vec![reference(type_index::<RealmRecord>(schema))],
            reference(type_index::<StoredValue>(schema)),
        ),
    ] {
        assert!(exports.contains(&helper));
        let wasmparser::CompositeInnerType::Func(signature) = &types
            [declarations[helper as usize] as usize]
            .composite_type
            .inner
        else {
            panic!("shared allocator has a real function signature");
        };
        assert_eq!(signature.params(), parameters);
        assert_eq!(signature.results(), [result]);
    }
    let nullable_reference = |index| {
        wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                true,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index)),
            )
            .unwrap(),
        )
    };
    assert!(exports.contains(&initialize));
    let wasmparser::CompositeInnerType::Func(signature) = &types
        [declarations[initialize as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("Realm initialization has a real function signature");
    };
    assert_eq!(
        signature.params(),
        [
            reference(type_index::<RealmRecord>(schema)),
            nullable_reference(type_index::<Environment>(schema)),
        ]
    );
    assert_eq!(
        signature.results(),
        [nullable_reference(type_index::<OrdinaryObject>(schema)); 5],
        "eager bootstrap returns only Reflect, Math, JSON, Atomics and Intl"
    );
    let wasmparser::CompositeInnerType::Func(signature) = &types
        [declarations[batch as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("compact installation has a real function signature");
    };
    assert_eq!(
        signature.params(),
        [
            wasmparser::ValType::I32,
            wasmparser::ValType::I64,
            wasmparser::ValType::Ref(wasmparser::RefType::EQREF),
            reference(type_index::<RealmRecord>(schema)),
            wasmparser::ValType::I32,
            wasmparser::ValType::I32,
        ]
    );
    assert!(signature.results().is_empty());
}

#[test]
fn actual_realm_bootstrap_installs_checked_rows_through_one_counted_loop() {
    let artifact = emit_script("Math.abs; Array.prototype.map; Temporal.Instant;").unwrap();
    expect_valid_module(&artifact, 0);
    let bytes = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(bytes);
    let named = |requested: &str| {
        names
            .iter()
            .find_map(|(&index, name)| (name == requested).then_some(index))
            .expect("actual registered R installation body")
    };
    let batch = named("helper::deferred_intrinsic_install_run");
    let install = named("helper::deferred_intrinsic_install");
    let resolve = named("helper::property_value_resolve");
    let bootstrap_index = named("helper::realm_initialize_intrinsics");
    let bootstrap = body(bytes, bootstrap_index);
    let temporal_index = named("helper::realm_ensure_temporal");
    let temporal = body(bytes, temporal_index);
    assert!(
        !bootstrap.iter().any(|op| matches!(op,
            Operator::Call { function_index } if *function_index == temporal_index
        )),
        "eager Realm initialization never forces the Temporal graph"
    );
    let loop_body = body(bytes, batch);
    assert_eq!(
        loop_body
            .iter()
            .filter(|op| matches!(op, Operator::Loop { .. }))
            .count(),
        1,
        "one physical counted loop, rather than unrolled row dispatch"
    );
    let loads: Vec<_> = loop_body
        .iter()
        .enumerate()
        .filter_map(|(i, op)| matches!(op, Operator::I32Load { .. }).then_some(i))
        .collect();
    assert_eq!(loads.len(), 4, "read exactly the four immutable row words");
    let calls: Vec<_> = loop_body
        .iter()
        .enumerate()
        .filter_map(|(i, op)| {
            matches!(op, Operator::Call { function_index } if *function_index == install)
                .then_some(i)
        })
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "all rows reuse the original recipe/publication helper"
    );
    assert!(loads[3] < calls[0]);
    assert!(matches!(loop_body[calls[0] + 1], Operator::Drop));
    assert!(
        !loop_body.iter().any(|op| matches!(
            op,
            Operator::StructNew { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewDefault { .. }
                | Operator::ArrayNewFixed { .. }
        )),
        "the batch adds no mutable GC record or packaging allocation"
    );
    assert!(
        !loop_body.iter().any(|op| matches!(op,
            Operator::Call { function_index } if *function_index == resolve
        )),
        "installation never observes the deferred callable"
    );
    assert!(
        loop_body[..loads[0]]
            .iter()
            .filter(|op| matches!(op, Operator::Unreachable))
            .count()
            >= 2,
        "static table/alignment and count checks precede the first memory read"
    );
    assert!(
        loop_body[calls[0]..]
            .iter()
            .any(|op| matches!(op, Operator::Br { relative_depth: 0 })),
        "the real helper returns to the counted loop"
    );

    let spans: Vec<_> = [(false, &bootstrap), (true, &temporal)]
        .into_iter()
        .flat_map(|(is_temporal, operators)| operators.windows(3).filter_map(move |ops| match ops {
            [Operator::I32Const { value: start }, Operator::I32Const { value: count },
             Operator::Call { function_index }] if *function_index == batch => Some((is_temporal, *start as u32, *count as u32)),
            _ => None,
        }))
        .collect();
    assert!(spans.iter().any(|(is_temporal, _, _)| !is_temporal));
    assert!(spans.iter().any(|(is_temporal, _, _)| *is_temporal));
    let total_rows = spans
        .iter()
        .map(|(_, _, count)| *count as usize)
        .sum::<usize>();
    assert!(spans.len() >= 40 && total_rows > 400,
        "the complete Realm graph batches methods across builtin families: {} spans, {total_rows} rows", spans.len());
    let direct_installs = bootstrap
        .iter()
        .chain(temporal.iter())
        .filter(|op| {
            matches!(op,
                Operator::Call { function_index } if *function_index == install
            )
        })
        .count();
    assert!(
        direct_installs < total_rows / 2,
        "most per-method operand setup has left both initializers; aliases keep the original path"
    );

    // Inspect actual active R data at the emitted call operands, not the Rust
    // plan or a source spelling. Every counted row is wholly in one sealed
    // static segment and retains a closed builtin/key/attribute domain.
    let mut segments = Vec::new();
    let mut bootstrap_bytes = 0;
    let mut next = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::CodeSectionEntry(encoded) => {
                if next == bootstrap_index {
                    bootstrap_bytes = encoded.range().len();
                }
                next += 1;
            }
            Payload::DataSection(data) => {
                for entry in data {
                    let entry = entry.unwrap();
                    if let wasmparser::DataKind::Active {
                        memory_index: 0,
                        offset_expr,
                    } = entry.kind
                    {
                        let mut ops = offset_expr.get_operators_reader();
                        let Operator::I32Const { value } = ops.read().unwrap() else {
                            panic!("R static data has a constant address");
                        };
                        segments.push((value as u32, entry.data));
                    }
                }
            }
            _ => {}
        }
    }
    let mut symbols = 0;
    let mut nonwritable = 0;
    for (is_temporal, start, count) in spans {
        assert!(count > 0);
        let row_bytes = count.checked_mul(16).unwrap();
        let end = start.checked_add(row_bytes).unwrap();
        let (base, data) = segments
            .iter()
            .find(|(base, data)| start >= *base && end as u64 <= *base as u64 + data.len() as u64)
            .expect("the complete installation span belongs to active R static data");
        let offset = (start - base) as usize;
        for row in data[offset..offset + row_bytes as usize].chunks_exact(16) {
            let words: [u32; 4] = std::array::from_fn(|i| {
                u32::from_le_bytes(row[i * 4..i * 4 + 4].try_into().unwrap())
            });
            let builtin = *StandardBuiltinId::all_functions()
                .iter()
                .find(|id| **id as u32 == words[0])
                .expect("encoded rows retain an actual StandardBuiltinId");
            assert!(!builtin.constructable() && builtin.global_name().is_none());
            assert_eq!(
                builtin.debug_name().starts_with("Temporal."),
                is_temporal,
                "actual {builtin:?} row must belong to its eager or demand initializer"
            );
            assert!(
                crate::module::standard_builtin_function_realm_slot(builtin).is_none(),
                "canonical identities are construction ordering barriers"
            );
            assert!(words[1] <= 1 && words[3] & !3 == 0);
            if words[1] == 1 {
                symbols += 1;
                assert!(words[2] < scriptw_ir::WellKnownSymbol::ALL.len() as u32);
            }
            nonwritable += usize::from(words[3] & 1 == 0);
        }
    }
    assert!(
        symbols >= 4 && nonwritable >= 1,
        "actual rows exercise Symbol identity and nonuniform writable attributes"
    );
    eprintln!("compact Realm bootstrap: {bootstrap_bytes} encoded body bytes, {total_rows} batched method rows, {direct_installs} direct deferred installations");
}
