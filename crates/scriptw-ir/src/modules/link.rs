//! Assembly of a linked graph into compiler-owned module activations.
//!
//! Module and Script roots share the canonical allocation, instantiation and
//! evaluation protocol. Deferred/TLA/cyclic targets retain their own cells and
//! completion state; only import jobs start a Script's module evaluations.
//! Static Source Text Module source imports reject during linking. Dynamic
//! source jobs reject after loading without a source object or target activation.

use super::namespace::collect_observed_namespaces;
use crate::*;

/// Result of merging a linked graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedProgram {
    /// The merged script, when every unit lowered.
    pub script: Option<ScriptIr>,
    /// Diagnostics collected while lowering and merging.
    pub diagnostics: Vec<IrDiagnostic>,
}

/// The order module bodies run in: one entry per strongly-connected component,
/// listing its members.
///
/// For an acyclic graph every component holds exactly one module, which
/// degenerates to the obvious "dependencies first" list.
#[must_use]
pub fn evaluation_components(graph: &ModuleGraphIr) -> Vec<Vec<ModuleUnitId>> {
    let mut components = Vec::with_capacity(graph.scc_starts.len());
    for (position, start) in graph.scc_starts.iter().copied().enumerate() {
        let end = graph
            .scc_starts
            .get(position + 1)
            .copied()
            .unwrap_or(graph.evaluation_order.len());
        if start < end {
            components.push(graph.evaluation_order[start..end].to_vec());
        }
    }
    components
}

#[derive(Debug)]
pub(crate) struct LinkedScriptSource {
    pub(crate) source: SourceUnit,
    pub(crate) definitions: super::LinkedScriptDefinitions,
}

/// Script-goal source text for the whole linked graph, or the reasons it could
/// not be linked.
///
/// Namespace collection runs first and unconditionally. Dynamic components
/// were already discovered for evaluation-mode classification and filtered to
/// materialized referrers during graph linking; together those steps make
/// `ProgramIr::modules` describe exactly the runtime objects and call sites the
/// artifact can observe, including on a failing program.
pub(crate) fn linked_script_source(
    sources: &ModuleGraphSources,
    graph: &mut ModuleGraphIr,
) -> Result<LinkedScriptSource, Vec<IrDiagnostic>> {
    collect_observed_namespaces(graph);
    let eligible = super::synchronous_source::ModuleInstantiationGraph::new(graph)?;
    super::synchronous_source::linked_module_execution_source(sources, eligible)
}

#[cfg(test)]
mod tests {
    use super::super::namespace::namespace_target_reference;
    use super::*;

    fn sources_of(
        sources: &[(&str, &str)],
        entry: usize,
        resolutions: Vec<(ModuleUnitId, ModuleRequestKeyIr, ModuleUnitId)>,
    ) -> ModuleGraphSources {
        let modules = sources
            .iter()
            .map(|(key, text)| {
                ModuleSourceIr::new(
                    ModuleKey::from_host(*key),
                    (*text).to_string(),
                    format!("file:///{key}"),
                )
            })
            .collect::<Vec<_>>();
        ModuleGraphSources {
            realm_requests: Default::default(),
            entry: ModuleUnitId::try_from(entry).expect("entry index fits"),
            modules,
            resolutions,
        }
    }

    fn graph_of(sources: &ModuleGraphSources) -> ModuleGraphIr {
        let mut graph = crate::modules::build_graph(sources).expect("graph should build");
        crate::modules::link(&mut graph);
        graph
    }

    fn script_graph_of(entry_text: &str) -> (ModuleGraphSources, ModuleGraphIr) {
        let entry = match scriptw_front::parse(
            entry_text.to_string(),
            scriptw_front::ParseOptions::script(),
        ) {
            Ok(scriptw_front::ParsedSource::Script(script)) => script,
            parsed => panic!("script entry fixture must parse as a script: {parsed:?}"),
        };
        let sources = ModuleGraphSources {
            realm_requests: Default::default(),
            entry: 0,
            modules: vec![
                ModuleSourceIr::from_parsed_script(
                    ModuleKey::from_host("entry"),
                    "file:///entry".to_string(),
                    entry,
                ),
                ModuleSourceIr::new(
                    ModuleKey::from_host("m"),
                    "export const value = 1;\n".to_string(),
                    "file:///m".to_string(),
                ),
            ],
            resolutions: vec![(0, request_key("m"), 1)],
        };
        // Resolutions name the request the entry actually wrote: entry
        // fixtures below must spell their specifier `m`.
        let mut graph = crate::modules::build_graph(&sources).expect("graph should build");
        graph.entry_is_script = true;
        crate::modules::link(&mut graph);
        (sources, graph)
    }

