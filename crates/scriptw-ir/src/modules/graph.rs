//! The module graph: loaded sources in, linked records out.
//!
//! Owns the linked graph record and linking orchestration.
//! `graph_evaluation_classification` owns evaluation-mode classification and
//! unsupported phase policy; `graph_evaluation_order` owns the
//! `InnerModuleEvaluation` DFS (16.2.1.5.3); and `graph_async_evaluation` owns
//! async-module propagation and pending-dependency queries.
//!
//! Loading is *not* here. `scriptw-ir` performs no IO: the host resolves and
//! reads every source, then hands the closure over as [`ModuleGraphSources`].

use crate::*;

use super::evaluation_mode::ModuleEvaluationModeIr;
use super::graph_evaluation_classification::classify_evaluation_modes;
use super::graph_evaluation_order::compute_evaluation_order;
use super::record::ModuleEvaluationDependencyIr;

#[cfg(test)]
use super::loaded_sources::{ModuleGraphSources, ModuleParse, ModuleSourceIr};

/// A linked module graph.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleGraphIr {
    pub realm_requests: BTreeMap<ModuleRequestKeyIr, super::RealmModuleResolutionIr>,
    /// Index of the entry module.
    pub entry: ModuleUnitId,
    /// Every module. Index is the [`ModuleUnitId`].
    pub units: Vec<ModuleUnitIr>,
    /// Host-normalized locator and module kind to unit index. A Script entry is outside
    /// the module map even when a Module has the same canonical URL.
    pub keys: BTreeMap<ModuleRecordKey, ModuleUnitId>,
    /// `(referrer, phase-free request key) -> target`, as resolved by the host.
    pub resolutions: BTreeMap<(ModuleUnitId, ModuleRequestKeyIr), ModuleUnitId>,
    /// Post-order DFS. Members of one strongly-connected component are
    /// contiguous.
    pub evaluation_order: Vec<ModuleUnitId>,
    /// Index into `evaluation_order` at which each component starts.
    pub scc_starts: Vec<usize>,
    /// Dynamic-import components compiled into this artifact.
    ///
    /// Discovery initially sees every call site in the loaded closure because
    /// those edges participate in evaluation-mode classification. After that
    /// fixed point, components whose referrer does not materialize are removed,
    /// so every row here names a call site that can run in this artifact.
    components: Vec<DynamicComponentIr>,
    pub(super) dynamic_rejections: Vec<super::admission::RejectedDynamicModule>,
    /// When each unit's body runs, indexed by [`ModuleUnitId`].
    ///
    /// Filled by [`link`]; empty on a graph that has not been linked, which
    /// [`Self::evaluation_mode`] reads as the default [`Eager`].
    ///
    /// [`Eager`]: ModuleEvaluationModeIr::Eager
    pub evaluation_modes: Vec<ModuleEvaluationModeIr>,
    /// Every linking failure found. Non-empty means the graph does not run.
    pub link_errors: Vec<ModuleLinkErrorIr>,
    /// The entry unit is Script text, not module code.
    ///
    /// A Script has no imports and no exports, so it is not a module of the
    /// graph in any spec sense — but `import()` is legal in Script goal
    /// (13.3.10 takes `GetActiveScriptOrModule`, which a Script satisfies), and
    /// serving it needs exactly the same compiled targets a module's `import()`
    /// needs. So the loader assembles the closure of the Script's `import()`
    /// specifiers into an ordinary graph and marks the entry with this flag,
    /// which changes three things in `modules::link`:
    ///
    /// * the entry's text is emitted verbatim — no `"use strict"` prologue is
    ///   forced on it, no module syntax is stripped from it, and its top-level
    ///   `this` stays `globalThis`, because all three are Script semantics and
    ///   the entry really is a Script;
    /// * every other unit owns a private strict module activation and
    ///   environment, with undefined lexical `this` and isolated bindings;
    /// * entry import jobs reach those canonical activations through trusted
    ///   dispatcher operations; graph instantiation never evaluates a target.
    pub entry_is_script: bool,
}

impl ModuleGraphIr {
    /// Dynamic-import occurrences retained in this artifact after linking.
    #[must_use]
    pub fn dynamic_components(&self) -> &[DynamicComponentIr] {
        &self.components
    }

    /// Borrows a unit by id.
    ///
    /// # Panics
    /// Panics if `id` is not a unit of this graph.
    #[must_use]
    pub fn unit(&self, id: ModuleUnitId) -> &ModuleUnitIr {
        &self.units[id as usize]
    }

    /// `true` when the entry module or anything it reaches uses top-level
    /// `await`, which forces the asynchronous evaluation driver.
    #[must_use]
    pub fn has_top_level_await(&self) -> bool {
        self.units
            .iter()
            .any(|unit| unit.record.has_top_level_await)
    }

    /// `[[HasTLA]]` (16.2.1.6.1 step 12) for one unit.
    ///
    /// `false` for an id this graph does not hold, which is the same answer a
    /// module with no `await` gives and never turns a graph asynchronous.
    #[must_use]
    pub fn has_tla(&self, module: ModuleUnitId) -> bool {
        self.units
            .get(module as usize)
            .is_some_and(|unit| unit.record.has_top_level_await)
    }

