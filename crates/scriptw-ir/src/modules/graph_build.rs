use std::collections::{BTreeMap, BTreeSet};

use crate::{IrDiagnostic, UnitLinkerSpelling, MAX_LINKABLE_MODULE_UNIT_ID};

use super::graph::ModuleGraphIr;
use super::link_error::ModuleLinkErrorIr;
use super::loaded_sources::{ModuleGraphSources, ModuleParse};
use super::module_key::ModuleKey;
use super::module_unit::ModuleUnitIr;
use super::record::{parse_module_record, ModuleUnitId};
use super::resolved_binding::ResolvedBindingIr;

/// Builds the graph from an already-loaded closure: turns every retained parse
/// product into a module record and records the host's resolutions.
///
/// A key the graph already holds is reused and never rebuilt. That is module
/// map identity (16.2.1.7 `HostLoadImportedModule`): however many importers
/// name a module, and however many times the host hands its source over, the
/// graph holds exactly one unit for it — which is what makes a module evaluate
/// once and hand out one namespace object.
pub(crate) fn build_graph(
    sources: &ModuleGraphSources,
) -> Result<ModuleGraphIr, Vec<IrDiagnostic>> {
    assemble_sources(sources, super::admission::GraphAdmission::LoadedClosure)
}

pub(super) fn build_complete_catalog(
    sources: &ModuleGraphSources,
) -> Result<ModuleGraphIr, Vec<IrDiagnostic>> {
    assemble_sources(sources, super::admission::GraphAdmission::CompleteCatalog)
}