    fn request_key(specifier: &str) -> ModuleRequestKeyIr {
        ModuleRequestKeyIr::plain(specifier)
    }

    #[test]
    fn strict_script_entries_keep_their_prologue_above_module_instantiation() {
        let (sources, mut graph) = script_graph_of("\"use strict\";\nimport(\"m\");\n");
        assert!(
            graph.unit(graph.entry).record.script_entry_strict,
            "the entry record must retain Script strictness"
        );
        let linked =
            linked_script_source(&sources, &mut graph).expect("strict script graph should link");
        let text = &linked.source.source_text;
        assert!(
            text.starts_with("\"use strict\";\n"),
            "the entry directive must head the merged script: {text}"
        );
        assert!(
            text.contains("async () => {\n\"use strict\";"),
            "the module activation keeps its own strict prologue: {text}"
        );
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical Script graph");
        assert_eq!(
            definitions.entry,
            super::super::synchronous_execution::ModuleExecutionEntry::Script(0)
        );
        assert_eq!(definitions.units.len(), 1);
        assert_eq!(definitions.units[0].module, 1);
        assert!(
            linked.definitions.entry.is_none(),
            "Script completion has no Module boundary"
        );
        scriptw_front::parse(text.clone(), scriptw_front::ParseOptions::script())
            .expect("hoisted prologue must keep the merged script valid");
    }

    #[test]
    fn sloppy_script_entries_gain_no_merged_prologue() {
        let (sources, mut graph) = script_graph_of("import(\"m\");\n");
        assert!(
            !graph.unit(graph.entry).record.script_entry_strict,
            "the sloppy entry record must stay sloppy"
        );
        let linked =
            linked_script_source(&sources, &mut graph).expect("sloppy script graph should link");
        let text = &linked.source.source_text;
        assert!(
            !text.starts_with("\"use strict\";"),
            "a sloppy entry must not strictify the merged script: {text}"
        );
        assert!(
            text.contains("async () => {\n\"use strict\";"),
            "the modules still compile strict in private activations: {text}"
        );
    }

    #[test]
    fn evaluation_components_lists_one_member_per_acyclic_unit() {
        let sources = sources_of(&[("m", "export const value = 1;")], 0, Vec::new());
        let graph = graph_of(&sources);
        let components = evaluation_components(&graph);
        assert_eq!(components, vec![vec![0]]);
    }

    #[test]
    fn a_synchronous_graph_keeps_its_declarations_in_a_private_activation() {
        let sources = sources_of(&[("m", "print(1);")], 0, Vec::new());
        let mut graph = graph_of(&sources);
        let linked = linked_script_source(&sources, &mut graph).expect("graph should link");
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical module definitions");
        assert_eq!(definitions.units.len(), 1);
        assert_eq!(
            definitions.entry,
            super::super::synchronous_execution::ModuleExecutionEntry::Module(0)
        );
        assert_eq!(definitions.units[0].evaluation.module(), 0);
        assert_eq!(
            definitions.units[0].kind,
            super::super::ModuleActivationKindIr::Synchronous
        );
        assert!(definitions.units[0].requests.is_empty());
        assert!(linked.source.source_text.contains("print(1);"));
    }

    #[test]
    fn top_level_await_owns_a_private_async_activation_and_evaluation_promise() {
        let sources = sources_of(
            &[("m", "const value = await 1;\nprint(value);")],
            0,
            Vec::new(),
        );
        let mut graph = graph_of(&sources);
        let linked = linked_script_source(&sources, &mut graph).expect("top-level await links");
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical async graph");
        assert_eq!(
            definitions.entry,
            super::super::synchronous_execution::ModuleExecutionEntry::Module(0)
        );
        assert_eq!(
            definitions.units[0].kind,
            super::super::ModuleActivationKindIr::Async
        );
        assert!(linked.source.source_text.contains("const value = await 1;"));
        assert!(matches!(
            linked.definitions.entry,
            Some(super::super::LinkedModuleEntry::CanonicalGraph(0))
        ));
    }

