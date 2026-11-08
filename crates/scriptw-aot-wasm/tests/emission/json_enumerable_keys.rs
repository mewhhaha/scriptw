//! JSON snapshots enumerable keys without observing ordinary property values.
use crate::{
    ordinary_receiver_definition::concrete_if_arms,
    property_read_plans::{calls, Module},
};
use scriptw_aot_wasm::{
    emit_with_rooted_snapshot, GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy,
};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{Operator, Validator, WasmFeatures};

#[test]
fn stringify_and_reviver_read_ordinary_enumerability_without_descriptor_snapshots() {
    std::thread::Builder::new()
        .name("json-enumerable-keys".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let ir = lower(&parse(
                "export function stringify(value) { return JSON.stringify(value); } export function parse(value, revive) { return JSON.parse(value, revive); }",
                ParseOptions::module(),
            ).unwrap());
            let artifact = emit_with_rooted_snapshot(
                &ir, PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            ).unwrap();
            let runtime_bytes = artifact.runtime().unwrap().bytes();
            for bytes in [runtime_bytes, artifact.bytes.as_slice()] {
                Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
            }
            let runtime = Module::read(runtime_bytes, true);
            let program = Module::read(&artifact.bytes, true);
            assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
            let object = runtime.witness_type(GcSnapshotLayout::Object);
            let shape = runtime.witness_type(GcSnapshotLayout::CompactShape);
            let descriptor = runtime.witness_type(GcSnapshotLayout::Descriptor);
            let (ordinary, _) = runtime.named("helper::ordinary_own_descriptor");
            let (exotic, _) = runtime.named("helper::object_get_own_property");
            let (find, _) = runtime.named("helper::ordinary_property_find");
            let (intrinsic_find, _) = runtime.named("helper::intrinsic_property_find");
            for name in ["helper::json_stringify_value", "builtin::JSON.parse"] {
                let (_, body) = runtime.named(name);
                let arms = concrete_if_arms(body, object).into_iter().filter(|arm| {
                    calls(arm, find) && calls(arm, intrinsic_find) && arm.iter().any(|op| matches!(op,
                        Operator::StructGet { struct_type_index, field_index }
                            if *struct_type_index == shape && *field_index == GcSnapshotField::CompactAttributes.index()))
                }).collect::<Vec<_>>();
                assert_eq!(arms.len(), 1, "{name} has one ordinary scalar key-enumeration arm");
                let arm = arms[0];
                assert!(!calls(arm, ordinary) && !calls(arm, exotic),
                    "{name} reads ordinary live flags without descriptor lookup helpers");
                assert!(!arm.iter().any(|op| matches!(op,
                    Operator::StructNew { struct_type_index } | Operator::StructNewDefault { struct_type_index }
                        if *struct_type_index == descriptor)), "{name} allocates no ordinary descriptor snapshot");
                assert!(!arm.iter().any(|op| matches!(op,
                    Operator::StructGet { struct_type_index, field_index }
                        if *struct_type_index == descriptor && *field_index != GcSnapshotField::DescriptorFlags.index())),
                    "{name} never reads ordinary value/getter/setter fields while snapshotting keys");
                assert!(calls(body, exotic), "{name} retains whole-completion exotic descriptor acquisition");
            }
        }).unwrap().join().unwrap();
}
