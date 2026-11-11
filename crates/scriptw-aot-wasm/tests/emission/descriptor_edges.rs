//! Data merges keep immutable accessor edges instead of rebuilding wrappers.
use crate::{
    ordinary_receiver_definition::concrete_if_arms,
    property_read_plans::{Module, calls},
};
use scriptw_aot_wasm::{
    GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy, emit_with_rooted_snapshot,
};
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::lower;
use wasmparser::{
    CompositeInnerType, HeapType, Operator, StorageType, UnpackedIndex, ValType, Validator,
    WasmFeatures,
};

fn concrete(value: ValType) -> u32 {
    let ValType::Ref(reference) = value else {
        panic!("typed GC edge");
    };
    let HeapType::Concrete(UnpackedIndex::Module(index)) = reference.heap_type() else {
        panic!("concrete GC edge");
    };
    assert!(!reference.is_nullable());
    index
}

fn edge_local(body: &[Operator<'_>], descriptor: u32, field: u32) -> u32 {
    body.windows(2)
        .find_map(|ops| match ops {
            [
                Operator::StructGet {
                    struct_type_index,
                    field_index,
                },
                Operator::LocalSet { local_index },
            ] if *struct_type_index == descriptor && *field_index == field => Some(*local_index),
            _ => None,
        })
        .expect("the original descriptor edge is retained directly in a rooted GC local")
}

#[test]
fn data_merge_boxes_only_supplied_value_and_reuses_complete_accessor_edges() {
    std::thread::Builder::new().name("descriptor-edges".into()).stack_size(64 * 1024 * 1024).spawn(|| {
        let ir = lower(&parse("export function literal(key, value) { return { [key]: value }; }", ParseOptions::module()).unwrap());
        let artifact = emit_with_rooted_snapshot(&ir, PromiseRejectionPolicy::Ignore, &scriptw_intl::IntlCompilationProfile::Minimal).unwrap();
        let runtime_bytes = artifact.runtime().unwrap().bytes();
        for bytes in [runtime_bytes, artifact.bytes.as_slice()] {
            Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
        }
        let runtime = Module::read(runtime_bytes, true);
        let program = Module::read(&artifact.bytes, true);
        let (resolve, _) = runtime.named("helper::property_value_resolve");
        let stored = concrete(runtime.function_type(resolve).results()[0]);
        let CompositeInnerType::Struct(wrapper) = &runtime.types[stored as usize].composite_type.inner else { panic!("StoredValue is an actual GC struct"); };
        assert!(wrapper.fields.iter().all(|field| !field.mutable), "retained wrapper edges are immutable");
        let descriptor = runtime.witness_type(GcSnapshotLayout::Descriptor);
        let CompositeInnerType::Struct(record) = &runtime.types[descriptor as usize].composite_type.inner else { panic!("complete descriptor is an actual GC struct"); };
        for field in [GcSnapshotField::DescriptorGet, GcSnapshotField::DescriptorSet] {
            let field = &record.fields[field.index() as usize];
            let StorageType::Val(value) = field.element_type else { panic!("descriptor owns a reference edge"); };
            assert_eq!(concrete(value), stored);
            assert!(field.mutable, "descriptor records retain independent mutable edges");
        }
        let object = runtime.witness_type(GcSnapshotLayout::Object);
        let (define, body) = runtime.named("helper::object_define_data");
        let (_, literal) = program.named("js::literal#");
        assert!(calls(literal, program.imported_runtime_function(&runtime, define)), "the fixture reaches the actual shared definition owner");
        let arms = concrete_if_arms(body, object).into_iter().filter(|arm| arm.iter().any(|op| matches!(op,
            Operator::StructGet { struct_type_index, field_index }
                if *struct_type_index == object && *field_index == GcSnapshotField::ObjectExtensible.index()))).collect::<Vec<_>>();
        assert_eq!(arms.len(), 1, "one actual ordinary data definition");
        let arm = arms[0];
        let completions = arm.iter().enumerate().filter_map(|(i, op)| matches!(op,
            Operator::StructNew { struct_type_index } if *struct_type_index == descriptor).then_some(i)).collect::<Vec<_>>();
        assert_eq!(completions.len(), 2, "both descriptor kinds retain fresh complete merge records");
        let before_output = &arm[..completions[0]];
        assert_eq!(before_output.iter().filter(|op| matches!(op,
            Operator::StructNew { struct_type_index } | Operator::StructNewDefault { struct_type_index }
                if *struct_type_index == stored)).count(), 1,
            "before constructing the merge result only the supplied data value is boxed; omitted accessor fields allocate nothing");
        let getter = edge_local(before_output, descriptor, GcSnapshotField::DescriptorGet.index());
        let setter = edge_local(before_output, descriptor, GcSnapshotField::DescriptorSet.index());
        assert_ne!(getter, setter, "each retained accessor edge has its own live root");
        for output in completions {
            assert!(matches!(&arm[output - 6..output], [
                Operator::LocalGet { .. }, Operator::RefAsNonNull,
                Operator::LocalGet { local_index: get }, Operator::RefAsNonNull,
                Operator::LocalGet { local_index: set }, Operator::RefAsNonNull
            ] if *get == getter && *set == setter),
                "both new complete descriptors consume the retained getter/setter edges directly");
        }
    }).unwrap().join().unwrap();
}