    #[test]
    fn source_tla_and_transitive_async_dependencies_have_distinct_activation_kinds() {
        let sources = sources_of(
            &[
                ("a", "export const value = await 7;"),
                ("b", "import { value } from \"a\";\nprint(value + 1);"),
            ],
            1,
            vec![(1, request_key("a"), 0)],
        );
        let mut graph = graph_of(&sources);
        assert_eq!(graph.async_evaluation(), vec![true, true]);
        let linked = linked_script_source(&sources, &mut graph).expect("graph should link");
        let definitions = linked.definitions.synchronous.unwrap();
        assert_eq!(
            definitions.entry,
            super::super::synchronous_execution::ModuleExecutionEntry::Module(1)
        );
        assert_eq!(
            definitions.units[0].kind,
            super::super::ModuleActivationKindIr::Async
        );
        assert_eq!(
            definitions.units[1].kind,
            super::super::ModuleActivationKindIr::Synchronous
        );
        assert_eq!(definitions.units[1].requests[0].target(), 0);
        assert_eq!(
            definitions.units[1].requests[0].phase(),
            super::super::ModuleRequestPhaseIr::Evaluation
        );
    }

    #[test]
    fn implicit_disposal_await_owns_async_activation_and_delays_importers() {
        for body in [
            "await using resource = null;",
            "{ await using resource = undefined; }",
            "if (false) { await using resource = null; }",
            "for (await using resource = null; false;) {}",
            "for (await using resource of []) {}",
            "for await (await using resource of []) {}",
        ] {
            let sources = sources_of(
                &[("resource", body), ("entry", "import 'resource';")],
                1,
                vec![(1, request_key("resource"), 0)],
            );
            let mut graph = graph_of(&sources);
            assert_eq!(graph.async_evaluation(), vec![true, true], "{body}");
            let linked = linked_script_source(&sources, &mut graph)
                .expect("implicit disposal Await keeps the original module activation");
            let definitions = linked.definitions.synchronous.unwrap();
            assert_eq!(
                definitions.units[0].kind,
                super::super::ModuleActivationKindIr::Async,
                "{body}"
            );
            assert_eq!(
                definitions.units[1].kind,
                super::super::ModuleActivationKindIr::Synchronous,
                "the importer waits without inventing a local Await: {body}"
            );
            assert_eq!(definitions.units[1].requests[0].target(), 0);
        }
    }

    #[test]
    fn a_two_unit_graph_visits_its_dependencies_through_the_entry_evaluator() {
        let sources = sources_of(
            &[
                ("a", "export const value = 41;"),
                ("b", "import { value } from \"a\";\nprint(value + 1);"),
            ],
            1,
            vec![(1, request_key("a"), 0)],
        );
        let mut graph = graph_of(&sources);
        let linked = linked_script_source(&sources, &mut graph).expect("graph should link");
        assert_eq!(linked.source.goal, ParseGoal::Script);
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical module definitions");
        assert_eq!(
            definitions.entry,
            super::super::synchronous_execution::ModuleExecutionEntry::Module(1)
        );
        let entry = definitions
            .units
            .iter()
            .find(|unit| unit.module == 1)
            .unwrap();
        assert_eq!(
            entry
                .requests
                .iter()
                .map(super::super::ModuleExecutionRequestIr::target)
                .collect::<Vec<_>>(),
            [0]
        );
        assert!(!linked.source.source_text.contains("import"));
    }

    /// A renamed import resolves to the exporter's canonical environment cell.
    #[test]
    fn renamed_imports_resolve_to_private_exporter_cells() {
        let sources = sources_of(
            &[
                ("a", "let value = 1;\nexport { value as outer };"),
                ("b", "import { outer } from \"a\";\nprint(outer);"),
            ],
            1,
            vec![(1, request_key("a"), 0)],
        );
        let mut graph = graph_of(&sources);
        let linked = linked_script_source(&sources, &mut graph).expect("alias should link");
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical module definitions");
        let entry = definitions
            .units
            .iter()
            .find(|unit| unit.module == 1)
            .unwrap();
        assert_eq!(entry.imports.len(), 1);
        assert_eq!(
            namespace_target_reference(&entry.imports[0])
                .unwrap()
                .as_str(),
            "value"
        );
        assert!(!linked
            .source
            .source_text
            .contains("Object.defineProperty(globalThis"));
        assert!(linked.source.source_text.contains("print(outer);"));
    }

