//! AOT dynamic-import request dispatch and module job continuations.
//!
//! The host loads every statically discoverable target before compilation.
//! Runtime dispatch uses the complete specifier/phase/attribute identity; no
//! runtime source parser or JavaScript interpreter is involved.
//!
//! Canonical Module- and Script-entry graphs use compiler-owned async
//! dispatchers. Operands are evaluated at the user call site. The dispatcher
//! creates its intrinsic promise before ToString and import-options reads;
//! these coercions still run synchronously and any abrupt value rejects it.
//! Trusted source-position metadata selects intrinsic error/descriptor/key
//! operations, module namespace cells and canonical module evaluation.
//!
//! ContinueDynamicImport first reacts to LoadRequestedModules. An evaluation
//! request then evaluates the module once and reacts to its completed Evaluate
//! promise; a deferred request without asynchronous dependencies resolves in
//! the first continuation. Await of undefined supplies these exact internal
//! reactions without consulting mutable Promise properties. The second
//! evaluation continuation preserves the cached abrupt value unchanged.
//!
//! Each call owns a fresh promise; module records own evaluation state and
//! cached completion, and eager/deferred namespace identities remain distinct.
//! Dynamic-only load/link syntax failures are emitted in the importing job.
//! Static dependency failures still reject the entry before execution.
//!
//! Source Text Module source requests reject with a fresh intrinsic SyntaxError
//! after loading and parsing, before dependency loading or evaluation. They
//! allocate no source object. Top-level-await targets share the canonical jobs.

use crate::*;

use boa_ast::expression::ImportCall;

/// One module reachable through `import()`.
///
/// A component is minted per `(referrer, ModuleRequest)` pair, not per module:
/// two modules may both write `import('./m.js')` and mean different files, and
/// two requests for one spelling may carry different phases or attributes.
/// Keeping the request whole makes it impossible for component registration
/// and runtime lookup to disagree about either field. The runtime match is
/// therefore on that pair, which
/// [`ModuleGraphIr::resolve_dynamic_component`] performs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicComponentIr {
    /// Host-normalized key of the target. Diagnostics and `import.meta` use
    /// it; the runtime string is *not* matched against it, because the runtime
    /// string is the specifier as written.
    key: ModuleKey,
    /// Full occurrence whose phase-free key the host resolved.
    request: ModuleRequestIr,
    /// Module that wrote the call site.
    referrer: ModuleUnitId,
    /// Module `specifier` resolves to.
    module: ModuleUnitId,
}

impl DynamicComponentIr {
    /// Host-normalized identity of the resolved target.
    #[must_use]
    pub const fn target_key(&self) -> &ModuleKey {
        &self.key
    }

    /// Full phaseful request written by the referrer.
    #[must_use]
    pub const fn request(&self) -> &ModuleRequestIr {
        &self.request
    }

    /// Module containing the dynamic-import occurrence.
    #[must_use]
    pub const fn referrer(&self) -> ModuleUnitId {
        self.referrer
    }

    /// Unit selected by host resolution for this occurrence.
    #[must_use]
    pub const fn target(&self) -> ModuleUnitId {
        self.module
    }
}

/// Discovers every statically knowable `import()` target in the loaded graph.
///
/// Loaded closures discover literal requests and let computed sites select
/// exact keys already discovered from their referrer's source. Complete
/// catalogs additionally project sites onto every declared host key. Neither
/// path loads a new computed name at runtime.
/// An unmatched key rejects, with no fallback that reaches a parser.
///
/// The returned set is intentionally wider than the artifact registry. Graph
/// classification needs edges from every loaded unit to decide which units
/// materialize; after that fixed point, graph linking retains only
/// components whose referrer can run.
pub(super) fn discover_components(
    graph: &ModuleGraphIr,
    admission: super::admission::GraphAdmission,
) -> Vec<DynamicComponentIr> {
    let mut components: Vec<DynamicComponentIr> = Vec::new();
    for index in 0..graph.units.len() {
        let referrer = ModuleUnitId::try_from(index)
            .expect("unit index is capped by build_graph, which rejects a graph with more units than MAX_LINKABLE_MODULE_UNIT_ID");
        let sites = graph.units[index].record.dynamic_import_sites.clone();
        for site in sites {
            let keys = graph
                .resolutions
                .keys()
                .filter_map(|(owner, key)| (*owner == referrer).then_some(key));
            for request in admission.occurrences(&site, &graph.units[index].record, keys) {
                let Some(module) = graph.resolve_request(referrer, &request) else {
                    continue;
                };
                if components
                    .iter()
                    .any(|existing| existing.referrer == referrer && existing.request == request)
                {
                    continue;
                }
                let key = graph.unit(module).record.key.clone();
                components.push(DynamicComponentIr {
                    key,
                    request,
                    referrer,
                    module,
                });
            }
        }
    }
    components
}

impl ModuleGraphIr {
    /// The component `request` in `referrer` resolves to.
    ///
    /// The single authority for the runtime lookup rule, so the backend and
    /// the registry cannot disagree about it.
    ///
    /// `referrer` is `None` for an `import()` whose call site belongs to no unit
    /// of this graph, which has no compile-time resolution context at all: such
    /// a call always rejects.
    #[must_use]
    pub fn resolve_dynamic_component(
        &self,
        referrer: Option<ModuleUnitId>,
        request: &ModuleRequestIr,
    ) -> Option<&DynamicComponentIr> {
        let referrer = referrer?;
        self.dynamic_components()
            .iter()
            .find(|component| component.referrer() == referrer && component.request() == request)
    }
}

/// Lowers `import(specifier, options)` to [`ExprIr::DynamicImport`].
///
/// `specifier` and `options` are the *already lowered* operands, in evaluation
/// order — the caller lowers them because only the caller owns a lowerer.
/// Neither is coerced here: `ToString` of the specifier happens after the
/// promise capability exists, so it must reject rather than throw.
///
/// `referrer` retains the call site's local or external resolution authority.
/// An external direct-eval caller stays distinct even without a discovered
/// Script record; its AOT capability boundary belongs to actual execution.
///
/// Every phase lowers the same way. What a phase changes is which object the
/// promise is settled with — an evaluated/deferred namespace or the source
/// phase rejection for the loaded Source Text Module — and that is
/// [`ModuleGraphIr::dynamic_import_dispatchers`]'s decision, not this one's.
///
/// # Errors
/// Currently infallible. The signature stays fallible because it is the seam
/// where a phase the linker cannot serve would be reported.
pub(crate) fn lower_import_call(
    call: &ImportCall,
    specifier: TypedExpr,
    options: Option<TypedExpr>,
    referrer: ImportCallSourceOriginIr,
) -> Result<TypedExpr, String> {
    let phase = ImportPhaseIr::from_ast(call.phase());
    // Always a Promise object, on every path including the rejecting one, so
    // the kind is a singleton and the backend emits the value directly.
    Ok(TypedExpr::from_info(
        ValueInfo {
            kind: ValueKind::Object,
            possible_kinds: KindSet::from_kind(ValueKind::Object),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::none(),
        },
        ExprIr::DynamicImport {
            specifier: Box::new(specifier),
            options: options.map(Box::new),
            phase,
            referrer,
        },
    ))
}

fn append_component_condition(text: &mut String, request: &ModuleRequestIr) {
    text.push_str(" if (key === ");
    text.push_str(&js_string_literal(request.specifier()));
    text.push_str(" && attributeKeys.length === ");
    text.push_str(&request.attributes().len().to_string());
    for (index, attribute) in request.attributes().iter().enumerate() {
        text.push_str(&format!(" && attributeKeys[{index}] === "));
        text.push_str(&js_string_literal(&attribute.key));
        text.push_str(&format!(" && attributeValues[{index}] === "));
        text.push_str(&js_string_literal(&attribute.value));
    }
    text.push_str(") {");
}

