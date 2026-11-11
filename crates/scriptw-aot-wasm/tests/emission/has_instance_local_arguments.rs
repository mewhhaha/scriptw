//! The default HasInstance builtin publishes its argv adapter and one narrow
//! local body; the canonical body remains the implementation of the method.
use crate::{linked_bodies::linked_bodies, property_read_plans::Module};
use scriptw_aot_wasm::{emit_with_rooted_snapshot, GcSnapshotLayout, PromiseRejectionPolicy};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{
    CompositeInnerType, HeapType, Operator, Parser, Payload, StorageType, UnpackedIndex, ValType,
    Validator, WasmFeatures,
};

fn local_for_function_reference(operators: &[Operator<'_>], target: u32) -> Option<u32> {
    operators.windows(2).find_map(|pair| match pair {
        [Operator::RefFunc { function_index }, Operator::LocalSet { local_index }]
            if *function_index == target =>
        {
            Some(*local_index)
        }
        _ => None,
    })
}

fn publishes_narrow_entry(bytes: &[u8], code_type: u32, ordinary: u32, local: u32) -> bool {
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("runtime module decodes") {
            Payload::CodeSectionEntry(body) => {
                let operators = body
                    .get_operators_reader()
                    .expect("runtime function opens")
                    .into_iter()
                    .map(|op| op.expect("runtime operator decodes"))
                    .collect::<Vec<_>>();
                let Some(ordinary_local) = local_for_function_reference(&operators, ordinary)
                else {
                    continue;
                };
                let Some(local_arguments_local) = local_for_function_reference(&operators, local)
                else {
                    continue;
                };
                for (end, op) in operators.iter().enumerate() {
                    if !matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == code_type)
                    {
                        continue;
                    }
                    let start = operators[..end]
                        .iter()
                        .rposition(|op| matches!(op,
                            Operator::StructNew { struct_type_index } if *struct_type_index == code_type))
                        .map_or(0, |index| index + 1);
                    let record = &operators[start..=end];
                    let ordinary_entry = record.iter().any(|op| {
                        matches!(op,
                            Operator::LocalGet { local_index } if *local_index == ordinary_local)
                    });
                    let local_tail = record.windows(3).any(|ops| {
                        matches!(ops,
                        [Operator::LocalGet { local_index }, Operator::RefNull { .. },
                            Operator::StructNew { struct_type_index }]
                            if *local_index == local_arguments_local && *struct_type_index == code_type)
                    });
                    if ordinary_entry && local_tail {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

#[test]
fn default_has_instance_has_one_published_narrow_body_and_adapter_tail_call() {
    std::thread::Builder::new()
        .name("has-instance-local-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let ir = lower(
                &parse(
                    "function Target() {} globalThis.target = Target;",
                    ParseOptions::script(),
                )
                .expect("small script parses"),
            );
            assert!(ir.is_wasm_supported(), "{:?}", ir.diagnostics);
            let artifact = emit_with_rooted_snapshot(
                &ir,
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            )
            .expect("emit actual rooted R/P modules");
            let runtime_bytes = artifact.runtime().expect("runtime module exists").bytes();
            for bytes in [runtime_bytes, artifact.bytes.as_slice()] {
                Validator::new_with_features(WasmFeatures::all())
                    .validate_all(bytes)
                    .expect("local and ordinary Wasm entry types validate");
            }

            let bodies = linked_bodies(&artifact);
            let canonical_name = "builtin::Function.prototype[Symbol.hasInstance]";
            let adapter_name = "builtin-arguments-adapter::Function.prototype[Symbol.hasInstance]";
            let canonical = bodies
                .runtime
                .iter()
                .find(|body| body.name == canonical_name)
                .expect("canonical native body has one emitted identity");
            assert_eq!(
                bodies
                    .runtime
                    .iter()
                    .filter(|body| body.name == canonical_name)
                    .count(),
                1,
                "there is one canonical implementation"
            );
            let adapter = bodies
                .runtime
                .iter()
                .find(|body| body.name == adapter_name)
                .expect("the ordinary argv entry has one adapter");
            assert_eq!(
                bodies
                    .runtime
                    .iter()
                    .filter(|body| body.name == adapter_name)
                    .count(),
                1,
                "there is one ordinary-to-local adapter"
            );

            let module = Module::read(runtime_bytes, true);
            let (_, adapter_ops) = module.named(adapter_name);
            let canonical_type = module.function_type(canonical.index);
            assert_eq!(canonical_type.params().len(), 15, "narrow local entry");
            assert_eq!(canonical_type.results().len(), 5, "whole Completion result");
            let adapter_type = module.function_type(adapter.index);
            assert_eq!(adapter_type.params().len(), 11, "ordinary argv entry");
            assert_eq!(adapter_type.results(), canonical_type.results());
            assert_eq!(
                adapter_ops
                    .iter()
                    .filter(|op| matches!(op,
                        Operator::ReturnCall { function_index } if *function_index == canonical.index))
                    .count(),
                1,
                "the adapter tail-calls the same canonical body"
            );
            let code_type = module.witness_type(GcSnapshotLayout::Code);
            let CompositeInnerType::Struct(code_layout) =
                &module.types[code_type as usize].composite_type.inner
            else {
                panic!("ExecutableCode is the published GC record");
            };
            assert_eq!(code_layout.fields.len(), 13);
            let published_signature = |field: usize| {
                let StorageType::Val(ValType::Ref(reference)) =
                    &code_layout.fields[field].element_type
                else {
                    panic!("entry slot {field} is a typed function reference");
                };
                let HeapType::Concrete(UnpackedIndex::Module(type_index)) =
                    reference.heap_type()
                else {
                    panic!("entry slot {field} has a module function type");
                };
                let CompositeInnerType::Func(signature) =
                    &module.types[type_index as usize].composite_type.inner
                else {
                    panic!("entry slot {field} is a function type");
                };
                signature
            };
            assert_eq!(published_signature(6), adapter_type,
                "ExecutableCode ordinary field publishes the argv adapter");
            assert_eq!(published_signature(11), canonical_type,
                "ExecutableCode local field publishes the canonical narrow body");
            let program_imports =
                crate::linked_bodies::runtime_function_imports(&artifact.bytes, runtime_bytes);
            let program_adapter = program_imports
                .iter()
                .find_map(|(&program, &runtime)| (runtime == adapter.index).then_some(program));
            let program_canonical = program_imports
                .iter()
                .find_map(|(&program, &runtime)| (runtime == canonical.index).then_some(program));
            let runtime_publication = publishes_narrow_entry(
                runtime_bytes,
                code_type,
                adapter.index,
                canonical.index,
            );
            let program_publication = match (program_adapter, program_canonical) {
                (Some(adapter), Some(canonical)) => publishes_narrow_entry(
                    &artifact.bytes,
                    code_type,
                    adapter,
                    canonical,
                ),
                _ => false,
            };
            assert!(
                runtime_publication || program_publication,
                "one actual ExecutableCode construction loads the argv entry and canonical local entry into their declared fields, then nulls the unsupported wide field"
            );
        })
        .expect("compiler worker starts")
        .join()
        .expect("emission check completes");
}
