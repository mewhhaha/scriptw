use super::*;
use crate::foreign::{
    ForeignErrorTag, ForeignFunctionDeclaration, ForeignModuleDeclaration, ForeignResultType,
    ForeignValueType,
};

fn registry(import: &str, count: usize, result: ForeignResultType) -> ForeignModuleRegistry {
    ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
        "dd:host",
        vec![ForeignFunctionDeclaration::new(
            "add",
            "dd",
            import,
            vec![ForeignValueType::Number; count],
            result,
        )
        .unwrap()],
    )
    .unwrap()])
    .unwrap()
}

#[test]
fn foreign_capability_schema_is_part_of_program_cache_authority() {
    let base = CompileOptions::default();
    let normal = registry("add", 2, ForeignResultType::Number);
    let schemas = [
        ForeignModuleRegistry::default(),
        normal.clone(),
        registry("renamed", 2, ForeignResultType::Number),
        registry("add", 1, ForeignResultType::Number),
        registry("add", 2, ForeignResultType::Void),
        normal.with_error_tag(ForeignErrorTag::code_v1()),
    ];
    let keys = schemas
        .into_iter()
        .map(|foreign_modules| {
            let options = CompileOptions {
                foreign_modules,
                ..base.clone()
            };
            // Fixed compiler fingerprint isolates application capability identity.
            program_wasm_cache_key_with_compiler_fingerprint(
                "import {add} from 'dd:host'; add(20,22);",
                ParseGoal::Module,
                &options,
                None,
                &[17; 32],
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        keys.len(),
        6,
        "all incompatible declarations need distinct P keys"
    );
}

#[test]
fn foreign_artifact_roundtrip_keeps_only_linked_capabilities_without_sources() {
    let add = registry("add", 2, ForeignResultType::Number);
    let unused = ForeignModuleDeclaration::new(
        "dd:unused",
        vec![ForeignFunctionDeclaration::new(
            "unused",
            "unused",
            "unused",
            vec![],
            ForeignResultType::Void,
        )
        .unwrap()],
    )
    .unwrap();
    let available = ForeignModuleRegistry::new(vec![add.modules()[0].clone(), unused]).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let unit = engine
        .compile_module(
            "import {add} from 'dd:host'; print(add(20,22));",
            CompileOptions {
                foreign_modules: available,
                module_loading_policy: ModuleLoadingPolicy::RejectAll,
                ..CompileOptions::default()
            },
        )
        .unwrap();
    let artifact = engine.emit_wasm(&unit).unwrap();
    assert_eq!(artifact.foreign_modules().unwrap(), add);
    let encoded = artifact
        .encode_portable_bundle(&IntlCompilationProfile::Minimal)
        .unwrap();
    let admitted = engine.load_portable_bundle(&encoded).unwrap();
    assert_eq!(admitted.bytes, artifact.bytes);
    assert_eq!(admitted.foreign_modules().unwrap(), add);
    assert!(
        admitted.compilation_policy.is_none(),
        "source-free loading has no compiler closure registry"
    );
}

#[test]
fn foreign_registry_never_authorizes_the_interpreter_oracle() {
    let engine = Engine::new(RealmBuilder::new().build());
    let error = engine
        .observe_script(
            "0;",
            CompileOptions {
                foreign_modules: registry("add", 2, ForeignResultType::Number),
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::SpecExec,
                ..RunOptions::default()
            },
        )
        .unwrap_err();
    assert!(error.to_string().contains("embedder-owned Linker"));
}
