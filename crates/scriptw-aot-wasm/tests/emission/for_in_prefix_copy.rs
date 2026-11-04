//! ForIn retains immutable key snapshots without reboxing its visited prefix.
use crate::property_read_plans::Module;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
use wasmparser::{
    CompositeInnerType, HeapType, Operator, RefType, StorageType, UnpackedIndex, ValType,
    Validator, WasmFeatures,
};

fn concrete(reference: RefType) -> u32 {
    let HeapType::Concrete(UnpackedIndex::Module(index)) = reference.heap_type() else {
        panic!("the actual signature uses a concrete GC type");
    };
    index
}

#[test]
fn for_in_copies_immutable_visited_keys_into_one_fresh_table() {
    let artifact = std::thread::Builder::new()
        .name("for-in-prefix-copy".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(
                "function collect(source) { var keys=''; for (var key in source) keys+=key; return keys; } globalThis.collect=collect;",
                ParseOptions::script(),
            )
            .expect("dynamic enumeration parses");
            scriptw_aot_wasm::emit(&lower_with_host_surface_policy(
                &parsed,
                HostSurfacePolicy::Test262,
            ))
            .expect("shared enumeration emits")
        })
        .unwrap()
        .join()
        .unwrap();
    for bytes in [
        artifact.runtime().unwrap().bytes(),
        artifact.bytes.as_slice(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .expect("the actual R/P cursor and copy instruction validate");
    }
    let runtime = Module::read(artifact.runtime().unwrap().bytes(), true);
    let program = Module::read(&artifact.bytes, false);
    let (advance, body) = runtime.named("helper::for_in_advance");
    let imported = program.imported_runtime_function(&runtime, advance);
    assert_eq!(
        program.function_type(imported),
        runtime.function_type(advance)
    );

    // Derive the accepted key table from the real cursor signature, without
    // assuming that R exports private snapshot witnesses or hardcoded indices.
    let ValType::Ref(cursor) = runtime.function_type(advance).params()[0] else {
        panic!("the cursor parameter is a typed reference");
    };
    let CompositeInnerType::Struct(cursor) = &runtime.types[concrete(cursor) as usize]
        .composite_type
        .inner
    else {
        panic!("the cursor is a GC record");
    };
    let tables = cursor
        .fields
        .iter()
        .filter_map(|field| {
            let StorageType::Val(ValType::Ref(reference)) = field.element_type else {
                return None;
            };
            if !reference.is_nullable() {
                return None;
            }
            let index = concrete(reference);
            if matches!(
                runtime.types[index as usize].composite_type.inner,
                CompositeInnerType::Array(_)
            ) {
                return Some(index);
            }
            None
        })
        .collect::<Vec<_>>();
    assert_eq!(tables.len(), 1, "the cursor has one nullable key snapshot");
    let keys = tables[0];
    let CompositeInnerType::Array(table) = &runtime.types[keys as usize].composite_type.inner
    else {
        unreachable!();
    };
    let StorageType::Val(ValType::Ref(snapshot)) = table.0.element_type else {
        panic!("the key table contains stored snapshot references");
    };
    assert!(!snapshot.is_nullable());
    let CompositeInnerType::Struct(snapshot) = &runtime.types[concrete(snapshot) as usize]
        .composite_type
        .inner
    else {
        panic!("each key snapshot is a GC record");
    };
    assert!(!snapshot.fields.is_empty());
    assert!(
        snapshot.fields.iter().all(|field| !field.mutable),
        "sharing old key snapshots cannot mutate their payloads"
    );
    assert_eq!(body.iter().filter(|op| matches!(op, Operator::ArrayNew { array_type_index } if *array_type_index == keys)).count(), 1,
        "append allocates one new table filled with the newly boxed key");
    assert_eq!(body.iter().filter(|op| matches!(op, Operator::ArrayCopy { array_type_index_dst, array_type_index_src } if *array_type_index_dst == keys && *array_type_index_src == keys)).count(), 1,
        "the accepted old prefix is copied with its real GC element type");
    assert_eq!(body.iter().filter(|op| matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == keys)).count(), 0,
        "append neither reboxes and rewrites old entries nor mutates published tables");
}
