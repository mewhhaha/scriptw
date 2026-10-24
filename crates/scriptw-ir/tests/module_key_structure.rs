const MODULES_SOURCE: &str = include_str!("../src/modules/mod.rs");
const OWNER_SOURCE: &str = include_str!("../src/modules/module_key.rs");
const LOADED_SOURCES_SOURCE: &str = include_str!("../src/modules/loaded_sources.rs");
const GRAPH_SOURCE: &str = include_str!("../src/modules/graph.rs");
const GRAPH_TESTS_SOURCE: &str = include_str!("../src/modules/graph_tests.rs");
const GRAPH_BUILD_SOURCE: &str = include_str!("../src/modules/graph_build.rs");
const GRAPH_RESOLUTION_SOURCE: &str = include_str!("../src/modules/graph_resolution.rs");
const RECORD_SOURCE: &str = include_str!("../src/modules/record.rs");
const DYNAMIC_SOURCE: &str = include_str!("../src/modules/dynamic.rs");
const EARLY_SOURCE: &str = include_str!("../src/modules/early.rs");
const LINK_SOURCE: &str = include_str!("../src/modules/link.rs");
const LINK_ERROR_SOURCE: &str = include_str!("../src/modules/link_error.rs");
const NAMESPACE_SOURCE: &str = include_str!("../src/modules/namespace.rs");
const LIB_SOURCE: &str = include_str!("../src/lib.rs");
const ENGINE_LOADER_SOURCE: &str = include_str!("../../scriptw-engine/src/module_loader.rs");
const ENGINE_LIB_SOURCE: &str = include_str!("../../scriptw-engine/src/lib.rs");

