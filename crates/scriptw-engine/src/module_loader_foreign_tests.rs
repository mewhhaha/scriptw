use super::*;
use scriptw_ir::foreign::{
    ForeignFunctionDeclaration, ForeignModuleDeclaration, ForeignModuleRegistry, ForeignResultType,
    ForeignValueType,
};

fn registry() -> ForeignModuleRegistry {
    ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
        "dd:host",
        vec![ForeignFunctionDeclaration::new(
            "add",
            "dd",
            "add",
            vec![ForeignValueType::Number; 2],
            ForeignResultType::Number,
        )
        .unwrap()],
    )
    .unwrap()])
    .unwrap()
}
struct NeverConsult;
impl HostModuleLoader for NeverConsult {
    fn resolve(
        &self,
        _: Option<&ModuleKey>,
        _: &ModuleRequestKeyIr,
    ) -> Result<ModuleKey, ModuleLoadError> {
        panic!("declared native module must resolve before fallback host IO");
    }
    fn load(&self, _: &ModuleRecordKey) -> Result<LoadedModule, ModuleLoadError> {
        panic!("declared native module must load before fallback host IO");
    }
}
fn entry(source: &str) -> ModuleEntry {
    ModuleEntry::InMemory {
        locator: "entry.mjs".to_owned(),
        source_text: source.to_owned(),
    }
}

#[test]
fn declared_capability_resolves_before_host_io_and_reuses_one_module() {
    let mut declarations = registry().modules().to_vec();
    declarations.push(
        ForeignModuleDeclaration::new(
            "unused:host",
            vec![ForeignFunctionDeclaration::new(
                "unused",
                "unused",
                "unused",
                vec![],
                ForeignResultType::Void,
            )
            .unwrap()],
        )
        .unwrap(),
    );
    let loader = ForeignModuleLoader::new(
        ForeignModuleRegistry::new(declarations).unwrap(),
        Box::new(NeverConsult),
    );
    let sources = load_module_graph(&entry("import {add as first} from 'dd:host';import * as second from 'dd:host';export const values=[first,second.add];"),&loader).unwrap();
    assert_eq!(sources.modules.len(), 2);
    assert!(sources.modules[1].foreign_modules().is_some());
    let lowered = scriptw_ir::lower_module_graph(&sources);
    assert!(lowered.is_wasm_supported(), "{:?}", lowered.diagnostics);
    assert_eq!(lowered.script.unwrap().foreign_modules, registry());
}

#[test]
fn missing_exports_and_undeclared_modules_keep_clear_link_rejections() {
    let loader = ForeignModuleLoader::new(registry(), Box::new(RejectAllModuleLoader));
    for source in [
        "import {missing} from 'dd:host';missing;",
        "import {add} from 'undeclared:host';add;",
    ] {
        let graph = load_module_graph(&entry(source), &loader).unwrap();
        let lowered = scriptw_ir::lower_module_graph(&graph);
        assert!(!lowered.is_wasm_supported(), "{source}");
        assert!(!lowered.diagnostics.is_empty());
    }
}

#[test]
fn declared_foreign_modules_do_not_accept_import_attributes_or_touch_fallback() {
    let loader = ForeignModuleLoader::new(registry(), Box::new(NeverConsult));
    let request = ModuleRequestKeyIr::try_new(
        "dd:host",
        [scriptw_ir::ImportAttributeIr {
            key: "type".into(),
            value: "json".into(),
        }],
    )
    .unwrap();
    assert!(matches!(
        loader.resolve(None, &request),
        Err(ModuleLoadError::UnsupportedAttribute { .. })
    ));
}

