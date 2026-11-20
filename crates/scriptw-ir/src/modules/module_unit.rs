use std::collections::BTreeMap;

use crate::{
    BlockIr, FunctionIr, LocalName, MergedName, ModuleNamespaceModeIr, OwnedEnvBindingIr,
    UnitLinkerSpelling,
};

use super::namespace::ModuleNamespaceIr;
use super::record::ModuleRecordIr;
use super::resolved_binding::ResolvedBindingIr;

/// One module of the graph: its record, its source, and the lowered artifacts
/// the link stage produces from them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleUnitIr {
    pub(super) foreign: Option<crate::foreign::ForeignModuleSource>,
    /// Static entry tables for this module.
    pub record: ModuleRecordIr,
    /// Spelling of the anonymous-default and `import.meta` cells inside this
    /// unit's activation, chosen away from the unit's own identifiers.
    pub(super) linker_spelling: UnitLinkerSpelling,
    /// The module source text, kept so the lowerer can slice spans from it.
    pub source_text: String,
    /// Value `import.meta.url` reports.
    pub meta_url: String,
    /// `InitializeEnvironment` for this unit. Filled by the link stage.
    pub hoist: Option<BlockIr>,
    /// `ExecuteModule` for this unit. Filled by the link stage.
    pub body: Option<BlockIr>,
    /// Functions lowered from this unit, with module-prefixed ids.
    pub functions: Vec<FunctionIr>,
    /// This unit's own top-level environment bindings.
    pub owned_env_bindings: Vec<OwnedEnvBindingIr>,
    /// One namespace object per observed mode: an entry exists when an
    /// importer or `import()` observes this module's eager or deferred
    /// namespace, and the two modes are distinct objects.
    pub namespaces: BTreeMap<ModuleNamespaceModeIr, ModuleNamespaceIr>,
    /// One entry per `record.import_entries[i]`, same index.
    pub resolved_imports: Vec<ResolvedBindingIr>,
    /// One entry per `record.indirect_export_entries[i]`, same index.
    ///
    /// `ResolveExport` runs on indirect exports at link time (16.2.1.6.4 step
    /// 2), so `export { x } from "m"` where `m` has no `x` fails before
    /// anything evaluates. Keeping the results makes the re-export's target
    /// cell addressable without resolving a second time.
    pub resolved_indirect_exports: Vec<ResolvedBindingIr>,
}

impl ModuleUnitIr {
    /// The merged spelling of one of *this* unit's `[[LocalName]]`s.
    ///
    /// [`LocalName::merged_in`] takes the owner's spelling, so "which unit does
    /// this name belong to" — the coordinate commit `e27c01b1e` got wrong — is
    /// fixed here by holding the unit: an importer's spelling cannot be
    /// supplied for an exporter's name.
    #[must_use]
    pub fn merged(&self, name: &LocalName) -> MergedName {
        name.merged_in(&self.linker_spelling)
    }

    pub(super) fn payload_bytes(&self) -> &[u8] {
        if let Some(foreign) = &self.foreign {
            return &foreign.canonical_bytes;
        }
        if let Some(tag) = self.record.module_source_tag() {
            return tag.as_bytes();
        }
        self.record
            .bytes_source()
            .unwrap_or_else(|| self.source_text.as_bytes())
    }
}
