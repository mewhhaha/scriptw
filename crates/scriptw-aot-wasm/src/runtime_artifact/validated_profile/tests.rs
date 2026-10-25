use super::*;
use crate::runtime_artifact::{CurrentRuntimeLayoutVersion, RuntimeLayout};
use scriptw_ir::{HostSurfacePolicy, RuntimeDirectEvalCaller, RuntimeDirectEvalCallerDescriptor};
use std::{borrow::Cow, sync::OnceLock};
use wasm_encoder::{CustomSection, Module};

pub(super) fn runtime() -> Arc<RuntimeArtifact> {
    static RUNTIME: OnceLock<Arc<RuntimeArtifact>> = OnceLock::new();
    Arc::clone(RUNTIME.get_or_init(|| {
        std::thread::Builder::new()
            .name("validated-runtime-profile-fixture".into())
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
    }))
}

// Fault injection preserves the genuine artifact's admission while corrupting
// immutable bytes in a new owner. No production constructor accepts raw bytes.
fn corrupt_runtime(bytes: Vec<u8>, admitted: bool) -> Arc<RuntimeArtifact> {
    let actual = runtime();
    let layout = &actual.layout;
    Arc::new(RuntimeArtifact {
        key: actual.key,
        source_compilation_profile: actual.source_compilation_profile(),
        compilation_data: actual.compilation_data.clone(),
        compiler_owned_pool: OnceLock::new(),
        build_admission: None,
        bytes: Cow::Owned(bytes),
        wasm_sha256: actual.wasm_sha256,
        layout: RuntimeLayout::new(
            layout.imported_functions,
            layout.function_types.clone(),
            layout.globals.clone(),
            layout.pool,
        ),
        layout_admission: if admitted {
            actual.layout_admission
        } else {
            None
        },
    })
}

fn profile_image(profile: IntlCompilationProfile, duplicate: bool) -> Vec<u8> {
    let selection = IntlDataSelection::new(profile);
    let selected = selection.selected().unwrap();
    let runtime = runtime();
    let mut module = Module::new();
    for payload in wasmparser::Parser::new(0).parse_all(runtime.bytes()) {
        let payload = payload.unwrap();
        if matches!(&payload, wasmparser::Payload::CustomSection(section) if section.name().starts_with("scriptw.intl"))
        {
            continue;
        }
        if let Some((id, range)) = payload.as_section() {
            module.section(&wasm_encoder::RawSection {
                id,
                data: &runtime.bytes()[range],
            });
        }
    }
    for (name, data) in selected.component_sections() {
        module.section(&CustomSection {
            name: Cow::Borrowed(name),
            data: Cow::Borrowed(&data),
        });
    }
    let artifact_identity = selected.identity().artifact_identity();
    let identity = artifact_identity.as_bytes();
    for _ in 0..(if duplicate { 2 } else { 1 }) {
        module.section(&CustomSection {
            name: Cow::Borrowed(scriptw_intl::INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION),
            data: Cow::Borrowed(identity),
        });
    }
    if let Some(services) = selected.service_selection() {
        module.section(&CustomSection {
            name: Cow::Borrowed(scriptw_intl::INTL_SERVICE_SELECTION_CUSTOM_SECTION),
            data: Cow::Owned(services.wire().to_le_bytes().to_vec()),
        });
    }
    module.finish()
}

#[test]
fn runtime_proof_retains_exact_arc_and_rejects_other_physical_profile() {
    let runtime = runtime();
    let proof =
        ValidatedRuntimeArtifact::new(Arc::clone(&runtime), IntlCompilationProfile::Minimal)
            .unwrap();
    assert!(Arc::ptr_eq(proof.runtime(), &runtime));
    assert_eq!(
        proof.source_compilation_profile(),
        crate::RuntimeSourceCompilationProfile::NativeHost
    );
    assert_eq!(
        crate::RuntimeSourceCompilationProfile::from_module(runtime.bytes()).unwrap(),
        proof.source_compilation_profile()
    );
    assert_eq!(proof.layout_version(), LAYOUT_VERSION);
    assert!(ValidatedRuntimeArtifact::new(runtime, IntlCompilationProfile::Conformance).is_err());
}

#[test]
fn current_layout_authority_cannot_be_inferred_from_valid_intl_bytes() {
    for wrong in (0..LAYOUT_VERSION).chain([LAYOUT_VERSION + 1, u32::MAX]) {
        assert!(CurrentRuntimeLayoutVersion::admit(wrong).is_none());
    }
    assert_eq!(
        CurrentRuntimeLayoutVersion::admit(LAYOUT_VERSION)
            .unwrap()
            .0,
        LAYOUT_VERSION
    );
    let unadmitted = corrupt_runtime(runtime().bytes().to_vec(), false);
    assert!(
        ValidatedRuntimeArtifact::new(unadmitted, IntlCompilationProfile::Minimal)
            .unwrap_err()
            .to_string()
            .contains("layout admission")
    );
}

#[test]
fn proof_and_standalone_raw_api_reject_malformed_missing_and_duplicate_frames() {
    let request = scriptw_ir::lower_runtime_function(
        scriptw_ir::DynamicFunctionKind::Ordinary,
        &["return 3;".encode_utf16().collect()],
        HostSurfacePolicy::default(),
    )
    .unwrap();
    for bytes in [
        vec![0, 97, 115, 109, 1, 0, 0],
        Module::new().finish(),
        profile_image(IntlCompilationProfile::Minimal, true),
    ] {
        let corrupt = corrupt_runtime(bytes, true);
        assert!(ValidatedRuntimeArtifact::new(
            Arc::clone(&corrupt),
            IntlCompilationProfile::Minimal
        )
        .is_err());
        assert!(
            crate::emit_runtime_function(&request, &corrupt, &IntlCompilationProfile::Minimal)
                .is_err()
        );
    }
}

