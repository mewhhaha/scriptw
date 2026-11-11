const LOADER_SOURCE: &str = include_str!("../../src/module_loader.rs");
const ENGINE_SOURCE: &str = include_str!("../../src/lib.rs");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn compact(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn module_entry_is_the_closed_two_variant_source_authority() {
    let declaration = bounded(
        LOADER_SOURCE,
        "pub enum ModuleEntry {",
        "impl ModuleEntry {",
    );
    let declaration = compact(declaration);
    assert!(declaration.contains("HostLoad{"));
    assert!(declaration.contains("InMemory{"));
    assert_eq!(declaration.matches("locator:String,").count(), 2);
    assert_eq!(declaration.matches("source_text:String,").count(), 1);
    assert!(!declaration.contains("Option<"));
    assert!(!LOADER_SOURCE.contains("source_override"));

    let locator = bounded(
        LOADER_SOURCE,
        "fn locator(&self) -> &str {",
        "/// A filesystem loader confined to a root directory.",
    );
    assert_eq!(
        compact(locator),
        "matchself{Self::HostLoad{locator}|Self::InMemory{locator,..}=>locator,}}}"
    );
}

#[test]
fn graph_loading_consumes_the_source_authority_exhaustively() {
    let loading = bounded(
        LOADER_SOURCE,
        "pub fn load_module_graph(",
        "/// [`load_module_graph`] with an entry already parsed",
    );
    let loading = compact(loading);
    assert!(loading.contains("letentry_key=loader.canonical_key(entry.locator());"));
    assert!(loading.contains(
        "ModuleEntry::InMemory{source_text,..}=>ModuleSourceIr::new(entry_key.clone(),source_text.clone(),"
    ));
    assert!(loading.contains("ModuleEntry::HostLoad{..}=>{letloaded=loader.load(&ModuleRecordKey::new(entry_key.clone(),ModuleKindIr::SourceText,))?;loaded_module_source(loaded)"));
    assert!(!loading.contains("_=>"));
}

#[test]
fn parsed_entry_handoffs_cannot_accept_a_second_source_authority() {
    let parsed_module = bounded(
        LOADER_SOURCE,
        "pub(crate) fn load_module_graph_from_parsed(",
        "/// Script-entry counterpart",
    );
    let parsed_script = bounded(
        LOADER_SOURCE,
        "pub(crate) fn load_module_graph_from_parsed_script(",
        "fn load_module_graph_from_entry(",
    );
    for handoff in [parsed_module, parsed_script] {
        assert!(handoff.contains("entry_locator: &str"));
        assert!(handoff.contains("loader.canonical_key(entry_locator)"));
        assert!(!handoff.contains("ModuleEntry"));
        assert!(!handoff.contains("source_text.clone()"));
    }

    assert_eq!(
        ENGINE_SOURCE
            .matches("let entry_locator = options.filename.as_deref().unwrap_or(\"<entry>\");")
            .count(),
        2
    );
    assert!(!ENGINE_SOURCE.contains("source_override"));
    assert!(!ENGINE_SOURCE.contains("let entry = ModuleEntry"));
}
