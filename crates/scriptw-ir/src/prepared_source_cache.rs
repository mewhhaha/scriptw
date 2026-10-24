//! Compilation-local syntax owners shared by finite-source discovery and the
//! final lowering pass. A cache entry includes its exact grammar/context and
//! retains both successful parse products and deferred failures.

mod function_globals;
pub(crate) use function_globals::FunctionGlobalConstructorHints;

mod returned_callable;
pub(crate) use returned_callable::ReturnedCallableSourceId;

pub(crate) const MAX_SOURCE_CANDIDATES: usize = 256;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use scriptw_front::{
    DirectEvalParseContext, EvalParseContext, FunctionParseKind, ParseError, ParsedScript,
};

use crate::{DynamicFunctionKind, ModuleRequestKeyIr, PreparedScriptKind};
#[cfg(test)]
use scriptw_front::ParseOptions;

#[derive(Debug, Clone, PartialEq, Eq)]
enum PreparedSourceInput {
    RealmScript(Vec<u16>),
    IndirectEval(Vec<u16>),
    ShadowRealmEvaluate(Vec<u16>),
    DirectEval(Vec<u16>, DirectEvalParseContext),
    Function(DynamicFunctionKind, Vec<Vec<u16>>),
}

#[derive(Debug, Default)]
struct PreparedSourceCacheState {
    function_globals: function_globals::FunctionGlobalHintOwners,
    returned_callables: returned_callable::ReturnedCallableSources,
    parsed: Vec<(PreparedSourceInput, Result<ParsedScript, ParseError>)>,
    realm_request_sources: Vec<scriptw_front::ParsedScriptIdentity>,
    realm_requests: BTreeSet<ModuleRequestKeyIr>,
    script_import_sites: Vec<crate::DynamicImportSiteIr>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PreparedSourceCache(Rc<RefCell<PreparedSourceCacheState>>);

impl PreparedSourceCache {
    pub(crate) fn parse_script(
        &self,
        kind: &PreparedScriptKind,
        source: &str,
    ) -> Result<ParsedScript, ParseError> {
        let source = crate::decode_js_string_utf16(source);
        let input = match kind {
            PreparedScriptKind::RealmScript => PreparedSourceInput::RealmScript(source),
            PreparedScriptKind::IndirectEval => PreparedSourceInput::IndirectEval(source),
            PreparedScriptKind::ShadowRealmEvaluate => {
                PreparedSourceInput::ShadowRealmEvaluate(source)
            }
            PreparedScriptKind::DirectEval(context) => {
                PreparedSourceInput::DirectEval(source, context.parse_context())
            }
        };
        let parsed = self.parse(input);
        if matches!(kind, PreparedScriptKind::DirectEval(context)
            if context.script_import_origin().is_some())
        {
            if let Ok(script) = &parsed {
                let sites = crate::modules::script_dynamic_import_sites(script);
                let mut state = self.0.borrow_mut();
                for site in sites
                    .into_iter()
                    .filter(|site| site.discovery_request().is_some())
                {
                    if !state.script_import_sites.contains(&site) {
                        state.script_import_sites.push(site);
                    }
                }
            }
        }
        parsed
    }

    pub(crate) fn parse_function(
        &self,
        kind: DynamicFunctionKind,
        arguments: &[String],
    ) -> Result<ParsedScript, ParseError> {
        self.parse(PreparedSourceInput::Function(
            kind,
            arguments
                .iter()
                .map(|argument| crate::decode_js_string_utf16(argument))
                .collect(),
        ))
    }

    fn parse(&self, input: PreparedSourceInput) -> Result<ParsedScript, ParseError> {
        let parsed = self.parse_syntax(input);
        if let Ok(source) = &parsed {
            let identity = source.syntax_identity();
            let already_scanned = self.0.borrow().realm_request_sources.contains(&identity);
            if !already_scanned {
                let requests = crate::modules::scan_script_realm_module_requests(source);
                let mut state = self.0.borrow_mut();
                state.realm_requests.extend(requests);
                state.realm_request_sources.push(identity);
            }
        }
        parsed
    }

