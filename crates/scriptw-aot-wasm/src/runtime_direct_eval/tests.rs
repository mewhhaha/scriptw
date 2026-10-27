use super::*;
mod empty_kernel;
mod primitive_kernel;
use crate::gc_types::{AbiType, GcLayout, GcNullability};
use crate::{module::StaticSignature, runtime_helpers::RuntimeHelperId};
use scriptw_ir::{
    HostSurfacePolicy, RuntimeDirectEvalCallerDescriptor, RuntimeDirectEvalPrivateScope,
};
use std::collections::BTreeMap;
use wasmparser::{CompositeInnerType, Parser, Payload, TypeRef, Validator, WasmFeatures};

fn caller(
    scopes: impl IntoIterator<Item = RuntimeDirectEvalPrivateScope>,
) -> RuntimeDirectEvalCaller {
    // Canonical sloppy function grammar, no derived owner/import record/names.
    let mut descriptor = b"SCRWDE01".to_vec();
    descriptor.extend_from_slice(&[0, 1, 0, 0]);
    descriptor.extend_from_slice(&0u64.to_le_bytes());
    RuntimeDirectEvalCaller::validate(
        RuntimeDirectEvalCallerDescriptor::decode(&descriptor).unwrap(),
        scopes,
    )
    .unwrap()
}
fn request(caller: &RuntimeDirectEvalCaller, source: &[u16]) -> RuntimeDirectEvalProgram {
    scriptw_ir::lower_runtime_direct_eval(caller, source, HostSurfacePolicy::default()).unwrap()
}

#[test]
fn direct_factory_root_witness_rejects_source_role_nested_id_and_deferred_substitution() {
    let program = request(
        &caller([]),
        &"eval('23;'); 17;".encode_utf16().collect::<Vec<_>>(),
    );
    let script = program.program().script.as_ref().unwrap();
    let original = &script.prepared_scripts[0];
    assert_eq!(
        witness::require_root_source_row(&program, original)
            .unwrap()
            .id,
        program.script_id()
    );
    let mut changed = original.clone();
    changed.source.push(' ');
    assert!(witness::require_root_source_row(&program, &changed).is_err());
    changed = original.clone();
    changed.kind = scriptw_ir::PreparedScriptKind::IndirectEval;
    assert!(witness::require_root_source_row(&program, &changed).is_err());
    let nested = script
        .prepared_scripts
        .iter()
        .find(|row| *row != original)
        .unwrap();
    assert!(witness::require_root_source_row(&program, nested).is_err());
    changed = original.clone();
    changed.outcome = scriptw_ir::PreparedScriptOutcome::DeferredSyntaxError {
        message: "forged".into(),
    };
    assert!(witness::require_root_source_row(&program, &changed).is_err());
}

#[test]
fn complete_caller_witness_distinguishes_hidden_ancestor_capacity() {
    let first = caller([RuntimeDirectEvalPrivateScope::new(7, 1)]);
    let second = caller([RuntimeDirectEvalPrivateScope::new(90, 1)]);
    assert_eq!(first.descriptor(), second.descriptor());
    assert_ne!(
        first.private_environment_floor(),
        second.private_environment_floor()
    );
    assert_ne!(first, second);
}

#[test]
fn native_direct_eval_import_and_factory_keep_distinct_exact_gc_operands() {
    use AbiType::{EqRef, Gc, I32, I64};
    use GcNullability::{NonNullable, Nullable};
    let signature = StaticSignature::HostCompileDirectEval;
    assert_eq!(
        signature.parameter_types(),
        &[
            Gc(GcLayout::StringValue, NonNullable),
            Gc(GcLayout::ImmutableByteArray, NonNullable),
            Gc(GcLayout::RealmRecord, NonNullable),
            Gc(GcLayout::Environment, Nullable),
            Gc(GcLayout::Environment, Nullable),
            Gc(GcLayout::PrivateEnvironment, Nullable),
            I32,
            I64,
            EqRef,
            I32,
            I64,
            EqRef,
            Gc(GcLayout::DirectEvalExecutionContext, NonNullable),
            Gc(GcLayout::ProgramInstance, NonNullable),
        ]
    );
    assert_eq!(
        signature.result_types(),
        &[
            I32,
            I64,
            EqRef,
            I32,
            I32,
            I32,
            Gc(GcLayout::StringValue, Nullable)
        ]
    );
    assert_eq!(
        crate::GcHostImport::CompileDirectEval.name(),
        "compile_direct_eval"
    );
    assert_ne!(
        signature.type_index(),
        StaticSignature::HostCompileGlobalScript.type_index()
    );
    assert_ne!(
        RuntimeHelperId::NativeDirectEvalFactory.type_index(),
        RuntimeHelperId::PreparedScript.type_index()
    );
}

