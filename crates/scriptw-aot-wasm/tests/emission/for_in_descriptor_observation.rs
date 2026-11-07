//! ForIn consumes descriptor predicates without materializing an ordinary snapshot.
use crate::{
    ordinary_receiver_definition::concrete_if_arms,
    property_read_plans::{calls, Module},
    scalar_masked_rows::type_group_sizes,
};
use scriptw_aot_wasm::{
    emit_with_rooted_snapshot, GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy,
};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
use wasmparser::{HeapType, Operator, UnpackedIndex, ValType, Validator, WasmFeatures};

#[test]
fn for_in_reads_flags_without_cloning_descriptors_and_keeps_exotic_lookup() {
    let artifact = std::thread::Builder::new()
        .name("for-in-descriptor-observation".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(
                "function collect(source) { var keys=''; for (var key in source) keys+=key; return keys; } globalThis.collect=collect;",
                ParseOptions::script(),
            ).expect("dynamic enumeration parses");
            emit_with_rooted_snapshot(
                &lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262),
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            ).expect("shared descriptor observation emits")
        }).unwrap().join().unwrap();
    for bytes in [
        artifact.runtime().unwrap().bytes(),
        artifact.bytes.as_slice(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .expect("actual R/P modules validate");
    }
    let runtime_bytes = artifact.runtime().unwrap().bytes();
    let runtime = Module::read(runtime_bytes, true);
    let program = Module::read(&artifact.bytes, true);
    assert!(program.types.len() >= runtime.types.len());
    assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
    let runtime_groups = type_group_sizes(runtime_bytes);
    let program_groups = type_group_sizes(&artifact.bytes);
    assert!(program_groups.len() >= runtime_groups.len());
    assert_eq!(&program_groups[..runtime_groups.len()], &runtime_groups);
    let (ordinary, _) = runtime.named("helper::ordinary_own_descriptor");
    let ValType::Ref(descriptor) = runtime.function_type(ordinary).results()[0] else {
        panic!("the ordinary lookup returns its nullable descriptor reference");
    };
    let HeapType::Concrete(UnpackedIndex::Module(descriptor)) = descriptor.heap_type() else {
        panic!("the descriptor uses the canonical concrete type");
    };
    let (exotic, _) = runtime.named("helper::object_get_own_property");
    let (advance_index, advance) = runtime.named("helper::for_in_advance");
    let imported = program.imported_runtime_function(&runtime, advance_index);
    assert_eq!(
        program.function_type(imported),
        runtime.function_type(advance_index)
    );
    for (helper, expected) in [(ordinary, 1), (exotic, 1)] {
        assert_eq!(
            advance
                .iter()
                .filter(|op| matches!(op,
                Operator::Call { function_index } if *function_index == helper))
                .count(),
            expected,
            "only Array-header and exotic lookup retain descriptor helper calls",
        );
    }
    let object = runtime.witness_type(GcSnapshotLayout::Object);
    let compact = runtime.witness_type(GcSnapshotLayout::CompactStorage);
    let shape = runtime.witness_type(GcSnapshotLayout::CompactShape);
    let (find, _) = runtime.named("helper::ordinary_property_find");
    let (intrinsic_find, _) = runtime.named("helper::intrinsic_property_find");
    let arms = concrete_if_arms(advance, object)
        .into_iter()
        .filter(|arm| arm.iter().any(|op| matches!(op,
            Operator::StructGet { struct_type_index, field_index }
                if *struct_type_index == descriptor && *field_index == GcSnapshotField::DescriptorFlags.index())))
        .collect::<Vec<_>>();
    assert_eq!(
        arms.len(),
        1,
        "one actual ordinary scalar enumerability arm"
    );
    let arm = arms[0];
    assert!(
        arm.iter().any(|op| matches!(op,
        Operator::RefTestNonNull { hty: HeapType::Concrete(UnpackedIndex::Module(index)) }
            if *index == compact)),
        "compact storage is queried directly"
    );
    assert!(arm.iter().any(|op| matches!(op,
        Operator::StructGet { struct_type_index, field_index }
            if *struct_type_index == shape && *field_index == GcSnapshotField::CompactAttributes.index())),
        "compact enumeration reads its live shared attributes");
    assert!(
        calls(arm, find) && calls(arm, intrinsic_find),
        "dictionary and intrinsic storage retain their canonical scalar probes"
    );
    assert!(
        !calls(arm, ordinary) && !calls(arm, exotic),
        "ordinary scalar queries never request a descriptor snapshot"
    );
    assert!(
        !arm.iter().any(|op| matches!(op,
        Operator::StructNew { struct_type_index } if *struct_type_index == descriptor)),
        "ordinary presence/enumerability never synthesizes a descriptor"
    );
    assert!(!arm.iter().any(|op| matches!(op,
        Operator::StructGet { struct_type_index, field_index }
            if *struct_type_index == descriptor && *field_index != GcSnapshotField::DescriptorFlags.index())),
        "ordinary scalar queries never decode values or accessors");
    // Packed Array indices and Array length still materialize descriptors in
    // their storage lookup. Scalar observation must not add either of the two
    // data/accessor snapshot constructions to either ordinary-storage branch.
    assert_eq!(
        advance
            .iter()
            .filter(|op| matches!(op,
            Operator::StructNew { struct_type_index } if *struct_type_index == descriptor))
            .count(),
        2,
        "only the packed-index and Array-length storage lookups create descriptors",
    );
    assert!(
        !advance.iter().any(|op| matches!(op,
            Operator::StructGet { struct_type_index, field_index }
                if *struct_type_index == descriptor && *field_index != 0)),
        "scalar observation cannot read value/getter/setter fields or clone a descriptor",
    );
    assert!(
        advance.iter().any(|op| matches!(op,
            Operator::StructGet { struct_type_index, field_index: 0 }
                if *struct_type_index == descriptor)),
        "presence/enumerability come from the existing descriptor observation",
    );
    let (_, read) = runtime.named("helper::object_read");
    assert!(
        read.iter().any(|op| matches!(op,
            Operator::StructNew { struct_type_index } if *struct_type_index == descriptor)),
        "consumers that retain descriptors across effects still copy their snapshots",
    );
}