fn append_import_options_validation(
    text: &mut String,
    execution: DynamicImportDispatcherExecution,
) {
    let (error_constructor, own_keys, descriptor, rejection_prefix, rejection_suffix) =
        match execution {
            DynamicImportDispatcherExecution::RetainedMerged => (
                "TypeError",
                "Reflect.ownKeys",
                "Object.getOwnPropertyDescriptor",
                "reject(",
                "); return;",
            ),
            // Sequence callees force ordinary value lowering before invocation;
            // eval-visible environment references must not claim private names.
            DynamicImportDispatcherExecution::CompiledModuleJobs => (
                "$scriptw$module$TypeError",
                "(0, $scriptw$module$ownKeys)",
                "(0, $scriptw$module$getOwnPropertyDescriptor)",
                "throw ",
                ";",
            ),
        };
    let reject = |message: &str| {
        format!(
            "{rejection_prefix}new {error_constructor}({}){rejection_suffix}",
            js_string_literal(message)
        )
    };
    let invalid_options = reject("Import options must be an object");
    let invalid_attributes = reject("Import attributes must be an object");
    let invalid_value = reject("Import attribute values must be strings");
    text.push_str(&format!(r#" var attributeKeys = [], attributeValues = []; if (options !== void 0) {{
        if (options === null || (typeof options !== "object" && typeof options !== "function")) {{ {invalid_options} }}
        var withObject = options.with; if (withObject !== void 0) {{
        if (withObject === null || (typeof withObject !== "object" && typeof withObject !== "function")) {{ {invalid_attributes} }}
        var ownKeys = {own_keys}(withObject), ownKeyIndex = 0, attributeKey, attributeDescriptor, attributeValue;
        while (ownKeyIndex < ownKeys.length) {{ attributeKey = ownKeys[ownKeyIndex];
        if (typeof attributeKey !== "string") {{ ownKeyIndex++; continue; }}
        attributeDescriptor = {descriptor}(withObject, attributeKey);
        if (attributeDescriptor !== void 0 && attributeDescriptor.enumerable) {{
        attributeValue = withObject[attributeKey]; if (typeof attributeValue !== "string") {{ {invalid_value} }}
        attributeKeys[attributeKeys.length] = attributeKey; attributeValues[attributeValues.length] = attributeValue;
        }} ownKeyIndex++; }}
        var sortIndex = 1, sortKey, sortValue, insertionIndex; while (sortIndex < attributeKeys.length) {{
        sortKey = attributeKeys[sortIndex]; sortValue = attributeValues[sortIndex]; insertionIndex = sortIndex;
        while (insertionIndex > 0 && attributeKeys[insertionIndex - 1] > sortKey) {{
        attributeKeys[insertionIndex] = attributeKeys[insertionIndex - 1];
        attributeValues[insertionIndex] = attributeValues[insertionIndex - 1]; insertionIndex--; }}
        attributeKeys[insertionIndex] = sortKey; attributeValues[insertionIndex] = sortValue; sortIndex++;
        }} }} }}"#).replace('\n', " "));
}

const IMPORT_OPTIONS_HELPER: &str = "$scriptw$module$importOptions";

/// Validation stays synchronous inside the dispatcher's already-created
/// intrinsic promise. Sharing it adds no promise or job, and all operations
/// retain compiler-owned intrinsic authority in this private source region.
fn import_options_helper_source() -> String {
    let mut text = format!("function {IMPORT_OPTIONS_HELPER}(options) {{");
    append_import_options_validation(
        &mut text,
        DynamicImportDispatcherExecution::CompiledModuleJobs,
    );
    text.push_str(" return [attributeKeys, attributeValues]; }");
    text
}

/// Whether one Script's outer source directly requires a host-provided module
/// graph.
///
/// This is deliberately a closed classification rather than a Boolean:
/// callers that persist or replay source must reject
/// [`OuterScriptModuleDependency::Indeterminate`] instead of silently treating
/// scanner uncertainty as absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OuterScriptModuleDependency {
    /// The outer Script AST contains no module request.
    None,
    /// A retained, successfully parsed Script AST contains dynamic import.
    RequiresModuleGraph,
    /// Parsing failed and lexical evidence cannot prove absence of a request.
    Indeterminate,
}

/// Classifies whether the outer `source` directly requests a module graph.
///
/// A successfully parsed Script is decided from its retained AST, so a method
/// or property named `import` is not confused with an import call. Parse-failure
/// probes use the ECMAScript-aware import-call scanner only as conservative
/// evidence: a possible call or scanner failure is
/// [`OuterScriptModuleDependency::Indeterminate`].
#[must_use]
pub fn classify_outer_script_module_dependency(source: &str) -> OuterScriptModuleDependency {
    match scriptw_front::parse(source, scriptw_front::ParseOptions::script()) {
        Ok(ParsedSource::Script(script)) => {
            if !super::record::script_has_dynamic_import(&script) {
                OuterScriptModuleDependency::None
            } else {
                OuterScriptModuleDependency::RequiresModuleGraph
            }
        }
        // Script parse options make this unreachable today. Treat a future
        // front-end contract violation as uncertainty rather than admitting
        // source whose dependency shape was classified under the wrong goal.
        Ok(ParsedSource::Module(_)) => OuterScriptModuleDependency::Indeterminate,
        Err(_) => match ImportCallScanner::new(source).run() {
            Ok(sites) if sites.is_empty() => OuterScriptModuleDependency::None,
            Ok(_) | Err(_) => OuterScriptModuleDependency::Indeterminate,
        },
    }
}

/// Whether a successfully parsed Script contains a dynamic import of any phase.
/// Hosts retaining the parsed Script should use [`super::script_has_dynamic_import`]
/// directly to avoid parsing the same source twice.
#[must_use]
pub fn source_writes_dynamic_import(source: &str) -> bool {
    matches!(
        classify_outer_script_module_dependency(source),
        OuterScriptModuleDependency::RequiresModuleGraph
    )
}

#[derive(Clone, Copy)]
enum DynamicImportDispatcherExecution {
    RetainedMerged,
    CompiledModuleJobs,
}

pub(super) fn module_evaluator_name(module: ModuleUnitId) -> String {
    format!("{LINKER_NAME_PREFIX}evaluate${module}")
}

pub(super) fn module_dispatcher_intrinsic(name: &str) -> Option<StandardBuiltinId> {
    match name {
        "$scriptw$module$TypeError" => Some(StandardBuiltinId::TypeErrorConstructor),
        "$scriptw$module$SyntaxError" => Some(StandardBuiltinId::SyntaxErrorConstructor),
        "$scriptw$module$ownKeys" => Some(StandardBuiltinId::ReflectOwnKeys),
        "$scriptw$module$getOwnPropertyDescriptor" => {
            Some(StandardBuiltinId::ObjectGetOwnPropertyDescriptor)
        }
        _ => None,
    }
}

/// Prefix every identifier the linker synthesizes for `import()` carries.
///
/// `$` is an identifier character in JavaScript, so these are ordinary names in
/// the merged top-level scope rather than anything the parser treats specially.
/// "Ordinary" would mean "collidable" if nothing checked, so
/// Source-record admission rejects parsed identifiers starting with this prefix,
/// including nested bindings that could capture a rewritten import call.
pub const LINKER_NAME_PREFIX: &str = "$scriptw$module$";

/// Validate parsed identifiers before any linker-private names enter the source.
/// Properties and string literals remain ordinary user data; identifier
/// references and bindings cannot expose or shadow the private dispatcher names.
pub(super) fn validate_linker_identifiers(
    node: boa_ast::visitor::NodeRef<'_>,
    interner: &Interner,
    key: &ModuleKey,
) -> Result<(), Vec<IrDiagnostic>> {
    struct IdentifierScan<'a> {
        interner: &'a Interner,
        collisions: BTreeSet<String>,
    }
    impl<'ast> Visitor<'ast> for IdentifierScan<'_> {
        type BreakTy = core::convert::Infallible;

        fn visit_identifier(
            &mut self,
            node: &'ast boa_ast::expression::Identifier,
        ) -> ControlFlow<Self::BreakTy> {
            let name = self.interner.resolve_expect(node.sym()).to_string();
            if name.starts_with(LINKER_NAME_PREFIX) {
                self.collisions.insert(name);
            }
            ControlFlow::Continue(())
        }
    }
    let mut scan = IdentifierScan {
        interner,
        collisions: BTreeSet::new(),
    };
    let _ = scan.visit(node);
    if scan.collisions.is_empty() {
        Ok(())
    } else {
        Err(scan
            .collisions
            .into_iter()
            .map(|name| {
                IrDiagnostic::unsupported(format!(
                    "unsupported in scriptw wasm-aot: module {}: identifier `{name}` collides \
                     with a linker-synthesized name",
                    key.as_str()
                ))
            })
            .collect())
    }
}

/// Merged-scope name of the `import()` dispatcher `unit`'s call sites call.
///
/// One per *referrer and phase*, not one per target: two modules may both write
/// `import('./m.js')` and mean different files, so the specifier alone does not
/// identify a target and the dispatcher has to be the thing that knows which
/// module asked — and one module may write both `import('./m.js')` and
/// `import.source('./m.js')`, which settle with different objects for the same
/// runtime string, so the phase cannot be recovered from the argument either.
pub(super) fn dispatcher_name(unit: ModuleUnitId, phase: ImportPhaseIr) -> String {
    match phase {
        // The unphased name is left exactly as it was: it is by far the common
        // case and every existing artifact spells it this way.
        ImportPhaseIr::Evaluation => format!("{LINKER_NAME_PREFIX}import${unit}"),
        ImportPhaseIr::Defer | ImportPhaseIr::Source => {
            format!("{LINKER_NAME_PREFIX}import${unit}${}", phase.as_str())
        }
    }
}

/// Merged-scope binding a dispatcher resolves a component with.
///
/// This is the whole phase distinction, in one place:
///
/// * evaluation — the module's namespace object after its evaluation
///   continuation completes;
/// * defer — the independent deferred namespace cell. `import.defer('m')`
///   and `import defer * as ns from 'm'` share this identity even when an
///   evaluation-phase request also evaluates the module eagerly;
/// * source — the actual host record's source cell; ordinary records reject first.
fn component_resolution_cell(component: &DynamicComponentIr) -> Option<MergedName> {
    match component.request().phase() {
        ImportPhaseIr::Evaluation => Some(MergedName::minted(
            component.target(),
            UnitCellRole::Namespace,
        )),
        ImportPhaseIr::Defer => Some(MergedName::minted(
            component.target(),
            UnitCellRole::DeferredNamespace,
        )),
        ImportPhaseIr::Source => Some(MergedName::minted(
            component.target(),
            UnitCellRole::ModuleSource,
        )),
    }
}

