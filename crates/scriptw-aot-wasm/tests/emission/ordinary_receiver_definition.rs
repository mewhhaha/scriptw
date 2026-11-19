//! Actual ordinary receivers define normalized fields through canonical storage.
use crate::{
    property_read_plans::{calls, Module},
    scalar_masked_rows::type_group_sizes,
};
use scriptw_aot_wasm::{
    emit_with_rooted_snapshot, GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy,
};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{HeapType, Operator, RefType, UnpackedIndex, ValType, Validator, WasmFeatures};

pub(super) fn concrete_if_arms<'a, 'b>(
    body: &'a [Operator<'b>],
    ty: u32,
) -> Vec<&'a [Operator<'b>]> {
    let mut arms = Vec::new();
    for start in 0..body.len().saturating_sub(1) {
        if !matches!(&body[start], Operator::RefTestNonNull {
            hty: HeapType::Concrete(UnpackedIndex::Module(index))
        } if *index == ty)
            || !matches!(body[start + 1], Operator::If { .. })
        {
            continue;
        }
        let mut depth = 0;
        let end = (start + 2..body.len())
            .find(|&index| {
                match &body[index] {
                    Operator::Block { .. }
                    | Operator::Loop { .. }
                    | Operator::If { .. }
                    | Operator::TryTable { .. } => depth += 1,
                    Operator::Else | Operator::End if depth == 0 => return true,
                    Operator::End => depth -= 1,
                    _ => {}
                }
                false
            })
            .expect("concrete reference admission arm closes");
        arms.push(&body[start + 2..end]);
    }
    arms
}

#[test]
fn ordinary_receiver_definitions_skip_js_descriptor_carriers_and_keep_exotic_fallback() {
    std::thread::Builder::new()
        .name("ordinary-receiver-definition".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let ir = lower(
                &parse(
                    "export function assign(object, key, value) { object[key] = value; return object; }",
                    ParseOptions::module(),
                )
                .unwrap(),
            );
            let artifact = emit_with_rooted_snapshot(
                &ir,
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            )
            .unwrap();
            let runtime_bytes = artifact.runtime().unwrap().bytes();
            for bytes in [runtime_bytes, artifact.bytes.as_slice()] {
                Validator::new_with_features(WasmFeatures::all())
                    .validate_all(bytes)
                    .expect("actual receiver-definition R/P modules validate");
            }
            let runtime = Module::read(runtime_bytes, true);
            let program = Module::read(&artifact.bytes, true);
            assert!(program.types.len() >= runtime.types.len());
            assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
            let runtime_groups = type_group_sizes(runtime_bytes);
            let program_groups = type_group_sizes(&artifact.bytes);
            assert!(program_groups.len() >= runtime_groups.len());
            assert_eq!(&program_groups[..runtime_groups.len()], &runtime_groups);

            let object = runtime.witness_type(GcSnapshotLayout::Object);
            let descriptor = runtime.witness_type(GcSnapshotLayout::Descriptor);
            let (public, _) = runtime.named("builtin::Reflect.defineProperty");
            let (allocate, _) = runtime.named("helper::ordinary_object_allocate");
            let (function_call, _) = runtime.named("helper::function_call");
            let (proxy_call, _) = runtime.named("helper::proxy_call");
            let (prepare, _) = runtime.named("helper::ordinary_property_prepare");
            let (_, write_body) = runtime.named("helper::object_write");
            let (ordinary_set, ordinary_set_body) = runtime.named("helper::ordinary_set");
            assert!(calls(write_body, ordinary_set), "ObjectWrite retains the ordinary Set and strictness owner");
            let (set, body) = runtime.named("helper::ordinary_set_data_on_receiver");
            assert!(calls(ordinary_set_body, set), "the imported ordinary Set owner reaches the receiver-definition kernel in R");
            assert_eq!(runtime.function_type(set).results(), [
                ValType::I32, ValType::I64, ValType::Ref(RefType::EQREF), ValType::I32, ValType::I32,
            ], "receiver definitions retain the whole Completion ABI");
            let arms = concrete_if_arms(body, object)
                .into_iter()
                .filter(|arm| arm.iter().any(|op| matches!(op,
                    Operator::StructGet { struct_type_index, field_index }
                        if *struct_type_index == object && *field_index == GcSnapshotField::ObjectExtensible.index())))
                .collect::<Vec<_>>();
            assert_eq!(arms.len(), 2, "create and update admit actual OrdinaryObject receivers");
            for arm in arms {
                assert!(arm.iter().any(|op| matches!(op,
                    Operator::StructNew { struct_type_index } if *struct_type_index == descriptor)),
                    "normalized definition merges into the canonical stored descriptor");
                for field in [GcSnapshotField::DescriptorFlags, GcSnapshotField::DescriptorValue] {
                    assert!(arm.iter().any(|op| matches!(op,
                        Operator::StructSet { struct_type_index, field_index }
                            if *struct_type_index == descriptor && *field_index == field.index())),
                        "existing descriptors retain canonical {field:?} publication");
                }
                assert!(calls(arm, prepare), "new keys retain canonical dictionary capacity preparation");
                assert!(!arm.iter().any(|op| matches!(op,
                    Operator::StructNew { struct_type_index } | Operator::StructNewDefault { struct_type_index }
                        if *struct_type_index == object)), "ordinary definitions allocate no synthetic JS descriptor object");
                assert!(!calls(arm, public), "ordinary definitions bypass public Reflect.defineProperty");
                assert!(!calls(arm, allocate), "ordinary definitions allocate no JS descriptor carrier through the shared allocator");
                assert!(!calls(arm, function_call), "ordinary definitions perform no generic JavaScript call");
                assert!(!calls(arm, proxy_call), "ordinary definitions bypass callable/Proxy dispatch");
                assert!(!arm.iter().any(|op| matches!(op,
                    Operator::CallRef { .. } | Operator::RefFunc { .. })),
                    "ordinary storage publication constructs no JS callable and performs no indirect JS call");
            }
            assert!(calls(body, proxy_call), "exotic receivers retain JavaScript definition forwarding");
            assert!(calls(body, allocate), "exotic forwarding retains its private descriptor carrier");
        })
        .unwrap()
        .join()
        .unwrap();
}