#[test]
fn embedded_catalog_adds_explicit_native_edges_and_rejects_conflicting_authority() {
    let source = ModuleSourceIr::new(
        ModuleKey::from_host("entry"),
        "import {add} from 'dd:host';export const f=add;".to_owned(),
        "entry".to_owned(),
    );
    let mut graph = ModuleGraphSources {
        modules: vec![source.clone()],
        entry: 0,
        resolutions: vec![],
        realm_requests: Default::default(),
    };
    include_foreign_module_registry(&mut graph, &registry()).unwrap();
    assert_eq!(graph.modules.len(), 2);
    let lowered = scriptw_ir::lower_complete_module_catalog(
        &graph,
        scriptw_ir::HostSurfacePolicy::default(),
        None,
    );
    assert!(lowered.is_wasm_supported(), "{:?}", lowered.diagnostics);
    assert_eq!(lowered.script.unwrap().foreign_modules, registry());
    let mut conflict = ModuleGraphSources {
        modules: vec![
            source,
            ModuleSourceIr::new(
                ModuleKey::from_host("embedded"),
                "export const add=1;".into(),
                "embedded".into(),
            ),
        ],
        entry: 0,
        resolutions: vec![(0, ModuleRequestKeyIr::plain("dd:host"), 1)],
        realm_requests: Default::default(),
    };
    assert!(matches!(
        include_foreign_module_registry(&mut conflict, &registry()),
        Err(ModuleLoadError::Denied { .. })
    ));
    // A host can explicitly own the native record's canonical key without
    // already resolving the request. Key reuse must reject that contradiction.
    conflict.resolutions.clear();
    conflict.modules[1] = ModuleSourceIr::new(
        ModuleKey::from_host("scriptw:foreign/dd:host"),
        "export const add=1;".into(),
        "embedded".into(),
    );
    assert!(matches!(
        include_foreign_module_registry(&mut conflict, &registry()),
        Err(ModuleLoadError::Denied { .. })
    ));
}

#[test]
fn empty_registry_does_not_change_embedded_sources_or_authority() {
    let mut graph = ModuleGraphSources {
        modules: vec![ModuleSourceIr::new(
            ModuleKey::from_host("entry"),
            "export const value=1;".into(),
            "entry".into(),
        )],
        entry: 0,
        resolutions: vec![],
        realm_requests: Default::default(),
    };
    let original = graph.clone();
    include_foreign_module_registry(&mut graph, &ForeignModuleRegistry::default()).unwrap();
    assert_eq!(graph, original);
}

fn prepare_embedded_foreign(source: &str) -> crate::PreparedCompilation {
    prepare_embedded_foreign_catalog(source, vec![], vec![])
}

fn prepare_embedded_foreign_catalog(
    source: &str,
    modules: Vec<scriptw_runtime::EmbeddedModuleSourceInput>,
    resolutions: Vec<scriptw_runtime::EmbeddedModuleResolutionInput>,
) -> crate::PreparedCompilation {
    let graph = scriptw_runtime::EmbeddedModuleGraph::try_new(
        scriptw_runtime::EmbeddedModuleEntryInput {
            goal: scriptw_runtime::EmbeddedModuleGoal::Script,
            identity: "entry.js".to_owned(),
            source: source.to_owned(),
            meta_url: "scriptw://entry.js".to_owned(),
        },
        modules,
        resolutions,
    )
    .unwrap();
    let options = crate::CompileOptions {
        module_loading_policy: crate::ModuleLoadingPolicy::Embedded(graph),
        foreign_modules: registry(),
        ..crate::CompileOptions::default()
    };
    crate::Engine::new(crate::RealmBuilder::new().build())
        .prepare_compilation(source, ParseGoal::Script, &options)
        .unwrap()
}

#[test]
fn embedded_prepared_script_import_closes_declared_foreign_capability() {
    let prepared = prepare_embedded_foreign("eval('import(\"dd:host\")'); true;");
    assert!(
        prepared.ir.is_wasm_supported(),
        "{:?}",
        prepared.ir.diagnostics
    );
    let graph = prepared.modules.graph().expect("retained embedded catalog");
    assert_eq!(
        graph.modules.len(),
        2,
        "only the declared native module is added"
    );
    assert!(graph
        .resolutions
        .iter()
        .any(|(owner, request, target)| *owner == graph.entry
            && request.specifier() == "dd:host"
            && graph.modules[*target as usize].foreign_modules() == Some(&registry())));
    assert!(
        graph.realm_requests.is_empty(),
        "prepared Script keeps its original referrer"
    );
    assert_eq!(
        prepared.ir.script.as_ref().unwrap().foreign_modules,
        registry()
    );
}

