//! Resolve finite prepared-source module requests before artifact identity is
//! computed. Each retry reuses retained syntax, and the accepted IR is the IR
//! later emitted rather than a discarded discovery product.

use super::*;

#[cfg(test)]
thread_local! {
    static FULL_CATALOG_LOWERINGS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static RETAIN_WHOLE_GRAPH_DISCOVERY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) fn prepare_source_catalog(
    source: &ParsedSource,
    prelude: Option<&scriptw_front::ParsedScript>,
    modules: &mut PreparedModuleCompilation,
    options: &CompileOptions,
) -> Result<ProgramIr, EngineError> {
    let session = scriptw_ir::LoweringSession::default();
    loop {
        #[cfg(test)]
        FULL_CATALOG_LOWERINGS.with(|count| count.set(count.get() + 1));
        let ir = match &*modules {
            PreparedModuleCompilation::Embedded { graph, .. } => {
                session.lower_complete_catalog(graph, options.host_surface_policy, prelude)
            }
            PreparedModuleCompilation::Ambient { graph, .. } => match (source, graph, prelude) {
                (_, Some(graph), prelude) => {
                    session.lower_loaded_graph(graph, options.host_surface_policy, prelude)
                }
                (ParsedSource::Module(source), None, prelude) => session.lower_loaded_graph(
                    &scriptw_ir::ModuleGraphSources::single(source),
                    options.host_surface_policy,
                    prelude,
                ),
                (ParsedSource::Script(_), None, None) => {
                    session.lower_source(source, options.host_surface_policy)
                }
                (ParsedSource::Script(_), None, Some(prelude)) => {
                    session.lower_source_with_prelude(source, prelude, options.host_surface_policy)
                }
            },
        };
        let ir = ir.with_language_features(options.language_features);
        if let PreparedModuleCompilation::Embedded { graph, .. } = modules {
            let sites = session.prepared_script_import_sites();
            let sites_changed = !sites.is_empty()
                && graph.modules[graph.entry as usize].include_prepared_script_import_sites(&sites);
            let foreign_requests = session
                .realm_module_requests()
                .into_iter()
                .filter(|request| {
                    options
                        .foreign_modules
                        .module(request.specifier())
                        .is_some()
                        && !graph.realm_requests.contains_key(request)
                })
                .collect::<Vec<_>>();
            if !sites_changed && foreign_requests.is_empty() {
                // Only declared host resolutions may satisfy these new sites.
                // Annotating retained syntax grants no ambient load authority.
                return Ok(ir);
            }
            if !options.foreign_modules.is_empty() {
                module_loader::include_foreign_module_registry_requests(
                    graph,
                    &options.foreign_modules,
                    &foreign_requests,
                )
                .map_err(|error| EngineError::new(error.to_string()))?;
            }
            continue;
        }
        let PreparedModuleCompilation::Ambient { loader, graph, .. } = modules else {
            unreachable!("the complete catalog was handled above")
        };
        let requests = session
            .realm_module_requests()
            .into_iter()
            .filter(|request| {
                graph
                    .as_ref()
                    .is_none_or(|graph| !graph.realm_requests.contains_key(request))
            })
            .collect::<Vec<_>>();
        let script_sites = session.prepared_script_import_sites();
        let script_changed = graph.as_mut().is_some_and(|graph| {
            !script_sites.is_empty()
                && graph.modules[graph.entry as usize]
                    .include_prepared_script_import_sites(&script_sites)
        });
        if requests.is_empty() && !script_changed && (graph.is_some() || script_sites.is_empty()) {
            return Ok(ir);
        }
        if graph.is_none() {
            let locator = options.filename.as_deref().unwrap_or("<entry>");
            let loaded = match source {
                ParsedSource::Script(source) => {
                    module_loader::load_module_graph_from_parsed_script(
                        locator,
                        source.clone(),
                        loader.as_ref(),
                    )
                }
                ParsedSource::Module(source) => module_loader::load_module_graph_from_parsed(
                    locator,
                    source.clone(),
                    loader.as_ref(),
                ),
            }
            .map_err(|error| EngineError::new(error.to_string()))?;
            *graph = Some(loaded);
        }
        let graph = graph
            .as_mut()
            .expect("prepared request has its retained graph");
        if !script_sites.is_empty() {
            graph.modules[graph.entry as usize].include_prepared_script_import_sites(&script_sites);
        }
        // The same source-record loading driver resolves these imports against
        // the actual Script key; Realm requests keep their distinct origin.
        module_loader::extend_module_graph_realm_requests(graph, &requests, loader.as_ref())
            .map_err(|error| EngineError::new(error.to_string()))?;
        if requests
            .iter()
            .any(|request| !graph.realm_requests.contains_key(request))
        {
            return Err(EngineError::new(
                "module loader omitted a prepared Realm-request resolution",
            ));
        }
        // Ordinary lowering admitted the current requests. For a closed,
        // mutation-free set of independent fresh-Realm bodies, process only
        // new retained sources until the loader reaches request closure. A
        // prelude or an unproved body keeps the original semantic retry path.
        let use_worklist = prelude.is_none() && ir.is_wasm_supported();
        #[cfg(test)]
        let use_worklist = use_worklist && !RETAIN_WHOLE_GRAPH_DISCOVERY.get();
        if use_worklist {
            while session.discover_pure_realm_sources(graph) {
                let requests = session
                    .realm_module_requests()
                    .into_iter()
                    .filter(|request| !graph.realm_requests.contains_key(request))
                    .collect::<Vec<_>>();
                if requests.is_empty() {
                    break;
                }
                module_loader::extend_module_graph_realm_requests(
                    graph,
                    &requests,
                    loader.as_ref(),
                )
                .map_err(|error| EngineError::new(error.to_string()))?;
                if requests
                    .iter()
                    .any(|request| !graph.realm_requests.contains_key(request))
                {
                    return Err(EngineError::new(
                        "module loader omitted a prepared Realm-request resolution",
                    ));
                }
            }
        }
        // Newly loaded modules can themselves contain finite prepared source.
        // Reuse all old parse products while closing those requests as well.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct ModuleFiles(PathBuf);

    impl ModuleFiles {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "scriptw-prepared-module-catalog-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, name: &str, source: &str) {
            std::fs::write(self.0.join(name), source).unwrap();
        }

        fn options(&self) -> CompileOptions {
            CompileOptions {
                filename: Some(self.0.join("entry.js").to_str().unwrap().into()),
                module_root: Some(self.0.to_str().unwrap().into()),
                ..CompileOptions::default()
            }
        }
    }

    impl Drop for ModuleFiles {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn prepare_with_discovery(
        engine: &Engine,
        source: &str,
        options: &CompileOptions,
        retain: bool,
    ) -> PreparedCompilation {
        struct Restore(bool);
        impl Drop for Restore {
            fn drop(&mut self) {
                RETAIN_WHOLE_GRAPH_DISCOVERY.set(self.0);
            }
        }
        let _restore = Restore(RETAIN_WHOLE_GRAPH_DISCOVERY.replace(retain));
        engine
            .prepare_compilation(source, ParseGoal::Script, options)
            .unwrap()
    }

    #[test]
    fn pure_prepared_module_chains_use_two_whole_graph_lowerings() {
        for depth in [2, 6] {
            let files = ModuleFiles::new();
            let source = r#"new ShadowRealm().evaluate('void new ShadowRealm().importValue("./0.js", "value")'); new ShadowRealm().evaluate('\uD800'); true;"#;
            files.write("entry.js", source);
            for index in 0..depth {
                let next = if index + 1 == depth {
                    String::new()
                } else {
                    format!(
                        r#"new ShadowRealm().evaluate('void new ShadowRealm().importValue("./{}.js", "value")');"#,
                        index + 1
                    )
                };
                files.write(
                    &format!("{index}.js"),
                    &format!("{next} export const value = {index};"),
                );
            }
            let engine = Engine::new(RealmBuilder::new().build());
            let options = files.options();
            FULL_CATALOG_LOWERINGS.set(0);
            let worklist = prepare_with_discovery(&engine, source, &options, false);
            assert_eq!(FULL_CATALOG_LOWERINGS.get(), 2, "depth {depth}");
            assert_eq!(worklist.modules.graph().unwrap().modules.len(), depth + 1);
            FULL_CATALOG_LOWERINGS.set(0);
            let retained = prepare_with_discovery(&engine, source, &options, true);
            assert_eq!(FULL_CATALOG_LOWERINGS.get(), depth + 1, "depth {depth}");
            assert_eq!(worklist.modules.graph(), retained.modules.graph());
            assert_eq!(worklist.ir, retained.ir);
        }
    }

    #[test]
    fn prepared_source_aliases_keep_semantic_discovery_fallback() {
        let files = ModuleFiles::new();
        let source = r#"new ShadowRealm().evaluate('void new ShadowRealm().importValue("./0.js", "value")'); true;"#;
        files.write("entry.js", source);
        for index in 0..3 {
            let next = if index == 2 {
                String::new()
            } else {
                format!(
                    r#"const next = 'void new ShadowRealm().importValue("./{}.js", "value")'; new ShadowRealm().evaluate(next);"#,
                    index + 1
                )
            };
            files.write(
                &format!("{index}.js"),
                &format!("{next} export const value = {index};"),
            );
        }
        let engine = Engine::new(RealmBuilder::new().build());
        let options = files.options();
        FULL_CATALOG_LOWERINGS.set(0);
        let worklist = prepare_with_discovery(&engine, source, &options, false);
        assert_eq!(FULL_CATALOG_LOWERINGS.get(), 4);
        let retained = prepare_with_discovery(&engine, source, &options, true);
        assert_eq!(worklist.modules.graph(), retained.modules.graph());
        assert_eq!(worklist.ir, retained.ir);
    }

    #[test]
    fn prepared_requests_close_before_artifact_identity_and_keep_the_final_ir() {
        let files = ModuleFiles::new();
        let source = r#"
new ShadowRealm().evaluate('void new ShadowRealm().importValue("./first.js", "value")');
Function('void new ShadowRealm().importValue("./function.js", "value")')();
new ShadowRealm().evaluate('new.target');
true;
"#;
        files.write("entry.js", source);
        files.write(
            "first.js",
            r#"
new ShadowRealm().evaluate('void new ShadowRealm().importValue("./second.js", "value")');
export const value = 1;
"#,
        );
        files.write("second.js", "export const value = 2;");
        files.write("function.js", "export const value = 3;");
        let options = files.options();
        let engine = Engine::new(RealmBuilder::new().build());
        let prepared = engine
            .prepare_compilation(source, ParseGoal::Script, &options)
            .unwrap();
        let graph = prepared
            .modules
            .graph()
            .expect("finite sources own a module catalog");
        for name in ["./first.js", "./second.js", "./function.js"] {
            assert!(
                matches!(
                    graph
                        .realm_requests
                        .get(&scriptw_ir::ModuleRequestKeyIr::plain(name)),
                    Some(scriptw_ir::RealmModuleResolutionIr::Loaded(_))
                ),
                "{name}"
            );
        }
        assert_eq!(graph.modules.len(), 4);
        assert!(
            prepared
                .ir
                .script
                .as_ref()
                .unwrap()
                .prepared_scripts
                .iter()
                .any(|script| {
                    script.kind == scriptw_ir::PreparedScriptKind::ShadowRealmEvaluate
                        && script.source == "new.target"
                        && matches!(
                            script.outcome,
                            scriptw_ir::PreparedScriptOutcome::DeferredSyntaxError { .. }
                        )
                })
        );
        let first_key = program_cache_key(source, ParseGoal::Script, &options, Some(graph));
        let final_ir = prepared.ir.clone();
        let unit = engine.compile_prepared_on_current_thread(prepared).unwrap();
        assert_eq!(unit.ir, final_ir);

        files.write("second.js", "export const value = 20;");
        let changed = engine
            .prepare_compilation(source, ParseGoal::Script, &options)
            .unwrap();
        let changed_key =
            program_cache_key(source, ParseGoal::Script, &options, changed.modules.graph());
        assert_ne!(first_key, changed_key);
    }

    #[test]
    fn prepared_requests_keep_reject_all_authority_and_hash_rejections() {
        let files = ModuleFiles::new();
        let source = r#"new ShadowRealm().evaluate('void new ShadowRealm().importValue("./present.js", "value")');"#;
        files.write("entry.js", source);
        files.write("present.js", "export const value = 42;");
        let mut options = files.options();
        options.module_loading_policy = ModuleLoadingPolicy::RejectAll;
        let engine = Engine::new(RealmBuilder::new().build());
        let prepared = engine
            .prepare_compilation(source, ParseGoal::Script, &options)
            .unwrap();
        let graph = prepared.modules.graph().unwrap();
        let request = scriptw_ir::ModuleRequestKeyIr::plain("./present.js");
        assert!(matches!(
            graph.realm_requests.get(&request),
            Some(scriptw_ir::RealmModuleResolutionIr::Rejected(_))
        ));
        assert_eq!(graph.modules.len(), 1);
        let mut changed = graph.clone();
        changed.realm_requests.insert(
            request,
            scriptw_ir::RealmModuleResolutionIr::Rejected("different host failure".into()),
        );
        assert_ne!(module_graph_digest(graph), module_graph_digest(&changed));
    }

    #[test]
    fn cached_program_cannot_hide_a_current_parse_error_or_script_prelude_rejection() {
        let files = ModuleFiles::new();
        let cache =
            Arc::new(cache::FunctionCache::new(files.0.join("cache"), 64 * 1024 * 1024).unwrap());
        let engine = Engine::new(RealmBuilder::new().build());
        let options = files.options();
        let valid = run_on_sized_stack(|| {
            engine.load_or_compile_program_wasm_on_current_thread(
                "true;",
                ParseGoal::Script,
                options.clone(),
                Some(Arc::clone(&cache)),
            )
        })
        .unwrap();
        for (source, mut options) in [("function {", options.clone()), ("true;", options)] {
            if source == "true;" {
                options.module_prelude = Some("export {};".into());
            }
            let key = program_cache_key_for_role(
                source,
                ParseGoal::Script,
                &options,
                None,
                module_loader::EmbeddedSourceRole::Entry,
            )
            .unwrap();
            assert!(cache.write(&key, wasm_runtime_link::encode_cache_entry(valid.program())));
            assert!(
                engine
                    .load_or_compile_program_wasm_on_current_thread(
                        source,
                        ParseGoal::Script,
                        options,
                        Some(Arc::clone(&cache)),
                    )
                    .is_err(),
                "cache must not mask current source admission: {source}"
            );
        }
    }

    #[test]
    fn cached_finite_source_program_tracks_dependency_edits_and_rejections() {
        let files = ModuleFiles::new();
        let source = r#"void new ShadowRealm().evaluate('void new ShadowRealm().importValue("./value.js", "value")'); true;"#;
        files.write("entry.js", source);
        files.write("value.js", "export const value = 1;");
        let options = files.options();
        let cache =
            Arc::new(cache::FunctionCache::new(files.0.join("cache"), 64 * 1024 * 1024).unwrap());
        let engine = Engine::new(RealmBuilder::new().build());
        let load = || {
            run_on_sized_stack(|| {
                engine.load_or_compile_program_wasm_on_current_thread(
                    source,
                    ParseGoal::Script,
                    options.clone(),
                    Some(Arc::clone(&cache)),
                )
            })
        };
        let first = load().unwrap();
        assert!(first.cached_entry.is_none());
        let warm = load().unwrap();
        assert!(warm.cached_entry.is_some());
        assert_eq!(first.bytes, warm.bytes);
        files.write("value.js", "export const value = 2;");
        let edited = load().unwrap();
        assert!(
            edited.cached_entry.is_none(),
            "finite-source dependency edit must miss"
        );
        assert_ne!(first.bytes, edited.bytes);
        let warm_edit = load().unwrap();
        assert!(warm_edit.cached_entry.is_some());
        assert_eq!(edited.bytes, warm_edit.bytes);
        std::fs::remove_file(files.0.join("value.js")).unwrap();
        let removed = load().unwrap();
        assert!(
            removed.cached_entry.is_none(),
            "a missing dependency cannot use loaded code"
        );
        assert_ne!(edited.bytes, removed.bytes);
    }
    #[test]
    fn prepared_script_import_edges_keep_the_actual_base_identity_and_load_policy() {
        let files = ModuleFiles::new();
        let source = r#"eval('import("./value.js")'); true;"#;
        files.write("entry.js", source);
        files.write("value.js", "export const value = 1;");
        let engine = Engine::new(RealmBuilder::new().build());
        let options = files.options();
        let first = engine
            .prepare_compilation(source, ParseGoal::Script, &options)
            .unwrap();
        let graph = first
            .modules
            .graph()
            .expect("prepared import owns its Script graph");
        assert_eq!(graph.modules.len(), 2);
        assert_eq!(graph.modules[0].goal(), ParseGoal::Script);
        assert!(
            graph
                .resolutions
                .iter()
                .any(|(owner, request, target)| *owner == graph.entry
                    && request.specifier() == "./value.js"
                    && *target == 1)
        );
        assert!(
            graph.realm_requests.is_empty(),
            "an eval import is not a Realm-origin request"
        );
        let key = program_cache_key(source, ParseGoal::Script, &options, Some(graph));
        files.write("value.js", "export const value = 2;");
        let changed = engine
            .prepare_compilation(source, ParseGoal::Script, &options)
            .unwrap();
        assert_ne!(
            key,
            program_cache_key(source, ParseGoal::Script, &options, changed.modules.graph())
        );
        let mut denied = options;
        denied.module_loading_policy = ModuleLoadingPolicy::RejectAll;
        let denied = engine
            .prepare_compilation(source, ParseGoal::Script, &denied)
            .unwrap();
        let graph = denied.modules.graph().unwrap();
        assert_eq!(graph.modules.len(), 1);
        assert!(graph.resolutions.is_empty());
        assert!(graph.realm_requests.is_empty());
        assert!(denied.ir.is_wasm_supported(), "{:?}", denied.ir.diagnostics);
    }
}
