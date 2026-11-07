//! The actual cursor uses canonical hashed Set storage in both R and P.
use crate::property_read_plans::Module;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
use wasmparser::{
    CompositeInnerType, HeapType, Operator, RefType, StorageType, StructType, UnpackedIndex,
    ValType, Validator, WasmFeatures,
};

fn concrete(reference: RefType) -> u32 {
    let HeapType::Concrete(UnpackedIndex::Module(index)) = reference.heap_type() else {
        panic!("the actual signature uses a concrete GC type");
    };
    index
}

fn reference(field: StorageType) -> RefType {
    let StorageType::Val(ValType::Ref(reference)) = field else {
        panic!("the actual cursor edge remains a typed GC reference");
    };
    reference
}

fn structure<'a>(module: &'a Module<'_>, index: u32) -> &'a StructType {
    let CompositeInnerType::Struct(record) = &module.types[index as usize].composite_type.inner
    else {
        panic!("the actual edge names a GC struct");
    };
    record
}

#[test]
fn for_in_uses_the_canonical_set_growth_helper_and_never_copies_a_visited_prefix() {
    let artifact = std::thread::Builder::new()
        .name("for-in-visited-set".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(
                "function collect(source) { var keys=''; for (var key in source) keys+=key; return keys; } globalThis.collect=collect;",
                ParseOptions::script(),
            ).expect("dynamic enumeration parses");
            scriptw_aot_wasm::emit(&lower_with_host_surface_policy(
                &parsed,
                HostSurfacePolicy::Test262,
            )).expect("shared enumeration emits")
        }).unwrap().join().unwrap();
    for bytes in [
        artifact.runtime().unwrap().bytes(),
        artifact.bytes.as_slice(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .expect("actual R/P cursor and canonical Set graph validate");
    }
    let runtime = Module::read(artifact.runtime().unwrap().bytes(), true);
    let program = Module::read(&artifact.bytes, true);
    assert_eq!(
        &runtime.types[..],
        &program.types[..runtime.types.len()],
        "the complete recursive runtime type prefix agrees"
    );
    let (advance, body) = runtime.named("helper::for_in_advance");
    let imported = program.imported_runtime_function(&runtime, advance);
    assert_eq!(
        program.function_type(imported),
        runtime.function_type(advance)
    );
    let ValType::Ref(cursor) = runtime.function_type(advance).params()[0] else {
        panic!("the cursor parameter is a typed reference");
    };
    let cursor_index = concrete(cursor);
    let cursor = structure(&runtime, cursor_index);
    assert_eq!(cursor.fields.len(), 4);
    let keys = reference(cursor.fields[1].element_type);
    assert!(keys.is_nullable());
    let keys = concrete(keys);
    assert!(matches!(
        runtime.types[keys as usize].composite_type.inner,
        CompositeInnerType::Array(_)
    ));
    let visited = reference(cursor.fields[3].element_type);
    assert!(!visited.is_nullable());
    assert!(
        !cursor.fields[3].mutable,
        "Set identity is sealed at creation"
    );
    let visited_index = concrete(visited);
    let (grow, _) = runtime.named("helper::collection_prepare_set");
    assert_eq!(
        runtime.function_type(grow).params()[0],
        ValType::Ref(visited),
        "the visited owner is exactly the canonical Set helper parameter type"
    );
    assert_eq!(
        body.iter()
            .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == grow))
            .count(),
        1,
        "insertion uses the one existing geometric-growth owner"
    );
    let set = structure(&runtime, visited_index);
    assert_eq!(set.fields.len(), 8);
    let entries = concrete(reference(set.fields[1].element_type));
    let buckets = concrete(reference(set.fields[4].element_type));
    assert!(body.iter().any(|op| matches!(op, Operator::ArrayGet { array_type_index } if *array_type_index == buckets)),
        "membership traverses the live canonical hash buckets");
    assert_eq!(body.iter().filter(|op| matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == entries)).count(), 1,
        "one new canonical Set entry is published per admitted missing key");
    assert!(
        !body.iter().any(|op| match op {
            Operator::ArrayCopy { .. } => true,
            Operator::ArrayNew { array_type_index } => *array_type_index == keys,
            _ => false,
        }),
        "advance no longer allocates or copies an exact visited prefix"
    );
    assert!(!body.iter().any(|op| matches!(op, Operator::StructSet { struct_type_index, field_index: 3 } if *struct_type_index == cursor_index)),
        "advance cannot replace the retained Set root");
    for name in [
        "builtin::Set",
        "builtin::Set.prototype.has",
        "builtin::Set.prototype.add",
    ] {
        let (public, _) = runtime.named(name);
        assert!(
            !body.iter().any(|op| matches!(op,
            Operator::Call { function_index } | Operator::ReturnCall { function_index }
                | Operator::RefFunc { function_index } if *function_index == public)),
            "private advancement never invokes or materializes {name}"
        );
    }
    let (_, source) = program.named("js::collect#");
    assert_eq!(source.iter().filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == visited_index)).count(), 1,
        "the source cursor allocates its one unpublished canonical Set");
}