    /// Index of the strongly-connected component each unit belongs to, indexed
    /// by [`ModuleUnitId`], ordered as [`evaluation_components`] orders them.
    ///
    /// A unit that no root reached — which cannot happen for a graph built by
    /// [`build_graph`], every unit of which is a Tarjan root at worst — is left
    /// in a component of its own past the end, so the propagation below still
    /// terminates on a hand-built graph.
    ///
    /// [`evaluation_components`]: crate::modules::evaluation_components
    #[must_use]
    pub fn component_of_unit(&self) -> Vec<usize> {
        let mut components = vec![usize::MAX; self.units.len()];
        let mut next = 0;
        for (position, start) in self.scc_starts.iter().copied().enumerate() {
            let end = self
                .scc_starts
                .get(position + 1)
                .copied()
                .unwrap_or(self.evaluation_order.len());
            if start >= end {
                continue;
            }
            for member in &self.evaluation_order[start..end] {
                if let Some(slot) = components.get_mut(*member as usize) {
                    *slot = next;
                }
            }
            next += 1;
        }
        for slot in &mut components {
            if *slot == usize::MAX {
                *slot = next;
                next += 1;
            }
        }
        components
    }

    /// When `module`'s body runs. [`Eager`] for an id this graph does not hold
    /// and for a graph that has not been linked.
    ///
    /// [`Eager`]: ModuleEvaluationModeIr::Eager
    #[must_use]
    pub fn evaluation_mode(&self, module: ModuleUnitId) -> ModuleEvaluationModeIr {
        self.evaluation_modes
            .get(module as usize)
            .copied()
            .unwrap_or_default()
    }

    /// Resolved evaluation dependencies of one unit, in
    /// `[[RequestedModules]]` order.
    ///
    /// Unresolved requests are dropped rather than reported: `link` has already
    /// recorded an [`ModuleLinkErrorIr::UnresolvedModule`] for each, and
    /// evaluation order is still wanted for the units that *did* resolve.
    /// Defer- and source-phase requests are resolved and linked, but they are
    /// not dependencies of `InnerModuleEvaluation`.
    pub(super) fn evaluation_dependencies_of(
        &self,
        module: ModuleUnitId,
    ) -> Vec<ModuleEvaluationDependencyIr> {
        let Some(unit) = self.units.get(module as usize) else {
            return Vec::new();
        };
        unit.record
            .requested_modules
            .iter()
            .filter_map(|request| {
                let target = self.resolve_request(module, request)?;
                ModuleEvaluationDependencyIr::from_resolved_request(request, target)
            })
            .collect()
    }
}

/// Resolves every import entry, records link errors, and computes the
/// evaluation order and its strongly-connected components.
#[cfg(test)]
pub(crate) fn link(graph: &mut ModuleGraphIr) {
    link_with_admission(graph, super::admission::GraphAdmission::LoadedClosure);
}

