use super::*;
use crate::runtime_artifact::RuntimeLayout;

#[test]
fn cold_and_warm_runtime_identity_ignore_the_owned_pool_memo_state() {
    let actual = super::tests::runtime();
    let layout = &actual.layout;
    let cold = Arc::new(RuntimeArtifact::new(
        actual.bytes.clone(),
        RuntimeLayout::new(
            layout.imported_functions,
            layout.function_types.clone(),
            layout.globals.clone(),
            layout.pool,
        ),
        actual.layout_admission.unwrap(),
        actual.source_compilation_profile(),
    ));
    assert!(cold.compiler_owned_pool.get().is_none());
    assert_eq!(actual.as_ref(), cold.as_ref());
    let before = (cold.key(), cold.wasm_sha256(), format!("{cold:?}"));
    let actual_memo = actual
        .compiler_owned_pool
        .get()
        .map(|result| Arc::as_ptr(result.as_ref().unwrap()));
    let program = scriptw_ir::lower_runtime_function(
        scriptw_ir::DynamicFunctionKind::Ordinary,
        &["return 19;".encode_utf16().collect()],
        scriptw_ir::HostSurfacePolicy::default(),
    )
    .unwrap();
    let first =
        ValidatedRuntimeArtifact::new(Arc::clone(&cold), IntlCompilationProfile::Minimal).unwrap();
    crate::emit_runtime_function_with_validated_runtime(&program, &first).unwrap();
    let memo = Arc::clone(cold.compiler_owned_pool.get().unwrap().as_ref().unwrap());
    assert_eq!(
        before,
        (cold.key(), cold.wasm_sha256(), format!("{cold:?}"))
    );
    assert_eq!(actual.as_ref(), cold.as_ref());
    let second =
        ValidatedRuntimeArtifact::new(Arc::clone(&cold), IntlCompilationProfile::Minimal).unwrap();
    crate::emit_runtime_function_with_validated_runtime(&program, &second).unwrap();
    assert!(Arc::ptr_eq(
        &memo,
        cold.compiler_owned_pool.get().unwrap().as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(first.runtime(), second.runtime()));
    assert_eq!(
        actual_memo,
        actual
            .compiler_owned_pool
            .get()
            .map(|result| Arc::as_ptr(result.as_ref().unwrap()))
    );
}
