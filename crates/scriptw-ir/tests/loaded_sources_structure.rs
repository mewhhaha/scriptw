const MODULES_SOURCE: &str = include_str!("../src/modules/mod.rs");
const OWNER_SOURCE: &str = include_str!("../src/modules/loaded_sources.rs");
const MODULE_KEY_SOURCE: &str = include_str!("../src/modules/module_key.rs");
const GRAPH_SOURCE: &str = include_str!("../src/modules/graph.rs");
const GRAPH_TESTS_SOURCE: &str = include_str!("../src/modules/graph_tests.rs");
const GRAPH_BUILD_SOURCE: &str = include_str!("../src/modules/graph_build.rs");
const GRAPH_RESOLUTION_SOURCE: &str = include_str!("../src/modules/graph_resolution.rs");
const LINK_SOURCE: &str = include_str!("../src/modules/link.rs");
const NAMESPACE_SOURCE: &str = include_str!("../src/modules/namespace.rs");
const DYNAMIC_SOURCE: &str = include_str!("../src/modules/dynamic.rs");
const ADMISSION_SOURCE: &str = include_str!("../src/modules/admission.rs");
const ADMISSION_CLOSURE_SOURCE: &str = include_str!("../src/modules/admission/closure.rs");
const SYNCHRONOUS_SOURCE: &str = include_str!("../src/modules/synchronous_source.rs");
const LOWERING_SOURCE: &str = include_str!("../src/lowering.rs");
const LOWERING_MODULE_GRAPH_SOURCE: &str = include_str!("../src/lowering/module_graph.rs");
const LIB_SOURCE: &str = include_str!("../src/lib.rs");
const ENGINE_LOADER_SOURCE: &str = include_str!("../../scriptw-engine/src/module_loader.rs");
const ENGINE_LIB_SOURCE: &str = include_str!("../../scriptw-engine/src/lib.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end after {start}: {end}"))
        .0
}

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
fn loaded_sources_have_one_private_owner_and_narrow_public_facade() {
    assert_eq!(MODULES_SOURCE.matches("\nmod loaded_sources;\n").count(), 1);
    assert_eq!(
        public_reexports(MODULES_SOURCE, "loaded_sources"),
        vec![vec!["ModuleGraphSources", "ModuleKindIr", "ModuleSourceIr"]]
    );
    assert!(!MODULES_SOURCE.contains("\npub mod loaded_sources;\n"));
    assert!(!MODULES_SOURCE.contains("\nmod loaded_sources {\n"));
    assert!(!GRAPH_SOURCE.contains("pub struct ModuleSourceIr"));
    assert!(!GRAPH_SOURCE.contains("enum ModuleParse"));
    assert!(!GRAPH_SOURCE.contains("pub struct ModuleGraphSources"));
    assert!(!GRAPH_SOURCE.contains("impl ModuleGraphSources"));
    assert!(!GRAPH_SOURCE.contains("ANONYMOUS_MODULE_KEY"));
    assert!(!GRAPH_TESTS_SOURCE.contains("pub struct ModuleSourceIr"));
    assert!(!GRAPH_TESTS_SOURCE.contains("enum ModuleParse"));
    assert!(!GRAPH_TESTS_SOURCE.contains("pub struct ModuleGraphSources"));
    assert!(!GRAPH_TESTS_SOURCE.contains("impl ModuleGraphSources"));
    assert!(!GRAPH_TESTS_SOURCE.contains("ANONYMOUS_MODULE_KEY"));
    assert_eq!(OWNER_SOURCE.matches("pub struct ModuleSourceIr").count(), 1);
    assert_eq!(OWNER_SOURCE.matches("enum ModuleParse").count(), 1);
    assert_eq!(
        OWNER_SOURCE
            .matches("pub struct ModuleGraphSources")
            .count(),
        1
    );
    assert_eq!(OWNER_SOURCE.matches("impl ModuleGraphSources").count(), 1);
    assert!(!OWNER_SOURCE.contains("super::graph"));
    assert_eq!(LIB_SOURCE.matches("ModuleSourceIr").count(), 1);
    assert_eq!(LIB_SOURCE.matches("ModuleGraphSources").count(), 1);
}