impl ModuleGraphIr {
    /// JavaScript the merged script must run before any module body, after the
    /// namespace prelude `modules::namespace` owns.
    ///
    /// This lane mints no namespace objects of its own. A dispatcher's
    /// `resolve` names the unit's `UnitCellRole::Namespace` cell, the *same* binding
    /// `import * as ns` aliases, so `import("./a.mjs")` and
    /// `import * as ns from "./a.mjs"` hand back one object — 16.2.1.10 caches
    /// `[[Namespace]]` per module and test262's `module-code/namespace/`
    /// compares the two with `===`. The dispatcher bodies are deferred, so a
    /// `function` declaration may precede the `const` it names.
    ///
    /// Emitted as a *single line* with no interior line terminator, and empty
    /// when the graph has nothing dynamic in it. Every line of every unit body
    /// in the merged script is therefore displaced by the same fixed amount no
    /// matter how large the graph is, which is what keeps a diagnostic's line
    /// number worth reading.
    #[must_use]
    pub fn dynamic_import_prelude(&self) -> String {
        self.dynamic_import_dispatchers()
    }

    /// One `import()` dispatcher function per module that writes an `import()`.
    ///
    /// A module with call sites but no resolvable target still gets one: its
    /// dispatcher falls straight through to `reject`, which is what a computed
    /// specifier naming nothing in the compiled graph must do.
    #[must_use]
    pub fn dynamic_import_dispatchers(&self) -> String {
        self.import_dispatchers(DynamicImportDispatcherExecution::RetainedMerged)
    }

    pub(super) fn module_initialization_dispatchers(&self, script_entry: bool) -> String {
        let dispatchers = self
            .materialized_units()
            .filter(|(id, _, _)| (self.entry_is_script && *id == self.entry) == script_entry)
            .flat_map(|(referrer, _, unit)| {
                unit.record
                    .dynamic_import_sites
                    .iter()
                    .map(|site| site.phase)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .map(move |phase| {
                        self.dispatcher_source(
                            referrer,
                            phase,
                            DynamicImportDispatcherExecution::CompiledModuleJobs,
                        )
                    })
            })
            .collect::<Vec<_>>()
            .join(" ");
        if dispatchers.is_empty() {
            return dispatchers;
        }
        format!("{} {dispatchers}", import_options_helper_source())
    }

    fn import_dispatchers(&self, execution: DynamicImportDispatcherExecution) -> String {
        let mut text = String::new();
        for (referrer, _, unit) in self.materialized_units() {
            // One per phase the unit actually writes, so an unphased graph
            // emits exactly the one dispatcher it always did.
            let phases: BTreeSet<ImportPhaseIr> = unit
                .record
                .dynamic_import_sites
                .iter()
                .map(|site| site.phase)
                .collect();
            for phase in phases {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&self.dispatcher_source(referrer, phase, execution));
            }
        }
        text
    }

    /// One dispatcher declaration, with no source-line displacement.
    fn dispatcher_source(
        &self,
        referrer: ModuleUnitId,
        phase: ImportPhaseIr,
        execution: DynamicImportDispatcherExecution,
    ) -> String {
        let synchronous = matches!(
            execution,
            DynamicImportDispatcherExecution::CompiledModuleJobs
        );
        let mut text = String::from(if synchronous {
            "async function "
        } else {
            "function "
        });
        text.push_str(&dispatcher_name(referrer, phase));
        text.push_str(if synchronous {
            "(specifier, options) {"
        } else {
            "(specifier, options) { return new Promise(function (resolve, reject) {"
        });
        if synchronous && self.entry_is_script && referrer == self.entry {
            // Prepared eval materializes this canonical Script dispatcher in
            // the selected Realm's global environment. Own its unchanged
            // validation helper locally rather than capture the Script prelude.
            text.push_str(&import_options_helper_source());
        }
        // Both operands were evaluated at the call site. AsyncFunctionStart
        // creates the intrinsic promise before this synchronous coercion.
        text.push_str(" var key = `${specifier}`;");
        if synchronous {
            text.push_str(&format!(
                " var attributes = (0, {IMPORT_OPTIONS_HELPER})(options), \
                 attributeKeys = attributes[0], attributeValues = attributes[1];"
            ));
        } else {
            append_import_options_validation(&mut text, execution);
        }
        for component in self.dynamic_components().iter().filter(|component| {
            component.referrer() == referrer && component.request().phase() == phase
        }) {
            append_component_condition(&mut text, component.request());
            // Source phase loads the record without opening dependencies or
            // executing it. Only actual host source records own source cells.
            if phase == ImportPhaseIr::Source
                && self
                    .unit(component.target())
                    .record
                    .module_source_tag()
                    .is_none()
            {
                text.push_str(if synchronous {
                    " throw new $scriptw$module$SyntaxError("
                } else {
                    " throw new SyntaxError("
                });
                text.push_str(&js_string_literal(
                    &ModuleLinkErrorIr::SourceUnavailable {
                        referrer,
                        request: component.request().clone(),
                    }
                    .message(),
                ));
                text.push_str("); }");
                continue;
            }
            let resolution = component_resolution_cell(component)
                .expect("namespace phases own a resolution cell");
            if synchronous {
                // ContinueDynamicImport first reacts to LoadRequestedModules.
                text.push_str(" await void 0;");
                match phase {
                    ImportPhaseIr::Evaluation => {
                        text.push_str(" await ");
                        text.push_str(&module_evaluator_name(component.target()));
                        text.push(';');
                    }
                    ImportPhaseIr::Defer => {
                        text.push_str(" if (");
                        text.push_str(&module_async_dependencies_name(component.target()));
                        text.push_str(") await ");
                        text.push_str(&module_deferred_import_name(component.target()));
                        text.push(';');
                    }
                    ImportPhaseIr::Source => {}
                }
                text.push_str(" return ");
                text.push_str(resolution.as_str());
                text.push_str("; }");
            } else {
                text.push_str(" resolve(");
                text.push_str(resolution.as_str());
                text.push_str("); return; }");
            }
        }
        for rejection in self.dynamic_rejections.iter().filter(|rejection| {
            rejection.referrer == referrer && rejection.request.phase() == phase
        }) {
            assert!(
                synchronous,
                "deferred load failures belong to canonical execution graphs"
            );
            append_component_condition(&mut text, &rejection.request);
            match rejection.stage {
                super::admission::DynamicModuleRejectionStage::ModuleLoad => {}
                super::admission::DynamicModuleRejectionStage::Dependencies => {
                    text.push_str(" await void 0;")
                }
            }
            text.push_str(match rejection.kind {
                super::admission::DynamicModuleRejectionKind::Syntax => {
                    " throw new $scriptw$module$SyntaxError("
                }
                super::admission::DynamicModuleRejectionKind::HostLoad => {
                    " throw new $scriptw$module$TypeError("
                }
            });
            text.push_str(&js_string_literal(&rejection.message));
            text.push_str("); }");
        }
        text.push_str(if synchronous {
            " throw new $scriptw$module$TypeError(\"Cannot find module \" + key); }"
        } else {
            " reject(new TypeError(\"Cannot find module \" + key)); }); }"
        });
        text
    }

    /// Rewrites dynamic-import heads retained by the original parser.
    ///
    /// The public entry accepts this unit's original source. The linker carries
    /// source-boundary mappings across its earlier edits, so inserted default
    /// declaration terminators cannot invalidate these ranges. Properties,
    /// methods, regular expressions and comments are never rediscovered here.
    ///
    /// # Errors
    /// Returns a diagnostic if the source does not belong to this unit, or if
    /// original parser ranges have been lost during an earlier rewrite.
    ///
    /// # Panics
    /// Panics if `unit` is not a unit of this graph.
    pub fn rewrite_dynamic_import_calls(
        &self,
        unit: ModuleUnitId,
        source: &str,
    ) -> Result<String, String> {
        if source != self.unit(unit).source_text {
            return Err("dynamic-import rewrite requires the unit's original source text".into());
        }
        self.rewrite_dynamic_import_source(
            unit,
            super::callable_source::OriginalUnitSource::new(source),
        )
        .map(super::callable_source::OriginalUnitSource::into_text)
    }

    pub(super) fn rewrite_dynamic_import_source(
        &self,
        unit: ModuleUnitId,
        mut source: super::callable_source::OriginalUnitSource,
    ) -> Result<super::callable_source::OriginalUnitSource, String> {
        let record = &self.unit(unit).record;
        let heads = &record.dynamic_import_heads;
        if heads.len()
            != record
                .dynamic_import_sites
                .iter()
                .filter(|site| site.has_retained_source_head())
                .count()
        {
            return Err("dynamic-import call is missing its original parser range".into());
        }
        let spans: Vec<_> = heads.iter().map(|head| head.span).collect();
        let ranges = source.current_ranges(&spans)?;
        for (head, range) in heads.iter().zip(ranges).rev() {
            let mut replacement = dispatcher_name(unit, head.phase);
            // Keep exact LineTerminatorSequences inside phase trivia. Spaces
            // between them prevent separate CR and LF from becoming a CRLF.
            for sequence in super::source::collect_ecmascript_line_terminator_sequences(
                &source.text()[range.clone()],
            ) {
                replacement.push(' ');
                replacement.push_str(sequence);
            }
            source.replace(range.start, range.end, &replacement);
        }
        Ok(source)
    }

    /// Every reason this graph's `import()` usage cannot be desugared.
    ///
    /// Empty means [`Self::dynamic_import_prelude`] and
    /// [`Self::rewrite_dynamic_import_calls`] describe the program exactly.
    ///
    /// Deliberately scoped to what `import()` reaches: a namespace observed only
    /// by a static `import * as ns` is somebody else's report to make, and
    /// duplicating it would print the same problem twice.
    #[must_use]
    pub fn check_dynamic_import_linkable(&self) -> Vec<IrDiagnostic> {
        let mut diagnostics = Vec::new();

        for (_, _, unit) in self.materialized_units() {
            let key = unit.record.key.as_str();
            for binding in &unit.record.environment {
                let merged = unit.record.merged(&binding.name);
                // The `$m<unit>$…` / `$d<unit>$` range is minted by
                // `MergedName::minted` and `MergedName::anonymous_default`, and
                // `merged_in` is the identity on a source name — so a module
                // that declares `$m0$namespace` lands in the same merged cell as
                // the prelude's `const $m0$namespace = Object.create(null);`.
                // Left unchecked that is a duplicate-declaration SyntaxError
                // from the merged script for a legal module. Pathological, but
                // it is the same merged-name hazard invariant M3 governs, and
                // parsed linker-prefix admission covers a different family.
                //
                // Asked of the *source* spelling, not of `merged`. `merged_in`
                // is the identity on a `LocalName::Source`, so for that variant
                // the two spellings are the same string and the question is
                // unchanged — but `LocalName::AnonymousDefault` is mapped *into*
                // the minted range on purpose, and asking `merged` there reports
                // every module with an anonymous `export default` as colliding
                // with the very cell the linker minted for it. `spec_name` is
                // `*default*` for that variant, which no `BindingIdentifier` can
                // spell and which is therefore never minted-shaped, so the two
                // generators stay distinguishable here by construction rather
                // than by a second predicate that could drift.
                if MergedName::is_minted_shaped(binding.name.spec_name()) {
                    diagnostics.push(IrDiagnostic::unsupported(format!(
                        "unsupported in scriptw wasm-aot: module {key}: top-level `{}` collides \
                         with a linker-minted per-unit cell name",
                        merged.as_str()
                    )));
                }
            }
        }

        // A Script entry owns root statements rather than a module activation,
        // so no request can evaluate it as a module or expose a module namespace.
        if self.entry_is_script
            && self
                .resolutions
                .values()
                .copied()
                .chain(
                    self.dynamic_components()
                        .iter()
                        .map(|component| component.target()),
                )
                .any(|target| target == self.entry)
        {
            diagnostics.push(IrDiagnostic::unsupported(format!(
                "unsupported in scriptw wasm-aot: script {} is also imported as a module of its \
                 own `import()` graph",
                self.unit(self.entry).record.key.as_str()
            )));
        }

        for (module, mode) in self.component_namespace_modules() {
            let Some(namespace) = self
                .units
                .get(module as usize)
                .and_then(|unit| unit.namespaces.get(&mode))
            else {
                diagnostics.push(IrDiagnostic::unsupported(format!(
                    "unsupported in scriptw wasm-aot: `import()` target module {module} has no \
                     namespace object"
                )));
                continue;
            };
            let key = self.unit(module).record.key.as_str();
            for export in &namespace.exports {
                // The same spellability rule `modules::namespace` applies when
                // it emits the getter, asked through the same function, so the
                // two cannot disagree about which exports are expressible.
                if super::namespace::namespace_target_reference(&export.target).is_none() {
                    diagnostics.push(IrDiagnostic::unsupported(format!(
                        "unsupported in scriptw wasm-aot: module {key}: `import()` cannot expose \
                         export `{}`, whose binding has no name in the merged scope",
                        export.export_name.as_str()
                    )));
                }
            }
        }

        diagnostics
    }

    /// Modules whose namespace object a materialized `import()` can reach.
    ///
    /// Transitive, because `export * as inner from "m"` makes one namespace's
    /// export *be* another module's namespace: resolving the outer one hands the
    /// inner object to the program even though no `import()` named it.
    fn component_namespace_modules(&self) -> BTreeSet<(ModuleUnitId, ModuleNamespaceModeIr)> {
        // Source phase rejects before any namespace is requested. Evaluation
        // and Defer retain their separate namespace identities.
        let mut observed: BTreeSet<(ModuleUnitId, ModuleNamespaceModeIr)> = self
            .dynamic_components()
            .iter()
            .filter_map(|entry| {
                entry
                    .request()
                    .phase()
                    .namespace_mode()
                    .map(|mode| (entry.target(), mode))
            })
            .collect();
        let mut pending: Vec<(ModuleUnitId, ModuleNamespaceModeIr)> =
            observed.iter().copied().collect();
        while let Some((module, mode)) = pending.pop() {
            let Some(namespace) = self
                .units
                .get(module as usize)
                .and_then(|unit| unit.namespaces.get(&mode))
            else {
                continue;
            };
            let nested: Vec<(ModuleUnitId, ModuleNamespaceModeIr)> = namespace
                .exports
                .iter()
                .filter_map(|export| match &export.target {
                    ResolvedBindingIr::Resolved {
                        module,
                        binding: ModuleBindingNameIr::Namespace(mode),
                    } => Some((*module, *mode)),
                    ResolvedBindingIr::Resolved { .. }
                    | ResolvedBindingIr::Ambiguous
                    | ResolvedBindingIr::NotFound => None,
                })
                .collect();
            for module in nested {
                if observed.insert(module) {
                    pending.push(module);
                }
            }
        }
        observed
    }
}

