use super::*;
use crate::{module::StaticSignature, runtime_helpers::RuntimeHelperId};
use std::collections::BTreeMap;
use wasmparser::{CompositeInnerType, Operator, Parser, Payload, TypeRef};

#[test]
fn emitted_native_function_is_only_p_with_typed_bootstrap_and_actual_factory_alias() {
    let profile = IntlCompilationProfile::Minimal;
    let runtime = crate::runtime_artifact_with_inputs(
        &IntlDataSelection::new(profile.clone()),
        crate::RuntimeArtifactInputs::new(None)
            .with_source_compilation_profile(crate::RuntimeSourceCompilationProfile::NativeHost),
    )
    .unwrap();
    let request = scriptw_ir::lower_runtime_function(
        scriptw_ir::DynamicFunctionKind::Ordinary,
        &["return 'fresh P';".encode_utf16().collect()],
        scriptw_ir::HostSurfacePolicy::default(),
    )
    .unwrap();
    let artifact = emit_runtime_function(&request, &runtime, &profile).unwrap();
    assert!(Arc::ptr_eq(artifact.runtime(), &runtime));
    assert_eq!(artifact.runtime().key(), runtime.key());
    assert_eq!(artifact.runtime().bytes(), runtime.bytes());
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
                    if let TypeRef::Func(index) | TypeRef::FuncExact(index) = import.unwrap().ty {
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
                    exports.insert(export.name.to_string(), export.index);
                }
            }
            Payload::CodeSectionEntry(body) => {
                let index = imported + defined;
                defined += 1;
                let mut direct = Vec::new();
                let initializer = exports.get(artifact.program_initialize_export()) == Some(&index);
                let mut operators = body.get_operators_reader().unwrap();
                while !operators.eof() {
                    let operator = operators.read().unwrap();
                    match operator {
                        Operator::Call { function_index } => direct.push(function_index),
                        Operator::MemoryInit { .. }
                        | Operator::GlobalSet { .. }
                        | Operator::CallIndirect { .. }
                        | Operator::CallRef { .. }
                        | Operator::ReturnCall { .. }
                        | Operator::ReturnCallRef { .. } => {
                            assert!(!initializer, "program_init cannot install P memory, mutate R state or execute source/Realm calls: {operator:?}");
                        }
                        _ => {}
                    }
                }
                calls.insert(index, direct);
            }
            _ => {}
        }
    }
    let initialize = exports[artifact.program_initialize_export()];
    let factory = exports[artifact.factory_export()];
    assert!(!exports.contains_key("main"));
    assert_eq!(
        calls.keys().next().copied(),
        Some(initialize),
        "initializer is the first defined P function"
    );
    assert_eq!(
        function_types[initialize as usize],
        StaticSignature::ProgramInitialize.type_index()
    );
    assert_eq!(
        function_types[factory as usize],
        RuntimeHelperId::PreparedDynamicFunction.type_index()
    );
    let CompositeInnerType::Func(initialize_type) = &types
        [StaticSignature::ProgramInitialize.type_index() as usize]
        .composite_type
        .inner
    else {
        panic!("bootstrap function type")
    };
    assert!(initialize_type.params().is_empty());
    assert!(
        matches!(initialize_type.results(), [wasmparser::ValType::Ref(reference)] if !reference.is_nullable())
    );
    let CompositeInnerType::Func(factory_type) = &types
        [RuntimeHelperId::PreparedDynamicFunction.type_index() as usize]
        .composite_type
        .inner
    else {
        panic!("factory function type")
    };
    assert_eq!(factory_type.params().len(), 8);
    assert!(
        matches!(factory_type.params().first(), Some(wasmparser::ValType::Ref(reference)) if !reference.is_nullable())
    );
    assert_eq!(factory_type.results().len(), 6);
    assert_eq!(
        factory_type.results().last(),
        Some(&wasmparser::ValType::I32)
    );
    let r_initialize = function_types
        .iter()
        .position(|index| *index == RuntimeHelperId::PooledStringsInitialize.type_index())
        .unwrap() as u32;
    assert_eq!(
        calls[&initialize],
        vec![r_initialize],
        "only the guarded R literal bootstrap may execute from program_init"
    );
}