#[test]
fn module_source_keeps_private_parse_state_and_the_exact_public_method_inventory() {
    let fields = bounded(
        OWNER_SOURCE,
        "pub struct ModuleSourceIr {",
        "/// Record syntax is part of host identity",
    );
    assert_eq!(
        code_without_whitespace(fields),
        "pub(super)foreign:Option<crate::foreign::ForeignModuleSource>,\
         key:ModuleKey,meta_url:String,pub(super)parse:ModuleParse,\
         pub(super)prepared_script_import_sites:Vec<super::DynamicImportSiteIr>,}"
    );

    let parse = bounded(
        OWNER_SOURCE,
        "pub(super) enum ModuleParse {",
        "impl ModuleSourceIr",
    );
    let parse = code_without_whitespace(parse);
    assert_eq!(
        parse,
        "Module(ParsedModule),Json(scriptw_front::ParsedJson),Bytes(Vec<u8>),Text(String),\
         ModuleSource{source_tag:String,},\
         JsonRejected{source_text:String,error:scriptw_front::JsonParseError,},\
         ScriptEntry(ParsedScript),Rejected{source:SourceUnit,error:scriptw_front::ParseError,},}"
    );
    assert!(!OWNER_SOURCE.contains("pub enum ModuleParse"));
    assert_eq!(
        OWNER_SOURCE.matches("pub(super) enum ModuleParse").count(),
        1
    );
    assert_eq!(
        OWNER_SOURCE
            .matches("pub(super) parse: ModuleParse")
            .count(),
        1
    );

    let methods = bounded(
        OWNER_SOURCE,
        "impl ModuleSourceIr {",
        "/// A request with no entry here",
    );
    let mut public_methods = methods
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line.starts_with("pub ") {
                return None;
            }
            let (method, is_const) = line
                .strip_prefix("pub fn ")
                .map(|method| (method, false))
                .or_else(|| {
                    line.strip_prefix("pub const fn ")
                        .map(|method| (method, true))
                })
                .unwrap_or_else(|| panic!("unreviewed public item: {line}"));
            Some((method.split_once('(').expect("method name").0, is_const))
        })
        .collect::<Vec<_>>();
    public_methods.sort();
    let mut expected_methods = [
        "foreign",
        "foreign_modules",
        "may_contain_direct_eval_call",
        "include_prepared_script_import_sites",
        "realm_module_requests",
        "new",
        "json",
        "bytes",
        "text",
        "module_source",
        "record_key",
        "payload_bytes",
        "from_parsed",
        "from_parsed_script",
        "key",
        "source_text",
        "meta_url",
        "module_requests",
        "module_loading_requests",
        "dynamic_import_sites",
        "script_has_dynamic_import_sites",
        "goal",
    ]
    .into_iter()
    .map(|method| (method, false))
    .chain([("kind", true)])
    .collect::<Vec<_>>();
    expected_methods.sort();
    assert_eq!(
        public_methods, expected_methods,
        "exact public method inventory"
    );
    assert_eq!(OWNER_SOURCE.matches("#[doc(hidden)]").count(), 3);
    assert_eq!(
        OWNER_SOURCE.matches("scan_module_requests(source)").count(),
        1
    );
    assert_eq!(
        OWNER_SOURCE
            .matches("scan_script_module_requests(source)")
            .count(),
        1
    );
}

#[test]
fn graph_sources_keep_the_exact_public_closure_record_and_single_constructor() {
    let fields = bounded(
        OWNER_SOURCE,
        "pub struct ModuleGraphSources {",
        "impl ModuleGraphSources",
    );
    assert_eq!(
        code_without_whitespace(fields),
        "pubrealm_requests:std::collections::BTreeMap<ModuleRequestKeyIr,super::RealmModuleResolutionIr>,\
         pubmodules:Vec<ModuleSourceIr>,pubentry:ModuleUnitId,\
         pubresolutions:Vec<(ModuleUnitId,ModuleRequestKeyIr,ModuleUnitId)>,}"
    );
    assert_eq!(OWNER_SOURCE.matches("pub fn single(").count(), 1);
    let single_constructor = OWNER_SOURCE
        .split_once("pub fn single(source: &ParsedModule) -> Self {")
        .expect("single parsed-source constructor")
        .1;
    assert_eq!(
        code_without_whitespace(single_constructor)
            .matches("realm_requests:Default::default()")
            .count(),
        1
    );
    assert!(OWNER_SOURCE.contains(".unwrap_or_else(|| ANONYMOUS_MODULE_KEY.to_string())"));
    assert!(OWNER_SOURCE.contains("ModuleKey::from_host(key.clone())"));
    assert_eq!(
        MODULE_KEY_SOURCE
            .matches("pub const ANONYMOUS_MODULE_KEY: &str = \"<entry>\";")
            .count(),
        1
    );
    assert_eq!(
        public_reexports(MODULES_SOURCE, "module_key"),
        vec![vec!["ANONYMOUS_MODULE_KEY", "ModuleKey", "ModuleRecordKey"]]
    );
}

