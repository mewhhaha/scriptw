const MODULES_SOURCE: &str = include_str!("../src/modules/mod.rs");
const OWNER_SOURCE: &str = include_str!("../src/modules/module_unit.rs");
const GRAPH_SOURCE: &str = include_str!("../src/modules/graph.rs");
const GRAPH_BUILD_SOURCE: &str = include_str!("../src/modules/graph_build.rs");
const GRAPH_MATERIALIZATION_SOURCE: &str = include_str!("../src/modules/graph_materialization.rs");
const GRAPH_RESOLUTION_SOURCE: &str = include_str!("../src/modules/graph_resolution.rs");
const LOADED_SOURCES_SOURCE: &str = include_str!("../src/modules/loaded_sources.rs");
const RECORD_SOURCE: &str = include_str!("../src/modules/record.rs");
const LIB_SOURCE: &str = include_str!("../src/lib.rs");

fn code_without_whitespace(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .flat_map(str::chars)
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn module_unit_has_one_private_owner_and_narrow_public_facade() {
    assert_eq!(MODULES_SOURCE.matches("\nmod module_unit;\n").count(), 1);
    assert_eq!(
        MODULES_SOURCE
            .matches("pub use module_unit::ModuleUnitIr;")
            .count(),
        1
    );
    assert!(!MODULES_SOURCE.contains("\npub mod module_unit;\n"));
    assert!(!MODULES_SOURCE.contains("\nmod module_unit {\n"));
    assert!(!GRAPH_SOURCE.contains("pub struct ModuleUnitIr"));
    assert_eq!(OWNER_SOURCE.matches("pub struct ModuleUnitIr").count(), 1);
    assert_eq!(OWNER_SOURCE.matches("impl ModuleUnitIr").count(), 1);
    let methods = OWNER_SOURCE
        .split_once("impl ModuleUnitIr {")
        .expect("private payload projection")
        .1;
    assert_eq!(
        code_without_whitespace(methods),
        "pub(super)fnpayload_bytes(&self)->&[u8]{\
         ifletSome(foreign)=&self.foreign{return&foreign.canonical_bytes;}\
         ifletSome(tag)=self.record.module_source_tag(){returntag.as_bytes();}\
         self.record.bytes_source().unwrap_or_else(||self.source_text.as_bytes())}}"
    );
    assert_eq!(LIB_SOURCE.matches("ModuleUnitIr").count(), 1);
}

#[test]
fn module_unit_preserves_the_exact_public_field_record() {
    let fields = OWNER_SOURCE
        .split_once("pub struct ModuleUnitIr {")
        .expect("ModuleUnitIr fields")
        .1
        .split_once("\nimpl ModuleUnitIr {")
        .expect("field record ends before the private projection")
        .0;
    assert_eq!(
        code_without_whitespace(fields),
        "pub(super)foreign:Option<crate::foreign::ForeignModuleSource>,\
         pubrecord:ModuleRecordIr,pubsource_text:String,pubmeta_url:String,\
         pubhoist:Option<BlockIr>,pubbody:Option<BlockIr>,pubfunctions:Vec<FunctionIr>,\
         pubowned_env_bindings:Vec<OwnedEnvBindingIr>,\
         pubnamespaces:BTreeMap<ModuleNamespaceModeIr,ModuleNamespaceIr>,\
         pubresolved_imports:Vec<ResolvedBindingIr>,\
         pubresolved_indirect_exports:Vec<ResolvedBindingIr>,}"
    );
}

#[test]
fn graph_keeps_its_domain_algorithms_while_construction_has_one_owner() {
    assert_eq!(OWNER_SOURCE.matches("ModuleUnitIr").count(), 2);
    assert_eq!(GRAPH_SOURCE.matches("ModuleUnitIr").count(), 2);
    assert_eq!(GRAPH_BUILD_SOURCE.matches("ModuleUnitIr").count(), 2);
    assert_eq!(
        GRAPH_MATERIALIZATION_SOURCE.matches("ModuleUnitIr").count(),
        2
    );
    assert_eq!(RECORD_SOURCE.matches("ModuleUnitIr").count(), 2);
    assert!(GRAPH_BUILD_SOURCE.contains("graph.units.push(ModuleUnitIr {"));
    assert!(GRAPH_SOURCE.contains("pub units: Vec<ModuleUnitIr>,"));
    assert!(GRAPH_SOURCE.contains("pub fn unit(&self, id: ModuleUnitId) -> &ModuleUnitIr"));
    assert!(GRAPH_BUILD_SOURCE.contains("pub(crate) fn build_graph("));
    assert!(!GRAPH_SOURCE.contains("pub(crate) fn build_graph("));
    assert!(GRAPH_RESOLUTION_SOURCE.contains("pub fn resolve_export("));
    assert!(!GRAPH_SOURCE.contains("pub fn resolve_export("));
    assert!(!GRAPH_SOURCE.contains("pub struct ModuleSourceIr"));
    assert!(!GRAPH_SOURCE.contains("enum ModuleParse"));
    assert_eq!(
        LOADED_SOURCES_SOURCE
            .matches("pub struct ModuleSourceIr")
            .count(),
        1
    );
    assert_eq!(LOADED_SOURCES_SOURCE.matches("enum ModuleParse").count(), 1);
}
