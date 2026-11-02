//! Actual R data reads normalize only the observed owner's deferred value.
use super::error_demand_initialization::{body, encode_realm_probe, named, type_index};
use super::*;
use crate::gc_types::{
    FinalizedRuntimeModule, GcSnapshotField, GlobalLedger, IntrinsicPropertyStorage,
    IntrinsicPropertyStorageSchema, IntrinsicPropertyValueCache, Nullable, PropertyDescriptor,
    RuntimeModuleTypes, StoredValue,
};

fn calls(operators: &[Operator<'_>], target: u32) -> usize {
    operators
        .iter()
        .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == target))
        .count()
}

fn if_arms<'a, 'b>(
    operators: &'a [Operator<'b>],
    start: usize,
) -> Option<(&'a [Operator<'b>], &'a [Operator<'b>])> {
    let mut depth = 1;
    let mut otherwise = None;
    for (index, op) in operators.iter().enumerate().skip(start + 1) {
        match op {
            Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::If { .. }
            | Operator::TryTable { .. } => depth += 1,
            Operator::Else if depth == 1 => otherwise = Some(index),
            Operator::End => {
                depth -= 1;
                if depth == 0 {
                    return otherwise
                        .map(|split| (&operators[start + 1..split], &operators[split + 1..index]));
                }
            }
            _ => {}
        }
    }
    None
}