    // A syntax-only hint cannot request host resources. Normal admission still
    // records requests when the same retained syntax was first parsed as a hint.
    fn parse_syntax(&self, input: PreparedSourceInput) -> Result<ParsedScript, ParseError> {
        if let Some((_, parsed)) = self
            .0
            .borrow()
            .parsed
            .iter()
            .find(|(candidate, _)| candidate == &input)
        {
            return parsed.clone();
        }
        let parsed = match &input {
            PreparedSourceInput::RealmScript(source) => scriptw_front::parse_script_utf16(source),
            PreparedSourceInput::IndirectEval(source)
            | PreparedSourceInput::ShadowRealmEvaluate(source) => {
                scriptw_front::prepare_eval_source_utf16(source, &EvalParseContext::Indirect)
            }
            PreparedSourceInput::DirectEval(source, context) => {
                scriptw_front::prepare_eval_source_utf16(
                    source,
                    &EvalParseContext::Direct(context.clone()),
                )
            }
            PreparedSourceInput::Function(kind, arguments) => {
                let kind = match kind {
                    DynamicFunctionKind::Ordinary => FunctionParseKind::Ordinary,
                    DynamicFunctionKind::Generator => FunctionParseKind::Generator,
                    DynamicFunctionKind::Async => FunctionParseKind::Async,
                    DynamicFunctionKind::AsyncGenerator => FunctionParseKind::AsyncGenerator,
                };
                scriptw_front::prepare_dynamic_function_utf16(kind, arguments)
            }
        };
        self.0.borrow_mut().parsed.push((input, parsed.clone()));
        parsed
    }

    pub(crate) fn realm_module_requests(&self) -> Vec<ModuleRequestKeyIr> {
        self.0.borrow().realm_requests.iter().cloned().collect()
    }

    pub(crate) fn record_realm_module_requests(
        &self,
        requests: impl IntoIterator<Item = ModuleRequestKeyIr>,
    ) {
        self.0.borrow_mut().realm_requests.extend(requests);
    }
}

/// One host compilation's finite-source parsing and request-discovery owner.
/// Lowering retries allocate fresh IR identities while retaining the exact
/// prepared syntax products. This owner is local to preparation, not a shared
/// process-wide artifact or parser cache.
#[derive(Debug, Default)]
pub struct LoweringSession {
    pub(crate) prepared_sources: PreparedSourceCache,
    pure_realm_discovery: RefCell<crate::modules::PureRealmDiscovery>,
}

impl LoweringSession {
    /// Close literal prepared sources for a certified mutation-free collection
    /// of independent Realm targets. Returns false when ordinary whole-graph
    /// semantic discovery must handle the supplied graph instead. This grants
    /// no host loading authority; the caller still resolves each discovered
    /// request under its existing policy and confirms the final graph normally.
    pub fn discover_pure_realm_sources(&self, sources: &crate::ModuleGraphSources) -> bool {
        self.pure_realm_discovery
            .borrow_mut()
            .discover(sources, &self.prepared_sources)
    }

    /// Literal imports from direct evals proved to retain this compilation's
    /// Script entry. Their sites retain phase and exact attribute eligibility.
    pub fn prepared_script_import_sites(&self) -> Vec<crate::DynamicImportSiteIr> {
        self.prepared_sources.0.borrow().script_import_sites.clone()
    }

