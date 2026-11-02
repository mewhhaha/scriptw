//! Check the actual fresh Realm initializer uses shared intrinsic layouts,
//! rather than allocating an entry, descriptor, or deferred cell per row.
use super::*;
use crate::gc_types::{
    GcHeapType, GcNullability, GlobalLedger, IntrinsicPropertyStorage, RuntimeModuleTypes,
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
        7,
        "the four namespaces and three prototypes each install one fresh storage record; Array unscopables is nested and remains unmaterialized"
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