#[test]
fn embedded_prepared_realm_request_closes_declared_foreign_capability() {
    let prepared = prepare_embedded_foreign(
        r#"new ShadowRealm().evaluate('void new ShadowRealm().importValue("dd:host", "add")'); true;"#,
    );
    assert!(
        prepared.ir.is_wasm_supported(),
        "{:?}",
        prepared.ir.diagnostics
    );
    let graph = prepared.modules.graph().expect("retained embedded catalog");
    assert_eq!(graph.modules.len(), 2);
    let request = ModuleRequestKeyIr::plain("dd:host");
    let Some(scriptw_ir::RealmModuleResolutionIr::Loaded(target)) =
        graph.realm_requests.get(&request)
    else {
        panic!("prepared Realm request must retain its explicit foreign resolution");
    };
    assert_eq!(
        graph.modules[*target as usize].foreign_modules(),
        Some(&registry())
    );
    assert!(
        graph.resolutions.is_empty(),
        "Realm authority is not an entry Script edge"
    );
    assert_eq!(
        prepared.ir.script.as_ref().unwrap().foreign_modules,
        registry()
    );
}

#[test]
fn embedded_computed_import_closes_foreign_dependencies_of_owned_catalog_records() {
    let prepared = prepare_embedded_foreign_catalog(
        "var specifier='dep';import(specifier);true;",
        vec![scriptw_runtime::EmbeddedModuleSourceInput {
            identity: "dep.js".into(),
            source: "import {add} from 'dd:host';export const f=add;".into(),
            meta_url: "scriptw://dep.js".into(),
        }],
        vec![scriptw_runtime::EmbeddedModuleResolutionInput {
            referrer: scriptw_runtime::EmbeddedModuleReferrer::Script("entry.js".into()),
            specifier: "dep".into(),
            attributes: vec![],
            target: "dep.js".into(),
        }],
    );
    assert!(
        prepared.ir.is_wasm_supported(),
        "{:?}",
        prepared.ir.diagnostics
    );
    let graph = prepared.modules.graph().expect("retained complete catalog");
    assert!(
        graph.modules[graph.entry as usize]
            .module_loading_requests()
            .unwrap()
            .is_empty(),
        "the original entry has no literal import discovery edge"
    );
    assert_eq!(
        graph.modules.len(),
        3,
        "one native target joins the two owned source records"
    );
    let dependency = graph
        .modules
        .iter()
        .position(|module| module.key().as_str() == "dep.js")
        .unwrap() as u32;
    assert!(graph
        .resolutions
        .iter()
        .any(|(owner, request, target)| *owner == dependency
            && request.specifier() == "dd:host"
            && graph.modules[*target as usize].foreign_modules() == Some(&registry())));
    assert_eq!(
        prepared.ir.script.as_ref().unwrap().foreign_modules,
        registry()
    );
}

#[test]
fn ambient_native_request_cannot_reuse_an_entry_source_at_the_foreign_host_key() {
    let source = "import {add as nativeAdd} from 'dd:host';export function add(a,b){return 0;}export const f=nativeAdd;";
    let loader = ForeignModuleLoader::new(registry(), Box::new(NeverConsult));
    let colliding_entry = ModuleEntry::InMemory {
        locator: "scriptw:foreign/dd:host".into(),
        source_text: source.into(),
    };
    assert!(
        matches!(
            load_module_graph(&colliding_entry, &loader),
            Err(ModuleLoadError::Denied { .. })
        ),
        "a reused entry record must not substitute its JavaScript add for the native declaration"
    );
    let engine = crate::Engine::new(crate::RealmBuilder::new().build());
    let options = crate::CompileOptions {
        filename: Some("scriptw:foreign/dd:host".into()),
        module_loading_policy: crate::ModuleLoadingPolicy::RejectAll,
        foreign_modules: registry(),
        ..crate::CompileOptions::default()
    };
    assert!(
        engine.compile_module(source, options.clone()).is_err(),
        "the actual ambient compilation path rejects the contradictory entry identity"
    );
    let ordinary = crate::CompileOptions {
        filename: Some("entry.js".into()),
        ..options
    };
    let prepared = engine
        .prepare_compilation(source, ParseGoal::Module, &ordinary)
        .unwrap();
    assert!(
        prepared.ir.is_wasm_supported(),
        "{:?}",
        prepared.ir.diagnostics
    );
    assert_eq!(
        prepared.ir.script.as_ref().unwrap().foreign_modules,
        registry(),
        "the same source with an ordinary entry still binds the declared native wrapper"
    );
}