#[test]
fn loaded_source_callers_use_the_facade_while_construction_has_one_private_owner() {
    for source in [
        GRAPH_SOURCE,
        GRAPH_TESTS_SOURCE,
        GRAPH_BUILD_SOURCE,
        LINK_SOURCE,
        NAMESPACE_SOURCE,
        ADMISSION_SOURCE,
        ADMISSION_CLOSURE_SOURCE,
        SYNCHRONOUS_SOURCE,
        LOWERING_MODULE_GRAPH_SOURCE,
        ENGINE_LOADER_SOURCE,
        ENGINE_LIB_SOURCE,
    ] {
        assert!(source.contains("ModuleGraphSources"));
        assert!(!source.contains("pub struct ModuleGraphSources"));
        assert!(!source.contains("impl ModuleGraphSources"));
        assert!(!source.contains("pub struct ModuleSourceIr"));
        assert!(!source.contains("impl ModuleSourceIr"));
        assert!(!source.contains("enum ModuleParse"));
    }
    let (dynamic_production, dynamic_tests) = DYNAMIC_SOURCE
        .split_once("\n#[cfg(test)]\nmod tests {")
        .expect("dynamic import graph fixtures are test-only");
    assert!(!dynamic_production.contains("ModuleGraphSources"));
    assert!(dynamic_tests.contains("ModuleGraphSources"));
    // The public graph-lowering entry points live in `lowering/module_graph.rs`;
    // `lowering.rs` keeps only the one-node `ModuleGraphSources::single` path.
    assert!(LOWERING_SOURCE.contains("&ModuleGraphSources::single(source)"));
    for entry in ["lower_module_graph", "lower_script_graph"] {
        assert!(LOWERING_MODULE_GRAPH_SOURCE.contains(&format!(
            "pub fn {entry}(sources: &ModuleGraphSources) -> ProgramIr"
        )));
        assert!(!LOWERING_SOURCE.contains(&format!("pub fn {entry}(")));
    }
    assert!(!dynamic_production.contains("ModuleSourceIr"));
    assert!(dynamic_tests.contains("ModuleSourceIr::new("));
    assert!(ENGINE_LOADER_SOURCE.contains("ModuleSourceIr::from_parsed_script("));

    assert!(GRAPH_BUILD_SOURCE.contains("pub(crate) fn build_graph("));
    assert!(!GRAPH_SOURCE.contains("pub(crate) fn build_graph("));
    for source in [
        ADMISSION_SOURCE,
        ADMISSION_CLOSURE_SOURCE,
        SYNCHRONOUS_SOURCE,
    ] {
        assert!(!source.contains("fn build_graph("));
        for constructor in ["new", "from_parsed", "from_parsed_script"] {
            assert!(!source.contains(&format!("ModuleSourceIr::{constructor}(")));
        }
    }
    let identity = bounded(
        ADMISSION_SOURCE,
        "fn source_identity(",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
    );
    assert_eq!(
        code_without_whitespace(identity),
        "source:&ModuleSourceIr)->LoadedSourceIdentity<'_>{matchsource.goal(){\
         ParseGoal::Script=>LoadedSourceIdentity::Script(source.key()),\
         ParseGoal::Module=>LoadedSourceIdentity::Module(source.key(),source.kind()),}}",
        "closure identity must read the retained source goal through the facade"
    );
    assert!(!ADMISSION_CLOSURE_SOURCE.contains("ModuleSourceIr"));
    assert!(!SYNCHRONOUS_SOURCE.contains("ModuleSourceIr"));
    assert!(GRAPH_SOURCE.contains("pub(crate) fn link(graph: &mut ModuleGraphIr)"));
    assert!(GRAPH_RESOLUTION_SOURCE.contains("pub fn resolve_export("));
    assert!(!GRAPH_SOURCE.contains("pub fn resolve_export("));
}