    /// Potential Realm-origin imports found in successfully parsed finite
    /// sources, including nested sources and Function constructor bodies.
    pub fn realm_module_requests(&self) -> Vec<ModuleRequestKeyIr> {
        self.prepared_sources.realm_module_requests()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_parse_identity_and_rejections_keep_the_exact_grammar_context() {
        let cache = PreparedSourceCache::default();
        let input = PreparedSourceInput::ShadowRealmEvaluate("1 + 2".encode_utf16().collect());
        let first = cache.parse(input.clone()).unwrap();
        let again = cache.parse(input).unwrap();
        assert_eq!(first.syntax_identity(), again.syntax_identity());
        let other_kind = cache
            .parse(PreparedSourceInput::RealmScript(
                "1 + 2".encode_utf16().collect(),
            ))
            .unwrap();
        assert_ne!(first.syntax_identity(), other_kind.syntax_identity());

        let context = |invocation| DirectEvalParseContext {
            strict_caller: false,
            invocation,
            private_names: Vec::new(),
        };
        let outside = PreparedSourceInput::DirectEval(
            "new.target".encode_utf16().collect(),
            context(scriptw_front::EvalInvocationContext::Script),
        );
        let rejected = cache.parse(outside.clone()).unwrap_err();
        let count = cache.0.borrow().parsed.len();
        assert_eq!(cache.parse(outside).unwrap_err(), rejected);
        assert_eq!(cache.0.borrow().parsed.len(), count);
        assert!(
            cache
                .parse(PreparedSourceInput::DirectEval(
                    "new.target".encode_utf16().collect(),
                    context(scriptw_front::EvalInvocationContext::Function),
                ))
                .is_ok()
        );
        assert!(
            cache
                .parse_function(DynamicFunctionKind::Ordinary, &["yield 1;".into()])
                .is_err()
        );
        assert!(
            cache
                .parse_function(DynamicFunctionKind::Generator, &["yield 1;".into()])
                .is_ok()
        );
    }

    #[test]
    fn nested_and_function_sources_discover_requests_without_reparsing_on_retry() {
        let source = scriptw_front::parse(
            r#"
const realm = new ShadowRealm();
const evaluate = realm.evaluate;
evaluate.call(realm, `new ShadowRealm().evaluate('void new ShadowRealm().importValue("./nested.js", "value")')`);
Function('void new ShadowRealm().importValue("./function.js", "value")')();
"#,
            ParseOptions::script(),
        )
        .unwrap();
        let session = LoweringSession::default();
        let program = session.lower_source(&source, crate::HostSurfacePolicy::default());
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        assert_eq!(
            session.realm_module_requests(),
            [
                ModuleRequestKeyIr::plain("./function.js"),
                ModuleRequestKeyIr::plain("./nested.js"),
            ]
        );
        let identities = || {
            session
                .prepared_sources
                .0
                .borrow()
                .parsed
                .iter()
                .map(|(_, parsed)| {
                    parsed
                        .as_ref()
                        .map(ParsedScript::syntax_identity)
                        .map_err(Clone::clone)
                })
                .collect::<Vec<_>>()
        };
        let first = identities();
        let retried = session.lower_source(&source, crate::HostSurfacePolicy::default());
        assert!(retried.is_wasm_supported(), "{:?}", retried.diagnostics);
        assert_eq!(identities(), first);
    }

    #[test]
    fn realm_import_aliases_forwarding_optional_calls_and_finite_names_are_candidates() {
        let source = scriptw_front::parse(
            r#"
const realm = new ShadowRealm();
const load = realm.importValue;
const specifier = './' + 'alias.js';
const requestKey = 'importValue', callKey = 'call', applyKey = 'apply';
load.call(realm, specifier, 'value');
Reflect.apply(load, realm, ['./reflected.js', 'value']);
realm['import' + 'Value']('./computed.js', 'value');
realm.importValue?.('./optional.js', 'value');
realm?.importValue('./optional-receiver.js', 'value');
realm?.['import' + 'Value']?.('./optional-computed.js', 'value');
load?.call(realm, './optional-call-target.js', 'value');
load.call?.(realm, './optional-call-method.js', 'value');
load?.apply(realm, ['./optional-apply-target.js', 'value']);
load.apply?.(realm, ['./optional-apply-method.js', 'value']);
realm?.importValue.call(realm, './optional-direct-call-target.js', 'value');
realm.importValue.call?.(realm, './optional-direct-call-method.js', 'value');
realm?.importValue.apply(realm, ['./optional-direct-apply-target.js', 'value']);
realm.importValue.apply?.(realm, ['./optional-direct-apply-method.js', 'value']);
realm[requestKey]('./named-key.js', 'value');
realm?.[requestKey]('./optional-key.js', 'value');
realm?.[requestKey][callKey](realm, './optional-key-call-target.js', 'value');
realm[requestKey][callKey]?.(realm, './optional-key-call-method.js', 'value');
load?.[applyKey](realm, ['./optional-key-apply.js', 'value']);
"#,
            ParseOptions::script(),
        )
        .unwrap();
        let session = LoweringSession::default();
        let program = session.lower_source(&source, crate::HostSurfacePolicy::default());
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        assert_eq!(
            session.realm_module_requests(),
            [
                ModuleRequestKeyIr::plain("./alias.js"),
                ModuleRequestKeyIr::plain("./computed.js"),
                ModuleRequestKeyIr::plain("./named-key.js"),
                ModuleRequestKeyIr::plain("./optional-apply-method.js"),
                ModuleRequestKeyIr::plain("./optional-apply-target.js"),
                ModuleRequestKeyIr::plain("./optional-call-method.js"),
                ModuleRequestKeyIr::plain("./optional-call-target.js"),
                ModuleRequestKeyIr::plain("./optional-computed.js"),
                ModuleRequestKeyIr::plain("./optional-direct-apply-method.js"),
                ModuleRequestKeyIr::plain("./optional-direct-apply-target.js"),
                ModuleRequestKeyIr::plain("./optional-direct-call-method.js"),
                ModuleRequestKeyIr::plain("./optional-direct-call-target.js"),
                ModuleRequestKeyIr::plain("./optional-key-apply.js"),
                ModuleRequestKeyIr::plain("./optional-key-call-method.js"),
                ModuleRequestKeyIr::plain("./optional-key-call-target.js"),
                ModuleRequestKeyIr::plain("./optional-key.js"),
                ModuleRequestKeyIr::plain("./optional-receiver.js"),
                ModuleRequestKeyIr::plain("./optional.js"),
                ModuleRequestKeyIr::plain("./reflected.js"),
            ]
        );
    }

    #[test]
    fn optional_source_projection_retains_original_function_parameter_ownership() {
        let source = scriptw_front::parse(
            r#"
const realm = new ShadowRealm();
((source) => realm.evaluate(source))?.('6 * 7');
(function (source) { return realm.evaluate(source); })?.('7 * 8');
({ invoke(source) { return realm.evaluate(source); } })?.invoke('8 * 9');
"#,
            ParseOptions::script(),
        )
        .unwrap();
        let program =
            LoweringSession::default().lower_source(&source, crate::HostSurfacePolicy::default());
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.expect("script IR");
        for source in ["6 * 7", "7 * 8", "8 * 9"] {
            assert!(
                script.prepared_scripts.iter().any(|prepared| {
                    prepared.kind == PreparedScriptKind::ShadowRealmEvaluate
                        && prepared.source == source
                        && matches!(
                            prepared.outcome,
                            crate::PreparedScriptOutcome::Executable(_)
                        )
                }),
                "original function parameter lost source candidate {source}"
            );
        }
    }

    #[test]
    fn semantic_utf16_cache_identity_keeps_codec_data_and_grammar_context_separate() {
        let cache = PreparedSourceCache::default();
        let source_units = [
            "'".encode_utf16().collect::<Vec<_>>(),
            vec![0xd83d, 0xdc38],
            "';".encode_utf16().collect(),
        ]
        .concat();
        let canonical = crate::encode_js_string_utf16(&source_units);
        let split = format!(
            "'{}{}';",
            crate::encode_js_string_utf16(&[0xd83d]),
            crate::encode_js_string_utf16(&[0xdc38])
        );
        let first = cache
            .parse_script(&PreparedScriptKind::IndirectEval, &canonical)
            .unwrap();
        let second = cache
            .parse_script(&PreparedScriptKind::IndirectEval, &split)
            .unwrap();
        assert_eq!(first.syntax_identity(), second.syntax_identity());
        assert_eq!(first.compiler_source_utf16(), source_units);
        let realm = cache
            .parse_script(&PreparedScriptKind::RealmScript, &canonical)
            .unwrap();
        assert_ne!(first.syntax_identity(), realm.syntax_identity());
        let marker_units = "'\u{F0000}D83D';".encode_utf16().collect::<Vec<_>>();
        let marker = cache
            .parse_script(
                &PreparedScriptKind::IndirectEval,
                &crate::encode_js_string_utf16(&marker_units),
            )
            .unwrap();
        assert_eq!(marker.compiler_source_utf16(), marker_units);
        assert_ne!(first.syntax_identity(), marker.syntax_identity());
    }
    #[test]
    fn nested_direct_eval_imports_retain_the_script_record_and_cached_syntax() {
        let source = scriptw_front::parse(
            r#"eval('eval("import(\\"./target.js\\");")');"#,
            ParseOptions::script(),
        )
        .unwrap();
        let session = LoweringSession::default();
        session.lower_source(&source, crate::HostSurfacePolicy::default());
        let sites = session.prepared_script_import_sites();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].static_specifier.as_deref(), Some("./target.js"));
        assert_eq!(sites[0].phase, crate::ImportPhaseIr::Evaluation);
        let parsed = session.prepared_sources.0.borrow().parsed.clone();
        let scriptw_front::ParsedSource::Script(entry) = source else {
            unreachable!()
        };
        let mut graph = crate::ModuleGraphSources {
            modules: vec![
                crate::ModuleSourceIr::from_parsed_script(
                    crate::ModuleKey::from_host("/suite/dir/entry.js"),
                    "file:///suite/dir/entry.js".into(),
                    entry,
                ),
                crate::ModuleSourceIr::new(
                    crate::ModuleKey::from_host("/suite/dir/target.js"),
                    "export const value = 42;".into(),
                    "file:///suite/dir/target.js".into(),
                ),
            ],
            entry: 0,
            resolutions: vec![(0, ModuleRequestKeyIr::plain("./target.js"), 1)],
            realm_requests: Default::default(),
        };
        assert!(graph.modules[0].include_prepared_script_import_sites(&sites));
        assert!(!graph.modules[0].include_prepared_script_import_sites(&sites));
        let program = session.lower_loaded_graph(&graph, crate::HostSurfacePolicy::default(), None);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let linked = program.modules.as_ref().unwrap();
        assert!(
            linked
                .dynamic_components()
                .iter()
                .any(|component| component.referrer() == linked.entry
                    && component.request().specifier() == "./target.js")
        );
        let script = program.script.as_ref().unwrap();
        assert!(script.prepared_scripts.iter().any(|prepared| {
            let crate::PreparedScriptOutcome::Executable(unit) = &prepared.outcome else {
                return false;
            };
            matches!(prepared.kind, PreparedScriptKind::DirectEval(_))
                && unit.body.statements.iter().any(|statement| {
                    matches!(
                        statement,
                        crate::StatementIr::Expression(crate::TypedExpr {
                            expr: crate::ExprIr::DynamicImport {
                                referrer: crate::ImportCallSourceOriginIr::Local(Some(0)),
                                ..
                            },
                            ..
                        })
                    )
                })
        }));
        assert!(
            script
                .executable_script_bodies()
                .any(|body| body.statements.iter().any(|statement| {
                    let crate::StatementIr::Expression(crate::TypedExpr {
                        expr: crate::ExprIr::ModuleExecutionGraph(execution),
                        ..
                    }) = statement
                    else {
                        return false;
                    };
                    execution
                        .script_import_dispatcher(0, crate::ImportPhaseIr::Evaluation)
                        .is_some()
                        && execution
                            .script_import_dispatcher(1, crate::ImportPhaseIr::Evaluation)
                            .is_none()
                }))
        );
        let retained = session.prepared_sources.0.borrow();
        for (input, old) in parsed {
            let current = &retained
                .parsed
                .iter()
                .find(|(key, _)| key == &input)
                .unwrap()
                .1;
            assert_eq!(
                old.as_ref().map(ParsedScript::syntax_identity),
                current.as_ref().map(ParsedScript::syntax_identity)
            );
        }
    }

