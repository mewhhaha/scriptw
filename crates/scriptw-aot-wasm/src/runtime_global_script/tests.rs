use super::*;
use crate::{module::StaticSignature, runtime_helpers::RuntimeHelperId};
use scriptw_ir::{HostSurfacePolicy, RuntimeGlobalScriptKind};
use std::collections::BTreeMap;
use wasmparser::{CompositeInnerType, Operator, Parser, Payload, TypeRef, Validator, WasmFeatures};

fn minimal_runtime() -> Arc<RuntimeArtifact> {
    std::thread::Builder::new()
        .name("native-global-script-runtime".into())
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
        .unwrap()
}

fn request(kind: RuntimeGlobalScriptKind, source: &str) -> RuntimeGlobalScriptProgram {
    scriptw_ir::lower_runtime_global_script(
        kind,
        &source.encode_utf16().collect::<Vec<_>>(),
        HostSurfacePolicy::default(),
    )
    .unwrap()
}

#[test]
fn declaration_only_global_scripts_share_checks_and_do_not_allocate_var_frame_caches() {
    let runtime = minimal_runtime();
    for directive in ["", "\"use strict\";"] {
        let declarations = (0..1200)
            .map(|index| format!("var declared_{index};"))
            .collect::<String>();
        let source = format!("{directive}{declarations}");
        let program = request(RuntimeGlobalScriptKind::RealmScript, &source);
        let artifact =
            emit_runtime_global_script(&program, &runtime, &IntlCompilationProfile::Minimal)
                .unwrap();
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(artifact.bytes())
            .unwrap();
        for payload in Parser::new(0).parse_all(artifact.bytes()) {
            if let Payload::CodeSectionEntry(body) = payload.unwrap() {
                let locals = body
                    .get_locals_reader()
                    .unwrap()
                    .into_iter()
                    .map(|row| row.unwrap().0)
                    .sum::<u32>();
                assert!(
                    locals < 512,
                    "bare global declarations retained {locals} frame locals ({directive})"
                );
                assert!(
                    body.range().len() < 256 * 1024,
                    "declaration checks expanded to {} bytes at every source name ({directive})",
                    body.range().len()
                );
            }
        }
    }
}

#[test]
fn native_script_factory_witness_rejects_source_kind_nested_id_and_deferred_substitution() {
    let program = request(
        RuntimeGlobalScriptKind::IndirectEval,
        "(0,eval)('23;'); 17;",
    );
    let script = program.program().script.as_ref().unwrap();
    let original = &script.prepared_scripts[0];
    assert_eq!(
        require_root_source_row(&program, original).unwrap().id,
        program.script_id()
    );
    let mut changed = original.clone();
    changed.source.push(' ');
    assert!(require_root_source_row(&program, &changed).is_err());
    changed = original.clone();
    changed.kind = scriptw_ir::PreparedScriptKind::RealmScript;
    assert!(require_root_source_row(&program, &changed).is_err());
    let nested = script
        .prepared_scripts
        .iter()
        .find(|row| row != &original)
        .unwrap();
    assert!(require_root_source_row(&program, nested).is_err());
    changed = original.clone();
    changed.outcome = PreparedScriptOutcome::DeferredSyntaxError {
        message: "forged".into(),
    };
    assert!(require_root_source_row(&program, &changed).is_err());
}

