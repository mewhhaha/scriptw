//! Boolean descriptor consumers need no snapshot or fresh argument-tail seed.
use crate::{
    ordinary_receiver_definition::concrete_if_arms,
    property_read_plans::{Module, calls},
};
use scriptw_aot_wasm::{
    GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy, WasmRuntimeValueTag,
    emit_with_rooted_snapshot,
};
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::{HostSurfacePolicy, lower_with_host_surface_policy};
use wasmparser::{
    HeapType, Operator, Parser, Payload, TypeRef, UnpackedIndex, ValType, Validator, WasmFeatures,
};

fn stored_undefined_global(bytes: &[u8], stored: u32) -> u32 {
    let mut next = 0;
    let mut found = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::ImportSection(reader) => {
                next += reader
                    .into_imports()
                    .filter(|item| matches!(item.as_ref().unwrap().ty, TypeRef::Global(_)))
                    .count() as u32;
            }
            Payload::GlobalSection(reader) => {
                for global in reader {
                    let global = global.unwrap();
                    let ops = global
                        .init_expr
                        .get_operators_reader()
                        .into_iter()
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap();
                    if matches!(ops.as_slice(), [
                        Operator::I64Const { value: 0 },
                        Operator::I32Const { value },
                        Operator::RefNull { hty: HeapType::Abstract { .. } },
                        Operator::StructNew { struct_type_index },
                        Operator::End,
                    ] if *value == WasmRuntimeValueTag::Undefined as i32 && *struct_type_index == stored)
                    {
                        assert!(!global.ty.mutable, "the shared seed cannot be replaced");
                        let ValType::Ref(reference) = global.ty.content_type else {
                            panic!("the seed is a GC reference");
                        };
                        assert!(
                            !reference.is_nullable(),
                            "declaration initializes the seed before any callable executes"
                        );
                        found.push(next);
                    }
                    next += 1;
                }
            }
            _ => {}
        }
    }
    assert_eq!(found.len(), 1, "one declaration-owned undefined snapshot");
    found[0]
}

#[test]
fn predicates_consume_scalar_flags_and_call_tail_uses_declared_undefined() {
    std::thread::Builder::new()
        .name("own-descriptor-predicates".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(
                "function own(object,key) { return Object.prototype.hasOwnProperty.call(object,key); } globalThis.own=own;",
                ParseOptions::script(),
            ).unwrap();
            let artifact = emit_with_rooted_snapshot(
                &lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262),
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            ).unwrap();
            let bytes = artifact.runtime().unwrap().bytes();
            for bytes in [bytes, artifact.bytes.as_slice()] {
                Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
            }
            let runtime = Module::read(bytes, true);
            let object = runtime.witness_type(GcSnapshotLayout::Object);
            let descriptor = runtime.witness_type(GcSnapshotLayout::Descriptor);
            let stored = runtime.witness_type(GcSnapshotLayout::Stored);
            let shape = runtime.witness_type(GcSnapshotLayout::CompactShape);
            let (ordinary, _) = runtime.named("helper::ordinary_own_descriptor");
            let (exotic, _) = runtime.named("helper::object_get_own_property");
            let (dictionary, _) = runtime.named("helper::ordinary_property_find");
            let (intrinsic, _) = runtime.named("helper::intrinsic_property_find");
            for name in ["builtin::Object.hasOwn", "builtin::Object.prototype.hasOwnProperty", "builtin::Object.prototype.propertyIsEnumerable"] {
                let (_, body) = runtime.named(name);
                let arms = concrete_if_arms(body, object).into_iter().filter(|arm| arm.iter().any(|op| matches!(op,
                    Operator::StructGet { struct_type_index, field_index }
                        if *struct_type_index == shape && *field_index == GcSnapshotField::CompactAttributes.index()))).collect::<Vec<_>>();
                assert_eq!(arms.len(), 1, "{name} has one actual ordinary scalar observation");
                let arm = arms[0];
                assert!(calls(arm, dictionary) && calls(arm, intrinsic), "{name} retains canonical dictionary and intrinsic lookup");
                assert!(!calls(arm, ordinary) && !calls(arm, exotic), "{name} does not materialize an ordinary descriptor");
                assert!(!arm.iter().any(|op| matches!(op,
                    Operator::StructNew { struct_type_index } if *struct_type_index == descriptor || *struct_type_index == stored)), "{name} needs neither a descriptor nor a value snapshot");
                assert!(!arm.iter().any(|op| matches!(op,
                    Operator::StructGet { struct_type_index, field_index }
                        if *struct_type_index == descriptor && *field_index != GcSnapshotField::DescriptorFlags.index())), "{name} reads no dictionary value/getter/setter");
                assert!(calls(body, ordinary) && calls(body, exotic), "{name} retains Array-header and whole exotic descriptor fallback");
                assert_eq!(body.iter().filter(|op| matches!(op,
                    Operator::StructNew { struct_type_index } if *struct_type_index == descriptor)).count(), 2,
                    "{name} only constructs packed-index and virtual Array-length descriptors");
            }
            let (_, read) = runtime.named("helper::object_read");
            assert!(read.iter().any(|op| matches!(op,
                Operator::StructNew { struct_type_index } if *struct_type_index == descriptor)),
                "Proxy invariant consumers still retain independent descriptor snapshots");

            let seed = stored_undefined_global(bytes, stored);
            let (call, _) = runtime.named("helper::function_call");
            let ValType::Ref(arguments) = runtime.function_type(call).params()[6] else { panic!("Call accepts the canonical argument vector"); };
            let HeapType::Concrete(UnpackedIndex::Module(arguments)) = arguments.heap_type() else { panic!("the argument vector has a concrete GC type"); };
            let (_, call) = runtime.named("builtin::Function.prototype.call");
            assert_eq!(call.windows(6).filter(|ops| matches!(ops, [
                Operator::GlobalGet { global_index },
                Operator::LocalSet { local_index: seed_local },
                Operator::LocalGet { local_index: loaded_seed },
                Operator::RefAsNonNull,
                Operator::LocalGet { .. },
                Operator::ArrayNew { array_type_index },
            ] if *global_index == seed && seed_local == loaded_seed && *array_type_index == arguments)).count(), 1,
                "Function.call fills its fresh tail vector from the initialized shared snapshot");
        }).unwrap().join().unwrap();
}