/// `value` as a double-quoted JavaScript string literal.
///
/// Escapes U+2028 and U+2029 as well as the obvious cases: both are ordinary
/// characters in an ECMAScript string but line terminators to the *scanner*,
/// and an export name or a specifier is arbitrary text that may contain either.
fn js_string_literal(value: &str) -> String {
    let mut text = String::with_capacity(value.len() + 2);
    text.push('"');
    for character in value.chars() {
        match character {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            '\u{2028}' | '\u{2029}' => {
                text.push_str(&format!("\\u{:04X}", character as u32));
            }
            character if (character as u32) < 0x20 => {
                text.push_str(&format!("\\u{:04X}", character as u32));
            }
            character => text.push(character),
        }
    }
    text.push('"');
    text
}

/// What a `/` means at the current position.
enum SlashMeaning {
    /// The previous significant token can end an expression, so `/` divides.
    Divide,
    /// The previous significant token cannot end an expression, so `/` opens a
    /// regular-expression literal.
    Regexp,
}

/// One `import(`, `import.defer(` or `import.source(` call site the scanner
/// found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ImportCallSite {
    /// Byte offset of the `import` keyword.
    start: usize,
    /// Byte offset one past the last byte the dispatcher name replaces — the
    /// end of `import` for an unphased call, and the end of the phase name for
    /// a phased one, so `import . defer (x)` loses its whole head in one piece.
    end: usize,
    phase: ImportPhaseIr,
}

/// Finds the byte range of every `import` keyword that opens an `import(` call.
///
/// Used only as conservative evidence after Script parsing has failed. Valid
/// source rewriting and dependency discovery consume the retained parser AST.
/// A scanner failure or possible call never admits a source as dependency-free.
struct ImportCallScanner<'a> {
    source: &'a str,
    bytes: &'a [u8],
    /// Byte ranges of `import` heads to replace, ascending, non-overlapping.
    sites: Vec<ImportCallSite>,
    /// Nesting depth of `(`, `[` and `{`, tracked only so that the `}` closing a
    /// template substitution is told apart from an ordinary `}`.
    depth: usize,
    /// One entry per open template substitution, holding the `depth` *inside*
    /// it.
    template_stack: Vec<usize>,
    slash: SlashMeaning,
    /// The previous significant token was `.`, so the next word is a property
    /// name rather than a keyword.
    previous_was_dot: bool,
    index: usize,
}

