//! Inspect actual R callers, GC allocations, ordering and declared helper ABI.
use super::*;
use crate::gc_types::{
    DeferredIntrinsicValue, ExecutableCode, FunctionContext, FunctionObject, GcHeapType,
    GcNullability, GlobalLedger, OrdinaryObject, PropertyValue, RealmRecord, RuntimeModuleTypes,
    RuntimeSchema,
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
    let constructor = named("helper::intrinsic_constructor_allocate");
    let publish = named("helper::intrinsic_data_publish");
    let key = named("helper::pooled_string_resolve");
    let header = named("helper::object_header_projection");
    let resolve = named("helper::property_value_resolve");
    let bootstrap = body(bytes, named("helper::realm_initialize_intrinsics"));
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
}
