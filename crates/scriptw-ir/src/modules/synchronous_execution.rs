//! Validated execution records for compiled modules reached from a Module or Script.

use crate::{FunctionId, ModuleNamespaceModeIr, ModuleUnitId};

/// Source-goal authority for the root that owns a compiled module graph.
/// A Module entry evaluates its activation; a Script runs in its own root
/// environment and only its import jobs evaluate module activations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModuleExecutionEntry {
    Module(ModuleUnitId),
    Script(ModuleUnitId),
}

/// An ultimate module environment cell, resolved before Wasm emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleCellIr {
    ModuleSource {
        module: ModuleUnitId,
    },
    Binding {
        module: ModuleUnitId,
        slot: u32,
    },
    Namespace {
        module: ModuleUnitId,
        mode: ModuleNamespaceModeIr,
    },
}

/// An actual host-owned source record; its identity is the graph's unit id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSourceRecordIr {
    module: ModuleUnitId,
    source_tag: String,
}
impl ModuleSourceRecordIr {
    pub(super) fn new(module: ModuleUnitId, source_tag: String) -> Self {
        Self { module, source_tag }
    }
    pub const fn module(&self) -> ModuleUnitId {
        self.module
    }
    pub fn source_tag(&self) -> &str {
        &self.source_tag
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleActivationKindIr {
    Synchronous,
    Async,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleRequestPhaseIr {
    Evaluation,
    Defer,
}

/// One original request. Phase and occurrence order survive linking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleExecutionRequestIr {
    phase: ModuleRequestPhaseIr,
    target: ModuleUnitId,
}

impl ModuleExecutionRequestIr {
    pub(super) const fn new(phase: ModuleRequestPhaseIr, target: ModuleUnitId) -> Self {
        Self { phase, target }
    }
    pub const fn phase(&self) -> ModuleRequestPhaseIr {
        self.phase
    }
    pub const fn target(&self) -> ModuleUnitId {
        self.target
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleActivationIr {
    module: ModuleUnitId,
    function: FunctionId,
    kind: ModuleActivationKindIr,
    requests: Vec<ModuleExecutionRequestIr>,
}

impl ModuleActivationIr {
    pub(super) fn new(
        module: ModuleUnitId,
        function: FunctionId,
        kind: ModuleActivationKindIr,
        requests: Vec<ModuleExecutionRequestIr>,
    ) -> Self {
        Self {
            module,
            function,
            kind,
            requests,
        }
    }
    pub const fn module(&self) -> ModuleUnitId {
        self.module
    }
    pub fn function(&self) -> &FunctionId {
        &self.function
    }
    pub const fn kind(&self) -> ModuleActivationKindIr {
        self.kind
    }
    pub fn requests(&self) -> &[ModuleExecutionRequestIr] {
        &self.requests
    }
}

/// Allocation precedes all instantiation; every request targets an owned record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleExecutionGraphIr {
    record_count: u32,
    activations: Vec<ModuleActivationIr>,
    source_records: Vec<ModuleSourceRecordIr>,
    initializer: Option<FunctionId>,
    realm_requests:
        std::collections::BTreeMap<super::ModuleRequestKeyIr, super::RealmModuleResolutionIr>,
    realm_import_dispatcher: Option<FunctionId>,
    entry: ModuleExecutionEntry,
    script_import_dispatchers: std::collections::BTreeMap<super::ImportPhaseIr, FunctionId>,
}

impl ModuleExecutionGraphIr {
    pub(super) fn new(
        record_count: u32,
        activations: Vec<ModuleActivationIr>,
        source_records: Vec<ModuleSourceRecordIr>,
        entry: ModuleExecutionEntry,
    ) -> Self {
        let functions: std::collections::BTreeSet<_> = activations
            .iter()
            .map(ModuleActivationIr::function)
            .collect();
        assert_eq!(
            functions.len(),
            activations.len(),
            "each module has its own compiled lexical owner"
        );
        let modules: std::collections::BTreeSet<_> =
            activations.iter().map(ModuleActivationIr::module).collect();
        assert_eq!(
            modules.len(),
            activations.len(),
            "a module has one activation"
        );
        let native_modules: std::collections::BTreeSet<_> = source_records
            .iter()
            .map(ModuleSourceRecordIr::module)
            .collect();
        assert_eq!(
            native_modules.len(),
            source_records.len(),
            "a native source record has one descriptor"
        );
        assert!(
            native_modules
                .iter()
                .all(|module| *module < record_count && !modules.contains(module)),
            "native source records own valid non-activation identities"
        );
        match entry {
            ModuleExecutionEntry::Module(module) => assert!(
                (modules.contains(&module) || native_modules.contains(&module)),
                "a Module entry owns a compiled activation"
            ),
            ModuleExecutionEntry::Script(script) => {
                assert!(
                    script < record_count,
                    "the Script belongs to the loaded graph"
                );
                assert!(
                    !modules.contains(&script) && !native_modules.contains(&script),
                    "a Script entry never becomes a module activation"
                );
            }
        }
        assert!(
            modules.iter().all(|&module| module < record_count),
            "module IDs fit the record vector"
        );
        assert!(
            activations
                .iter()
                .flat_map(ModuleActivationIr::requests)
                .all(|request| modules.contains(&request.target())
                    || native_modules.contains(&request.target())),
            "every requested module has an activation"
        );
        Self {
            record_count,
            activations,
            source_records,
            initializer: None,
            realm_requests: Default::default(),
            realm_import_dispatcher: None,
            entry,
            script_import_dispatchers: Default::default(),
        }
    }
    /// Root execution invokes the same compiled initializer that another Realm
    /// uses. Only the initializer's private body instantiates the records.
    pub(super) fn initialize_with(mut self, function: FunctionId) -> Self {
        assert!(self.initializer.is_none(), "initialization has one owner");
        assert!(
            self.activations
                .iter()
                .all(|activation| activation.function() != &function),
            "the graph initializer is not a module activation"
        );
        self.initializer = Some(function);
        self
    }
    pub fn initializer(&self) -> Option<&FunctionId> {
        self.initializer.as_ref()
    }
    pub(super) fn with_realm_requests(
        mut self,
        requests: std::collections::BTreeMap<
            super::ModuleRequestKeyIr,
            super::RealmModuleResolutionIr,
        >,
        dispatcher: FunctionId,
    ) -> Self {
        assert!(
            requests.values().all(|resolution| match resolution {
                super::RealmModuleResolutionIr::Loaded(module) =>
                    self.activations
                        .iter()
                        .any(|activation| activation.module() == *module)
                        || self
                            .source_records
                            .iter()
                            .any(|source| source.module() == *module),
                super::RealmModuleResolutionIr::Rejected(_) => true,
            }),
            "every successful Realm request owns a compiled activation"
        );
        self.realm_requests = requests;
        self.realm_import_dispatcher = Some(dispatcher);
        self
    }
    pub(super) fn with_script_import_dispatchers(
        mut self,
        dispatchers: std::collections::BTreeMap<super::ImportPhaseIr, FunctionId>,
    ) -> Self {
        assert!(
            matches!(self.entry, ModuleExecutionEntry::Script(_)) || dispatchers.is_empty(),
            "Script-origin imports require an actual Script record"
        );
        self.script_import_dispatchers = dispatchers;
        self
    }
    pub fn script_import_dispatchers(
        &self,
    ) -> &std::collections::BTreeMap<super::ImportPhaseIr, FunctionId> {
        &self.script_import_dispatchers
    }
    pub fn script_import_dispatcher(
        &self,
        referrer: ModuleUnitId,
        phase: super::ImportPhaseIr,
    ) -> Option<&FunctionId> {
        (self.entry == ModuleExecutionEntry::Script(referrer))
            .then(|| self.script_import_dispatchers.get(&phase))
            .flatten()
    }

    /// The actual Module entry, distinguished from a Script importing modules.
    pub const fn module_entry(&self) -> Option<ModuleUnitId> {
        match self.entry {
            ModuleExecutionEntry::Module(module) => Some(module),
            ModuleExecutionEntry::Script(_) => None,
        }
    }

    pub fn realm_requests(
        &self,
    ) -> &std::collections::BTreeMap<super::ModuleRequestKeyIr, super::RealmModuleResolutionIr>
    {
        &self.realm_requests
    }
    pub fn realm_import_dispatcher(&self) -> Option<&FunctionId> {
        self.realm_import_dispatcher.as_ref()
    }
    pub fn source_records(&self) -> &[ModuleSourceRecordIr] {
        &self.source_records
    }
    pub const fn record_count(&self) -> u32 {
        self.record_count
    }
    pub fn activations(&self) -> &[ModuleActivationIr] {
        &self.activations
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleImportBindingIr {
    pub name: String,
    pub target: ModuleCellIr,
}

/// Evaluation topology is a runtime property, never a static SCC schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleEvaluationIr {
    module: ModuleUnitId,
}

impl ModuleEvaluationIr {
    pub(super) const fn new(module: ModuleUnitId) -> Self {
        Self { module }
    }
    pub const fn module(&self) -> ModuleUnitId {
        self.module
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredModuleEvaluationIr {
    module: ModuleUnitId,
}

impl DeferredModuleEvaluationIr {
    pub(super) const fn new(module: ModuleUnitId) -> Self {
        Self { module }
    }
    pub const fn module(&self) -> ModuleUnitId {
        self.module
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_preserve_phase_and_occurrence_order() {
        let requests = vec![
            ModuleExecutionRequestIr::new(ModuleRequestPhaseIr::Defer, 0),
            ModuleExecutionRequestIr::new(ModuleRequestPhaseIr::Evaluation, 0),
        ];
        let graph = ModuleExecutionGraphIr::new(
            1,
            vec![ModuleActivationIr::new(
                0,
                "module".into(),
                ModuleActivationKindIr::Async,
                requests.clone(),
            )],
            Vec::new(),
            ModuleExecutionEntry::Module(0),
        );
        assert_eq!(graph.activations()[0].requests(), requests);
    }

    #[test]
    #[should_panic(expected = "every requested module has an activation")]
    fn missing_request_activation_is_rejected() {
        ModuleExecutionGraphIr::new(
            2,
            vec![ModuleActivationIr::new(
                0,
                "module".into(),
                ModuleActivationKindIr::Synchronous,
                vec![ModuleExecutionRequestIr::new(
                    ModuleRequestPhaseIr::Evaluation,
                    1,
                )],
            )],
            Vec::new(),
            ModuleExecutionEntry::Module(0),
        );
    }
    #[test]
    #[should_panic(expected = "a Module entry owns a compiled activation")]
    fn a_module_execution_graph_cannot_be_empty() {
        ModuleExecutionGraphIr::new(1, Vec::new(), Vec::new(), ModuleExecutionEntry::Module(0));
    }

    #[test]
    fn a_script_with_only_rejected_imports_owns_no_module_activation() {
        let graph =
            ModuleExecutionGraphIr::new(1, Vec::new(), Vec::new(), ModuleExecutionEntry::Script(0));
        assert_eq!(graph.record_count(), 1);
        assert!(graph.activations().is_empty());
    }

    #[test]
    #[should_panic(expected = "a Script entry never becomes a module activation")]
    fn a_script_cannot_become_a_module_activation() {
        ModuleExecutionGraphIr::new(
            1,
            vec![ModuleActivationIr::new(
                0,
                "script".into(),
                ModuleActivationKindIr::Synchronous,
                Vec::new(),
            )],
            Vec::new(),
            ModuleExecutionEntry::Script(0),
        );
    }

    #[test]
    #[should_panic(expected = "its own compiled lexical owner")]
    fn distinct_modules_cannot_share_one_source_activation_function() {
        ModuleExecutionGraphIr::new(
            2,
            vec![
                ModuleActivationIr::new(
                    0,
                    "owner".into(),
                    ModuleActivationKindIr::Synchronous,
                    Vec::new(),
                ),
                ModuleActivationIr::new(
                    1,
                    "owner".into(),
                    ModuleActivationKindIr::Async,
                    Vec::new(),
                ),
            ],
            Vec::new(),
            ModuleExecutionEntry::Module(0),
        );
    }
    #[test]
    fn native_source_owner_is_retained_without_a_javascript_activation() {
        let graph = ModuleExecutionGraphIr::new(
            2,
            vec![ModuleActivationIr::new(
                0,
                "entry".into(),
                ModuleActivationKindIr::Synchronous,
                vec![ModuleExecutionRequestIr::new(
                    ModuleRequestPhaseIr::Evaluation,
                    1,
                )],
            )],
            vec![ModuleSourceRecordIr::new(1, "源🌐".into())],
            ModuleExecutionEntry::Module(0),
        );
        assert_eq!(graph.activations().len(), 1);
        assert_eq!(graph.source_records()[0].module(), 1);
        assert_eq!(graph.source_records()[0].source_tag(), "源🌐");
        assert!(graph.initializer().is_none());
        assert_eq!(
            graph
                .clone()
                .initialize_with("initialize".into())
                .source_records(),
            graph.source_records()
        );
    }

    #[test]
    #[should_panic(expected = "native source records own valid non-activation identities")]
    fn native_source_descriptor_cannot_steal_a_javascript_activation_owner() {
        ModuleExecutionGraphIr::new(
            1,
            vec![ModuleActivationIr::new(
                0,
                "entry".into(),
                ModuleActivationKindIr::Synchronous,
                Vec::new(),
            )],
            vec![ModuleSourceRecordIr::new(0, "Source".into())],
            ModuleExecutionEntry::Module(0),
        );
    }

    #[test]
    #[should_panic(expected = "a native source record has one descriptor")]
    fn native_source_record_cannot_have_competing_host_tag_descriptors() {
        ModuleExecutionGraphIr::new(
            2,
            Vec::new(),
            vec![
                ModuleSourceRecordIr::new(1, "First".into()),
                ModuleSourceRecordIr::new(1, "Second".into()),
            ],
            ModuleExecutionEntry::Script(0),
        );
    }
}
