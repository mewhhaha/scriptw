//! The actual cursor uses canonical hashed Set storage in both R and P.
use crate::property_read_plans::Module;
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::{HostSurfacePolicy, lower_with_host_surface_policy};
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
fn for_in_defers_the_canonical_set_until_compact_keys_need_generic_observation() {
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
    assert_eq!(cursor.fields.len(), 6);
    let keys = reference(cursor.fields[1].element_type);
    assert!(keys.is_nullable());
    let keys = concrete(keys);
    assert!(matches!(
        runtime.types[keys as usize].composite_type.inner,
        CompositeInnerType::Array(_)
    ));
    let visited = reference(cursor.fields[3].element_type);
    assert!(visited.is_nullable());
    assert!(
        cursor.fields[3].mutable,
        "the canonical Set is published lazily on generic fallback"
    );
    let visited_index = concrete(visited);
    let (grow, _) = runtime.named("helper::collection_prepare_set");
    assert_eq!(
        runtime.function_type(grow).params()[0],
        ValType::Ref(RefType::new(false, visited.heap_type()).unwrap()),
        "the visited owner is exactly the canonical Set helper parameter type"
    );
    assert_eq!(
        body.iter()
            .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == grow))
            .count(),
        3,
        "generic insertion and both prefix seed paths share canonical growth"
    );
    let set = structure(&runtime, visited_index);
    assert_eq!(set.fields.len(), 8);
    let entries = concrete(reference(set.fields[1].element_type));
    let buckets = concrete(reference(set.fields[4].element_type));
    assert!(body.iter().any(|op| matches!(op, Operator::ArrayGet { array_type_index } if *array_type_index == buckets)),
        "membership traverses the live canonical hash buckets");
    assert_eq!(body.iter().filter(|op| matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == entries)).count(), 3,
        "generic insertion and both prefix seed paths publish canonical entries");
    assert!(
        !body
            .iter()
            .any(|op| matches!(op, Operator::ArrayCopy { .. })),
        "fallback seeds keys through the canonical Set without copying Set storage"
    );
    assert!(body.iter().any(|op| matches!(op, Operator::StructSet { struct_type_index, field_index: 3 } if *struct_type_index == cursor_index)),
        "advance publishes a lazy canonical Set for generic enumeration");
    let compact = reference(cursor.fields[4].element_type);
    assert!(compact.is_nullable());
    let compact = concrete(compact);
    let shape = concrete(reference(
        structure(&runtime, compact).fields[0].element_type,
    ));
    let shape = structure(&runtime, shape);
    assert_eq!(shape.fields.len(), 3);
    assert!(
        !shape.fields[2].mutable,
        "OwnKeys order is proven once at shape creation"
    );
    let compact_keys = concrete(reference(shape.fields[0].element_type));
    let key_read = body.iter().position(|op| matches!(op, Operator::ArrayGet { array_type_index } if *array_type_index == compact_keys)).unwrap();
    let first_set = body.iter().position(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == visited_index)).unwrap();
    assert!(
        key_read < first_set,
        "the compact key yield precedes all fallback Set allocation"
    );
    assert!(
        body[..key_read]
            .iter()
            .any(|op| matches!(op, Operator::RefEq)),
        "a live storage identity guard dominates the compact key read"
    );
    assert!(
        body[key_read..first_set]
            .iter()
            .any(|op| matches!(op, Operator::Br { .. })),
        "the compact key path exits before generic allocation"
    );
    assert_eq!(
        cursor.fields[5].element_type,
        StorageType::Val(ValType::I64)
    );
    assert!(cursor.fields[5].mutable);
    let first_mark = body.iter().position(|op| matches!(op,
        Operator::StructSet { struct_type_index, field_index: 5 } if *struct_type_index == cursor_index)).unwrap();
    let (exotic, _) = runtime.named("helper::object_get_own_property");
    let descriptor_read = body
        .iter()
        .position(|op| {
            matches!(op,
        Operator::Call { function_index } if *function_index == exotic)
        })
        .unwrap();
    assert!(
        descriptor_read < first_mark,
        "the live descriptor observation precedes first-phase found-key marking"
    );
    assert!(
        body[..first_mark]
            .iter()
            .any(|op| matches!(op, Operator::I64Or)),
        "the first snapshot accumulates presence bits without allocating entries"
    );
    assert!(
        body.windows(2)
            .any(|ops| matches!(ops, [Operator::I32Const { value: 64 }, Operator::I32GtU])),
        "snapshots above 64 entries select canonical Set storage before shifting"
    );
    assert!(
        body.windows(4).any(|ops| matches!(
            ops,
            [
                Operator::I64Shl,
                Operator::I64And,
                Operator::I64Eqz,
                Operator::I32Eqz,
            ]
        )),
        "presence tests use nonzero bits, including the signed high bit at index 63"
    );
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
    assert_eq!(source.iter().filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == visited_index)).count(), 0,
        "source cursor creation allocates no visited Set");
}