pub(super) fn link_with_admission(
    graph: &mut ModuleGraphIr,
    admission: super::admission::GraphAdmission,
) {
    let unit_count = graph.units.len();
    // Loading a source-phase target parses that one record without linking
    // its dependencies or creating an environment. Fix reachability before
    // resolving imports so inactive source-only records cannot poison linkage.
    let components = super::dynamic::discover_components(graph, admission);
    classify_evaluation_modes(graph, &components);

    for module in 0..unit_count {
        let id = ModuleUnitId::try_from(module).expect("unit index is capped by build_graph, which rejects a graph with more units than MAX_LINKABLE_MODULE_UNIT_ID");
        if graph.materialization_mode(id).is_none() {
            continue;
        }

        // 16.2.3.1: duplicate export names are an early error.
        for export_name in graph.units[module].record.duplicate_export_names() {
            graph.link_errors.push(ModuleLinkErrorIr::DuplicateExport {
                module: id,
                export_name,
            });
        }

        // Requests the host could not resolve.
        let requests = graph.units[module].record.requested_modules.clone();
        let mut checked = BTreeSet::new();
        for request in requests {
            let request = request.key().clone();
            if !checked.insert(request.clone()) {
                continue;
            }
            let inconsistent = graph.link_errors.iter().any(|error| {
                matches!(
                    error,
                    ModuleLinkErrorIr::InconsistentResolution {
                        referrer,
                        request: inconsistent_request,
                    } if *referrer == id && inconsistent_request == &request
                )
            });
            if graph.resolve_request_key(id, &request).is_none() && !inconsistent {
                graph.link_errors.push(ModuleLinkErrorIr::UnresolvedModule {
                    referrer: id,
                    request,
                });
            }
        }

        // ResolveExport for every import entry.
        let entries = graph.units[module].record.import_entries.clone();
        let mut resolved = Vec::with_capacity(entries.len());
        for entry in &entries {
            let Some(target) = graph.resolve_request(id, &entry.request) else {
                resolved.push(ResolvedBindingIr::NotFound);
                continue;
            };
            // Source resolution names the record rather than an export. The
            // loader supplies only Source Text Module Records, whose source
            // representation is unavailable; InitializeEnvironment rejects it.
            let binding = if entry.request.phase() == ImportPhaseIr::Source {
                ResolvedBindingIr::Resolved {
                    module: target,
                    binding: ModuleBindingNameIr::ModuleSource,
                }
            } else {
                match &entry.import_name {
                    ImportNameIr::Namespace => ResolvedBindingIr::Resolved {
                        module: target,
                        binding: ModuleBindingNameIr::Namespace(
                            entry
                                .request
                                .phase()
                                .namespace_mode()
                                .expect("source imports resolve separately"),
                        ),
                    },
                    ImportNameIr::Name(name) => graph.resolve_export(target, name),
                }
            };
            match &binding {
                ResolvedBindingIr::Resolved {
                    binding: ModuleBindingNameIr::ModuleSource,
                    module: source,
                } if graph.unit(*source).record.module_source_tag().is_none() => graph
                    .link_errors
                    .push(ModuleLinkErrorIr::SourceUnavailable {
                        referrer: id,
                        request: entry.request.clone(),
                    }),
                ResolvedBindingIr::Resolved { .. } => {}
                ResolvedBindingIr::Ambiguous => {
                    graph.link_errors.push(ModuleLinkErrorIr::AmbiguousExport {
                        module: target,
                        export_name: import_name_text(&entry.import_name),
                    });
                }
                ResolvedBindingIr::NotFound => {
                    graph.link_errors.push(ModuleLinkErrorIr::MissingExport {
                        referrer: id,
                        request: entry.request.clone(),
                        import_name: import_name_text(&entry.import_name),
                    });
                }
            }
            resolved.push(binding);
        }
        graph.units[module].resolved_imports = resolved;

        // 16.2.1.6.4 `InitializeEnvironment` step 2: every indirect export
        // resolves at link time, so `export { x } from "m"` where `m` has no
        // `x` is a SyntaxError before anything evaluates. Nothing imports the
        // re-export for this to be caught (`instn-iee-err-*`).
        let indirect = graph.units[module].record.indirect_export_entries.clone();
        let mut resolved_indirect = Vec::with_capacity(indirect.len());
        for entry in &indirect {
            if graph.resolve_request(id, &entry.request).is_none() {
                // Already reported as `UnresolvedModule` above; a missing
                // export on top of it would say the same thing twice.
                resolved_indirect.push(ResolvedBindingIr::NotFound);
                continue;
            }
            // Resolved through *this* module, as the spec states it: a name
            // that is both re-exported and reachable through `export *` is
            // ambiguous, and only the local view sees that.
            let resolution = graph.resolve_export(id, &entry.export_name);
            match &resolution {
                ResolvedBindingIr::Resolved {
                    binding: ModuleBindingNameIr::ModuleSource,
                    module: source,
                } if graph.unit(*source).record.module_source_tag().is_none() => graph
                    .link_errors
                    .push(ModuleLinkErrorIr::SourceUnavailable {
                        referrer: id,
                        request: entry.request.clone(),
                    }),
                ResolvedBindingIr::Resolved { .. } => {}
                ResolvedBindingIr::Ambiguous => {
                    graph.link_errors.push(ModuleLinkErrorIr::AmbiguousExport {
                        module: id,
                        export_name: entry.export_name.clone(),
                    });
                }
                ResolvedBindingIr::NotFound => {
                    graph.link_errors.push(ModuleLinkErrorIr::MissingExport {
                        referrer: id,
                        request: entry.request.clone(),
                        import_name: import_name_text(&entry.import_name),
                    });
                }
            }
            resolved_indirect.push(resolution);
        }
        graph.units[module].resolved_indirect_exports = resolved_indirect;
    }

    compute_evaluation_order(graph);
    // Discovery must precede classification: a dynamic request carries a phase
    // too, so it can make its target eager, deferred or source-only. Runtime
    // components are fixed only after that reachability fixed point; a call
    // site in a source-only referrer is link metadata, not artifact code.
    let components: Vec<_> = components
        .into_iter()
        .filter(|component| graph.materialization_mode(component.referrer()).is_some())
        .collect();
    graph.components = components;
}

/// `[[ImportName]]` as it reads in a diagnostic.
///
/// The `Namespace` arm has no `[[ExportName]]` behind it — `import * as ns`
/// resolves to a namespace object, not to an export — so `"*"` here is a
/// diagnostic spelling standing in for one.
///
/// It is **not** a name no module could have exported under: 16.2.3.1 makes
/// `ModuleExportName : StringLiteral` legal, so `export { x as "*" } from "m"`
/// spells exactly this, and invariant E1 relies on that openness. A
/// `MissingExport`/`AmbiguousExport` diagnostic naming `*` is therefore
/// ambiguous between a namespace import and a literal `"*"` export. Diagnostic
/// text only — no miscompilation — but the claim that it could not happen was
/// false and is not repeated here.
fn import_name_text(import_name: &ImportNameIr) -> ExportName {
    match import_name {
        ImportNameIr::Namespace => ExportName::new("*"),
        ImportNameIr::Name(name) => name.clone(),
    }
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "graph_request_identity_tests.rs"]
mod request_identity_tests;