fn code_without_whitespace(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .flat_map(str::chars)
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn public_reexports(source: &str, owner: &str) -> Vec<Vec<String>> {
    let source = code_without_whitespace(source);
    source
        .split(&format!("pubuse{owner}::"))
        .skip(1)
        .map(|suffix| {
            let declaration = suffix.split_once(';').expect("complete public re-export").0;
            let members = declaration
                .strip_prefix('{')
                .and_then(|members| members.strip_suffix('}'))
                .unwrap_or(declaration);
            let mut members = members
                .split(',')
                .filter(|member| !member.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            members.sort();
            members
        })
        .collect()
}

#[test]
fn module_key_has_one_private_owner_and_narrow_public_facade() {
    assert_eq!(MODULES_SOURCE.matches("\nmod module_key;\n").count(), 1);
    assert_eq!(
        public_reexports(MODULES_SOURCE, "module_key"),
        vec![vec!["ANONYMOUS_MODULE_KEY", "ModuleKey", "ModuleRecordKey"]]
    );
    assert!(!MODULES_SOURCE.contains("\npub mod module_key;\n"));
    assert!(!MODULES_SOURCE.contains("\nmod module_key {\n"));
    assert!(!GRAPH_SOURCE.contains("pub struct ModuleKey"));
    assert!(!GRAPH_SOURCE.contains("impl ModuleKey"));
    assert!(!GRAPH_TESTS_SOURCE.contains("pub struct ModuleKey"));
    assert!(!GRAPH_TESTS_SOURCE.contains("impl ModuleKey"));
    assert_eq!(OWNER_SOURCE.matches("pub struct ModuleKey").count(), 1);
    assert_eq!(OWNER_SOURCE.matches("impl ModuleKey").count(), 1);
    assert_eq!(
        OWNER_SOURCE
            .matches("pub const ANONYMOUS_MODULE_KEY: &str = \"<entry>\";")
            .count(),
        1
    );
    assert_eq!(LIB_SOURCE.matches("ModuleKey").count(), 1);
}

#[test]
fn module_key_keeps_opaque_storage_and_one_host_constructor() {
    assert!(OWNER_SOURCE.starts_with("/// A stable module identity"));
    assert!(OWNER_SOURCE.contains("[`crate::ModuleRequestIr::specifier`]"));
    assert_eq!(
        OWNER_SOURCE
            .matches("pub struct ModuleKey(String);")
            .count(),
        1
    );
    assert_eq!(OWNER_SOURCE.matches("pub fn from_host(").count(), 1);
    assert_eq!(OWNER_SOURCE.matches("pub fn as_str(").count(), 1);
    assert!(!OWNER_SOURCE.contains("impl From<"));
    assert!(OWNER_SOURCE.contains("pub struct ModuleRecordKey"));
    assert!(OWNER_SOURCE.contains("locator: ModuleKey"));
    assert!(OWNER_SOURCE.contains("kind: super::ModuleKindIr"));
    assert!(!OWNER_SOURCE.contains("pub struct ModuleKey(pub String)"));

    let methods = OWNER_SOURCE
        .split_once("impl ModuleKey {")
        .expect("ModuleKey methods")
        .1
        .split_once("/// Key a one-node graph uses")
        .expect("anonymous module key owner")
        .0;
    assert_eq!(
        code_without_whitespace(methods),
        "#[must_use]pubfnfrom_host(key:implInto<String>)->Self{Self(key.into())}\
         #[must_use]pubfnas_str(&self)->&str{&self.0}}"
    );
}

#[test]
fn module_key_callers_keep_the_public_identity_domain_without_compatibility_exports() {
    // Callers may add parse-goal and request projections without introducing
    // another identity owner or converting graph identities to specifier text.
    for source in [
        LOADED_SOURCES_SOURCE,
        GRAPH_TESTS_SOURCE,
        GRAPH_BUILD_SOURCE,
        GRAPH_RESOLUTION_SOURCE,
        RECORD_SOURCE,
        DYNAMIC_SOURCE,
        EARLY_SOURCE,
        LINK_SOURCE,
        LINK_ERROR_SOURCE,
        NAMESPACE_SOURCE,
        ENGINE_LOADER_SOURCE,
        ENGINE_LIB_SOURCE,
    ] {
        assert!(source.contains("ModuleKey"));
        assert!(!source.contains("pub struct ModuleKey"));
        assert!(!source.contains("impl ModuleKey"));
        assert!(!source.contains("pub type ModuleKey"));
    }
    // The graph stores the typed record identity (host locator plus kind).
    assert!(GRAPH_SOURCE.contains("ModuleRecordKey"));
    assert!(!GRAPH_SOURCE.contains("pub struct ModuleRecordKey"));
    assert!(!GRAPH_SOURCE.contains("impl ModuleRecordKey"));
    assert!(!GRAPH_SOURCE.contains("pub type ModuleRecordKey"));
    let (dynamic_production, dynamic_tests) = DYNAMIC_SOURCE
        .split_once("\n#[cfg(test)]\nmod tests {")
        .expect("dynamic import fixture keys are test-only");
    assert!(dynamic_production.contains("key: &ModuleKey,"));
    assert!(!dynamic_production.contains("ModuleKey::from_host("));
    assert!(dynamic_tests.contains("ModuleKey::from_host("));
    assert!(LOADED_SOURCES_SOURCE.contains("ANONYMOUS_MODULE_KEY"));
    assert!(RECORD_SOURCE.contains("ModuleKey::from_host(ANONYMOUS_MODULE_KEY)"));
    assert!(!LINK_SOURCE.contains("ANONYMOUS_MODULE_KEY"));
    assert!(!GRAPH_SOURCE.contains("ANONYMOUS_MODULE_KEY"));
    assert!(LOADED_SOURCES_SOURCE.contains("key: ModuleKey,"));
    assert!(GRAPH_SOURCE.contains("pub keys: BTreeMap<ModuleRecordKey, ModuleUnitId>,"));
    let record = code_without_whitespace(RECORD_SOURCE);
    assert!(record.contains("pubkey:ModuleKey,"));
    assert!(record.contains("key:ModuleKey,"));
    let loader = code_without_whitespace(ENGINE_LOADER_SOURCE);
    assert_eq!(
        public_reexports(ENGINE_LOADER_SOURCE, "scriptw_ir"),
        vec![vec![
            "ModuleKey",
            "ModuleKindIr",
            "ModuleRecordKey",
            "ModuleRequestKeyIr"
        ]]
    );
    assert!(loader.contains("referrer:Option<&ModuleKey>,"));
    assert!(loader.contains("Result<ModuleKey,ModuleLoadError>"));
    assert!(
        loader.contains("fnload(&self,key:&ModuleRecordKey)->Result<LoadedModule,ModuleLoadError>")
    );
    assert!(LINK_ERROR_SOURCE.contains(
        "InconsistentLoad {\n        /// The key loaded inconsistently.\n        key: ModuleKey,"
    ));
}
