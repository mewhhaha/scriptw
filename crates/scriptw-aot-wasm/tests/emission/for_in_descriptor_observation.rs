//! ForIn consumes descriptor predicates without materializing an ordinary snapshot.
use crate::property_read_plans::Module;
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
            scriptw_aot_wasm::emit(&lower_with_host_surface_policy(
                &parsed,
                HostSurfacePolicy::Test262,
            )).expect("shared descriptor observation emits")
        }).unwrap().join().unwrap();
    for bytes in [
        artifact.runtime().unwrap().bytes(),
        artifact.bytes.as_slice(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .expect("actual R/P modules validate");
    }
    let runtime = Module::read(artifact.runtime().unwrap().bytes(), true);
    let (ordinary, _) = runtime.named("helper::ordinary_own_descriptor");
    let ValType::Ref(descriptor) = runtime.function_type(ordinary).results()[0] else {
        panic!("the ordinary lookup returns its nullable descriptor reference");
    };
    let HeapType::Concrete(UnpackedIndex::Module(descriptor)) = descriptor.heap_type() else {
        panic!("the descriptor uses the canonical concrete type");
    };
    let (exotic, _) = runtime.named("helper::object_get_own_property");
    let (_, advance) = runtime.named("helper::for_in_advance");
    for (helper, expected) in [(ordinary, 2), (exotic, 1)] {
        assert_eq!(
            advance
                .iter()
                .filter(|op| matches!(op,
                Operator::Call { function_index } if *function_index == helper))
                .count(),
            expected,
            "ForIn retains ordinary-object, Array-header and exotic lookup boundaries",
        );
    }
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