    #[test]
    fn computed_and_unproved_eval_origins_cannot_claim_script_import_edges() {
        for (text, options, expected_gap) in [
            (
                r#"eval('import(specifier)');"#,
                ParseOptions::script(),
                None,
            ),
            (
                r#"(0, eval)('import("./indirect.js")');"#,
                ParseOptions::script(),
                Some(crate::DynamicSourceGap::aot_known_source(
                    crate::DynamicSourceKind::IndirectEval,
                )),
            ),
            (
                r#"eval('import("./module-relative.js")');"#,
                ParseOptions::module(),
                None,
            ),
        ] {
            let source = scriptw_front::parse(text, options).unwrap();
            let session = LoweringSession::default();
            let program = session.lower_source(&source, crate::HostSurfacePolicy::default());
            // A resolved intrinsic indirect eval keeps its exact unsupported
            // environment gap; optional runtime candidates may omit that unit.
            let expected_diagnostics = expected_gap
                .map(crate::IrDiagnostic::unsupported_dynamic_source)
                .into_iter()
                .collect::<Vec<_>>();
            assert_eq!(program.diagnostics, expected_diagnostics, "{text}");
            assert_eq!(
                program.is_wasm_supported(),
                expected_gap.is_none(),
                "{text}: {:?}",
                program.diagnostics
            );
            assert!(session.prepared_script_import_sites().is_empty(), "{text}");
            if let Some(script) = program.script {
                assert!(script.prepared_scripts.iter().all(|prepared| !matches!(
                    prepared.outcome,
                    crate::PreparedScriptOutcome::Executable(_)
                )));
            }
        }
    }