    /// Unrelated module declarations cannot shadow a private import cell.
    #[test]
    fn an_alias_is_separate_from_another_modules_top_level_declaration() {
        let sources = sources_of(
            &[
                ("a", "export const value = 1;"),
                ("c", "const outer = 99;\nprint(outer);"),
                (
                    "b",
                    "import { value as outer } from \"a\";\nimport \"c\";\nprint(outer);",
                ),
            ],
            2,
            vec![(2, request_key("a"), 0), (2, request_key("c"), 1)],
        );
        let mut graph = graph_of(&sources);
        let linked =
            linked_script_source(&sources, &mut graph).expect("separate module bindings link");
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical module definitions");
        assert_eq!(definitions.units.len(), 3);
        let entry = definitions
            .units
            .iter()
            .find(|unit| unit.module == 2)
            .unwrap();
        assert_eq!(
            namespace_target_reference(&entry.imports[0])
                .unwrap()
                .as_str(),
            "value"
        );
    }

    /// Import-local spellings do not need to be unique across a graph.
    #[test]
    fn two_modules_can_alias_the_same_name_to_different_exports() {
        let sources = sources_of(
            &[
                ("a", "export const first = 1;\nexport const second = 2;"),
                ("c", "import { first as z } from \"a\";\nprint(z);"),
                (
                    "b",
                    "import { second as z } from \"a\";\nimport \"c\";\nprint(z);",
                ),
            ],
            2,
            vec![
                (1, request_key("a"), 0),
                (2, request_key("a"), 0),
                (2, request_key("c"), 1),
            ],
        );
        let mut graph = graph_of(&sources);
        let linked = linked_script_source(&sources, &mut graph).expect("private aliases link");
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical module definitions");
        let imports = |module| {
            &definitions
                .units
                .iter()
                .find(|unit| unit.module == module)
                .unwrap()
                .imports
        };
        assert_eq!(
            namespace_target_reference(&imports(1)[0]).unwrap().as_str(),
            "first"
        );
        assert_eq!(
            namespace_target_reference(&imports(2)[0]).unwrap().as_str(),
            "second"
        );
        assert!(!linked
            .source
            .source_text
            .contains("Object.defineProperty(globalThis"));
    }

    /// Distinct importer cells can share one ultimate export target.
    #[test]
    fn distinct_import_cells_can_resolve_to_the_same_export() {
        let sources = sources_of(
            &[
                ("a", "export const first = 1;"),
                ("c", "import { first as z } from \"a\";\nprint(z);"),
                (
                    "b",
                    "import { first as z } from \"a\";\nimport \"c\";\nprint(z);",
                ),
            ],
            2,
            vec![
                (1, request_key("a"), 0),
                (2, request_key("a"), 0),
                (2, request_key("c"), 1),
            ],
        );
        let mut graph = graph_of(&sources);
        let linked = linked_script_source(&sources, &mut graph).expect("agreeing aliases link");
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical module definitions");
        let first = definitions
            .units
            .iter()
            .find(|unit| unit.module == 1)
            .unwrap();
        let second = definitions
            .units
            .iter()
            .find(|unit| unit.module == 2)
            .unwrap();
        assert_eq!(first.imports, second.imports);
        assert_eq!(first.imports.len(), 1);
        assert_eq!(linked.source.source_text.matches("const z = 0;").count(), 2);
        assert!(!linked
            .source
            .source_text
            .contains("Object.defineProperty(globalThis"));
    }

