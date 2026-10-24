//! An optional, closed certificate for incremental finite Realm-source discovery.
//!
//! The ordinary lowering pass still admits the first requests and emits the
//! final graph. This worklist only closes independent fresh-Realm requests
//! between those passes. A shadow, write, alias, unknown call, source-phase
//! edge, or imported binding declines the certificate and keeps normal lowering.

use super::loaded_sources::ModuleParse;
use super::*;
use crate::prepared_source_cache::PreparedSourceCache;
use crate::*;

#[derive(Debug)]
struct SourceMemo {
    owner: SourceOwner,
    prepared: Option<Vec<String>>,
    admitted: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum SourceOwner {
    Script(scriptw_front::ParsedScriptIdentity),
    Module(scriptw_front::ParsedModuleIdentity),
    Inert,
}

#[derive(Debug, Default)]
pub(crate) struct PureRealmDiscovery {
    sources: BTreeMap<ModuleRecordKey, SourceMemo>,
    prepared: std::collections::HashSet<scriptw_front::ParsedScriptIdentity>,
    #[cfg(test)]
    pub(crate) scanned_sources: usize,
}

impl PureRealmDiscovery {
    pub(crate) fn discover(
        &mut self,
        graph: &ModuleGraphSources,
        cache: &PreparedSourceCache,
    ) -> bool {
        // No source/ordinary import edges are projected or guessed here. Each
        // non-entry body must be the target of an already admitted Realm job.
        // In particular an unused declared catalog or a source-only record
        // cannot acquire authority from merely containing a source literal.
        let realm_targets = graph
            .realm_requests
            .values()
            .filter_map(|resolution| match resolution {
                RealmModuleResolutionIr::Loaded(target) => Some(*target),
                RealmModuleResolutionIr::Rejected(_) => None,
            })
            .collect::<BTreeSet<_>>();
        if !graph.resolutions.is_empty()
            || graph.modules.iter().enumerate().any(|(index, _)| {
                index != graph.entry as usize && !realm_targets.contains(&(index as u32))
            })
        {
            return false;
        }

        // Check the complete current parent-Realm surface before admitting any
        // new source. Cached certificates retain the actual source owner; a
        // changed payload at the same host key must be checked again.
        for source in &graph.modules {
            let key = source.record_key();
            if source.foreign.is_some()
                || matches!(
                    source.parse,
                    ModuleParse::Rejected { .. }
                        | ModuleParse::JsonRejected { .. }
                        | ModuleParse::ModuleSource { .. }
                )
            {
                return false;
            }
            let owner = syntax_owner(source);
            if self
                .sources
                .get(&key)
                .is_none_or(|memo| memo.owner != owner)
            {
                #[cfg(test)]
                {
                    self.scanned_sources += 1;
                }
                self.sources.insert(
                    key.clone(),
                    SourceMemo {
                        owner,
                        prepared: pure_source(source),
                        admitted: false,
                    },
                );
            }
            if self.sources[&key].prepared.is_none() {
                return false;
            }
        }

        let mut pending = Vec::new();
        for source in &graph.modules {
            let memo = self
                .sources
                .get_mut(&source.record_key())
                .expect("certified source");
            if !memo.admitted {
                memo.admitted = true;
                pending.extend(std::mem::take(
                    memo.prepared.as_mut().expect("certified source"),
                ));
            }
        }
        while let Some(source) = pending.pop() {
            // This uses normal admission, with exact UTF-16, grammar-specific
            // failures and Realm request collection owned by the same cache.
            let Ok(parsed) =
                cache.parse_script(&crate::PreparedScriptKind::ShadowRealmEvaluate, &source)
            else {
                continue;
            };
            let identity = parsed.syntax_identity();
            if !self.prepared.insert(identity) {
                continue;
            }
            // Arbitrary code in the newly created child Realm cannot mutate
            // its parent. Recurse further only when this child's own parent
            // surface has the same closed no-mutation certificate.
            if let Some(sources) = parsed.with_compiler_session(pure_script) {
                pending.extend(sources);
            }
        }
        true
    }
}

fn syntax_owner(source: &ModuleSourceIr) -> SourceOwner {
    match &source.parse {
        ModuleParse::Module(source) => SourceOwner::Module(source.syntax_identity()),
        ModuleParse::ScriptEntry(source) => SourceOwner::Script(source.syntax_identity()),
        ModuleParse::Json(_)
        | ModuleParse::Bytes(_)
        | ModuleParse::Text(_)
        | ModuleParse::ModuleSource { .. }
        | ModuleParse::Rejected { .. }
        | ModuleParse::JsonRejected { .. } => SourceOwner::Inert,
    }
}

fn pure_source(source: &ModuleSourceIr) -> Option<Vec<String>> {
    if source.foreign.is_some() {
        return None;
    }
    match &source.parse {
        ModuleParse::Module(parsed) => {
            // Newly loaded records have not necessarily gone through the next
            // graph lowering yet. Keep module early errors/private-identifier
            // admission ahead of any requests discovered from their bodies.
            super::record::parse_module_record(parsed, 0, source.key().clone()).ok()?;
            parsed.with_compiler_session(|module, interner| {
                let mut prepared = Vec::new();
                for item in module.items().items() {
                    let pure = match item {
                        ModuleItem::StatementListItem(item) => {
                            pure_item(item, interner, &mut prepared)
                        }
                        // Imported aliases are intentionally left to semantic
                        // lowering, including aliases named ShadowRealm.
                        ModuleItem::ImportDeclaration(_) => false,
                        ModuleItem::ExportDeclaration(export) => match export.as_ref() {
                            ExportDeclaration::List(_) => true,
                            ExportDeclaration::Declaration(declaration) => {
                                pure_declaration(declaration, interner)
                            }
                            ExportDeclaration::VarStatement(_) => false,
                            ExportDeclaration::DefaultAssignmentExpression(expression) => {
                                pure_literal(expression)
                            }
                            ExportDeclaration::ReExport { .. }
                            | ExportDeclaration::DefaultFunctionDeclaration(_)
                            | ExportDeclaration::DefaultGeneratorDeclaration(_)
                            | ExportDeclaration::DefaultAsyncFunctionDeclaration(_)
                            | ExportDeclaration::DefaultAsyncGeneratorDeclaration(_)
                            | ExportDeclaration::DefaultClassDeclaration(_) => false,
                        },
                    };
                    if !pure {
                        return None;
                    }
                }
                Some(prepared)
            })
        }
        ModuleParse::ScriptEntry(source) => source.with_compiler_session(pure_script),
        ModuleParse::Json(_) | ModuleParse::Bytes(_) | ModuleParse::Text(_) => Some(Vec::new()),
        ModuleParse::ModuleSource { .. }
        | ModuleParse::Rejected { .. }
        | ModuleParse::JsonRejected { .. } => None,
    }
}

fn pure_script(script: &Script, interner: &Interner) -> Option<Vec<String>> {
    let mut prepared = Vec::new();
    script
        .statements()
        .statements()
        .iter()
        .all(|item| pure_item(item, interner, &mut prepared))
        .then_some(prepared)
}

fn pure_item(item: &StatementListItem, interner: &Interner, prepared: &mut Vec<String>) -> bool {
    match item {
        StatementListItem::Declaration(declaration) => pure_declaration(declaration, interner),
        StatementListItem::Statement(statement) => match statement.as_ref() {
            Statement::Empty => true,
            Statement::Expression(expression) => pure_expression(expression, interner, prepared),
            // Var creates object-backed entry bindings, and every remaining
            // statement can introduce effects/control not covered by this proof.
            _ => false,
        },
    }
}

fn pure_declaration(declaration: &Declaration, interner: &Interner) -> bool {
    let Declaration::Lexical(LexicalDeclaration::Let(list) | LexicalDeclaration::Const(list)) =
        declaration
    else {
        return false;
    };
    list.as_ref().iter().all(|variable| {
        let Binding::Identifier(identifier) = variable.binding() else {
            return false;
        };
        interner.resolve_expect(identifier.sym()).utf8() != Some("ShadowRealm")
            && variable.init().is_none_or(pure_literal)
    })
}

fn pure_literal(expression: &Expression) -> bool {
    match expression.flatten() {
        Expression::Literal(_) => true,
        Expression::Unary(unary)
            if matches!(
                unary.op(),
                UnaryOp::Minus | UnaryOp::Plus | UnaryOp::Void | UnaryOp::Not | UnaryOp::Tilde
            ) =>
        {
            matches!(unary.target().flatten(), Expression::Literal(_))
        }
        _ => false,
    }
}

fn pure_expression(
    expression: &Expression,
    interner: &Interner,
    prepared: &mut Vec<String>,
) -> bool {
    let expression = expression.flatten();
    if pure_literal(expression) {
        return true;
    }
    if let Expression::Unary(unary) = expression {
        return unary.op() == UnaryOp::Void && pure_expression(unary.target(), interner, prepared);
    }
    let Expression::Call(call) = expression else {
        return false;
    };
    let Expression::PropertyAccess(PropertyAccess::Simple(access)) = call.function().flatten()
    else {
        return false;
    };
    let Expression::New(realm) = access.target().flatten() else {
        return false;
    };
    let Expression::Identifier(constructor) = realm.constructor().flatten() else {
        return false;
    };
    if interner.resolve_expect(constructor.sym()).utf8() != Some("ShadowRealm")
        || !realm.arguments().is_empty()
    {
        return false;
    }
    let PropertyAccessField::Const(method) = access.field() else {
        return false;
    };
    match interner.resolve_expect(method.sym()).utf8() {
        Some("evaluate") => {
            let [Expression::Literal(source)] = call.args() else {
                return false;
            };
            let LiteralKind::String(symbol) = source.kind() else {
                return false;
            };
            prepared.push(crate::encode_js_string_utf16(
                interner.resolve_expect(*symbol).utf16(),
            ));
            true
        }
        Some("importValue") => {
            matches!(call.args(), [Expression::Literal(specifier), Expression::Literal(name)]
                if matches!(specifier.kind(), LiteralKind::String(_))
                    && matches!(name.kind(), LiteralKind::String(_)))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph_with_target(entry: &str, target: Option<&str>) -> ModuleGraphSources {
        let ParsedSource::Script(parsed) =
            scriptw_front::parse(entry, scriptw_front::ParseOptions::script()).unwrap()
        else {
            panic!("Script goal");
        };
        let mut graph = ModuleGraphSources {
            realm_requests: BTreeMap::new(),
            modules: vec![ModuleSourceIr::from_parsed_script(
                ModuleKey::from_host("entry.js"),
                "entry.js".into(),
                parsed,
            )],
            entry: 0,
            resolutions: Vec::new(),
        };
        if let Some(target) = target {
            graph.modules.push(ModuleSourceIr::new(
                ModuleKey::from_host("target.js"),
                target.into(),
                "target.js".into(),
            ));
            graph.realm_requests.insert(
                ModuleRequestKeyIr::plain("target.js"),
                RealmModuleResolutionIr::Loaded(1),
            );
        }
        graph
    }

    #[test]
    fn pure_realm_worklist_checks_each_retained_source_once_and_closes_nested_sources() {
        let mut graph = graph_with_target(
            r#"new ShadowRealm().evaluate(`new ShadowRealm().evaluate('void new ShadowRealm().importValue("./nested.js", "value")')`);"#,
            Some("export const value = 1;"),
        );
        // Templates are deliberately outside the closed certificate.
        let mut discovery = PureRealmDiscovery::default();
        let cache = PreparedSourceCache::default();
        assert!(!discovery.discover(&graph, &cache));
        graph = graph_with_target(
            r#"new ShadowRealm().evaluate("new ShadowRealm().evaluate('void new ShadowRealm().importValue(\"./nested.js\", \"value\")')");"#,
            Some("export const value = 1;"),
        );
        let mut discovery = PureRealmDiscovery::default();
        assert!(discovery.discover(&graph, &cache));
        assert_eq!(
            cache.realm_module_requests(),
            [ModuleRequestKeyIr::plain("./nested.js")]
        );
        assert_eq!(discovery.scanned_sources, 2);
        assert!(discovery.discover(&graph, &cache));
        assert_eq!(discovery.scanned_sources, 2);
        graph.modules.push(ModuleSourceIr::new(
            ModuleKey::from_host("nested.js"),
            "export const value = 42;".into(),
            "nested.js".into(),
        ));
        graph.realm_requests.insert(
            ModuleRequestKeyIr::plain("./nested.js"),
            RealmModuleResolutionIr::Loaded(2),
        );
        assert!(discovery.discover(&graph, &cache));
        assert_eq!(discovery.scanned_sources, 3);
    }

    #[test]
    fn pure_realm_certificate_refuses_shadows_mutation_aliases_and_unadmitted_bodies() {
        for entry in [
            r#"const ShadowRealm = null; new ShadowRealm().evaluate('new ShadowRealm().importValue("./hidden.js", "value")');"#,
            r#"globalThis.ShadowRealm = replacement; new ShadowRealm().evaluate('new ShadowRealm().importValue("./hidden.js", "value")');"#,
            r#"const source = 'new ShadowRealm().importValue("./hidden.js", "value")'; new ShadowRealm().evaluate(source);"#,
            r#"unknown(); new ShadowRealm().evaluate('new ShadowRealm().importValue("./hidden.js", "value")');"#,
            r#"function dormant() { new ShadowRealm().evaluate('new ShadowRealm().importValue("./hidden.js", "value")'); }"#,
        ] {
            let cache = PreparedSourceCache::default();
            assert!(
                !PureRealmDiscovery::default().discover(&graph_with_target(entry, None), &cache),
                "{entry}"
            );
            assert!(cache.realm_module_requests().is_empty(), "{entry}");
        }
        let mut unused = graph_with_target(
            "true;",
            Some(
                r#"new ShadowRealm().evaluate('new ShadowRealm().importValue("./hidden.js", "value")'); export const value = 1;"#,
            ),
        );
        unused.realm_requests.clear();
        let cache = PreparedSourceCache::default();
        assert!(!PureRealmDiscovery::default().discover(&unused, &cache));
        assert!(cache.realm_module_requests().is_empty());
    }

    #[test]
    fn pure_realm_discovery_preserves_prepared_grammar_rejection_before_request_admission() {
        let source = graph_with_target(
            r#"new ShadowRealm().evaluate('new.target; new ShadowRealm().importValue("./hidden.js", "value")');"#,
            None,
        );
        let cache = PreparedSourceCache::default();
        assert!(PureRealmDiscovery::default().discover(&source, &cache));
        assert!(cache.realm_module_requests().is_empty());
    }

    #[test]
    fn pure_realm_discovery_keeps_exact_utf16_source_units() {
        let source = graph_with_target(
            r#"new ShadowRealm().evaluate("'\uD800'; void new ShadowRealm().importValue('./utf16.js', 'value');");"#,
            None,
        );
        let cache = PreparedSourceCache::default();
        assert!(PureRealmDiscovery::default().discover(&source, &cache));
        assert_eq!(
            cache.realm_module_requests(),
            [ModuleRequestKeyIr::plain("./utf16.js")]
        );
        let units = [
            vec![b'\'' as u16, 0xd800, b'\'' as u16],
            "; void new ShadowRealm().importValue('./utf16.js', 'value');"
                .encode_utf16()
                .collect(),
        ]
        .concat();
        let parsed = cache
            .parse_script(
                &crate::PreparedScriptKind::ShadowRealmEvaluate,
                &crate::encode_js_string_utf16(&units),
            )
            .unwrap();
        assert_eq!(parsed.compiler_source_utf16(), units);

        let invalid = graph_with_target(r#"new ShadowRealm().evaluate('\uD800');"#, None);
        let cache = PreparedSourceCache::default();
        assert!(PureRealmDiscovery::default().discover(&invalid, &cache));
        assert!(cache.realm_module_requests().is_empty());
        assert!(
            cache
                .parse_script(
                    &crate::PreparedScriptKind::ShadowRealmEvaluate,
                    &crate::encode_js_string_utf16(&[0xd800]),
                )
                .is_err()
        );
    }
}