#[test]
fn emitted_native_global_scripts_are_fresh_valid_p_with_owned_hook_factory_and_init_only_bootstrap()
{
    let profile = IntlCompilationProfile::Minimal;
    let runtime = minimal_runtime();
    for kind in [
        RuntimeGlobalScriptKind::IndirectEval,
        RuntimeGlobalScriptKind::RealmScript,
    ] {
        for strict in [false, true] {
            let source = format!(
                "{}var value = 17; let lexical = 'fresh P'; value;",
                if strict { "'use strict'; " } else { "" }
            );
            let program = request(kind, &source);
            let artifact = emit_runtime_global_script(&program, &runtime, &profile).unwrap();
            assert!(Arc::ptr_eq(artifact.runtime(), &runtime));
            assert_eq!(artifact.runtime().key(), runtime.key());
            assert_eq!(artifact.runtime().bytes(), runtime.bytes());
            Validator::new_with_features(WasmFeatures::all())
                .validate_all(artifact.bytes())
                .unwrap();
            let expected_imports = crate::tests::linked_function_imports(&runtime);
            let host_count = expected_imports
                .iter()
                .take_while(|(module, _, _)| module != crate::RUNTIME_IMPORT_NAMESPACE)
                .count();
            let mut runtime_cursor = host_count;
            let mut types = Vec::new();
            let mut function_types = Vec::new();
            let mut exports = BTreeMap::new();
            let mut imported = 0u32;
            let mut defined = 0u32;
            let mut calls = BTreeMap::<u32, Vec<u32>>::new();
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
                        panic!("native P may not execute a start function")
                    }
                    Payload::CodeSectionEntry(body) => {
                        let index = imported + defined;
                        defined += 1;
                        let initializer =
                            exports.get(artifact.program_initialize_export()) == Some(&index);
                        let mut direct = Vec::new();
                        let mut operators = body.get_operators_reader().unwrap();
                        while !operators.eof() {
                            match operators.read().unwrap() {
                                Operator::Call { function_index } => direct.push(function_index),
                                Operator::MemoryInit { .. } | Operator::GlobalSet { .. } |
                                Operator::CallIndirect { .. } | Operator::CallRef { .. } |
                                Operator::ReturnCall { .. } | Operator::ReturnCallRef { .. } =>
                                    assert!(!initializer, "P init cannot execute source, mutate R/P globals or install linear data"),
                                _ => {}
                            }
                        }
                        calls.insert(index, direct);
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
            let initialize = exports[artifact.program_initialize_export()];
            let factory = exports[artifact.factory_export()];
            assert!(!exports.contains_key("main"));
            assert_eq!(initialize, imported);
            assert!(!exports.contains_key(crate::runtime_function::RUNTIME_FUNCTION_FACTORY_EXPORT));
            assert_eq!(
                function_types[initialize as usize],
                StaticSignature::ProgramInitialize.type_index()
            );
            assert_eq!(
                function_types[factory as usize],
                RuntimeHelperId::PreparedScript.type_index()
            );
            let CompositeInnerType::Func(factory_type) = &types
                [RuntimeHelperId::PreparedScript.type_index() as usize]
                .composite_type
                .inner
            else {
                panic!("factory function type")
            };
            assert_eq!(factory_type.params().len(), 4);
            assert_eq!(factory_type.results().len(), 6);
            assert!(
                matches!(factory_type.params()[0], wasmparser::ValType::Ref(reference) if !reference.is_nullable())
            );
            assert_eq!(
                factory_type.results().last(),
                Some(&wasmparser::ValType::I32)
            );
            let r_init = function_types
                .iter()
                .position(|index| *index == RuntimeHelperId::PooledStringsInitialize.type_index())
                .unwrap() as u32;
            assert_eq!(calls[&initialize], [r_init]);
        }
    }
}

#[test]
fn actual_runtime_declares_both_compiler_imports_with_distinct_types() {
    let runtime = minimal_runtime();
    let mut found = BTreeMap::new();
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::ImportSection(reader) = payload.unwrap() {
            for import in reader.into_imports() {
                let import = import.unwrap();
                if [
                    crate::GcHostImport::CompileGlobalScript.name(),
                    crate::GcHostImport::CompileDynamicFunction.name(),
                ]
                .contains(&import.name)
                {
                    assert_eq!(
                        import.module,
                        crate::GcHostImport::CompileGlobalScript.module()
                    );
                    let TypeRef::Func(index) = import.ty else {
                        panic!("compiler host import type")
                    };
                    assert!(found.insert(import.name.to_string(), index).is_none());
                }
            }
        }
    }
    assert_eq!(found.len(), 2);
    assert_eq!(
        found["compile_global_script"],
        StaticSignature::HostCompileGlobalScript.type_index()
    );
    assert_eq!(
        found["compile_dynamic_function"],
        StaticSignature::HostCompileDynamicFunction.type_index()
    );
}