    /// `export default` links: the keywords become a declaration of the minted
    /// name, in place and without moving the initializer.
    #[test]
    fn an_anonymous_export_default_becomes_a_declaration_of_its_minted_name() {
        let sources = sources_of(
            &[
                ("a", "export default 42;"),
                ("b", "import d from \"a\";\nprint(d);"),
            ],
            1,
            vec![(1, request_key("a"), 0)],
        );
        let mut graph = graph_of(&sources);
        let linked = linked_script_source(&sources, &mut graph).expect("default export links");

        let binding = MergedName::anonymous_default(0);
        let binding = binding.as_str();
        assert!(
            linked
                .source
                .source_text
                .contains(&format!("let {binding}     = 42;")),
            "got {}",
            linked.source.source_text
        );
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical module definitions");
        let importer = definitions
            .units
            .iter()
            .find(|unit| unit.module == 1)
            .unwrap();
        assert_eq!(
            namespace_target_reference(&importer.imports[0])
                .unwrap()
                .as_str(),
            binding
        );
    }

    /// There is no line-terminator restriction between `export` and `default`.
    /// The dependency's split pair still declares the minted cell that the
    /// importer's live alias reads, and the merged result remains Script text.
    #[test]
    fn a_split_anonymous_default_links_to_its_importer() {
        let sources = sources_of(
            &[
                ("a", "export\ndefault 42;"),
                ("b", "import answer from \"a\";\nanswer;"),
            ],
            1,
            vec![(1, request_key("a"), 0)],
        );
        let mut graph = graph_of(&sources);
        let linked = linked_script_source(&sources, &mut graph).expect("split default links");

        let binding = MergedName::anonymous_default(0);
        assert!(
            linked
                .source
                .source_text
                .contains(&format!("let {}    =\n 42;", binding.as_str())),
            "got {}",
            linked.source.source_text
        );
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical module definitions");
        let importer = definitions
            .units
            .iter()
            .find(|unit| unit.module == 1)
            .unwrap();
        assert_eq!(
            namespace_target_reference(&importer.imports[0])
                .unwrap()
                .as_str(),
            binding.as_str()
        );
        scriptw_front::parse(
            linked.source.source_text,
            scriptw_front::ParseOptions::script(),
        )
        .expect("span-stable linked output should remain valid Script text");
    }

    /// Two units may both have an anonymous `export default`: the spec calls
    /// both bindings `*default*`, but the merged scope names them apart.
    #[test]
    fn two_anonymous_default_exports_do_not_collide() {
        let sources = sources_of(
            &[
                ("a", "export default 1;"),
                ("c", "export default 2;"),
                (
                    "b",
                    "import x from \"a\";\nimport y from \"c\";\nprint(x + y);",
                ),
            ],
            2,
            vec![(2, request_key("a"), 0), (2, request_key("c"), 1)],
        );
        let mut graph = graph_of(&sources);
        let linked =
            linked_script_source(&sources, &mut graph).expect("two anonymous defaults link");
        assert!(
            linked.source.source_text.contains(&format!(
                "let {}     = 1;",
                MergedName::anonymous_default(0).as_str()
            )),
            "got {}",
            linked.source.source_text
        );
        assert!(
            linked.source.source_text.contains(&format!(
                "let {}     = 2;",
                MergedName::anonymous_default(1).as_str()
            )),
            "got {}",
            linked.source.source_text
        );
    }

    /// `export * from` needs nothing of this stage: `ResolveExport` already
    /// walked the star path, so the importer shares the *originating* unit's
    /// cell and no alias is minted at all.
    #[test]
    fn a_star_re_export_resolves_through_to_the_originating_cell() {
        let sources = sources_of(
            &[
                ("a", "export const value = 10;"),
                ("s", "export * from \"a\";"),
                ("b", "import { value } from \"s\";\nprint(value);"),
            ],
            2,
            vec![(1, request_key("a"), 0), (2, request_key("s"), 1)],
        );
        let mut graph = graph_of(&sources);
        let linked = linked_script_source(&sources, &mut graph).expect("star re-export links");

        assert!(
            linked.source.source_text.contains("const value = 10;"),
            "got {}",
            linked.source.source_text
        );
        assert!(
            !linked
                .source
                .source_text
                .contains("defineProperty(globalThis"),
            "a star re-export needs no alias: {}",
            linked.source.source_text
        );
        assert!(
            !linked.source.source_text.contains("export"),
            "got {}",
            linked.source.source_text
        );
    }