fn cache_field(runtime: &FinalizedRuntimeModule) -> u32 {
    let schema = runtime.globals().schema();
    let mut function = crate::code_sink::Function::with_parameters(
        crate::code_sink::LocalDeclarations::from_types([]),
        vec![wasm_encoder::ValType::Ref(
            schema.reference_type::<crate::gc_types::RealmRecord>(
                crate::gc_types::GcNullability::NonNullable,
            ),
        )],
    );
    let storage = schema
        .reserve_gc_local::<IntrinsicPropertyStorage, Nullable>(&mut function)
        .initialize_null(schema, &mut function);
    let cache = schema.reserve_gc_local(&mut function).initialize(
        schema
            .field(IntrinsicPropertyStorageSchema::CACHE)
            .read(&storage, schema, &mut function)
            .reference(),
        &mut function,
    );
    cache.clear(&mut function);
    storage.clear(&mut function);
    function.instruction(&wasm_encoder::Instruction::End);
    let bytes = encode_realm_probe(runtime, function);
    let fields: Vec<_> = body(&bytes, 0)
        .into_iter()
        .filter_map(|op| match op {
            Operator::StructGet {
                struct_type_index,
                field_index,
            } if struct_type_index == type_index::<IntrinsicPropertyStorage>(schema) => {
                Some(field_index)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        fields.len(),
        1,
        "typed cache probe identifies its actual field"
    );
    fields[0]
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Origin {
    Local(u32),
    Resolved,
    Field(Box<Self>, u32, u32),
    Other,
}

#[derive(Debug)]
enum Write {
    Field(Origin, u32, u32, Origin),
    Element(Origin, u32, Origin),
}

/// Track only the straight-line deferred arm, including reference upcasts and
/// temporary roots. A local-number change cannot disguise writing the recipe
/// back instead of the canonical resolver result.
fn deferred_writes(operators: &[Operator<'_>], resolve: u32) -> (Origin, Vec<Write>) {
    let mut stack = Vec::new();
    let mut locals = BTreeMap::new();
    let mut writes = Vec::new();
    let mut input = None;
    for op in operators {
        match *op {
            Operator::LocalGet { local_index } => stack.push(
                locals
                    .get(&local_index)
                    .cloned()
                    .unwrap_or(Origin::Local(local_index)),
            ),
            Operator::LocalSet { local_index } => {
                locals.insert(local_index, stack.pop().unwrap());
            }
            Operator::LocalTee { local_index } => {
                locals.insert(local_index, stack.last().unwrap().clone());
            }
            Operator::RefAsNonNull
            | Operator::RefCastNonNull { .. }
            | Operator::RefCastNullable { .. } => {}
            Operator::RefNull { .. } | Operator::I32Const { .. } | Operator::I64Const { .. } => {
                stack.push(Origin::Other);
            }
            Operator::StructGet {
                struct_type_index,
                field_index,
            } => {
                let owner = stack.pop().unwrap();
                stack.push(Origin::Field(
                    Box::new(owner),
                    struct_type_index,
                    field_index,
                ));
            }
            Operator::Call { function_index } if function_index == resolve => {
                input = Some(stack.pop().unwrap());
                stack.push(Origin::Resolved);
            }
            Operator::StructSet {
                struct_type_index,
                field_index,
            } => {
                let value = stack.pop().unwrap();
                let owner = stack.pop().unwrap();
                writes.push(Write::Field(owner, struct_type_index, field_index, value));
            }
            Operator::ArraySet { array_type_index } => {
                let value = stack.pop().unwrap();
                stack.pop().unwrap();
                let owner = stack.pop().unwrap();
                writes.push(Write::Element(owner, array_type_index, value));
            }
            ref unexpected => {
                panic!("unexpected operator in straight-line resolution: {unexpected:?}")
            }
        }
    }
    assert!(stack.is_empty());
    (input.expect("one resolver input"), writes)
}

fn assignment_input<'a, 'b>(operators: &'a [Operator<'b>], local: u32) -> &'a [Operator<'b>] {
    let index = operators
        .iter()
        .rposition(|op| {
            matches!(op,
        Operator::LocalSet { local_index } | Operator::LocalTee { local_index }
        if *local_index == local)
        })
        .expect("the recipe was loaded before its type test");
    &operators[..index]
}

fn preceding_reference_local(operators: &[Operator<'_>]) -> u32 {
    match operators
        .iter()
        .rev()
        .find(|op| !matches!(op, Operator::RefAsNonNull))
        .unwrap()
    {
        Operator::LocalGet { local_index } => *local_index,
        unexpected => panic!("expected the loaded owner reference: {unexpected:?}"),
    }
}

#[test]
fn observed_data_recipes_write_back_the_resolved_record_without_warming_promotion() {
    let artifact = emit_script(
        "(function(){var expected=RegExp,n=0;for(var i=0;i<10000;i++){if(RegExp===expected)n++;}print(n);})();",
    )
    .unwrap();
    expect_valid_module(&artifact, 0);
    let bytes = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(bytes);
    let resolve = named(&names, "helper::property_value_resolve");
    let read = named(&names, "helper::object_read");
    let intrinsic_value = named(&names, "helper::intrinsic_property_value");
    for helper in [
        "helper::environment_identifier_get_sloppy",
        "helper::environment_identifier_get_strict",
    ] {
        assert!(
            calls(&body(bytes, named(&names, helper)), read) > 0,
            "global identifier reads reach the actual ObjectRead owner"
        );
    }
    let runtime = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = runtime.globals().schema();
    let descriptor = type_index::<PropertyDescriptor>(schema);
    let descriptor_value = GcSnapshotField::DescriptorValue.index();
    let storage = type_index::<IntrinsicPropertyStorage>(schema);
    let cache = type_index::<IntrinsicPropertyValueCache>(schema);
    let cache_field = cache_field(&runtime);
    let stored = type_index::<StoredValue>(schema);
    let stored_heap = wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(stored));
    let operators = body(bytes, read);
    let mut descriptor_writes = 0;
    let mut cache_writes = 0;
    for (index, op) in operators.iter().enumerate() {
        if !matches!(op, Operator::RefTestNonNull { hty } if *hty == stored_heap) {
            continue;
        }
        assert!(matches!(operators[index + 1], Operator::If { .. }));
        let (fast, deferred) = if_arms(&operators, index + 1).unwrap();
        assert!(
            !fast.iter().any(|op| matches!(
                op,
                Operator::Call { .. } | Operator::StructSet { .. } | Operator::ArraySet { .. }
            )),
            "already StoredValue reads do not resolve or write"
        );
        assert_eq!(
            fast.iter()
                .filter(|op| matches!(op,
            Operator::StructGet { struct_type_index, .. } if *struct_type_index == stored))
                .count(),
            3
        );
        assert_eq!(calls(deferred, resolve), 1);
        let (input, writes) = deferred_writes(deferred, resolve);
        let Origin::Local(recipe_local) = input else {
            panic!("resolution starts with the retained recipe");
        };
        let loaded = assignment_input(&operators[..index], recipe_local);
        for write in writes {
            match write {
                Write::Field(owner, ty, field, value) => {
                    assert_eq!((ty, field), (descriptor, descriptor_value));
                    assert!(matches!(loaded.last().unwrap(),
                        Operator::StructGet { struct_type_index, field_index }
                        if *struct_type_index == ty && *field_index == field));
                    assert_eq!(
                        owner,
                        Origin::Local(preceding_reference_local(&loaded[..loaded.len() - 1])),
                        "write the same descriptor whose recipe was read"
                    );
                    assert_eq!(value, Origin::Resolved);
                    descriptor_writes += 1;
                }
                Write::Element(owner, ty, value) => {
                    assert!(
                        matches!(loaded.last().unwrap(), Operator::Call { function_index }
                        if *function_index == intrinsic_value)
                    );
                    let arguments: Vec<_> = loaded[..loaded.len() - 1]
                        .iter()
                        .rev()
                        .filter(|op| !matches!(op, Operator::RefAsNonNull))
                        .take(3)
                        .map(|op| match op {
                            Operator::LocalGet { local_index } => *local_index,
                            unexpected => {
                                panic!("intrinsic value receives typed locals: {unexpected:?}")
                            }
                        })
                        .collect();
                    assert_eq!(
                        owner,
                        Origin::Field(Box::new(Origin::Local(arguments[2])), storage, cache_field),
                        "write the cache belonging to the storage that supplied this recipe"
                    );
                    assert_eq!(ty, cache);
                    assert_eq!(value, Origin::Resolved);
                    cache_writes += 1;
                }
            }
        }
    }
    assert!(
        descriptor_writes > 0 && cache_writes > 0,
        "both actual DATA storage representations normalize on observation"
    );
    assert_eq!(
        operators
            .iter()
            .filter(|op| matches!(op,
        Operator::StructSet { struct_type_index, field_index }
        if *struct_type_index == descriptor && *field_index == descriptor_value))
            .count(),
        descriptor_writes
    );
    assert_eq!(
        operators
            .iter()
            .filter(|op| matches!(op,
        Operator::ArraySet { array_type_index } if *array_type_index == cache))
            .count(),
        cache_writes,
        "no accessor invocation result is cached outside a recipe-resolution arm"
    );

    assert_eq!(
        calls(&body(bytes, intrinsic_value), resolve),
        0,
        "recipe lookup still does not observe deferred values"
    );
    let promotion = body(
        bytes,
        named(&names, "helper::ordinary_property_storage_ensure"),
    );
    let mut data_arms = 0;
    for (index, op) in promotion.iter().enumerate() {
        if !matches!(op, Operator::If { .. }) {
            continue;
        }
        if let Some((accessor, data)) = if_arms(&promotion, index) {
            if calls(accessor, resolve) == 2 && calls(data, intrinsic_value) == 1 {
                assert_eq!(
                    calls(data, resolve),
                    0,
                    "promotion retains unobserved DATA recipes while resolving accessor identities"
                );
                data_arms += 1;
            }
        }
    }
    assert_eq!(
        data_arms, 1,
        "inspect the actual intrinsic descriptor builder in promotion"
    );
}