fn assemble_sources(
    sources: &ModuleGraphSources,
    admission: super::admission::GraphAdmission,
) -> Result<ModuleGraphIr, Vec<IrDiagnostic>> {
    let mut graph = ModuleGraphIr::default();
    let mut diagnostics = Vec::new();
    // Source position -> unit id. The two differ as soon as one key arrives
    // twice, and `resolutions` is stated in source positions.
    let mut remap: Vec<ModuleUnitId> = Vec::with_capacity(sources.modules.len());
    let mut inconsistent: Vec<ModuleKey> = Vec::new();

    for (source_index, source) in sources.modules.iter().enumerate() {
        let is_script = matches!(&source.parse, ModuleParse::ScriptEntry(_));
        if is_script && source_index != sources.entry as usize {
            diagnostics.push(IrDiagnostic::unsupported(
                "unsupported in scriptw wasm-aot: a Script source is only valid as the graph entry",
            ));
            return Err(diagnostics);
        }
        // Only Module Records belong to the host module map. A Script entry
        // may share a canonical URL with an imported Module without sharing
        // either its parse goal or its environment.
        if !is_script {
            if let Some(&existing) = graph.keys.get(&source.record_key()) {
                // Same module key, different bytes: the host contradicted
                // itself, and there is no honest way to pick a winner.
                if (graph.units[existing as usize].payload_bytes() != source.payload_bytes()
                    || graph.units[existing as usize].meta_url != source.meta_url())
                    && !inconsistent.iter().any(|key| key == source.key())
                {
                    inconsistent.push(source.key().clone());
                }
                remap.push(existing);
                continue;
            }
        }
        // The one place a unit id is minted, and therefore the one place the
        // byte budgets B1/B2 can be enforced at run time (contract ledger R3).
        // The previous `unwrap_or(ModuleUnitId::MAX)` was worse than unchecked:
        // it saturated to a ten-digit id, which violates both budgets silently
        // and then fails downstream as a confusing `StripError`.
        let Some(id) = ModuleUnitId::try_from(graph.units.len())
            .ok()
            .filter(|id| *id <= MAX_LINKABLE_MODULE_UNIT_ID)
        else {
            diagnostics.push(
                ModuleLinkErrorIr::TooManyUnits {
                    count: sources.modules.len(),
                }
                .to_diagnostic(),
            );
            return Err(diagnostics);
        };
        remap.push(id);
        let record = match &source.parse {
            ModuleParse::ModuleSource { source_tag } => Ok(
                super::record::ModuleRecordIr::module_source(source_tag, id, source.key().clone()),
            ),
            ModuleParse::Json(parsed) => Ok(super::record::ModuleRecordIr::json(
                parsed,
                id,
                source.key().clone(),
            )),
            ModuleParse::Bytes(bytes) => Ok(super::record::ModuleRecordIr::bytes(
                bytes,
                id,
                source.key().clone(),
            )),
            ModuleParse::Text(text) => Ok(super::record::ModuleRecordIr::text(
                text,
                id,
                source.key().clone(),
            )),
            ModuleParse::JsonRejected { error, .. } => {
                Err(vec![super::early::json_parse_failure_diagnostic(error)])
            }
            ModuleParse::Module(unit_source) => match admission {
                super::admission::GraphAdmission::LoadedClosure => {
                    parse_module_record(unit_source, id, source.key().clone())
                }
                super::admission::GraphAdmission::CompleteCatalog => {
                    super::record::parse_module_record_for_catalog(
                        unit_source,
                        id,
                        source.key().clone(),
                    )
                }
            },
            ModuleParse::ScriptEntry(unit_source) => match admission {
                super::admission::GraphAdmission::LoadedClosure => {
                    super::record::script_entry_record(unit_source, id, source.key().clone())
                }
                super::admission::GraphAdmission::CompleteCatalog => {
                    super::record::script_entry_record_for_catalog(
                        unit_source,
                        id,
                        source.key().clone(),
                    )
                }
            },
            ModuleParse::Rejected { error, .. } => {
                Err(vec![super::early::module_parse_failure_diagnostic(error)])
            }
        };
        let record = record.and_then(|mut record| {
            record
                .dynamic_import_sites
                .extend(source.prepared_script_import_sites.iter().cloned());
            // A first spelling against the unit's own identifiers, so the unit
            // can be constructed and an exhausted unit is reported in its own
            // module; it is re-chosen against the whole graph below.
            match UnitLinkerSpelling::choose(id, record.linker_reserved_names()) {
                Ok(linker_spelling) => Ok((record, linker_spelling)),
                Err(exhausted) => Err(vec![exhausted.to_diagnostic(record.key.as_str())]),
            }
        });
        match record {
            Ok((record, linker_spelling)) => {
                let resolved_imports =
                    vec![ResolvedBindingIr::NotFound; record.import_entries.len()];
                let resolved_indirect_exports =
                    vec![ResolvedBindingIr::NotFound; record.indirect_export_entries.len()];
                if !is_script {
                    graph.keys.insert(source.record_key(), id);
                }
                graph.units.push(ModuleUnitIr {
                    foreign: source.foreign.clone(),
                    record,
                    linker_spelling,
                    source_text: source.source_text().to_string(),
                    meta_url: source.meta_url().to_string(),
                    hoist: None,
                    body: None,
                    functions: Vec::new(),
                    owned_env_bindings: Vec::new(),
                    namespaces: BTreeMap::new(),
                    resolved_imports,
                    resolved_indirect_exports,
                });
            }
            Err(parse_diagnostics) => {
                // A rejected non-entry module fails while the graph loads/links.
                let in_entry = source_index == sources.entry as usize;
                diagnostics.extend(parse_diagnostics.into_iter().map(|diagnostic| {
                    if in_entry {
                        diagnostic
                    } else {
                        diagnostic.in_dependency_module()
                    }
                }));
            }
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    // Lowering recognizes the activation cells by name in every unit (they
    // never take a runtime named lookup, and no eval table names them), so no
    // identifier of *any* unit may spell any unit's cell. Cells are still
    // addressed by `(unit, name)`; the decimal tag stays first.
    let graph_reserved: BTreeSet<String> = graph
        .units
        .iter()
        .flat_map(|unit| unit.record.linker_reserved_names().iter().cloned())
        .collect();
    for unit in &mut graph.units {
        match UnitLinkerSpelling::choose(unit.record.id, &graph_reserved) {
            Ok(spelling) => unit.linker_spelling = spelling,
            Err(exhausted) => diagnostics.push(exhausted.to_diagnostic(unit.record.key.as_str())),
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    for (request, resolution) in &sources.realm_requests {
        let resolution = match resolution {
            super::RealmModuleResolutionIr::Loaded(source) => {
                let Some(&target) = remap.get(*source as usize) else {
                    return Err(vec![IrDiagnostic::lowering(
                        "Realm module request target is outside the declared catalog",
                    )]);
                };
                if !graph.units[target as usize]
                    .record
                    .kind()
                    .matches_request(request)
                {
                    return Err(vec![IrDiagnostic::lowering(
                        "Realm module request must select a matching Module Record",
                    )]);
                }
                super::RealmModuleResolutionIr::Loaded(target)
            }
            super::RealmModuleResolutionIr::Rejected(message) => {
                super::RealmModuleResolutionIr::Rejected(message.clone())
            }
        };
        graph.realm_requests.insert(request.clone(), resolution);
    }

    graph.entry = remap
        .get(sources.entry as usize)
        .copied()
        .unwrap_or_default();
    let mut inconsistent_resolutions = BTreeSet::new();
    for (referrer, request, target) in &sources.resolutions {
        if sources
            .modules
            .get(*target as usize)
            .is_some_and(|source| matches!(&source.parse, ModuleParse::ScriptEntry(_)))
        {
            diagnostics.push(IrDiagnostic::unsupported(
                "unsupported in scriptw wasm-aot: host module resolution selected a Script Record",
            ));
            continue;
        }
        let (Some(&referrer), Some(&target)) =
            (remap.get(*referrer as usize), remap.get(*target as usize))
        else {
            continue;
        };
        if admission == super::admission::GraphAdmission::CompleteCatalog
            && !graph.units[referrer as usize]
                .record
                .catalog_key_is_eligible(request)
        {
            continue;
        }
        let identity = (referrer, request.clone());
        // Public resolution rows cannot turn a Source Text record into JSON,
        // or bypass JSON's mandatory type attribute on a cached host key.
        if !graph.units[target as usize]
            .record
            .kind()
            .matches_request(request)
        {
            continue;
        }
        if inconsistent_resolutions.contains(&identity) {
            continue;
        }
        match graph.resolutions.get(&identity).copied() {
            None => {
                graph.resolutions.insert(identity, target);
            }
            Some(existing) if existing == target => {}
            Some(_) => {
                // A phase-free request has exactly one host resolution. Keep
                // no winner: last-write-wins would make public row order part
                // of module identity.
                graph.resolutions.remove(&identity);
                inconsistent_resolutions.insert(identity);
            }
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    for key in inconsistent {
        graph
            .link_errors
            .push(ModuleLinkErrorIr::InconsistentLoad { key });
    }
    for (referrer, request) in inconsistent_resolutions {
        graph
            .link_errors
            .push(ModuleLinkErrorIr::InconsistentResolution { referrer, request });
    }
    Ok(graph)
}
