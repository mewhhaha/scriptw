//! Read-only descriptor queries cannot publish a descriptor backing for an Array.
use crate::{property_read_plans::Module, scalar_masked_rows::type_group_sizes};
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::{HostSurfacePolicy, lower_with_host_surface_policy};
use wasmparser::{
    CompositeInnerType, HeapType, Operator, UnpackedIndex, ValType, Validator, WasmFeatures,
};

fn concrete(value: ValType) -> u32 {
    let ValType::Ref(reference) = value else {
        panic!("typed GC reference")
    };
    let HeapType::Concrete(UnpackedIndex::Module(index)) = reference.heap_type() else {
        panic!("actual module-owned GC type")
    };
    index
}

fn branches(body: &[Operator<'_>]) -> Vec<(usize, usize, usize)> {
    let mut stack = Vec::new();
    let mut branches = Vec::new();
    for (index, operator) in body.iter().enumerate() {
        match operator {
            Operator::If { .. } => stack.push((true, index, None)),
            Operator::Block { .. } | Operator::Loop { .. } | Operator::TryTable { .. } => {
                stack.push((false, index, None));
            }
            Operator::Else => stack.last_mut().unwrap().2 = Some(index),
            Operator::End => {
                if let Some((true, start, Some(other))) = stack.pop() {
                    branches.push((start, other, index));
                }
            }
            _ => {}
        }
    }
    branches
}

#[test]
fn descriptor_observation_preserves_packed_backings_and_reuses_immutable_value_edges() {
    std::thread::Builder::new().name("packed-descriptors".into()).stack_size(64 * 1024 * 1024).spawn(|| {
        let source = parse("function copy(source) { return {...source}; } globalThis.copy=copy;", ParseOptions::script()).unwrap();
        let artifact = scriptw_aot_wasm::emit(&lower_with_host_surface_policy(&source, HostSurfacePolicy::Test262)).unwrap();
        let bytes = artifact.runtime().unwrap().bytes();
        for module in [bytes, artifact.bytes.as_slice()] {
            Validator::new_with_features(WasmFeatures::all()).validate_all(module).unwrap();
        }
        let runtime = Module::read(bytes, true);
        let program = Module::read(&artifact.bytes, true);
        assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
        let runtime_groups = type_group_sizes(bytes);
        let program_groups = type_group_sizes(&artifact.bytes);
        assert_eq!(&program_groups[..runtime_groups.len()], &runtime_groups);
        let (publish, publish_body) = runtime.named("helper::array_indexed_publish");
        let signature = runtime.function_type(publish);
        let array = concrete(signature.params()[0]);
        let descriptor = concrete(signature.params()[2]);
        let CompositeInnerType::Struct(layout) = &runtime.types[array as usize].composite_type.inner else { panic!("canonical Array struct") };
        assert_eq!(layout.fields.len(), 10);
        assert_eq!(layout.fields[6].element_type.unpack(), ValType::I32, "MODE follows the actual Array declaration");
        let dense = concrete(layout.fields[5].element_type.unpack());
        let numbers = concrete(layout.fields[7].element_type.unpack());
        let values = concrete(layout.fields[9].element_type.unpack());
        let CompositeInnerType::Array(dense_layout) = &runtime.types[dense as usize].composite_type.inner else { panic!("descriptor backing") };
        assert_eq!(concrete(dense_layout.0.element_type.unpack()), descriptor);
        let CompositeInnerType::Array(value_layout) = &runtime.types[values as usize].composite_type.inner else { panic!("packed immutable value edges") };
        let (resolve, _) = runtime.named("helper::property_value_resolve");
        let stored = concrete(runtime.function_type(resolve).results()[0]);
        assert_eq!(concrete(value_layout.0.element_type.unpack()), stored);
        for name in ["helper::object_get_own_property", "helper::object_has_property", "helper::for_in_advance"] {
            let (_, body) = runtime.named(name);
            assert!(!body.iter().any(|op| matches!(op, Operator::StructSet { struct_type_index, .. } if *struct_type_index == array)),
                "{name} cannot change any actual Array field while observing descriptors");
            assert!(!body.iter().any(|op| matches!(op, Operator::ArrayNew { array_type_index } | Operator::ArrayNewDefault { array_type_index } if *array_type_index == dense)),
                "{name} cannot allocate a whole mutable descriptor backing");
        }
        assert!(publish_body.iter().any(|op| matches!(op, Operator::StructSet { struct_type_index, field_index: 6 } if *struct_type_index == array)),
            "real descriptor publication still owns storage transitions");
        assert!(publish_body.iter().any(|op| matches!(op, Operator::ArrayNew { array_type_index } | Operator::ArrayNewDefault { array_type_index } if *array_type_index == dense)),
            "accessor/nondefault publication still materializes packed storage");
        let (own, body) = runtime.named("helper::object_get_own_property");
        let imported = program.imported_runtime_function(&runtime, own);
        assert_eq!(program.function_type(imported), runtime.function_type(own));
        assert_eq!(runtime.function_type(own).results().len(), 6, "unchanged whole Completion plus descriptor ABI");
        let selected = branches(body).into_iter().filter(|(start, other, end)| {
            body[start + 1..*other].iter().any(|op| matches!(op, Operator::ArrayGet { array_type_index } if *array_type_index == values)) &&
            body[other + 1..*end].iter().any(|op| matches!(op, Operator::ArrayGet { array_type_index } if *array_type_index == numbers))
        }).min_by_key(|(start, _, end)| end - start).expect("one canonical packed Values/Numbers decoder");
        let (start, other, end) = selected;
        assert!(!body[start + 1..other].iter().any(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == stored)),
            "PackedValues snapshots reuse the exact immutable StoredValue, without reboxing");
        assert_eq!(body[other + 1..end].iter().filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == stored)).count(), 1,
            "only a present packed Number requires a new immutable box at the descriptor boundary");
    }).unwrap().join().unwrap();
}