impl<'a> ImportCallScanner<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            sites: Vec::new(),
            depth: 0,
            template_stack: Vec::new(),
            slash: SlashMeaning::Regexp,
            previous_was_dot: false,
            index: 0,
        }
    }

    fn run(mut self) -> Result<Vec<ImportCallSite>, String> {
        while self.index < self.bytes.len() {
            let byte = self.bytes[self.index];
            match byte {
                b'/' if self.bytes.get(self.index + 1) == Some(&b'/') => self.skip_line_comment(),
                b'/' if self.bytes.get(self.index + 1) == Some(&b'*') => {
                    self.skip_block_comment()?;
                }
                b'/' => match &self.slash {
                    SlashMeaning::Regexp => {
                        self.skip_regexp()?;
                        self.slash = SlashMeaning::Divide;
                        self.previous_was_dot = false;
                    }
                    SlashMeaning::Divide => {
                        self.slash = SlashMeaning::Regexp;
                        self.previous_was_dot = false;
                        self.index += self.char_len_at(self.index);
                    }
                },
                b'\'' | b'"' => {
                    self.index = self.string_end(self.index, byte)?;
                    self.slash = SlashMeaning::Divide;
                    self.previous_was_dot = false;
                }
                b'`' => {
                    self.index += 1;
                    self.scan_template_body()?;
                }
                b'(' | b'[' | b'{' => {
                    self.depth += 1;
                    self.index += 1;
                    self.slash = SlashMeaning::Regexp;
                    self.previous_was_dot = false;
                }
                b')' | b']' => {
                    self.depth = self.depth.saturating_sub(1);
                    self.index += 1;
                    self.slash = SlashMeaning::Divide;
                    self.previous_was_dot = false;
                }
                b'}' => {
                    // The stack holds the depth *inside* the substitution, so
                    // the match is against the current depth before unwinding.
                    let closes_substitution = self
                        .template_stack
                        .last()
                        .is_some_and(|open_depth| *open_depth == self.depth);
                    self.depth = self.depth.saturating_sub(1);
                    self.index += 1;
                    if closes_substitution {
                        self.template_stack.pop();
                        self.scan_template_body()?;
                        continue;
                    }
                    self.slash = SlashMeaning::Divide;
                    self.previous_was_dot = false;
                }
                b'.' => {
                    self.index += 1;
                    self.slash = SlashMeaning::Regexp;
                    self.previous_was_dot = true;
                }
                // Non-ASCII whitespace is tested *before* the identifier arm.
                // `is_identifier_start_byte` accepts every non-ASCII byte, so
                // without this a U+FEFF byte-order mark or a U+00A0 in front of
                // the keyword would be scanned as part of the word, yielding
                // `\u{FEFF}import` — not `import` — and silently losing the
                // call site. Neither `slash` nor `previous_was_dot` moves, for
                // the same reason the ASCII whitespace arm below leaves both
                // alone: trivia does not end a token.
                byte if !byte.is_ascii() && is_js_whitespace(self.char_at(self.index)) => {
                    self.index += self.char_len_at(self.index);
                }
                byte if is_identifier_start_byte(byte) => self.scan_word(),
                byte if byte.is_ascii_digit() => {
                    self.skip_number();
                    self.slash = SlashMeaning::Divide;
                    self.previous_was_dot = false;
                }
                byte if byte.is_ascii_whitespace() => self.index += 1,
                _ => {
                    // Any other punctuator. `++`/`--` end an expression; every
                    // other operator opens one.
                    let two = self.source.get(self.index..self.index + 2);
                    self.slash = if two == Some("++") || two == Some("--") {
                        SlashMeaning::Divide
                    } else {
                        SlashMeaning::Regexp
                    };
                    self.previous_was_dot = false;
                    self.index += self.char_len_at(self.index);
                }
            }
        }
        Ok(self.sites)
    }

    /// Words after which a `/` starts a regular expression rather than a
    /// division.
    ///
    /// Purely a lexing concern: `return /re/` is a regexp, `x / y` is a divide.
    /// `let`, `static` and friends are contextual and legal as binding names, so
    /// they are deliberately absent.
    fn is_reserved_word(word: &str) -> bool {
        matches!(
            word,
            "await"
                | "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "debugger"
                | "default"
                | "delete"
                | "do"
                | "else"
                | "enum"
                | "export"
                | "extends"
                | "false"
                | "finally"
                | "for"
                | "function"
                | "if"
                | "import"
                | "in"
                | "instanceof"
                | "new"
                | "null"
                | "return"
                | "super"
                | "switch"
                | "this"
                | "throw"
                | "true"
                | "try"
                | "typeof"
                | "var"
                | "void"
                | "while"
                | "with"
                | "yield"
        )
    }

    fn scan_word(&mut self) {
        let start = self.index;
        while let Some(character) = self.source[self.index..].chars().next() {
            // Byte-wise for ASCII, char-wise for the rest: a word must not
            // swallow the non-ASCII whitespace that ends it, or the word it
            // yields is not the word that is written.
            if character.is_ascii() {
                if !is_identifier_part_byte(character as u8) {
                    break;
                }
                self.index += 1;
            } else {
                if is_js_whitespace(character) {
                    break;
                }
                self.index += character.len_utf8();
            }
        }
        let word = &self.source[start..self.index];
        if word == "import" && !self.previous_was_dot {
            if self.peek_significant() == Some(b'(') {
                self.sites.push(ImportCallSite {
                    start,
                    end: self.index,
                    phase: ImportPhaseIr::Evaluation,
                });
            } else if let Some((phase, end)) = self.peek_phased_call() {
                // The index is deliberately *not* advanced past the phase name.
                // The main loop rescans `.defer` as an ordinary property access,
                // which records nothing and leaves `slash` and `previous_was_dot`
                // exactly where a hand-written pass would have left them.
                self.sites.push(ImportCallSite { start, end, phase });
            }
        }
        self.slash = match word {
            "this" | "super" | "true" | "false" | "null" => SlashMeaning::Divide,
            word if Self::is_reserved_word(word) => SlashMeaning::Regexp,
            _ => SlashMeaning::Divide,
        };
        self.previous_was_dot = false;
    }

    fn char_len_at(&self, index: usize) -> usize {
        self.source[index..]
            .chars()
            .next()
            .map_or(1, char::len_utf8)
    }

    fn char_at(&self, index: usize) -> char {
        self.source[index..].chars().next().unwrap_or(' ')
    }

    /// `(phase, end)` when the `import` keyword just scanned is the head of a
    /// phased call — `import.defer(` or `import.source(` — with `end` one past
    /// the phase name.
    ///
    /// `import.meta`, `import.defer` used as anything but a call, and a phase
    /// name that is not one of the two all answer `None`, so the caller records
    /// no site and the ordinary member-access path handles the text.
    ///
    /// This is a *lexical* recognition of the `import . defer` sequence, so
    /// whitespace and comments between the three tokens are as legal here as
    /// they are to the parser.
    fn peek_phased_call(&self) -> Option<(ImportPhaseIr, usize)> {
        let dot = self.skip_trivia_from(self.index).ok()?;
        if self.bytes.get(dot).copied() != Some(b'.') {
            return None;
        }
        let name_start = self.skip_trivia_from(dot + 1).ok()?;
        let mut name_end = name_start;
        while self
            .bytes
            .get(name_end)
            .copied()
            .is_some_and(is_identifier_part_byte)
        {
            name_end += 1;
        }
        let phase = match self.source.get(name_start..name_end)? {
            "defer" => ImportPhaseIr::Defer,
            "source" => ImportPhaseIr::Source,
            _ => return None,
        };
        let open = self.skip_trivia_from(name_end).ok()?;
        if self.bytes.get(open).copied() != Some(b'(') {
            return None;
        }
        Some((phase, name_end))
    }

    /// First non-whitespace, non-comment byte at or after `self.index`.
    fn peek_significant(&self) -> Option<u8> {
        let index = self.skip_trivia_from(self.index).ok()?;
        self.bytes.get(index).copied()
    }

    /// Skips whitespace and comments starting at `index`.
    fn skip_trivia_from(&self, mut index: usize) -> Result<usize, String> {
        loop {
            match self.bytes.get(index).copied() {
                Some(byte) if byte.is_ascii_whitespace() => index += 1,
                Some(b'/') if self.bytes.get(index + 1) == Some(&b'/') => {
                    while self
                        .bytes
                        .get(index)
                        .copied()
                        .is_some_and(|byte| byte != b'\n')
                    {
                        index += 1;
                    }
                }
                Some(b'/') if self.bytes.get(index + 1) == Some(&b'*') => {
                    let mut end = index + 2;
                    loop {
                        if end + 1 >= self.bytes.len() {
                            return Err("unterminated block comment".to_string());
                        }
                        if self.bytes[end] == b'*' && self.bytes[end + 1] == b'/' {
                            end += 2;
                            break;
                        }
                        end += 1;
                    }
                    index = end;
                }
                Some(byte) if !byte.is_ascii() => {
                    let character = self.char_at(index);
                    if is_js_whitespace(character) {
                        index += character.len_utf8();
                    } else {
                        return Ok(index);
                    }
                }
                _ => return Ok(index),
            }
        }
    }

    fn string_end(&self, start: usize, quote: u8) -> Result<usize, String> {
        let mut index = start + 1;
        while index < self.bytes.len() {
            match self.bytes[index] {
                b'\\' => index += 1 + self.char_len_at((index + 1).min(self.bytes.len())),
                byte if byte == quote => return Ok(index + 1),
                _ => index += self.char_len_at(index),
            }
        }
        Err("unterminated string literal".to_string())
    }

    fn skip_line_comment(&mut self) {
        while self
            .bytes
            .get(self.index)
            .copied()
            .is_some_and(|byte| byte != b'\n')
        {
            self.index += 1;
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), String> {
        let mut index = self.index + 2;
        loop {
            if index + 1 >= self.bytes.len() {
                return Err("unterminated block comment".to_string());
            }
            if self.bytes[index] == b'*' && self.bytes[index + 1] == b'/' {
                self.index = index + 2;
                return Ok(());
            }
            index += 1;
        }
    }

    fn skip_number(&mut self) {
        while self.bytes.get(self.index).copied().is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_' || byte == b'$'
        }) {
            self.index += 1;
        }
    }

    fn skip_regexp(&mut self) -> Result<(), String> {
        let mut index = self.index + 1;
        let mut in_class = false;
        loop {
            let Some(byte) = self.bytes.get(index).copied() else {
                return Err("unterminated regular expression literal".to_string());
            };
            match byte {
                b'\\' => index += 1 + self.char_len_at((index + 1).min(self.bytes.len())),
                b'[' => {
                    in_class = true;
                    index += 1;
                }
                b']' => {
                    in_class = false;
                    index += 1;
                }
                b'/' if !in_class => {
                    index += 1;
                    break;
                }
                b'\n' => return Err("unterminated regular expression literal".to_string()),
                _ => index += self.char_len_at(index),
            }
        }
        while self
            .bytes
            .get(index)
            .copied()
            .is_some_and(is_identifier_part_byte)
        {
            index += 1;
        }
        self.index = index;
        Ok(())
    }

    /// Consumes a template body, stopping after its closing backtick or inside a
    /// `${` substitution (which is ordinary source and must keep being scanned).
    fn scan_template_body(&mut self) -> Result<(), String> {
        while let Some(byte) = self.bytes.get(self.index).copied() {
            match byte {
                b'\\' => {
                    self.index += 1;
                    self.index += self.char_len_at(self.index.min(self.bytes.len()));
                }
                b'`' => {
                    self.index += 1;
                    self.slash = SlashMeaning::Divide;
                    self.previous_was_dot = false;
                    return Ok(());
                }
                b'$' if self.bytes.get(self.index + 1) == Some(&b'{') => {
                    self.index += 2;
                    self.depth += 1;
                    self.template_stack.push(self.depth);
                    self.slash = SlashMeaning::Regexp;
                    self.previous_was_dot = false;
                    return Ok(());
                }
                _ => self.index += self.char_len_at(self.index),
            }
        }
        Err("unterminated template literal".to_string())
    }
}