#[test]
fn same_custom_profile_name_does_not_substitute_different_physical_services() {
    let id = scriptw_intl::CustomProfileId::parse("runtime-proof-control").unwrap();
    let list = IntlCompilationProfile::CustomProjection(
        scriptw_intl::CustomIntlProfile::for_services(id.clone(), &["ListFormat"]).unwrap(),
    );
    let number = IntlCompilationProfile::CustomProjection(
        scriptw_intl::CustomIntlProfile::for_services(id, &["NumberFormat"]).unwrap(),
    );
    let corrupt = corrupt_runtime(profile_image(list.clone(), false), true);
    // The fault-injected frames match one selection, but the same custom name
    // cannot certify a different service set or its different physical data.
    assert!(ValidatedRuntimeArtifact::new(Arc::clone(&corrupt), list).is_ok());
    assert!(ValidatedRuntimeArtifact::new(corrupt, number).is_err());
}

#[test]
fn all_three_checked_raw_emitters_match_proof_emitters_and_keep_exact_r() {
    let runtime = runtime();
    let profile = IntlCompilationProfile::Minimal;
    let proof = ValidatedRuntimeArtifact::new(Arc::clone(&runtime), profile.clone()).unwrap();
    let function = scriptw_ir::lower_runtime_function(
        scriptw_ir::DynamicFunctionKind::Ordinary,
        &["return 3;".encode_utf16().collect()],
        HostSurfacePolicy::default(),
    )
    .unwrap();
    let raw = crate::emit_runtime_function(&function, &runtime, &profile).unwrap();
    let fast = crate::emit_runtime_function_with_validated_runtime(&function, &proof).unwrap();
    assert_eq!(raw.bytes(), fast.bytes());
    assert!(Arc::ptr_eq(fast.runtime(), &runtime));
    let prefix = Arc::clone(runtime.compiler_owned_pool.get().unwrap().as_ref().unwrap());
    let second_proof =
        ValidatedRuntimeArtifact::new(Arc::clone(&runtime), profile.clone()).unwrap();
    let another =
        crate::emit_runtime_function_with_validated_runtime(&function, &second_proof).unwrap();
    assert_eq!(another.bytes(), fast.bytes());
    assert!(Arc::ptr_eq(
        &prefix,
        runtime.compiler_owned_pool.get().unwrap().as_ref().unwrap()
    ));
    assert!(crate::emit_runtime_function(
        &function,
        &runtime,
        &IntlCompilationProfile::Conformance
    )
    .is_err());
    let global = scriptw_ir::lower_runtime_global_script(
        scriptw_ir::RuntimeGlobalScriptKind::IndirectEval,
        &"var proofValue = 4; proofValue;"
            .encode_utf16()
            .collect::<Vec<_>>(),
        HostSurfacePolicy::default(),
    )
    .unwrap();
    let raw = crate::emit_runtime_global_script(&global, &runtime, &profile).unwrap();
    let fast = crate::emit_runtime_global_script_with_validated_runtime(&global, &proof).unwrap();
    assert_eq!(raw.bytes(), fast.bytes());
    assert!(Arc::ptr_eq(fast.runtime(), &runtime));
    assert!(Arc::ptr_eq(
        &prefix,
        runtime.compiler_owned_pool.get().unwrap().as_ref().unwrap()
    ));
    assert!(crate::emit_runtime_global_script(
        &global,
        &runtime,
        &IntlCompilationProfile::Conformance
    )
    .is_err());
    // Canonical sloppy function caller with no derived owner, import record,
    // or private bindings. Use the public host codec rather than IR internals.
    let mut descriptor = b"SCRWDE01".to_vec();
    descriptor.extend_from_slice(&[0, 1, 0, 0]);
    descriptor.extend_from_slice(&0u64.to_le_bytes());
    let caller = RuntimeDirectEvalCaller::validate(
        RuntimeDirectEvalCallerDescriptor::decode(&descriptor).unwrap(),
        [],
    )
    .unwrap();
    let direct = scriptw_ir::lower_runtime_direct_eval(
        &caller,
        &"proofLocal = 5;".encode_utf16().collect::<Vec<_>>(),
        HostSurfacePolicy::default(),
    )
    .unwrap();
    let raw = crate::emit_runtime_direct_eval(&direct, &runtime, &profile).unwrap();
    let fast = crate::emit_runtime_direct_eval_with_validated_runtime(&direct, &proof).unwrap();
    assert_eq!(raw.bytes(), fast.bytes());
    assert_eq!(raw.caller(), fast.caller());
    assert_eq!(raw.source(), fast.source());
    assert!(Arc::ptr_eq(fast.runtime(), &runtime));
    assert!(Arc::ptr_eq(
        &prefix,
        runtime.compiler_owned_pool.get().unwrap().as_ref().unwrap()
    ));
    assert!(crate::emit_runtime_direct_eval(
        &direct,
        &runtime,
        &IntlCompilationProfile::Conformance
    )
    .is_err());
}