    #[test]
    fn same_spelled_top_level_names_belong_to_separate_modules() {
        let sources = sources_of(
            &[
                ("a", "const shared = 1;\nexport { shared };"),
                ("b", "const shared = 2;\nprint(shared);"),
            ],
            1,
            Vec::new(),
        );
        let mut graph = graph_of(&sources);
        let linked =
            linked_script_source(&sources, &mut graph).expect("separate module declarations link");
        assert_eq!(linked.definitions.synchronous.unwrap().units.len(), 2);
        assert!(linked.source.source_text.contains("const shared = 1;"));
        assert!(linked.source.source_text.contains("const shared = 2;"));
    }

    /// `import()` is linked, not rejected: the call site becomes a call to the
    /// referrer's dispatcher, and the dispatcher resolves with the *same*
    /// namespace binding `import * as ns` would alias.
    #[test]
    fn dynamic_import_is_desugared_into_a_dispatcher_call() {
        let sources = sources_of(&[("m", "import('m');")], 0, vec![(0, request_key("m"), 0)]);
        let mut graph = graph_of(&sources);
        let linked = linked_script_source(&sources, &mut graph).expect("import() should link");

        assert_eq!(graph.dynamic_components().len(), 1);
        assert!(graph.units[0]
            .namespaces
            .contains_key(&ModuleNamespaceModeIr::Eager));
        assert!(
            linked
                .source
                .source_text
                .contains("function $scriptw$module$import$0("),
            "got {}",
            linked.source.source_text
        );
        assert!(
            linked.source.source_text.contains(&format!(
                "return {};",
                MergedName::minted(0, UnitCellRole::Namespace).as_str()
            )),
            "got {}",
            linked.source.source_text
        );
        // The call site itself no longer spells `import`, so no `ImportCall`
        // node survives to the backend.
        assert!(
            linked
                .source
                .source_text
                .contains("$scriptw$module$import$0('m');"),
            "got {}",
            linked.source.source_text
        );
    }

    /// Namespace imports resolve to the exporting record's private cell.
    #[test]
    fn a_namespace_import_binds_its_local_to_the_private_namespace_cell() {
        let sources = sources_of(
            &[
                ("a", "export const value = 41;"),
                ("c", "import * as ns from \"a\";\nprint(ns.value);"),
            ],
            1,
            vec![(1, request_key("a"), 0)],
        );
        let mut graph = graph_of(&sources);
        let linked = linked_script_source(&sources, &mut graph).expect("namespace should link");
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical module definitions");
        let exporter = definitions
            .units
            .iter()
            .find(|unit| unit.module == 0)
            .unwrap();
        assert_eq!(exporter.namespaces.len(), 1);
        assert_eq!(exporter.namespaces[0].0, ModuleNamespaceModeIr::Eager);
        let importer = definitions
            .units
            .iter()
            .find(|unit| unit.module == 1)
            .unwrap();
        assert_eq!(
            importer.imports,
            [ResolvedBindingIr::Resolved {
                module: 0,
                binding: ModuleBindingNameIr::Namespace(ModuleNamespaceModeIr::Eager),
            }]
        );
    }

    /// Deferred bodies are instantiated without starting their evaluation.
    #[test]
    fn a_deferred_dependency_has_an_activation_without_initial_evaluation() {
        let sources = sources_of(
            &[
                ("a", "print(\"side effect\");\nexport const value = 41;"),
                (
                    "d",
                    "import defer * as ns from \"a\";\nprint(\"entry\");\nprint(ns.value);",
                ),
            ],
            1,
            vec![(1, request_key("a"), 0)],
        );
        let mut graph = graph_of(&sources);
        assert_eq!(graph.evaluation_mode(0), ModuleEvaluationModeIr::Deferred);
        let linked = linked_script_source(&sources, &mut graph).expect("defer should link");
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical module definitions");
        assert_eq!(
            definitions.entry,
            super::super::synchronous_execution::ModuleExecutionEntry::Module(1)
        );
        let deferred = definitions
            .units
            .iter()
            .find(|unit| unit.module == 0)
            .unwrap();
        assert_eq!(deferred.namespaces[0].0, ModuleNamespaceModeIr::Deferred);
        assert_eq!(deferred.evaluation.module(), 0);
        assert!(linked
            .source
            .source_text
            .contains("print(\"side effect\");"));
    }

