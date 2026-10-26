//! Inspect the actual native bootstrap path, including its shared allocator.
use super::*;
use crate::gc_types::{
    CompactDataStorage, FunctionObject, GcHeapType, GcNullability, GlobalLedger,
    OrdinaryPropertyStorage, PropertyDescriptor, PropertyEntry, RuntimeModuleTypes, StoredValue,
};

fn type_index<T: GcHeapType>(schema: &crate::gc_types::RuntimeSchema) -> u32 {
    let wasm_encoder::HeapType::Concrete(index) = schema
        .reference_type::<T>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("registered GC type has a concrete index");
    };
    index
}

fn named_body<'a>(bytes: &'a [u8], index: u32) -> Vec<Operator<'a>> {
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
    panic!("registered helper has a body");
}

#[test]
fn native_bootstrap_allocates_compact_metadata_without_eager_property_records() {
    let artifact = emit_script("Math.max.name;").unwrap();
    let runtime = artifact.runtime().unwrap();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let names = function_names_in_bytes(runtime.bytes());
    let index = |name| {
        names
            .iter()
            .find_map(|(index, actual)| (actual == name).then_some(*index))
            .expect("actual runtime helper is registered")
    };
    let allocate = index("helper::native_function_allocate");
    let compact = index("helper::compact_literal_allocate");
    let bootstrap = named_body(
        runtime.bytes(),
        index("helper::realm_initialize_intrinsics"),
    );
    let callables = bootstrap
        .iter()
        .filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == type_index::<FunctionObject>(schema)))
        .count();
    let native_headers = bootstrap
        .iter()
        .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == allocate))
        .count();
    assert!(callables > 0, "inspect the real intrinsic bootstrap");
    assert_eq!(
        native_headers, callables,
        "each native callable gets its own compact header"
    );
    let allocator = named_body(runtime.bytes(), allocate);
    assert_eq!(
        allocator
            .iter()
            .filter(
                |op| matches!(op, Operator::Call { function_index } if *function_index == compact)
            )
            .count(),
        1
    );
    let compact_allocator = named_body(runtime.bytes(), compact);
    assert_eq!(compact_allocator.iter().filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == type_index::<CompactDataStorage>(schema))).count(), 1);
    for operators in [&allocator, &compact_allocator] {
        for dictionary_type in [
            type_index::<OrdinaryPropertyStorage>(schema),
            type_index::<PropertyDescriptor>(schema),
            type_index::<PropertyEntry>(schema),
        ] {
            assert!(!operators.iter().any(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == dictionary_type)), "native metadata allocation must not eagerly create dictionary records");
        }
    }
    let stored_values = |operators: &[Operator<'_>]| {
        operators.iter().filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == type_index::<StoredValue>(schema))).count()
    };
    assert_eq!(
        stored_values(&allocator),
        0,
        "native name/length values remain unboxed"
    );
    assert_eq!(
        stored_values(&compact_allocator),
        1,
        "the ordinary object header owns one prototype snapshot; metadata adds no value boxes"
    );
    expect_valid_module(&artifact, 0);
}