    #[test]
    fn prepared_script_dispatchers_own_options_helpers_without_caller_captures() {
        for directive in ["", "'use strict';\n"] {
            let source = scriptw_front::parse(
                &format!("{directive}eval('import(\"./target.js\", options)');"),
                ParseOptions::script(),
            )
            .unwrap();
            let scriptw_front::ParsedSource::Script(entry) = &source else {
                unreachable!()
            };
            let session = LoweringSession::default();
            session.lower_source(&source, crate::HostSurfacePolicy::default());
            let mut entry = crate::ModuleSourceIr::from_parsed_script(
                crate::ModuleKey::from_host("/suite/entry.js"),
                "file:///suite/entry.js".into(),
                entry.clone(),
            );
            assert!(
                entry.include_prepared_script_import_sites(&session.prepared_script_import_sites())
            );
            let graph = crate::ModuleGraphSources {
                modules: vec![
                    entry,
                    crate::ModuleSourceIr::new(
                        crate::ModuleKey::from_host("/suite/target.js"),
                        "export const value = 42;".into(),
                        "file:///suite/target.js".into(),
                    ),
                ],
                entry: 0,
                resolutions: vec![(0, ModuleRequestKeyIr::plain("./target.js"), 1)],
                realm_requests: Default::default(),
            };
            let program =
                session.lower_loaded_graph(&graph, crate::HostSurfacePolicy::default(), None);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let script = program.script.as_ref().unwrap();
            let execution = script
                .executable_script_bodies()
                .flat_map(|body| &body.statements)
                .find_map(|statement| match statement {
                    crate::StatementIr::Expression(crate::TypedExpr {
                        expr: crate::ExprIr::ModuleExecutionGraph(graph),
                        ..
                    }) => Some(graph),
                    _ => None,
                })
                .expect("canonical Script graph is retained");
            let id = execution
                .script_import_dispatcher(0, crate::ImportPhaseIr::Evaluation)
                .expect("prepared import has a canonical dispatcher");
            let dispatcher = script
                .functions
                .iter()
                .find(|function| &function.id == id)
                .unwrap();
            assert!(
                dispatcher.captured_bindings.is_empty(),
                "{:?}",
                dispatcher.captured_bindings
            );
            assert!(!dispatcher.captures_private_environment);
            assert!(!dispatcher.captures_lexical_this);
            assert!(!dispatcher.captures_lexical_arguments);
            assert_eq!(
                dispatcher.protocol.execution_kind(),
                crate::FunctionExecutionKind::Async
            );
            fn local_callable(statements: &[crate::StatementIr]) -> Option<&crate::FunctionId> {
                statements.iter().find_map(|statement| match statement {
                    crate::StatementIr::Lexical {
                        init:
                            crate::TypedExpr {
                                expr: crate::ExprIr::FunctionValue(id),
                                ..
                            },
                        ..
                    } => Some(id),
                    crate::StatementIr::Block(block) => local_callable(&block.statements),
                    crate::StatementIr::LexicalBlock(statements)
                    | crate::StatementIr::ParameterInitialization { statements, .. } => {
                        local_callable(statements)
                    }
                    _ => None,
                })
            }
            let helper_id = local_callable(&dispatcher.body.statements)
                .expect("the dispatcher initializes its own validation callable");
            let helper = script
                .functions
                .iter()
                .find(|function| &function.id == helper_id)
                .unwrap();
            assert!(helper.is_nested);
            assert_eq!(helper.name, "$scriptw$module$importOptions");
        }
    }
}
