//! ECMAScript module records, linking and loading (T12).
//!
//! `record` owns `ParseModule` and the static entry tables (16.2.1.6.1,
//! 16.2.2, 16.2.3). `early` owns the module early errors (16.2.3.1) and the
//! classification of boa's own module-goal static-semantics failures.
//! `admission` assigns dynamic-only loading failures to import jobs while
//! preserving static rejection. `graph_build` owns transitive assembly; `graph_resolution` owns
//! `GetExportedNames` / `ResolveExport`; `graph_evaluation_classification` owns
//! evaluation-mode classification;
//! `graph_evaluation_order` owns static order/component queries for linking
//! and graph inspection; canonical execution selects components at runtime.
//! `graph_async_evaluation` owns async-module
//! propagation and pending-dependency queries; and `graph_materialization`
//! owns the evaluation-to-runtime query boundary.
//! `graph` retains the linked record and linking orchestration. `link` merges
//! the per-module bodies into the single `ScriptIr` the backend emits, and
//! `source` uses parser-retained ranges to delete module-goal-only syntax
//! from a unit's text. `default_export_definition` carries exact definition identity
//! through canonical activation assembly for NamedEvaluation and instantiation without
//! exposing a minted storage name or changing a callable's exact source.
//! `namespace` owns module namespace exotic objects and deferred namespaces. `dynamic` owns the `import()` component registry.
//!
//! Evaluation and defer requests use canonical activations. Source requests
//! load and parse their target, but this loader supplies only ECMAScript Source
//! Text Module Records: static source bindings fail at linking and dynamic
//! source jobs reject before dependency loading or evaluation.
//! `graph_evaluation_classification::classify_evaluation_modes` is the single
//! authority for which unit gets which treatment.
//!
//! `scriptw-ir` performs no IO. The host resolves and reads every source and
//! hands the closure over as a [`ModuleGraphSources`]; nothing in this
//! directory touches the filesystem.

mod admission;
mod default_export_definition;
mod dynamic;
mod early;
mod entry_evaluation;
mod evaluation_mode;
pub(crate) use entry_evaluation::{LinkedModuleEntry, ModuleEntryEvaluationBoundary};
pub use entry_evaluation::{ModuleEntryEvaluationIr, ModuleEntryEvaluationKindIr};
mod callable_source;
mod graph;
mod graph_async_evaluation;
mod graph_build;
mod graph_evaluation_classification;
mod graph_evaluation_order;
mod graph_materialization;
mod graph_resolution;
mod import_phase;
mod json;
pub(crate) use json::SyntheticModuleValueIr;
pub use json::{BytesModuleValueIr, JsonModuleValueIr, JsonValue};
mod link;
mod link_error;
mod loaded_sources;
mod module_key;
mod module_unit;
mod namespace;
mod namespace_definition;
mod pure_realm_discovery;
mod realm_request;
pub(crate) use pure_realm_discovery::PureRealmDiscovery;
mod record;
pub use realm_request::{RealmModuleResolutionIr, scan_script_realm_module_requests};
mod resolved_binding;
mod source;
mod synchronous_definition;
mod synchronous_execution;
mod synchronous_source;
pub use synchronous_execution::*;

pub use dynamic::*;
pub use evaluation_mode::ModuleEvaluationModeIr;
pub use graph::*;
pub use import_phase::ImportPhaseIr;
pub use link::*;
pub use link_error::ModuleLinkErrorIr;
pub use loaded_sources::{ModuleGraphSources, ModuleKindIr, ModuleSourceIr};
pub use module_key::{ANONYMOUS_MODULE_KEY, ModuleKey, ModuleRecordKey};
pub use module_unit::ModuleUnitIr;
pub use namespace::*;
pub(crate) use record::script_dynamic_import_sites;
pub use record::*;
pub use resolved_binding::{ModuleBindingNameIr, ResolvedBindingIr};

pub(crate) use admission::{GraphAdmission, link_complete_catalog, link_loaded_graph};
pub(crate) use dynamic::lower_import_call;
pub(crate) use graph::link;
pub(crate) use graph_build::build_graph;
pub(crate) use link::linked_script_source;

pub(crate) use namespace_definition::LinkedScriptDefinitions;

pub(crate) use synchronous_definition::ModuleExecutionAnalysis;