fn is_identifier_start_byte(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' || !byte.is_ascii()
}

fn is_identifier_part_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || !byte.is_ascii()
}

/// JavaScript `WhiteSpace` and `LineTerminator` outside ASCII (12.2, 12.3).
///
/// `char::is_whitespace` is Unicode `White_Space`, which covers U+00A0, U+1680,
/// U+2000..U+200A, U+2028, U+2029, U+202F, U+205F and U+3000. U+FEFF is
/// category `Cf` rather than `Zs`, so it is not in that set and has to be named:
/// it is the byte-order mark, and a file that starts with one starts with it
/// immediately before the first token.
fn is_js_whitespace(character: char) -> bool {
    character.is_whitespace() || character == '\u{FEFF}'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_script_dependency_uses_retained_ast_dynamic_import_sites() {
        assert_eq!(
            classify_outer_script_module_dependency("1 + 2;"),
            OuterScriptModuleDependency::None
        );
        assert_eq!(
            classify_outer_script_module_dependency("import('./ambient.mjs');"),
            OuterScriptModuleDependency::RequiresModuleGraph
        );
        assert_eq!(
            classify_outer_script_module_dependency("const name = './ambient.mjs'; import(name);"),
            OuterScriptModuleDependency::RequiresModuleGraph
        );
    }

    #[test]
    fn outer_script_dependency_does_not_mistake_a_method_named_import_for_a_call() {
        let source = "const object = { import(value) { return value; } }; object.import(1);";
        assert_eq!(
            classify_outer_script_module_dependency(source),
            OuterScriptModuleDependency::None
        );
    }

    #[test]
    fn outer_script_dependency_is_conservative_for_unparseable_possible_imports() {
        assert_eq!(
            classify_outer_script_module_dependency("import("),
            OuterScriptModuleDependency::Indeterminate
        );
        assert_eq!(
            classify_outer_script_module_dependency("const text = \"unterminated"),
            OuterScriptModuleDependency::Indeterminate
        );
        assert_eq!(
            classify_outer_script_module_dependency("let ="),
            OuterScriptModuleDependency::None,
            "a parse failure with no possible import remains a source-closed negative probe"
        );
    }

    fn sources_of(
        sources: &[(&str, &str)],
        entry: usize,
        resolutions: Vec<(ModuleUnitId, ModuleRequestKeyIr, ModuleUnitId)>,
    ) -> ModuleGraphSources {
        ModuleGraphSources {
            realm_requests: Default::default(),
            entry: ModuleUnitId::try_from(entry).expect("entry index fits"),
            modules: sources
                .iter()
                .map(|(key, text)| {
                    ModuleSourceIr::new(
                        ModuleKey::from_host(*key),
                        (*text).to_string(),
                        format!("file:///{key}"),
                    )
                })
                .collect(),
            resolutions,
        }
    }

    /// Builds a graph the way the linker does: link fixes the active component
    /// registry, then namespace collection materializes what those components
    /// observe.
    fn graph_of(sources: &ModuleGraphSources) -> ModuleGraphIr {
        let mut graph = crate::modules::build_graph(sources).expect("graph should build");
        crate::modules::link(&mut graph);
        crate::modules::namespace::collect_observed_namespaces(&mut graph);
        graph
    }

    fn request_key(specifier: &str) -> ModuleRequestKeyIr {
        ModuleRequestKeyIr::plain(specifier)
    }

    fn attributed(specifier: &str, attributes: &[(&str, &str)]) -> ModuleRequestKeyIr {
        ModuleRequestKeyIr::try_new(
            specifier,
            attributes.iter().map(|(key, value)| ImportAttributeIr {
                key: (*key).to_string(),
                value: (*value).to_string(),
            }),
        )
        .expect("the test attribute keys are unique")
    }

    /// The `d.mjs` shape the lane exists for: a dispatcher that answers the
    /// specifier as written with the target's namespace object, and a namespace
    /// object whose getter reads the exporter's own merged-scope binding.
    #[test]
    fn prelude_serves_the_target_specifier_from_the_exporter_binding() {
        let sources = sources_of(
            &[
                ("a", "export const value = 41;"),
                ("d", "import(\"./a.mjs\").then(m => print(m.value));"),
            ],
            1,
            vec![(1, request_key("./a.mjs"), 0)],
        );
        let graph = graph_of(&sources);
        let prelude = graph.dynamic_import_prelude();

        // The dispatcher resolves with the object `modules::namespace` emits,
        // never one of its own — that shared binding is the 16.2.1.10 identity.
        let resolution = format!(
            "if (key === \"./a.mjs\" && attributeKeys.length === 0) {{ resolve({}); return; }}",
            MergedName::minted(0, UnitCellRole::Namespace).as_str()
        );
        assert!(
            prelude.contains(&resolution),
            "dispatcher matches the specifier as written, got: {prelude}"
        );
        assert!(
            prelude.contains("reject(new TypeError("),
            "an unmatched specifier rejects rather than traps, got: {prelude}"
        );
        assert!(
            prelude.contains("var key = `${specifier}`;"),
            "ToString happens inside the executor, got: {prelude}"
        );
        let tostring = prelude.find("var key = `${specifier}`;").unwrap();
        let with_get = prelude.find("var withObject = options.with;").unwrap();
        let enumerate = prelude.find("Reflect.ownKeys(withObject)").unwrap();
        let descriptor = prelude
            .find("Object.getOwnPropertyDescriptor(withObject, attributeKey)")
            .unwrap();
        let value_get = prelude.find("withObject[attributeKey]").unwrap();
        let resolve = prelude.find(&resolution).unwrap();
        assert!(
            tostring < with_get
                && with_get < enumerate
                && enumerate < descriptor
                && descriptor < value_get
                && value_get < resolve,
            "EvaluateImportCall coercion/Get order drifted: {prelude}"
        );
    }

    #[test]
    fn component_identity_includes_sorted_import_attributes() {
        let sources = sources_of(
            &[
                ("plain", "export const value = 'plain';"),
                ("typed", "export const value = 'typed';"),
                (
                    "d",
                    "import('./a.mjs'); import('./a.mjs', { with: { type: 'javascript', mode: 'stable' } });",
                ),
            ],
            2,
            vec![
                (2, request_key("./a.mjs"), 0),
                (
                    2,
                    attributed("./a.mjs", &[("type", "javascript"), ("mode", "stable")]),
                    1,
                ),
            ],
        );
        let graph = graph_of(&sources);
        assert_eq!(graph.dynamic_components().len(), 2);
        assert_ne!(
            graph.dynamic_components()[0].request(),
            graph.dynamic_components()[1].request()
        );

        let prelude = graph.dynamic_import_prelude();
        assert!(
            prelude.contains("key === \"./a.mjs\" && attributeKeys.length === 0"),
            "bare request branch missing: {prelude}"
        );
        assert!(
            prelude.contains(
                "key === \"./a.mjs\" && attributeKeys.length === 2 && attributeKeys[0] === \
                 \"mode\" && attributeValues[0] === \"stable\" && attributeKeys[1] === \
                 \"type\" && attributeValues[1] === \"javascript\""
            ),
            "attributed request branch missing: {prelude}"
        );
    }

    /// One line, always: the merged script displaces every unit's line numbers by
    /// a fixed amount no matter how big the graph is.
    #[test]
    fn the_prelude_is_a_single_line() {
        let sources = sources_of(
            &[
                ("a", "export const value = 41;"),
                ("b", "export const other = 1;"),
                (
                    "d",
                    "import(\"./a.mjs\");\nimport(\"./b.mjs\");\nimport(x);",
                ),
            ],
            2,
            vec![
                (2, request_key("./a.mjs"), 0),
                (2, request_key("./b.mjs"), 1),
            ],
        );
        let graph = graph_of(&sources);
        let prelude = graph.dynamic_import_prelude();
        assert!(!prelude.contains('\n'), "got: {prelude}");
        assert!(!prelude.contains('\r'), "got: {prelude}");
    }

    /// A graph with no `import()` in it contributes nothing, so the linker can
    /// skip the line entirely rather than emit an empty one.
    #[test]
    fn a_graph_without_import_calls_has_an_empty_prelude() {
        let sources = sources_of(&[("a", "export const value = 41;")], 0, Vec::new());
        let graph = graph_of(&sources);
        assert_eq!(graph.dynamic_import_prelude(), "");
    }

    /// A referrer whose only specifier is computed still gets a dispatcher; it
    /// falls through to `reject`, which is a rejected promise and not a trap.
    #[test]
    fn a_computed_specifier_gets_an_always_rejecting_dispatcher() {
        let sources = sources_of(&[("d", "import(x);")], 0, Vec::new());
        let graph = graph_of(&sources);
        let dispatchers = graph.dynamic_import_dispatchers();
        assert!(dispatchers.contains("function $scriptw$module$import$0("));
        assert!(!dispatchers.contains("if (key ==="), "got: {dispatchers}");
        assert!(dispatchers.contains("reject(new TypeError("));
    }

    #[test]
    fn a_call_site_is_rewritten_to_the_referrer_dispatcher() {
        let sources = sources_of(&[("d", "import(\"./a.mjs\").then(f);")], 0, Vec::new());
        let graph = graph_of(&sources);
        assert_eq!(
            graph
                .rewrite_dynamic_import_calls(0, "import(\"./a.mjs\").then(f);")
                .expect("rewrite should succeed"),
            "$scriptw$module$import$0(\"./a.mjs\").then(f);"
        );
    }

    #[test]
    fn a_script_entry_call_site_uses_the_same_canonical_dispatcher() {
        let source = "import(\"./a.mjs\").then(f); import.defer(\"./a.mjs\");";
        let ParsedSource::Script(parsed) =
            scriptw_front::parse(source, scriptw_front::ParseOptions::script())
                .expect("Script entry parses")
        else {
            panic!("Script parse goal");
        };
        let sources = ModuleGraphSources {
            realm_requests: Default::default(),
            modules: vec![ModuleSourceIr::from_parsed_script(
                ModuleKey::from_host("d"),
                "file:///d".into(),
                parsed,
            )],
            entry: 0,
            resolutions: Vec::new(),
        };
        let mut graph = graph_of(&sources);
        graph.entry_is_script = true;
        assert_eq!(
            graph
                .rewrite_dynamic_import_calls(graph.entry, source)
                .expect("rewrite should succeed"),
            "$scriptw$module$import$0(\"./a.mjs\").then(f); $scriptw$module$import$0$defer(\"./a.mjs\");"
        );
    }

    /// The call site is an expression, so it is not confined to nesting depth
    /// zero the way a declaration is.
    #[test]
    fn a_nested_call_site_is_rewritten() {
        let source = "function load() { return import(\"m\"); }\nimport(\"m\");";
        let sources = sources_of(&[("d", source)], 0, Vec::new());
        let graph = graph_of(&sources);
        let rewritten = graph
            .rewrite_dynamic_import_calls(0, source)
            .expect("rewrite should succeed");
        assert_eq!(rewritten.matches("$scriptw$module$import$0(").count(), 2);
        assert!(!rewritten.contains("import("), "got: {rewritten}");
    }

    /// `import.meta` belongs to another owner and must survive untouched, and a
    /// property access named `import` is not the keyword.
    #[test]
    fn import_meta_and_property_access_are_left_alone() {
        // The record is built from the *same* text that is scanned. Handing the
        // graph a different source would let the record's site count and the
        // scanner's disagree for a reason the production path cannot produce,
        // and that disagreement is now the check this file relies on.
        let source = "print(import.meta.url); obj.import(1);";
        let sources = sources_of(&[("d", source)], 0, Vec::new());
        let graph = graph_of(&sources);
        assert_eq!(
            graph
                .rewrite_dynamic_import_calls(0, source)
                .expect("rewrite should succeed"),
            source
        );
    }

    /// The scanner is lexical, so `import(` that is not code stays put.
    #[test]
    fn import_calls_inside_literals_and_comments_are_left_alone() {
        let source = concat!(
            "const s = \"import('m')\";\n",
            "const t = `${ 1 } import('m')`;\n",
            "// import('m')\n",
            "/* import('m') */\n",
            "const r = /import\\('m'\\)/;\n"
        );
        let sources = sources_of(&[("d", source)], 0, Vec::new());
        let graph = graph_of(&sources);
        assert_eq!(
            graph
                .rewrite_dynamic_import_calls(0, source)
                .expect("rewrite should succeed"),
            source
        );
    }

    #[test]
    fn division_slash_does_not_consume_the_following_import_call_as_a_regexp() {
        let source = "const quotient = dividend / divisor;\nimport(\"m\");";
        let sources = sources_of(&[("d", source)], 0, Vec::new());
        let graph = graph_of(&sources);
        assert_eq!(
            graph
                .rewrite_dynamic_import_calls(0, source)
                .expect("rewrite should succeed"),
            "const quotient = dividend / divisor;\n$scriptw$module$import$0(\"m\");"
        );
    }

    /// A byte-order mark, or any other non-ASCII whitespace, immediately before
    /// the keyword must not be scanned as part of it. Before this was fixed the
    /// scanner read `\u{FEFF}import` as one word, found zero call sites, and the
    /// unrewritten `ImportCall` reached the backend stub with no diagnostic.
    #[test]
    fn non_ascii_whitespace_before_the_keyword_does_not_hide_a_call_site() {
        for space in ["\u{FEFF}", "\u{00A0}", "\u{3000}", "\u{2028}"] {
            let source = format!("{space}import(\"m\");");
            let sources = sources_of(&[("d", source.as_str())], 0, Vec::new());
            let graph = graph_of(&sources);
            let rewritten = graph
                .rewrite_dynamic_import_calls(0, &source)
                .unwrap_or_else(|error| panic!("rewrite should succeed for {space:?}: {error}"));
            assert_eq!(
                rewritten,
                format!("{space}$scriptw$module$import$0(\"m\");")
            );
        }
    }

    #[test]
    fn rewriting_requires_the_original_unit_source() {
        let sources = sources_of(&[("d", "import(\"m\");")], 0, Vec::new());
        let graph = graph_of(&sources);
        for other in ["print(1);", "import(\"n\");", "const s = \"import('m')"] {
            let error = graph
                .rewrite_dynamic_import_calls(0, other)
                .expect_err("ranges cannot be applied to a different source");
            assert!(error.contains("original source text"), "{error}");
        }
    }

    /// A `${}` substitution is ordinary source: a call site inside one is real.
    #[test]
    fn a_call_site_inside_a_template_substitution_is_rewritten() {
        let source = "const t = `${ import(\"m\") }`;";
        let sources = sources_of(&[("d", source)], 0, Vec::new());
        let graph = graph_of(&sources);
        assert_eq!(
            graph
                .rewrite_dynamic_import_calls(0, source)
                .expect("rewrite should succeed"),
            "const t = `${ $scriptw$module$import$0(\"m\") }`;"
        );
    }

    /// The replacement is longer than the keyword but adds no line terminator,
    /// so a diagnostic's line number still points at the right line.
    #[test]
    fn rewriting_preserves_line_structure() {
        let source = "print(1);\nimport(\"m\");\nprint(2);\n";
        let sources = sources_of(&[("d", source)], 0, Vec::new());
        let graph = graph_of(&sources);
        let rewritten = graph
            .rewrite_dynamic_import_calls(0, source)
            .expect("rewrite should succeed");
        assert_eq!(rewritten.lines().count(), source.lines().count());
        assert_eq!(
            rewritten.lines().nth(1),
            Some("$scriptw$module$import$0(\"m\");")
        );
    }

    #[test]
    fn a_linkable_graph_reports_nothing() {
        let sources = sources_of(
            &[
                ("a", "export const value = 41;"),
                ("d", "import(\"./a.mjs\");"),
            ],
            1,
            vec![(1, request_key("./a.mjs"), 0)],
        );
        let graph = graph_of(&sources);
        assert_eq!(graph.check_dynamic_import_linkable(), Vec::new());
    }

    /// Deferred requests share their deferred namespace cell. Source Text
    /// Module source jobs reject instead of manufacturing an object.
    #[test]
    fn deferred_phase_resolves_and_source_phase_rejects_without_a_cell() {
        let sources = sources_of(
            &[
                ("a", "export const value = 41;"),
                ("b", "export const other = 1;"),
                (
                    "d",
                    "import.defer(\"./a.mjs\"); import.source(\"./b.mjs\");",
                ),
            ],
            2,
            vec![
                (2, request_key("./a.mjs"), 0),
                (2, request_key("./b.mjs"), 1),
            ],
        );
        let graph = graph_of(&sources);
        assert_eq!(graph.check_dynamic_import_linkable(), Vec::new());

        let prelude = graph.dynamic_import_prelude();
        assert!(
            prelude.contains(&format!(
                "if (key === \"./a.mjs\" && attributeKeys.length === 0) {{ resolve({}); return; }}",
                MergedName::minted(0, UnitCellRole::DeferredNamespace).as_str()
            )),
            "defer resolves with the (deferred) namespace object, got: {prelude}"
        );
        assert!(
            prelude.contains(
                "if (key === \"./b.mjs\" && attributeKeys.length === 0) { throw new SyntaxError("
            ),
            "Source Text Modules have no source cell: {prelude}"
        );
        assert!(
            !prelude.contains(&format!(
                "resolve({});",
                MergedName::minted(0, UnitCellRole::Namespace).as_str()
            )),
            "defer must not resolve the eager namespace identity: {prelude}"
        );
        // One dispatcher per phase: the runtime argument is only a specifier
        // string, so the two cannot share one.
        assert_eq!(
            prelude.matches("function ").count(),
            4,
            "two dispatchers, each with one executor, got: {prelude}"
        );

        assert_eq!(
            graph.evaluation_mode(0),
            ModuleEvaluationModeIr::Deferred,
            "`import.defer()` defers its target"
        );
        assert_eq!(
            graph.evaluation_mode(1),
            ModuleEvaluationModeIr::NotEvaluated,
            "`import.source()` never evaluates its target"
        );
    }

    /// The scanner is what tells the two phased forms apart, and it has to do it
    /// lexically: the phase decides which dispatcher a call site is rewritten
    /// onto, and getting it wrong hands back the wrong object.
    #[test]
    fn the_scanner_reads_a_phase_through_trivia() {
        let sites = ImportCallScanner::new(
            "import(a); import . /*x*/ defer (b); import.source(c); import.meta.url; obj.import(d);",
        )
        .run()
        .expect("the scan should lex");
        assert_eq!(
            sites.iter().map(|site| site.phase).collect::<Vec<_>>(),
            vec![
                ImportPhaseIr::Evaluation,
                ImportPhaseIr::Defer,
                ImportPhaseIr::Source
            ],
            "got {sites:?}"
        );
        // The whole meta-property is replaced, not just the keyword, or the
        // rewritten text would read `$scriptw$module$import$0$defer . defer (b)`.
        assert_eq!(&"import . /*x*/ defer"[..], {
            let site = sites[1];
            &"import(a); import . /*x*/ defer (b); import.source(c); import.meta.url; obj.import(d);"
                [site.start..site.end]
        });
    }

    /// The synthesized names live in the same merged scope as user code, so a
    /// user binding that reaches into the reserved prefix is reported rather
    /// than allowed to shadow a dispatcher.
    #[test]
    fn a_user_binding_in_the_reserved_prefix_is_reported() {
        let source = format!("const {LINKER_NAME_PREFIX}namespace$0 = 1;\nimport(x);");
        let sources = sources_of(&[("d", source.as_str())], 0, Vec::new());
        let diagnostics = crate::modules::build_graph(&sources)
            .expect_err("reserved identifiers cannot enter a source record");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("linker-synthesized name")),
            "got {diagnostics:?}"
        );
    }

    /// The minted-cell half of the same hazard, and the positive control for
    /// the `spec_name` narrowing beside it: source text *can* spell `$m0$…` and
    /// `$d0$`, and a top-level declaration of one lands in the merged cell the
    /// prelude declares, so it is reported.
    ///
    /// Paired deliberately with
    /// [`an_anonymous_default_export_is_exposed_under_its_minted_name`], which
    /// is the same predicate's negative control: that module's `*default*`
    /// binding is spelled `$d0$` in the merged scope too, and must *not* be
    /// reported. A predicate asked of the merged spelling passes this test and
    /// fails that one, which is how the two together pin the right question.
    #[test]
    fn a_user_binding_shaped_like_a_minted_cell_is_reported() {
        for spelling in [
            MergedName::minted(0, UnitCellRole::Namespace)
                .as_str()
                .to_string(),
            LocalName::AnonymousDefault
                .merged_in(0)
                .as_str()
                .to_string(),
        ] {
            let source = format!("const {spelling} = 1;\nimport(x);");
            let sources = sources_of(&[("d", source.as_str())], 0, Vec::new());
            let graph = graph_of(&sources);
            let diagnostics = graph.check_dynamic_import_linkable();
            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic
                    .message
                    .contains("linker-minted per-unit cell name")),
                "{spelling}: got {diagnostics:?}"
            );
        }
    }

    /// An anonymous `export default` binds `*default*`, which no source text
    /// can spell — but the merged script declares it under a minted name, so
    /// `import()` exposes it like any other export.
    #[test]
    fn an_anonymous_default_export_is_exposed_under_its_minted_name() {
        let sources = sources_of(
            &[
                ("a", "export default function () { return 1; }"),
                ("d", "import(\"./a.mjs\");"),
            ],
            1,
            vec![(1, request_key("./a.mjs"), 0)],
        );
        let graph = graph_of(&sources);
        assert_eq!(graph.check_dynamic_import_linkable(), Vec::new());
        let namespace = graph.units[0]
            .namespaces
            .get(&ModuleNamespaceModeIr::Eager)
            .expect("the import() target has a namespace");
        let source = namespace.source.as_ref().expect("namespace is expressible");
        assert!(
            source.contains(&format!(
                ", () => {}",
                LocalName::AnonymousDefault.merged_in(0).as_str()
            )),
            "got {source}"
        );
    }

    /// Two specifiers naming one module resolve with one binding, so `import()`
    /// twice observes the same object (16.2.1.10).
    #[test]
    fn two_specifiers_for_one_module_share_one_namespace_binding() {
        let sources = sources_of(
            &[
                ("a", "export const value = 41;"),
                ("d", "import(\"./a.mjs\");\nimport(\"a\");"),
            ],
            1,
            vec![(1, request_key("./a.mjs"), 0), (1, request_key("a"), 0)],
        );
        let graph = graph_of(&sources);
        let prelude = graph.dynamic_import_prelude();
        assert_eq!(
            prelude
                .matches(&format!(
                    "resolve({})",
                    MergedName::minted(0, UnitCellRole::Namespace).as_str()
                ))
                .count(),
            2,
            "got: {prelude}"
        );
    }

    /// The whole point of routing `resolve` through
    /// one namespace cell: `import()` and `import * as ns` of one
    /// module name the same binding, so the objects are `===`. This lane must
    /// never mint a namespace binding of its own.
    #[test]
    fn a_dispatcher_resolves_with_the_static_namespace_binding() {
        let sources = sources_of(
            &[
                ("a", "export const value = 41;"),
                (
                    "d",
                    "import * as ns from \"./a.mjs\";\nimport(\"./a.mjs\");",
                ),
            ],
            1,
            vec![(1, request_key("./a.mjs"), 0)],
        );
        let graph = graph_of(&sources);
        let prelude = graph.dynamic_import_prelude();
        assert!(
            prelude.contains(&format!(
                "resolve({})",
                MergedName::minted(0, UnitCellRole::Namespace).as_str()
            )),
            "got: {prelude}"
        );
        assert!(
            !prelude.contains("$scriptw$module$namespace$"),
            "this lane must mint no namespace binding of its own: {prelude}"
        );
    }

    #[test]
    fn parser_heads_preserve_method_names_regex_and_nested_imports() {
        let source = "if (true) {} /export/.test('export'); if (true) /}/.test('}'); const o = { import() { return 1; } }; import(import(\"a\"));";
        let sources = sources_of(&[("d", source)], 0, Vec::new());
        let graph = graph_of(&sources);
        let rewritten = graph.rewrite_dynamic_import_calls(0, source).unwrap();
        assert_eq!(
            rewritten,
            source.replace(
                "import(import(\"a\"))",
                "$scriptw$module$import$0($scriptw$module$import$0(\"a\"))"
            )
        );
    }

    #[test]
    fn phase_head_trivia_preserves_exact_line_terminator_sequences() {
        for trivia in ["/*\r x \n*/", "/*\r\n*/", "/*\u{2028}\u{2029}*/"] {
            let source = format!("// 🦀\nimport {trivia} . defer ('a');");
            let sources = sources_of(&[("d", &source)], 0, Vec::new());
            let graph = graph_of(&sources);
            let rewritten = graph.rewrite_dynamic_import_calls(0, &source).unwrap();
            assert_eq!(
                super::super::source::collect_ecmascript_line_terminator_sequences(&rewritten),
                super::super::source::collect_ecmascript_line_terminator_sequences(&source)
            );
            scriptw_front::parse(&rewritten, scriptw_front::ParseOptions::script()).unwrap();
            assert!(
                rewritten.contains("$scriptw$module$import$0$defer"),
                "{rewritten}"
            );
        }
    }

    #[test]
    fn a_string_literal_escapes_quotes_and_line_separators() {
        assert_eq!(js_string_literal("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(js_string_literal("a\u{2028}b"), "\"a\\u2028b\"");
    }
}

pub(super) fn module_async_dependencies_name(module: ModuleUnitId) -> String {
    format!("$scriptw$module$asyncDependencies${module}")
}

pub(super) fn module_deferred_import_name(module: ModuleUnitId) -> String {
    format!("$scriptw$module$deferredImport${module}")
}