#[test]
fn emitted_direct_eval_factory_retains_full_witness_exact_r_and_valid_typed_hook_closure() {
    let profile = IntlCompilationProfile::Minimal;
    let runtime = std::thread::Builder::new()
        .name("native-direct-eval-runtime".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            crate::runtime_artifact_with_inputs(
                &IntlDataSelection::new(IntlCompilationProfile::Minimal),
                crate::RuntimeArtifactInputs::new(None).with_source_compilation_profile(
                    crate::RuntimeSourceCompilationProfile::NativeHost,
                ),
            )
            .unwrap()
        })
        .unwrap()
        .join()
        .unwrap();
    let caller = caller([RuntimeDirectEvalPrivateScope::new(90, 1)]);
    // An unexecuted foreign import must not reject compilation eagerly.
    let source = "function later(){ import('unexecuted'); } class C { #brand; field = 17; } 42;"
        .encode_utf16()
        .collect::<Vec<_>>();
    let program = request(&caller, &source);
    let artifact = emit_runtime_direct_eval(&program, &runtime, &profile).unwrap();
    assert_eq!(artifact.caller(), &caller);
    assert_eq!(artifact.source(), source);
    assert!(Arc::ptr_eq(artifact.runtime(), &runtime));
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(artifact.bytes())
        .unwrap();
    let expected_imports = crate::tests::linked_function_imports(&runtime);
    let host_count = expected_imports
        .iter()
        .take_while(|(module, _, _)| module != crate::RUNTIME_IMPORT_NAMESPACE)
        .count();
    let mut runtime_cursor = host_count;
    let mut imported = 0usize;
    let mut types = Vec::new();
    let mut function_types = Vec::new();
    let mut exports = BTreeMap::new();
    for payload in Parser::new(0).parse_all(artifact.bytes()) {
        match payload.unwrap() {
            Payload::TypeSection(reader) => {
                for group in reader {
                    types.extend(group.unwrap().types().cloned());
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.unwrap();
                    if let TypeRef::Func(index) | TypeRef::FuncExact(index) = import.ty {
                        let expected = if (imported as usize) < host_count {
                            &expected_imports[imported as usize]
                        } else {
                            assert_eq!(import.module, crate::RUNTIME_IMPORT_NAMESPACE,
                                "P must retain the complete exact host prefix before runtime exports");
                            let relative = expected_imports[runtime_cursor..].iter().position(|row|
                                row.0 == import.module && row.1 == import.name)
                                .expect("P runtime imports must be a unique ordered subset of actual R exports");
                            runtime_cursor += relative;
                            let expected = &expected_imports[runtime_cursor];
                            runtime_cursor += 1;
                            expected
                        };
                        assert_eq!(
                            (import.module, import.name, index),
                            (expected.0.as_str(), expected.1.as_str(), expected.2),
                            "P retains exact host declarations and each selected R export's original type"
                        );
                        function_types.push(index);
                        imported += 1;
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                function_types.extend(reader.into_iter().map(Result::unwrap))
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.unwrap();
                    exports.insert(export.name.to_owned(), export.index);
                }
            }
            Payload::StartSection { .. } => {
                panic!("native direct P cannot execute a start function")
            }
            _ => {}
        }
    }
    assert!(
        (imported as usize) > host_count,
        "factory retains its actual R dependencies"
    );
    assert!(
        (imported as usize) < expected_imports.len(),
        "small factory must not restore all unused R imports"
    );
    assert!(!exports.contains_key("main"));
    assert_eq!(
        exports[artifact.program_initialize_export()],
        imported as u32
    );
    assert!(
        !exports.contains_key(crate::runtime_global_script::RUNTIME_GLOBAL_SCRIPT_FACTORY_EXPORT)
    );
    assert!(!exports.contains_key(crate::runtime_function::RUNTIME_FUNCTION_FACTORY_EXPORT));
    let factory = exports[artifact.factory_export()];
    assert_eq!(
        function_types[factory as usize],
        RuntimeHelperId::NativeDirectEvalFactory.type_index()
    );
    let CompositeInnerType::Func(ty) = &types[function_types[factory as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("direct factory function type")
    };
    assert_eq!(ty.params().len(), 15);
    assert_eq!(ty.results().len(), 6);
    assert_eq!(ty.results().last(), Some(&wasmparser::ValType::I32));
}

#[test]
fn proven_empty_eval_kernel_has_only_source_independent_completion_code() {
    empty_kernel::proven_empty_eval_kernel_has_only_source_independent_completion_code();
}

#[test]
fn proven_primitive_eval_kernel_shares_code_and_keeps_literal_operands_per_call() {
    primitive_kernel::proven_primitive_eval_kernel_shares_code_and_keeps_literal_operands_per_call(
    );
}