    #[test]
    fn a_static_source_phase_dependency_rejects_before_source_assembly() {
        let sources = sources_of(
            &[
                ("a", "print('must not run'); export default 1;"),
                ("d", "import source src from 'a'; print(typeof src);"),
            ],
            1,
            vec![(1, request_key("a"), 0)],
        );
        let mut graph = graph_of(&sources);
        assert_eq!(
            graph.evaluation_mode(0),
            ModuleEvaluationModeIr::NotEvaluated
        );
        let diagnostics = linked_script_source(&sources, &mut graph).unwrap_err();
        assert_eq!(
            diagnostics[0].code(),
            Some(EarlyErrorCode::ModuleSourceUnavailable)
        );
    }

    /// A source-only unit is still parsed and linked, but none of the runtime
    /// scaffolding named by its own body belongs to the artifact. This is one
    /// contract rather than four feature tests: every collector must consume
    /// the same materialization witness or an inactive alias leaks into the
    /// entry's merged scope.
    #[test]
    fn module_source_only_units_contribute_no_runtime_scaffolding() {
        let sources = sources_of(
            &[
                (
                    "inactive",
                    "import * as ghost from \"namespace\";\n\
                     import source nested from \"nested\";\n\
                     import.meta;\n\
                     import(\"dynamic\");\n\
                     export const value = 1;",
                ),
                ("namespace", "export const visible = 1;"),
                ("nested", "export const nested = 1;"),
                ("dynamic", "export const dynamic = 1;"),
                (
                    "entry",
                    "import.source(\"inactive\");\n\
                     print(typeof ghost);\n\
                     void 0;",
                ),
            ],
            4,
            vec![
                (0, request_key("namespace"), 1),
                (0, request_key("nested"), 2),
                (0, request_key("dynamic"), 3),
                (4, request_key("inactive"), 0),
            ],
        );
        let mut graph = graph_of(&sources);

        for module in 0..4 {
            assert_eq!(
                graph.evaluation_mode(module),
                ModuleEvaluationModeIr::NotEvaluated
            );
        }
        assert!(
            graph
                .dynamic_components()
                .iter()
                .all(|component| component.referrer() == 4),
            "only the active referrer's source job is an artifact component"
        );
        assert_eq!(
            crate::modules::namespace::ensure_namespace(
                &mut graph,
                0,
                ModuleNamespaceModeIr::Eager
            ),
            None,
            "a source-only unit has no environment for a namespace"
        );

        let linked = linked_script_source(&sources, &mut graph)
            .expect("source-only runtime scaffolding must not reject the active graph");
        let text = &linked.source.source_text;
        for absent in [
            MergedName::minted(1, UnitCellRole::Namespace),
            MergedName::minted(3, UnitCellRole::Namespace),
            MergedName::minted(0, UnitCellRole::ImportMeta),
        ] {
            assert!(
                !text.contains(absent.as_str()),
                "inactive runtime cell {} leaked into: {text}",
                absent.as_str()
            );
        }
        assert!(
            !text.contains("const ghost ="),
            "inactive alias leaked: {text}"
        );
        assert!(
            !text.contains("function $scriptw$module$import$0("),
            "inactive dispatcher leaked: {text}"
        );
        assert!(
            text.contains("print(typeof ghost)"),
            "the active observation must remain: {text}"
        );
    }

    /// Dynamic-only targets instantiate eagerly but evaluate from import jobs.
    #[test]
    fn initial_evaluation_starts_only_the_entry_static_traversal() {
        let sources = sources_of(
            &[
                ("d", "import(\"a\");\nprint(\"entry\");"),
                ("a", "print(\"target\");"),
            ],
            0,
            vec![(0, request_key("a"), 1)],
        );
        let mut graph = graph_of(&sources);
        assert_eq!(graph.evaluation_order.first().copied(), Some(0));
        let linked = linked_script_source(&sources, &mut graph).expect("graph should link");
        let definitions = linked
            .definitions
            .synchronous
            .expect("canonical module definitions");
        assert_eq!(
            definitions.entry,
            super::super::synchronous_execution::ModuleExecutionEntry::Module(0)
        );
        assert!(definitions
            .dispatcher_evaluations
            .contains_key("$scriptw$module$evaluate$1"));
    }
}
