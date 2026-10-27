//! Compact P must still admit the source-owned native template transformation.
use super::*;

fn runtime() -> crate::ValidatedRuntimeArtifact {
    let profile = scriptw_intl::IntlCompilationProfile::Minimal;
    let runtime = crate::runtime_artifact_with_inputs(
        &scriptw_intl::IntlDataSelection::new(profile.clone()),
        crate::RuntimeArtifactInputs::new(None)
            .with_source_compilation_profile(crate::RuntimeSourceCompilationProfile::NativeHost),
    )
    .unwrap();
    crate::ValidatedRuntimeArtifact::new(runtime, profile).unwrap()
}

fn check_admission(raw: &[u8], runtime: &[u8], template: &NativeProgramTemplate) {
    let mut runtime_functions = 0;
    let mut runtime_globals = 0;
    for payload in wasmparser::Parser::new(0).parse_all(runtime) {
        if let wasmparser::Payload::ExportSection(exports) = payload.unwrap() {
            for export in exports {
                match export.unwrap().kind {
                    wasmparser::ExternalKind::Func => runtime_functions += 1,
                    wasmparser::ExternalKind::Global => runtime_globals += 1,
                    _ => {}
                }
            }
        }
    }
    let mut functions = 0;
    let mut globals = 0;
    let mut contexts = false;
    let mut literals = false;
    let mut packed_data = false;
    for payload in wasmparser::Parser::new(0).parse_all(raw) {
        match payload.unwrap() {
            wasmparser::Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    let import = import.unwrap();
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE {
                        match import.ty {
                            wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_) => {
                                functions += 1
                            }
                            wasmparser::TypeRef::Global(_) => globals += 1,
                            _ => {}
                        }
                    }
                }
            }
            wasmparser::Payload::CustomSection(section) => {
                contexts |= section.name() == crate::native_program_contexts::SECTION_NAME;
                literals |= section.name() == crate::program_literal_operands::SECTION_NAME;
            }
            wasmparser::Payload::CodeSectionEntry(body) => {
                let mut operators = body.get_operators_reader().unwrap();
                while !operators.eof() {
                    packed_data |= matches!(
                        operators.read().unwrap(),
                        wasmparser::Operator::ArrayNewData { .. }
                            | wasmparser::Operator::ArrayNewFixed { .. }
                    );
                }
            }
            _ => {}
        }
    }
    assert!(
        functions > 0 && functions * 2 < runtime_functions,
        "P retains {functions} functions from {runtime_functions} R exports"
    );
    assert!(
        globals > 0 && globals * 2 < runtime_globals,
        "P retains {globals} globals from {runtime_globals} R exports"
    );
    assert!(
        contexts && literals && packed_data,
        "actual P carries source-owner ordinal witnesses and packed construction sites"
    );
    // Admission checks the original body/operator ordinal witnesses against the
    // actual compacted code, including initializer export indices after remapping.
    let payload = template
        .program_payload()
        .expect("compact P keeps owned template admission");
    assert!(payload.permits_instance_reuse());
    assert!(!payload.bytes().is_empty());
    assert!(!template.data_images().is_empty());
    assert!(
        template
            .array_factories()
            .iter()
            .any(|factory| factory.width() == NativeProgramArrayWidth::I16),
        "UTF-16 data still receives a fresh typed constructor"
    );
    assert_ne!(
        raw,
        template.bytes(),
        "admission must not silently take the raw-P fallback"
    );
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(raw)
        .unwrap();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(template.bytes())
        .unwrap();
}

#[test]
fn compact_native_function_preserves_owned_string_and_packed_array_admission() {
    let proof = runtime();
    let text = "packed UTF-16 source payload \u{100} ".repeat(16);
    let source = format!("return ['first', '{text}', 'last'].join(':');");
    let program = scriptw_ir::lower_runtime_function(
        scriptw_ir::DynamicFunctionKind::Ordinary,
        &[source.encode_utf16().collect()],
        scriptw_ir::HostSurfacePolicy::default(),
    )
    .unwrap();
    let raw = crate::emit_runtime_function_with_validated_runtime(&program, &proof).unwrap();
    let template = NativeProgramTemplate::for_function(&raw).unwrap();
    check_admission(raw.bytes(), raw.runtime().bytes(), &template);
}

#[test]
fn compact_native_global_script_preserves_owned_string_and_packed_array_admission() {
    let proof = runtime();
    let text = "global packed UTF-16 payload \u{101} ".repeat(16);
    let source = format!("['first', '{text}', 'last'].join(':');");
    let program = scriptw_ir::lower_runtime_global_script(
        scriptw_ir::RuntimeGlobalScriptKind::IndirectEval,
        &source.encode_utf16().collect::<Vec<_>>(),
        scriptw_ir::HostSurfacePolicy::default(),
    )
    .unwrap();
    let raw = crate::emit_runtime_global_script_with_validated_runtime(&program, &proof).unwrap();
    let template = NativeProgramTemplate::for_global_script(&raw).unwrap();
    check_admission(raw.bytes(), raw.runtime().bytes(), &template);
}
