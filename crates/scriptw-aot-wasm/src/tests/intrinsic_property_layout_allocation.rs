//! Check the actual fresh Realm initializer uses shared intrinsic layouts,
//! rather than allocating an entry, descriptor, or deferred cell per row.
use super::*;
use crate::gc_types::{
    DeferredIntrinsicValue, GcHeapType, GcNullability, GlobalLedger, IntrinsicPropertyStorage,
    OrdinaryObject, PropertyDescriptor, PropertyEntry, RuntimeModuleTypes, RuntimeSchema,
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
fn fresh_intrinsic_bootstrap_allocates_layout_records_not_per_property_records() {
    let artifact = emit_script(
        "Math.abs; Reflect.ownKeys; JSON.parse; Atomics.isLockFree; Object.prototype; Array.prototype; String.prototype;",
    )
    .expect("full standard bootstrap emits");
    expect_valid_module(&artifact, 1);
    let bytes = artifact.runtime().expect("heap program links R").bytes();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let names = function_names_in_bytes(bytes);
    let initializer = names
        .iter()
        .find_map(|(&index, name)| (name == "helper::realm_initialize_intrinsics").then_some(index))
        .expect("the actual Realm initializer is named in R");
    let operators = body(bytes, initializer);
    assert_eq!(
        operators
            .iter()
            .filter(|operator| matches!(operator, Operator::StructNew { struct_type_index } if *struct_type_index == type_index::<IntrinsicPropertyStorage>(schema)))
            .count(),
        61,
        "four namespaces, three original prototypes, 24 constructors and 30 newly covered prototypes are born with complete storage; nested Array unscopables remains unmaterialized"
    );
    for (position, operator) in operators.iter().enumerate() {
        if !matches!(operator, Operator::StructNew { struct_type_index }
            if *struct_type_index == type_index::<IntrinsicPropertyStorage>(schema))
        {
            continue;
        }
        assert!(
            matches!(operators[position - 1], Operator::RefNull { .. }),
            "each birth layout starts without an observation cache"
        );
        assert!(
            operators[position + 1..]
                .iter()
                .find_map(|op| match op {
                    Operator::StructNew { struct_type_index }
                        if *struct_type_index == type_index::<IntrinsicPropertyStorage>(schema) =>
                        Some(false),
                    Operator::StructNew { struct_type_index }
                        if *struct_type_index == type_index::<OrdinaryObject>(schema) =>
                        Some(true),
                    _ => None,
                })
                .expect("each storage has its ordinary header"),
            "a complete storage is constructed before its owning header"
        );
    }
    for ty in [
        type_index::<DeferredIntrinsicValue>(schema),
        type_index::<PropertyDescriptor>(schema),
        type_index::<PropertyEntry>(schema),
    ] {
        assert!(
            !operators.iter().any(|op| matches!(op,
            Operator::StructNew { struct_type_index } if *struct_type_index == ty)),
            "Realm birth allocates no per-property recipes, descriptors or entries"
        );
    }
    assert!(
        !operators.iter().any(|op| matches!(op,
        Operator::StructSet { struct_type_index, field_index }
        if *struct_type_index == type_index::<OrdinaryObject>(schema)
            && *field_index == crate::gc_types::GcSnapshotField::ObjectProperties.index())),
        "complete layouts are supplied at birth, never attached over transient storage"
    );
}

#[test]
fn data_only_unscopables_uses_compact_storage_without_a_realm_edge() {
    let artifact = emit_script("Array.prototype[Symbol.unscopables];").unwrap();
    let bytes = artifact.runtime().unwrap().bytes();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let names = function_names_in_bytes(bytes);
    let helper = names
        .iter()
        .find_map(|(&index, name)| (name == "helper::intrinsic_property_value").then_some(index))
        .unwrap();
    let operators = body(bytes, helper);
    let allocations = |ty| {
        operators.iter().filter(|op|
        matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == ty)).count()
    };
    assert_eq!(
        allocations(type_index::<IntrinsicPropertyStorage>(schema)),
        0,
        "data-only child construction must not capture its defining Realm"
    );
    assert_eq!(
        allocations(type_index::<crate::gc_types::ArrayCompactDataStorage>(
            schema
        )),
        1
    );
    assert_eq!(
        allocations(type_index::<crate::gc_types::CompactDataShape>(schema)),
        1
    );
}
