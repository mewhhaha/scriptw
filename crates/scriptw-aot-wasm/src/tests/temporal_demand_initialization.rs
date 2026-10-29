//! The emitted canonical R must keep Temporal allocation out of Realm bootstrap.
use super::*;
use crate::gc_types::{
    GcHeapType, GcNullability, GlobalLedger, RealmRecord, RuntimeModuleTypes, RuntimeSchema,
    StoredValue,
};
use crate::intrinsics::temporal::TemporalIntrinsicFamily;

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
fn canonical_runtime_defers_the_actual_temporal_graph_and_exports_typed_ensure() {
    // The input never mentions Temporal. R must still retain its complete
    // compiled graph for later programs, host access and created Realms.
    let artifact = emit_script("this;").unwrap();
    expect_valid_module(&artifact, 0);
    let bytes = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(bytes);
    let named = |requested: &str| {
        let indices: Vec<_> = names
            .iter()
            .filter_map(|(&index, name)| (name == requested).then_some(index))
            .collect();
        assert_eq!(indices.len(), 1, "one physical R body for {requested}");
        indices[0]
    };
    let ensure_index = named("helper::realm_ensure_temporal");
    let initial = body(bytes, named("helper::realm_initialize_intrinsics"));
    let ensure = body(bytes, ensure_index);
    let resolve = body(bytes, named("helper::property_value_resolve"));
    assert!(
        !initial.iter().any(|operator| matches!(operator,
            Operator::Call { function_index } if *function_index == ensure_index)),
        "actual initial Realm bootstrap cannot force Temporal"
    );
    assert!(
        resolve.iter().any(|operator| matches!(operator,
            Operator::Call { function_index } if *function_index == ensure_index)),
        "actual deferred-property resolution must enter the canonical ensure owner"
    );
    let temporal_entries: BTreeMap<_, _> = StandardBuiltinId::all_functions()
        .iter()
        .copied()
        .filter(|builtin| {
            builtin
                .debug_name()
                .strip_prefix("get ")
                .unwrap_or(builtin.debug_name())
                .starts_with("Temporal.")
        })
        .map(|builtin| {
            (
                builtin,
                named(&format!("builtin::{}", builtin.debug_name())),
            )
        })
        .collect();
    assert!(!temporal_entries.is_empty());
    for (&builtin, &entry) in &temporal_entries {
        assert!(
            !initial.iter().any(|operator| matches!(operator,
                Operator::RefFunc { function_index } if *function_index == entry)),
            "initial Realm bootstrap still allocates {}",
            builtin.debug_name()
        );
    }
    let eager_entries = TemporalIntrinsicFamily::ALL
        .into_iter()
        .map(TemporalIntrinsicFamily::constructor)
        .chain(temporal_entries.keys().copied().filter(|builtin| {
            builtin
                .native_function_name()
                .is_some_and(|name| name.starts_with("get "))
        }));
    for builtin in eager_entries {
        let entry = temporal_entries[&builtin];
        assert_eq!(
            ensure
                .iter()
                .filter(|operator| matches!(operator,
                    Operator::RefFunc { function_index } if *function_index == entry))
                .count(),
            1,
            "the actual ensure body owns complete construction of {}",
            builtin.debug_name()
        );
    }

    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let mut types = Vec::new();
    let mut declarations = Vec::new();
    let mut exported = false;
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
            Payload::ExportSection(exports) => {
                for export in exports {
                    let export = export.unwrap();
                    exported |= export.kind == wasmparser::ExternalKind::Func
                        && export.index == ensure_index;
                }
            }
            _ => {}
        }
    }
    assert!(
        exported,
        "actual R exports Temporal ensure for later P modules"
    );
    let wasmparser::CompositeInnerType::Func(signature) = &types
        [declarations[ensure_index as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("Temporal ensure has its registered function signature");
    };
    let reference = |index| {
        wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                false,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index)),
            )
            .unwrap(),
        )
    };
    assert_eq!(
        signature.params(),
        [reference(type_index::<RealmRecord>(schema))]
    );
    assert_eq!(
        signature.results(),
        [reference(type_index::<StoredValue>(schema))]
    );
}
