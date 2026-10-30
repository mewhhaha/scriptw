use super::*;
mod binding_inference;
pub(crate) use binding_inference::{BindingInferenceRecipe, OwnerBindingInference};
mod direct_eval_capture;
mod environment_binding;
mod eval_environment;
mod function_environment;
mod module_locals;
use boa_ast::pattern::{ArrayPattern, ObjectPattern};
pub(crate) use eval_environment::{
    CompleteResumableForInOwner, CompleteResumableForOfOwner, MixedAsyncGeneratorWithOwner,
};
use std::sync::Arc;

/// Necessary syntax condition only; intrinsic eval identity remains a lowering proof.
pub(crate) fn contains_direct_eval_call(script: &Script) -> bool {
    eval_environment::contains_direct_eval_call(script)
}

#[derive(Debug, Clone)]
pub(crate) struct PendingFunction<'a> {
    pub(crate) id: FunctionId,
    pub(crate) name: String,
    pub(crate) to_string_representation: CallableToStringRepresentation,
    pub(crate) protocol: FunctionProtocolIr,
    pub(crate) strict: bool,
    pub(crate) self_binding_name: Option<String>,
    pub(crate) parameters: &'a FormalParameterList,
    pub(crate) body: &'a FunctionBody,
    pub(crate) is_expression: bool,
    pub(crate) capture_aliases: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CaptureBindingPlan {
    pub(crate) owner_id: String,
    pub(crate) environment_id: EnvironmentId,
    pub(crate) source_name: String,
    pub(crate) mode: BindingMode,
    pub(crate) slot: u32,
    pub(crate) hops: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnnexBFunctionPlan {
    pub(crate) owner_id: String,
    pub(crate) source_name: String,
    pub(crate) block_storage_name: String,
    pub(crate) copy_to_variable_environment: bool,
}

enum AnnexBDirectFunctionCollection {
    Skip,
    Record,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EnvironmentId(usize);

/// Compiler-only storage for an Object Environment Record's binding object.
///
/// The `.` makes the spelling unreachable from ECMAScript source. Construction
/// is tied to an allocated [`EnvironmentId`], so analysis is the sole producer
/// and lowering cannot silently invent a different binding for the same
/// environment.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct WithObjectBindingName(String);

impl WithObjectBindingName {
    fn for_environment(environment_id: EnvironmentId) -> Self {
        Self(format!("$with.object.{}", environment_id.0))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WithObjectEnvironmentPlan {
    pub(crate) binding_name: WithObjectBindingName,
    pub(crate) continuation_owner: WithContinuationOwner,
}

/// The actual source region decides which compiler owns this record's lifetime.
/// A resumable function alone does not give an iterator-body With whole phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WithContinuationOwner {
    Immediate,
    OrdinaryWhole,
    PlainAsyncWhole,
    MixedAsyncGeneratorWhole(MixedAsyncGeneratorWithOwner),
    LinearResumable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForInContinuationOwner {
    CompleteWhole(CompleteResumableForInOwner),
    ImmediateOrLinear,
}

impl WithContinuationOwner {
    fn for_execution(execution_kind: FunctionExecutionKind) -> Self {
        match execution_kind {
            FunctionExecutionKind::Ordinary => Self::Immediate,
            FunctionExecutionKind::Generator
            | FunctionExecutionKind::Async
            | FunctionExecutionKind::AsyncGenerator => Self::LinearResumable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PrivateEnvironmentId(pub(crate) u32);

#[derive(Debug, Clone)]
pub(crate) struct PrivateEnvironmentPlan {
    pub(crate) id: PrivateEnvironmentId,
    pub(crate) parent: Option<PrivateEnvironmentId>,
    pub(crate) bindings: BTreeMap<String, PrivateNameId>,
    pub(crate) auto_accessor_backings: BTreeMap<usize, AutoAccessorBackingNameIr>,
    pub(crate) slot_count: u32,
}

mod environment_kind;
pub(crate) use environment_kind::EnvironmentKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnvironmentCursor {
    pub(crate) owner_id: String,
    pub(crate) environment_id: EnvironmentId,
}

#[derive(Debug, Clone)]
pub(crate) struct EnvironmentPlan {
    pub(crate) eval_visible: bool,
    pub(crate) eval_environment: Option<EvalEnvironmentRoleIr>,
    pub(crate) id: EnvironmentId,
    pub(crate) owner_id: String,
    pub(crate) kind: EnvironmentKind,
    pub(crate) parent_cursor: Option<EnvironmentCursor>,
    pub(crate) binding_storage_names: BTreeSet<String>,
    pub(crate) binding_modes: BTreeMap<String, BindingMode>,
    pub(crate) owned_env_slots: BTreeMap<String, u32>,
}

#[derive(Debug, Clone)]
pub(crate) struct OwnerPlan {
    pub(crate) parameter_names: BTreeSet<String>,
    pub(crate) parameter_external_refs: BTreeSet<String>,
    pub(crate) eval_invocation: scriptw_front::EvalInvocationContext,
    pub(crate) flavor: FunctionFlavor,
    pub(crate) execution_kind: FunctionExecutionKind,
    pub(crate) lexical_super_owner_role: LexicalSuperOwnerRole,
    pub(crate) strict: bool,
    pub(crate) parent_owner_id: Option<String>,
    pub(crate) activation_environment_id: EnvironmentId,
    pub(crate) body_environment_id: Option<EnvironmentId>,
    pub(crate) parameter_eval_environment_id: Option<EnvironmentId>,
    pub(crate) definition_environment_cursor: EnvironmentCursor,
    pub(crate) root_bindings: BTreeSet<String>,
    pub(crate) function_bindings: BTreeMap<String, FunctionId>,
    pub(crate) owned_env_slots: BTreeMap<String, u32>,
    pub(crate) private_environment_id: Option<PrivateEnvironmentId>,
}

#[derive(Debug, Clone)]
pub(crate) struct FunctionPlan<'a> {
    pub(crate) id: FunctionId,
    pub(crate) name: String,
    pub(crate) to_string_representation: CallableToStringRepresentation,
    pub(crate) protocol: FunctionProtocolIr,
    pub(crate) strict: bool,
    pub(crate) self_binding_name: Option<String>,
    pub(crate) parent_owner_id: String,
    pub(crate) parameters: &'a FormalParameterList,
    pub(crate) body: &'a FunctionBody,
    pub(crate) is_expression: bool,
    pub(crate) root_functions: Vec<PendingFunction<'a>>,
    pub(crate) captures: BTreeMap<String, CaptureBindingPlan>,
    pub(crate) lexical_derived_activation_owner: Option<FunctionId>,
}

/// The analyzed execution owner of an ordinary statement-list `using` scope.
///
/// Resumable owners remain named so lowering must mint the corresponding
/// activation-backed proof or reject that owner explicitly rather than
/// falling through to immediate storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncDisposableScopeOwnerPlan {
    Immediate,
    PlainGenerator,
    AsyncFunction,
    AsyncGenerator,
}

/// The exhaustive function owner decision for an ordinary statement-list
/// `await using` declaration.
///
/// Unsupported owners stay named instead of collapsing into `Option`, so a new
/// execution kind cannot silently acquire the plain-async capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncDisposableScopeOwnerPlan {
    Ordinary,
    Generator,
    AsyncFunction,
    AsyncGenerator,
}

impl FunctionPlan<'_> {
    pub(crate) fn sync_disposable_scope_owner(&self) -> SyncDisposableScopeOwnerPlan {
        match self.protocol.source_execution_kind() {
            FunctionExecutionKind::Ordinary => SyncDisposableScopeOwnerPlan::Immediate,
            FunctionExecutionKind::Generator => SyncDisposableScopeOwnerPlan::PlainGenerator,
            FunctionExecutionKind::Async => SyncDisposableScopeOwnerPlan::AsyncFunction,
            FunctionExecutionKind::AsyncGenerator => SyncDisposableScopeOwnerPlan::AsyncGenerator,
        }
    }

    pub(crate) fn async_disposable_scope_owner(&self) -> AsyncDisposableScopeOwnerPlan {
        match self.protocol.source_execution_kind() {
            FunctionExecutionKind::Ordinary => AsyncDisposableScopeOwnerPlan::Ordinary,
            FunctionExecutionKind::Generator => AsyncDisposableScopeOwnerPlan::Generator,
            FunctionExecutionKind::Async => AsyncDisposableScopeOwnerPlan::AsyncFunction,
            FunctionExecutionKind::AsyncGenerator => AsyncDisposableScopeOwnerPlan::AsyncGenerator,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct AnalysisAllocationState {
    pub(crate) prepared_sources: crate::prepared_source_cache::PreparedSourceCache,
    pub(crate) script_import_origin: Option<crate::direct_eval_context::ScriptImportOrigin>,
    pub(crate) template_sources: crate::template_site::TemplateSourceOwners,
    next_static_script_id: u32,
    next_function_id: usize,
    next_environment_id: usize,
    next_private_environment_id: u32,
}

/// Analysis retains only its live semantic cache and allocation counters.
/// The parsed-template registry stays with the caller across compilation units.
#[derive(Debug, Clone)]
pub(crate) struct AnalysisAllocationProgress {
    pub(crate) prepared_sources: crate::prepared_source_cache::PreparedSourceCache,
    pub(crate) script_import_origin: Option<crate::direct_eval_context::ScriptImportOrigin>,
    next_static_script_id: u32,
    next_function_id: usize,
    next_environment_id: usize,
    next_private_environment_id: u32,
}

impl AnalysisAllocationState {
    pub(crate) fn commit_analysis(&mut self, progress: &AnalysisAllocationProgress) {
        self.next_static_script_id = progress.next_static_script_id;
        self.next_function_id = progress.next_function_id;
        self.next_environment_id = progress.next_environment_id;
        self.next_private_environment_id = progress.next_private_environment_id;
    }

    pub(crate) fn with_private_environment_floor(mut self, floor: u32) -> Self {
        self.next_private_environment_id = floor;
        self
    }

    pub(crate) fn require_private_environment_capacity(
        &self,
        count: u32,
    ) -> Result<(), &'static str> {
        self.next_private_environment_id
            .checked_add(count)
            .map(|_| ())
            .ok_or("private-scope identity space is exhausted")
    }

    pub(crate) fn allocate_static_script_id(&mut self) -> StaticScriptId {
        let id = StaticScriptId(self.next_static_script_id);
        self.next_static_script_id = self
            .next_static_script_id
            .checked_add(1)
            .expect("static Script id space exhausted");
        id
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Analysis<'a> {
    pub(crate) binding_inference: OwnerBindingInference<'a>,
    pub(crate) module_local_candidates: BTreeMap<FunctionId, BTreeSet<String>>,
    pub(crate) template_source: Option<TemplateSourceIr>,
    pub(crate) function_global_hints: crate::prepared_source_cache::FunctionGlobalConstructorHints,
    pub(crate) source_function_global_hypotheses:
        crate::prepared_source_cache::FunctionGlobalConstructorHints,
    pub(crate) script_instantiation: ScriptInstantiation,
    pub(crate) allocations: AnalysisAllocationProgress,
    pub(crate) owner_plans: BTreeMap<String, OwnerPlan>,
    pub(crate) environment_plans: BTreeMap<EnvironmentId, EnvironmentPlan>,
    pub(crate) physical_binding_environments: BTreeMap<String, BTreeSet<EnvironmentId>>,
    pub(crate) block_environment_ids: BTreeMap<usize, EnvironmentId>,
    pub(crate) with_environment_ids: BTreeMap<usize, EnvironmentId>,
    pub(crate) with_object_environment_plans: BTreeMap<EnvironmentId, WithObjectEnvironmentPlan>,
    pub(crate) for_in_continuation_owners: BTreeMap<usize, ForInContinuationOwner>,
    pub(crate) complete_for_of_owners: BTreeMap<usize, CompleteResumableForOfOwner>,
    pub(crate) switch_environment_ids: BTreeMap<usize, EnvironmentId>,
    pub(crate) catch_parameter_environment_ids: BTreeMap<usize, EnvironmentId>,
    pub(crate) for_lexical_environment_ids: BTreeMap<usize, EnvironmentId>,
    pub(crate) for_in_of_tdz_environment_ids: BTreeMap<usize, EnvironmentId>,
    pub(crate) for_in_of_iteration_environment_ids: BTreeMap<usize, EnvironmentId>,
    pub(crate) function_plans: BTreeMap<FunctionId, FunctionPlan<'a>>,
    pub(crate) planned_source_function_ids: Arc<BTreeSet<FunctionId>>,
    pub(crate) function_declaration_ids: BTreeMap<String, FunctionId>,
    pub(crate) annex_b_function_plans: BTreeMap<String, AnnexBFunctionPlan>,
    pub(crate) function_expr_ids: BTreeMap<String, FunctionId>,
    pub(crate) class_execution_ids: BTreeMap<String, FunctionId>,
    pub(crate) module_execution: crate::modules::ModuleExecutionAnalysis,
    pub(crate) module_entry_evaluation: Option<crate::modules::ModuleEntryEvaluationBoundary<'a>>,
    pub(crate) namespace_initializers: BTreeMap<usize, ModuleNamespaceModeIr>,
    pub(crate) default_export_class_ids: BTreeSet<FunctionId>,
    pub(crate) hoisted_default_export_function_ids: BTreeSet<FunctionId>,
    pub(crate) class_name_environment_ids: BTreeMap<String, EnvironmentId>,
    pub(crate) private_environment_plans: BTreeMap<PrivateEnvironmentId, PrivateEnvironmentPlan>,
    pub(crate) class_private_environment_ids: BTreeMap<String, PrivateEnvironmentId>,
    pub(crate) owner_free_refs: BTreeMap<String, BTreeMap<String, String>>,
    pub(crate) function_order: Vec<FunctionId>,
    pub(crate) script_root_functions: Vec<PendingFunction<'a>>,
    pub(crate) script_items: &'a [StatementListItem],
}

impl Analysis<'_> {
    pub(crate) fn resolve_private_name(
        &self,
        mut environment_id: Option<PrivateEnvironmentId>,
        name: &str,
    ) -> Option<(PrivateEnvironmentId, PrivateNameId)> {
        while let Some(id) = environment_id {
            let environment = self.private_environment_plans.get(&id)?;
            if let Some(private_name_id) = environment.bindings.get(name) {
                return Some((id, *private_name_id));
            }
            environment_id = environment.parent;
        }
        None
    }

    pub(crate) fn materialized_stage_a_environment(
        &self,
        environment_id: EnvironmentId,
    ) -> Option<&EnvironmentPlan> {
        let environment = self.environment_plans.get(&environment_id)?;
        environment
            .kind
            .is_materialized_in_stage_a()
            .then_some(environment)
    }

    pub(crate) fn environment_has_runtime_storage(&self, environment: &EnvironmentPlan) -> bool {
        environment_has_runtime_storage(
            environment,
            self.owner_plans[&environment.owner_id].execution_kind,
        )
    }

    pub(crate) fn materialized_environment(
        &self,
        environment_id: EnvironmentId,
    ) -> Option<&EnvironmentPlan> {
        let environment = self.environment_plans.get(&environment_id)?;
        environment.kind.is_materialized().then_some(environment)
    }
}

fn environment_has_runtime_storage(
    environment: &EnvironmentPlan,
    execution_kind: FunctionExecutionKind,
) -> bool {
    // Lowering adds suspension-owned operands after capture hops are fixed.
    // Every resumable activation therefore owns a frame even before it has
    // slots, so adding those operands cannot change the environment chain.
    let resumable_activation = environment.kind == EnvironmentKind::Activation
        && match execution_kind {
            FunctionExecutionKind::Ordinary => false,
            FunctionExecutionKind::Generator
            | FunctionExecutionKind::Async
            | FunctionExecutionKind::AsyncGenerator => true,
        };
    (environment.eval_visible
        || resumable_activation
        || matches!(
            environment.kind,
            EnvironmentKind::FunctionBody
                | EnvironmentKind::FunctionParameters
                | EnvironmentKind::NamedFunctionExpression
        )
        || !environment.owned_env_slots.is_empty())
        && (environment.kind == EnvironmentKind::Activation || environment.kind.is_materialized())
}

#[derive(Default)]
pub(crate) struct AnalysisBuilder<'a> {
    prepared_sources: crate::prepared_source_cache::PreparedSourceCache,
    script_import_origin: Option<crate::direct_eval_context::ScriptImportOrigin>,
    template_source: Option<TemplateSourceIr>,
    script_instantiation: ScriptInstantiation,
    eval_visible: bool,
    realm_reusable_module_environments: bool,
    module_local_candidates: BTreeMap<FunctionId, BTreeSet<String>>,
    source_names_by_storage: BTreeMap<String, String>,
    source_identifiers: BTreeSet<String>,
    owner_plans: BTreeMap<String, OwnerPlan>,
    environment_plans: BTreeMap<EnvironmentId, EnvironmentPlan>,
    physical_binding_environments: BTreeMap<String, BTreeSet<EnvironmentId>>,
    block_environment_ids: BTreeMap<usize, EnvironmentId>,
    with_environment_ids: BTreeMap<usize, EnvironmentId>,
    with_object_environment_plans: BTreeMap<EnvironmentId, WithObjectEnvironmentPlan>,
    for_in_continuation_owners: BTreeMap<usize, ForInContinuationOwner>,
    complete_for_of_owners: BTreeMap<usize, CompleteResumableForOfOwner>,
    switch_environment_ids: BTreeMap<usize, EnvironmentId>,
    catch_parameter_environment_ids: BTreeMap<usize, EnvironmentId>,
    for_lexical_environment_ids: BTreeMap<usize, EnvironmentId>,
    for_in_of_tdz_environment_ids: BTreeMap<usize, EnvironmentId>,
    for_in_of_iteration_environment_ids: BTreeMap<usize, EnvironmentId>,
    complete_resumable_for_of_iteration_environment_ids: BTreeSet<EnvironmentId>,
    function_plans: BTreeMap<FunctionId, FunctionPlan<'a>>,
    function_declaration_ids: BTreeMap<String, FunctionId>,
    annex_b_function_plans: BTreeMap<String, AnnexBFunctionPlan>,
    function_expr_ids: BTreeMap<String, FunctionId>,
    class_execution_ids: BTreeMap<String, FunctionId>,
    class_method_bodies: BTreeMap<FunctionId, &'a FunctionBody>,
    class_name_environment_ids: BTreeMap<String, EnvironmentId>,
    private_environment_plans: BTreeMap<PrivateEnvironmentId, PrivateEnvironmentPlan>,
    class_private_environment_ids: BTreeMap<String, PrivateEnvironmentId>,
    function_free_refs: BTreeMap<FunctionId, BTreeMap<String, String>>,
    parameter_environment_bindings: BTreeMap<String, BTreeSet<String>>,
    function_order: Vec<FunctionId>,
    next_static_script_id: u32,
    next_function_id: usize,
    next_environment_id: usize,
    next_private_environment_id: u32,
    environment_cursor_stack: Vec<EnvironmentCursor>,
    private_environment_stack: Vec<PrivateEnvironmentId>,
    lexical_strictness: Option<Strictness>,
}

impl<'a> AnalysisBuilder<'a> {
    /// A module graph can execute in a Realm whose global declarations differ
    /// from the entry Script's. Named environment lookup keeps those bindings
    /// attached to that Realm instead of capturing entry-script slot indices.
    pub(crate) fn with_realm_reusable_module_environments(mut self, enabled: bool) -> Self {
        self.realm_reusable_module_environments = enabled;
        self
    }

    pub(crate) fn with_allocations(
        allocations: &AnalysisAllocationState,
        script_instantiation: ScriptInstantiation,
        template_source: Option<TemplateSourceIr>,
    ) -> Self {
        Self {
            template_source,
            prepared_sources: allocations.prepared_sources.clone(),
            script_import_origin: allocations.script_import_origin,
            script_instantiation,
            next_static_script_id: allocations.next_static_script_id,
            next_function_id: allocations.next_function_id,
            next_environment_id: allocations.next_environment_id,
            next_private_environment_id: allocations.next_private_environment_id,
            ..Self::default()
        }
    }

    pub(crate) fn finish(
        mut self,
        script: &'a Script,
        interner: &'a Interner,
        source_text: impl Into<CompilerSourceText<'a>>,
    ) -> Analysis<'a> {
        let source_text = source_text.into();
        self.eval_visible = self.eval_visible
            || matches!(
                &self.script_instantiation,
                ScriptInstantiation::Prepared(PreparedScriptKind::DirectEval(_))
            )
            || (script.strict()
                && matches!(
                    &self.script_instantiation,
                    ScriptInstantiation::Prepared(
                        PreparedScriptKind::IndirectEval | PreparedScriptKind::ShadowRealmEvaluate
                    )
                ))
            || eval_environment::contains_direct_eval_call(script)
            || eval_environment::contains_annex_b_arguments_candidate(script)
            || eval_environment::contains_arguments_reference(script);
        // Reusable Realm lookup needs named nonlocals, but does not expose
        // uncaptured function locals to eval. Actual eval/arguments syntax
        // conservatively retains the complete existing environment plan.
        let module_local_candidates = self.realm_reusable_module_environments && !self.eval_visible;
        self.eval_visible |= self.realm_reusable_module_environments;
        self.source_identifiers = eval_environment::source_identifiers(script, interner);
        let direct_context = match &self.script_instantiation {
            ScriptInstantiation::Prepared(PreparedScriptKind::DirectEval(context)) => {
                Some(context.clone())
            }
            _ => None,
        };
        if let Some(context) = &direct_context {
            if !context.private_names().is_empty() {
                let id = PrivateEnvironmentId(self.next_private_environment_id);
                self.next_private_environment_id = self
                    .next_private_environment_id
                    .checked_add(1)
                    .expect("private-scope capacity was reserved before analysis");
                self.private_environment_plans.insert(
                    id,
                    PrivateEnvironmentPlan {
                        id,
                        parent: None,
                        bindings: context.private_names().clone(),
                        auto_accessor_backings: BTreeMap::new(),
                        slot_count: 0,
                    },
                );
                self.private_environment_stack.push(id);
            }
        }

        self.collect_owner_annex_b_function_plans(
            SCRIPT_OWNER_ID,
            annex_b_function_declarations(script),
            lexically_declared_names(script),
            script.statements().statements(),
            script.strict(),
            &[],
            interner,
        );
        let script_root_functions = self.collect_root_functions(
            interner,
            source_text,
            script.statements().statements(),
            script.strict(),
            &BTreeMap::new(),
        );
        let script_root_bindings = {
            let mut bindings = self.collect_owner_bindings(
                interner,
                &[],
                None,
                false,
                false,
                false,
                script.statements().statements(),
                &script_root_functions,
            );
            bindings.extend(
                self.annex_b_function_plans
                    .values()
                    .filter(|plan| plan.owner_id == SCRIPT_OWNER_ID)
                    .map(|plan| plan.block_storage_name.clone()),
            );
            self.remove_ineligible_block_function_owner_bindings(
                SCRIPT_OWNER_ID,
                &mut bindings,
                self.collect_independent_owner_binding_names(
                    interner,
                    &[],
                    script.statements().statements(),
                    &script_root_functions,
                ),
            );
            bindings
        };
        let mut script_activation_binding_modes = self.activation_binding_modes(
            interner,
            &[],
            None,
            script.statements().statements(),
            &script_root_bindings,
        );
        self.apply_annex_b_variable_environment_binding_modes(
            SCRIPT_OWNER_ID,
            &mut script_activation_binding_modes,
        );
        let script_activation_environment_id =
            self.register_activation_environment(SCRIPT_OWNER_ID, script_root_bindings.clone());
        self.set_environment_binding_modes(
            script_activation_environment_id,
            script_activation_binding_modes,
        );
        self.owner_plans.insert(
            SCRIPT_OWNER_ID.to_string(),
            OwnerPlan {
                parameter_external_refs: BTreeSet::new(),
                body_environment_id: None,
                parameter_eval_environment_id: None,
                parameter_names: BTreeSet::new(),
                eval_invocation: direct_context.as_ref().map_or(
                    scriptw_front::EvalInvocationContext::Script,
                    DirectEvalContextIr::invocation,
                ),
                flavor: FunctionFlavor::Ordinary,
                execution_kind: FunctionExecutionKind::Ordinary,
                lexical_super_owner_role: LexicalSuperOwnerRole::None,
                strict: script.strict(),
                parent_owner_id: None,
                activation_environment_id: script_activation_environment_id,
                definition_environment_cursor: EnvironmentCursor {
                    owner_id: SCRIPT_OWNER_ID.to_string(),
                    environment_id: script_activation_environment_id,
                },
                root_bindings: script_root_bindings,
                function_bindings: script_root_functions
                    .iter()
                    .map(|function| (function.name.clone(), function.id.clone()))
                    .collect(),
                owned_env_slots: BTreeMap::new(),
                private_environment_id: self.current_private_environment_id(),
            },
        );
        self.scan_owner_items(
            SCRIPT_OWNER_ID,
            &[],
            script.statements().statements(),
            interner,
            source_text,
            None,
            &BTreeMap::new(),
        );
        for function in script_root_functions.iter().cloned() {
            self.collect_function_plan(
                function,
                SCRIPT_OWNER_ID.to_string(),
                self.activation_environment_cursor(SCRIPT_OWNER_ID),
                interner,
                source_text,
            );
        }
        self.finalize_with_continuation_owners();
        if module_local_candidates {
            self.plan_module_local_candidates();
        }
        self.finalize_capture_plans(interner);
        let planned_source_function_ids = Arc::new(
            self.function_plans
                .keys()
                .chain(self.class_execution_ids.values())
                .cloned()
                .collect::<BTreeSet<_>>(),
        );
        let binding_inference = OwnerBindingInference::collect(
            script.statements().statements(),
            &self.function_plans,
            interner,
        );
        Analysis {
            binding_inference,
            module_local_candidates: self.module_local_candidates,
            function_global_hints: Default::default(),
            source_function_global_hypotheses: Default::default(),
            template_source: self.template_source,
            script_instantiation: self.script_instantiation,
            allocations: AnalysisAllocationProgress {
                prepared_sources: self.prepared_sources,
                script_import_origin: self.script_import_origin,
                next_static_script_id: self.next_static_script_id,
                next_function_id: self.next_function_id,
                next_environment_id: self.next_environment_id,
                next_private_environment_id: self.next_private_environment_id,
            },
            owner_plans: self.owner_plans,
            environment_plans: self.environment_plans,
            physical_binding_environments: self.physical_binding_environments,
            block_environment_ids: self.block_environment_ids,
            with_environment_ids: self.with_environment_ids,
            with_object_environment_plans: self.with_object_environment_plans,
            for_in_continuation_owners: self.for_in_continuation_owners,
            complete_for_of_owners: self.complete_for_of_owners,
            switch_environment_ids: self.switch_environment_ids,
            catch_parameter_environment_ids: self.catch_parameter_environment_ids,
            for_lexical_environment_ids: self.for_lexical_environment_ids,
            for_in_of_tdz_environment_ids: self.for_in_of_tdz_environment_ids,
            for_in_of_iteration_environment_ids: self.for_in_of_iteration_environment_ids,
            function_plans: self.function_plans,
            planned_source_function_ids,
            function_declaration_ids: self.function_declaration_ids,
            annex_b_function_plans: self.annex_b_function_plans,
            function_expr_ids: self.function_expr_ids,
            class_execution_ids: self.class_execution_ids,
            module_execution: crate::modules::ModuleExecutionAnalysis::default(),
            module_entry_evaluation: None,
            namespace_initializers: BTreeMap::new(),
            default_export_class_ids: BTreeSet::new(),
            hoisted_default_export_function_ids: BTreeSet::new(),
            class_name_environment_ids: self.class_name_environment_ids,
            private_environment_plans: self.private_environment_plans,
            class_private_environment_ids: self.class_private_environment_ids,
            owner_free_refs: self.function_free_refs,
            function_order: self.function_order,
            script_root_functions,
            script_items: script.statements().statements(),
        }
    }

    fn alloc_function_id(&mut self) -> FunctionId {
        let id = format!("f{}", self.next_function_id);
        self.next_function_id += 1;
        id
    }

    fn alloc_environment_id(&mut self) -> EnvironmentId {
        let id = EnvironmentId(self.next_environment_id);
        self.next_environment_id += 1;
        id
    }

    fn activation_environment_cursor(&self, owner_id: &str) -> EnvironmentCursor {
        let owner = self
            .owner_plans
            .get(owner_id)
            .expect("environment owner must be planned before it is scanned");
        EnvironmentCursor {
            owner_id: owner_id.to_string(),
            environment_id: owner.activation_environment_id,
        }
    }

    fn current_environment_cursor(&self) -> EnvironmentCursor {
        self.environment_cursor_stack
            .last()
            .cloned()
            .expect("analysis scan must have an active lexical environment")
    }

    fn current_private_environment_id(&self) -> Option<PrivateEnvironmentId> {
        self.private_environment_stack.last().copied()
    }

    fn register_class_private_environment(
        &mut self,
        constructor_execution_key: String,
        elements: &[ClassElement],
        interner: &Interner,
    ) -> Option<PrivateEnvironmentId> {
        let mut private_names = Vec::new();
        let mut auto_accessor_element_indexes = Vec::new();
        for (element_index, element) in elements.iter().enumerate() {
            let private_name = match element {
                ClassElement::MethodDefinition(method) => match method.name() {
                    ClassElementName::PrivateName(name) => Some(*name),
                    ClassElementName::PropertyName(_) => None,
                },
                ClassElement::PrivateFieldDefinition(field)
                | ClassElement::PrivateStaticFieldDefinition(field) => {
                    if field.kind() == boa_ast::function::PrivateFieldDefinitionKind::AutoAccessor {
                        auto_accessor_element_indexes.push(element_index);
                    }
                    Some(*field.name())
                }
                ClassElement::FieldDefinition(_)
                | ClassElement::StaticFieldDefinition(_)
                | ClassElement::StaticBlock(_) => None,
                ClassElement::AccessorFieldDefinition(_)
                | ClassElement::StaticAccessorFieldDefinition(_) => {
                    auto_accessor_element_indexes.push(element_index);
                    None
                }
            };
            if let Some(private_name) = private_name {
                private_names.push(private_name_key(interner, private_name));
            }
        }
        if private_names.is_empty() && auto_accessor_element_indexes.is_empty() {
            return None;
        }

        let id = PrivateEnvironmentId(self.next_private_environment_id);
        self.next_private_environment_id = self
            .next_private_environment_id
            .checked_add(1)
            .expect("private-scope capacity was reserved before analysis");
        let mut bindings = BTreeMap::new();
        for private_name in private_names {
            if bindings.contains_key(&private_name) {
                continue;
            }
            let name_ordinal =
                u32::try_from(bindings.len()).expect("class private name count must fit in u32");
            bindings.insert(private_name, PrivateNameId::new(id.0, name_ordinal));
        }
        let mut auto_accessor_backings = BTreeMap::new();
        for element_index in auto_accessor_element_indexes {
            let name_ordinal = u32::try_from(bindings.len() + auto_accessor_backings.len())
                .expect("class private slot count must fit in u32");
            auto_accessor_backings.insert(
                element_index,
                AutoAccessorBackingNameIr::new(PrivateNameId::new(id.0, name_ordinal)),
            );
        }
        let slot_count = u32::try_from(bindings.len() + auto_accessor_backings.len())
            .expect("class private slot count must fit in u32");
        self.private_environment_plans.insert(
            id,
            PrivateEnvironmentPlan {
                id,
                parent: self.current_private_environment_id(),
                bindings,
                auto_accessor_backings,
                slot_count,
            },
        );
        self.class_private_environment_ids
            .insert(constructor_execution_key, id);
        Some(id)
    }

    fn register_activation_environment(
        &mut self,
        owner_id: &str,
        binding_storage_names: BTreeSet<String>,
    ) -> EnvironmentId {
        let id = self.alloc_environment_id();
        self.register_environment_plan(
            id,
            owner_id,
            EnvironmentKind::Activation,
            None,
            binding_storage_names,
        );
        id
    }

    fn register_lexical_environment(
        &mut self,
        owner_id: &str,
        kind: EnvironmentKind,
        parent_cursor: EnvironmentCursor,
        binding_storage_names: BTreeSet<String>,
    ) -> EnvironmentCursor {
        let id = self.alloc_environment_id();
        self.register_environment_plan(
            id,
            owner_id,
            kind,
            Some(parent_cursor),
            binding_storage_names,
        );
        EnvironmentCursor {
            owner_id: owner_id.to_string(),
            environment_id: id,
        }
    }

    fn register_lexical_environment_with_modes(
        &mut self,
        owner_id: &str,
        kind: EnvironmentKind,
        parent_cursor: EnvironmentCursor,
        binding_storage_names: BTreeSet<String>,
        binding_modes: BTreeMap<String, BindingMode>,
    ) -> EnvironmentCursor {
        let cursor =
            self.register_lexical_environment(owner_id, kind, parent_cursor, binding_storage_names);
        self.set_environment_binding_modes(cursor.environment_id, binding_modes);
        cursor
    }

    fn register_class_name_environment(
        &mut self,
        owner_id: &str,
        constructor_execution_key: String,
        parent_cursor: EnvironmentCursor,
        storage_name: String,
    ) -> EnvironmentCursor {
        let cursor = self.register_lexical_environment_with_modes(
            owner_id,
            EnvironmentKind::ClassName,
            parent_cursor,
            BTreeSet::from([storage_name.clone()]),
            BTreeMap::from([(storage_name.clone(), BindingMode::Const)]),
        );
        self.environment_plans
            .get_mut(&cursor.environment_id)
            .expect("class name environment must be registered")
            .owned_env_slots
            .insert(storage_name, 0);
        self.class_name_environment_ids
            .insert(constructor_execution_key, cursor.environment_id);
        cursor
    }

    fn register_with_object_environment(
        &mut self,
        owner_id: &str,
        parent_cursor: EnvironmentCursor,
    ) -> EnvironmentCursor {
        let id = self.alloc_environment_id();
        let binding_name = WithObjectBindingName::for_environment(id);
        self.register_environment_plan(
            id,
            owner_id,
            EnvironmentKind::WithObject,
            Some(parent_cursor),
            BTreeSet::from([binding_name.as_str().to_string()]),
        );
        self.with_object_environment_plans.insert(
            id,
            WithObjectEnvironmentPlan {
                binding_name: binding_name.clone(),
                continuation_owner: WithContinuationOwner::for_execution(
                    self.owner_plans[owner_id].execution_kind,
                ),
            },
        );
        EnvironmentCursor {
            owner_id: owner_id.to_string(),
            environment_id: id,
        }
    }

    fn set_environment_parent_cursor(
        &mut self,
        environment_id: EnvironmentId,
        parent_cursor: EnvironmentCursor,
    ) {
        self.environment_plans
            .get_mut(&environment_id)
            .expect("environment must be planned before its parent cursor is assigned")
            .parent_cursor = Some(parent_cursor);
    }

    fn register_environment_plan(
        &mut self,
        id: EnvironmentId,
        owner_id: &str,
        kind: EnvironmentKind,
        parent_cursor: Option<EnvironmentCursor>,
        binding_storage_names: BTreeSet<String>,
    ) {
        for binding_storage_name in &binding_storage_names {
            self.physical_binding_environments
                .entry(binding_storage_name.clone())
                .or_default()
                .insert(id);
        }
        self.environment_plans.insert(
            id,
            EnvironmentPlan {
                eval_visible: self.eval_visible,
                eval_environment: None,
                id,
                owner_id: owner_id.to_string(),
                kind,
                parent_cursor,
                binding_modes: binding_storage_names
                    .iter()
                    .map(|name| (name.clone(), BindingMode::Let))
                    .collect(),
                binding_storage_names,
                owned_env_slots: BTreeMap::new(),
            },
        );
    }

    fn set_environment_binding_modes(
        &mut self,
        environment_id: EnvironmentId,
        binding_modes: BTreeMap<String, BindingMode>,
    ) {
        let environment = self
            .environment_plans
            .get_mut(&environment_id)
            .expect("environment must be planned before its binding modes are assigned");
        for (name, mode) in binding_modes {
            if environment.binding_storage_names.contains(&name) {
                environment.binding_modes.insert(name, mode);
            }
        }
    }

    fn finalize_activation_environment_bindings(&mut self, owner_id: &str) {
        let owner = &self.owner_plans[owner_id];
        let activation_environment_id = owner
            .body_environment_id
            .unwrap_or(owner.activation_environment_id);
        let parameter_environment_id = owner.activation_environment_id;
        let parameter_eval_environment_id = owner.parameter_eval_environment_id;
        let nested_bindings = self
            .environment_plans
            .values()
            .filter(|plan| {
                plan.owner_id == owner_id
                    && plan.kind != EnvironmentKind::NamedFunctionExpression
                    && plan.id != activation_environment_id
                    && plan.id != parameter_environment_id
                    && Some(plan.id) != parameter_eval_environment_id
            })
            .flat_map(|plan| plan.binding_storage_names.iter().cloned())
            .collect::<BTreeSet<_>>();
        let activation_bindings = &mut self
            .environment_plans
            .get_mut(&activation_environment_id)
            .expect("activation environment must be planned")
            .binding_storage_names;
        let removed_bindings = activation_bindings
            .iter()
            .filter(|binding| nested_bindings.contains(*binding))
            .cloned()
            .collect::<Vec<_>>();
        activation_bindings.retain(|binding| !nested_bindings.contains(binding));
        for binding_storage_name in removed_bindings {
            self.environment_plans
                .get_mut(&activation_environment_id)
                .expect("activation environment must be planned")
                .binding_modes
                .remove(&binding_storage_name);
            let environments = self
                .physical_binding_environments
                .get_mut(&binding_storage_name)
                .expect("activation binding must have an environment mapping");
            environments.remove(&activation_environment_id);
            if environments.is_empty() {
                self.physical_binding_environments
                    .remove(&binding_storage_name);
            }
        }
    }

    fn collect_owner_annex_b_function_plans(
        &mut self,
        owner_id: &str,
        candidates: Vec<&'a FunctionDeclaration>,
        lexical_names: Vec<boa_interner::Sym>,
        items: &'a [StatementListItem],
        owner_strict: bool,
        parameters: &[FormalParameter],
        interner: &Interner,
    ) {
        if owner_strict {
            return;
        }

        let lexical_names = lexical_names
            .into_iter()
            .map(|name| interner.resolve_expect(name).to_string())
            .collect::<BTreeSet<_>>();
        let mut blocked_parameter_names = BTreeSet::new();
        for parameter in parameters {
            let mut names = Vec::new();
            collect_binding_names(interner, parameter.variable().binding(), &mut names);
            blocked_parameter_names.extend(names);
        }
        // Annex B checks formal parameterNames, not FDI's parameterBindings.
        // The latter also contains an implicit arguments binding, which may
        // receive a selected block function without being a formal parameter.
        let eligible_keys = candidates
            .into_iter()
            .filter(|function| {
                let name = function_name(interner, function, None);
                !lexical_names.contains(&name) && !blocked_parameter_names.contains(&name)
            })
            .map(function_declaration_key)
            .collect::<BTreeSet<_>>();

        self.collect_annex_b_nested_items(
            owner_id,
            items,
            &eligible_keys,
            interner,
            AnnexBDirectFunctionCollection::Skip,
        );
    }

    fn collect_annex_b_nested_items(
        &mut self,
        owner_id: &str,
        items: &'a [StatementListItem],
        eligible_keys: &BTreeSet<String>,
        interner: &Interner,
        direct_function_collection: AnnexBDirectFunctionCollection,
    ) {
        match direct_function_collection {
            AnnexBDirectFunctionCollection::Skip => {}
            AnnexBDirectFunctionCollection::Record => {
                let direct_functions = items
                    .iter()
                    // A storage plan also reserves a named environment cell.
                    // Label chains have no Block lexical declaration to own it.
                    .filter_map(lexically_scoped_statement_list_item_function_declaration)
                    .collect::<Vec<_>>();
                self.record_annex_b_direct_functions(
                    owner_id,
                    &direct_functions,
                    eligible_keys,
                    interner,
                );
            }
        }

        for item in items {
            if let StatementListItem::Statement(statement) = item {
                self.collect_annex_b_nested_statement(owner_id, statement, eligible_keys, interner);
            }
        }
    }

    fn record_annex_b_direct_functions(
        &mut self,
        owner_id: &str,
        functions: &[&'a FunctionDeclaration],
        eligible_keys: &BTreeSet<String>,
        interner: &Interner,
    ) {
        let mut last_function_by_name = BTreeMap::new();
        for function in functions {
            last_function_by_name.insert(function_name(interner, function, None), *function);
        }
        for function in functions {
            let key = function_declaration_key(function);
            let copy_to_variable_environment = eligible_keys.contains(&key);
            let source_name = function_name(interner, function, None);
            let last_function = last_function_by_name
                .get(&source_name)
                .expect("direct function group must contain its last declaration");
            self.annex_b_function_plans.insert(
                key,
                AnnexBFunctionPlan {
                    owner_id: owner_id.to_string(),
                    source_name: source_name.clone(),
                    block_storage_name: annex_b_block_storage_name(last_function, &source_name),
                    copy_to_variable_environment,
                },
            );
        }
    }

    fn remove_ineligible_block_function_owner_bindings(
        &self,
        owner_id: &str,
        bindings: &mut BTreeSet<String>,
        independent_owner_binding_names: BTreeSet<String>,
    ) {
        let block_function_names = self
            .annex_b_function_plans
            .values()
            .filter(|plan| plan.owner_id == owner_id)
            .map(|plan| plan.source_name.clone())
            .collect::<BTreeSet<_>>();

        for name in block_function_names {
            let has_variable_environment_copy = self.annex_b_function_plans.values().any(|plan| {
                plan.owner_id == owner_id
                    && plan.source_name == name
                    && plan.copy_to_variable_environment
            });
            // FDI's Annex B insertion excludes `arguments` from new var
            // creation, but still performs its evaluation-time assignment.
            // Without an independent declaration, keep that name absent from
            // static function/body cells and resolve it through named records.
            let creates_static_binding = has_variable_environment_copy
                && (owner_id == SCRIPT_OWNER_ID
                    || name != "arguments"
                    || independent_owner_binding_names.contains(&name));
            if creates_static_binding {
                bindings.insert(name);
            } else if !independent_owner_binding_names.contains(&name) {
                bindings.remove(&name);
            }
        }
    }

    fn collect_independent_owner_binding_names(
        &self,
        interner: &Interner,
        parameters: &[FormalParameter],
        items: &'a [StatementListItem],
        root_functions: &[PendingFunction<'a>],
    ) -> BTreeSet<String> {
        let mut names = root_functions
            .iter()
            .map(|function| function.name.clone())
            .collect::<BTreeSet<_>>();
        for parameter in parameters {
            let mut parameter_names = Vec::new();
            collect_binding_names(
                interner,
                parameter.variable().binding(),
                &mut parameter_names,
            );
            names.extend(parameter_names);
        }
        self.collect_independent_owner_bindings_from_items(interner, items, true, &mut names);
        names
    }

    fn collect_independent_owner_bindings_from_items(
        &self,
        interner: &Interner,
        items: &'a [StatementListItem],
        owner_level: bool,
        names: &mut BTreeSet<String>,
    ) {
        for item in items {
            match item {
                StatementListItem::Declaration(declaration) if owner_level => {
                    match declaration.as_ref() {
                        Declaration::Lexical(lexical) => {
                            self.collect_root_lexical_declaration_bindings(
                                interner, lexical, names,
                            );
                        }
                        Declaration::ClassDeclaration(class) => {
                            names.insert(interner.resolve_expect(class.name().sym()).to_string());
                        }
                        Declaration::FunctionDeclaration(function) => {
                            names.insert(function_name(interner, function, None));
                        }
                        _ => {}
                    }
                }
                StatementListItem::Statement(statement) => {
                    self.collect_independent_owner_bindings_from_statement(
                        interner, statement, names,
                    );
                }
                StatementListItem::Declaration(_) => {}
            }
        }
    }

    fn collect_independent_owner_bindings_from_statement(
        &self,
        interner: &Interner,
        statement: &'a Statement,
        names: &mut BTreeSet<String>,
    ) {
        match statement {
            Statement::Block(block) => self.collect_independent_owner_bindings_from_items(
                interner,
                block.statement_list().statements(),
                false,
                names,
            ),
            Statement::If(statement) => {
                self.collect_independent_owner_bindings_from_statement(
                    interner,
                    statement.body(),
                    names,
                );
                if let Some(else_node) = statement.else_node() {
                    self.collect_independent_owner_bindings_from_statement(
                        interner, else_node, names,
                    );
                }
            }
            Statement::WhileLoop(statement) => self
                .collect_independent_owner_bindings_from_statement(
                    interner,
                    statement.body(),
                    names,
                ),
            Statement::DoWhileLoop(statement) => self
                .collect_independent_owner_bindings_from_statement(
                    interner,
                    statement.body(),
                    names,
                ),
            Statement::ForLoop(statement) => {
                if let Some(ForLoopInitializer::Var(var)) = statement.init() {
                    self.collect_var_declaration_bindings(interner, var, names);
                }
                self.collect_independent_owner_bindings_from_statement(
                    interner,
                    statement.body(),
                    names,
                );
            }
            Statement::ForInLoop(statement) => {
                if let IterableLoopInitializer::Var(variable) = statement.initializer() {
                    if let Some(bound_names) = supported_bound_names(interner, variable.binding()) {
                        names.extend(bound_names.into_iter().map(|bound| bound.source_name));
                    }
                }
                self.collect_independent_owner_bindings_from_statement(
                    interner,
                    statement.body(),
                    names,
                );
            }
            Statement::ForOfLoop(statement) => {
                if let IterableLoopInitializer::Var(variable) = statement.initializer() {
                    if let Some(bound_names) = supported_bound_names(interner, variable.binding()) {
                        names.extend(bound_names.into_iter().map(|bound| bound.source_name));
                    }
                }
                self.collect_independent_owner_bindings_from_statement(
                    interner,
                    statement.body(),
                    names,
                );
            }
            Statement::Switch(statement) => {
                for case in statement.cases() {
                    self.collect_independent_owner_bindings_from_items(
                        interner,
                        case.body().statements(),
                        false,
                        names,
                    );
                }
            }
            Statement::Labelled(statement) => {
                if let Some(statement) = labelled_base_statement(statement) {
                    self.collect_independent_owner_bindings_from_statement(
                        interner, statement, names,
                    );
                }
            }
            Statement::Try(statement) => {
                self.collect_independent_owner_bindings_from_items(
                    interner,
                    statement.block().statement_list().statements(),
                    false,
                    names,
                );
                if let Some(catch) = statement.catch() {
                    self.collect_independent_owner_bindings_from_items(
                        interner,
                        catch.block().statement_list().statements(),
                        false,
                        names,
                    );
                }
                if let Some(finally) = statement.finally() {
                    self.collect_independent_owner_bindings_from_items(
                        interner,
                        finally.block().statement_list().statements(),
                        false,
                        names,
                    );
                }
            }
            Statement::Var(var) => self.collect_var_declaration_bindings(interner, var, names),
            Statement::With(statement) => self.collect_independent_owner_bindings_from_statement(
                interner,
                statement.statement(),
                names,
            ),
            Statement::Expression(_)
            | Statement::Empty
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Debugger
            | Statement::Return(_)
            | Statement::Throw(_) => {}
        }
    }

    fn collect_annex_b_nested_statement(
        &mut self,
        owner_id: &str,
        statement: &'a Statement,
        eligible_keys: &BTreeSet<String>,
        interner: &Interner,
    ) {
        match statement {
            Statement::Block(block) => self.collect_annex_b_nested_items(
                owner_id,
                block.statement_list().statements(),
                eligible_keys,
                interner,
                AnnexBDirectFunctionCollection::Record,
            ),
            Statement::If(statement) => {
                self.collect_annex_b_nested_statement(
                    owner_id,
                    statement.body(),
                    eligible_keys,
                    interner,
                );
                if let Some(else_node) = statement.else_node() {
                    self.collect_annex_b_nested_statement(
                        owner_id,
                        else_node,
                        eligible_keys,
                        interner,
                    );
                }
            }
            Statement::WhileLoop(statement) => self.collect_annex_b_nested_statement(
                owner_id,
                statement.body(),
                eligible_keys,
                interner,
            ),
            Statement::DoWhileLoop(statement) => self.collect_annex_b_nested_statement(
                owner_id,
                statement.body(),
                eligible_keys,
                interner,
            ),
            Statement::ForLoop(statement) => self.collect_annex_b_nested_statement(
                owner_id,
                statement.body(),
                eligible_keys,
                interner,
            ),
            Statement::ForInLoop(statement) => self.collect_annex_b_nested_statement(
                owner_id,
                statement.body(),
                eligible_keys,
                interner,
            ),
            Statement::ForOfLoop(statement) => self.collect_annex_b_nested_statement(
                owner_id,
                statement.body(),
                eligible_keys,
                interner,
            ),
            Statement::Switch(statement) => {
                let direct_functions = statement
                    .cases()
                    .iter()
                    .flat_map(|case| case.body().statements())
                    // Use the same lexical inventory as CaseBlock initialization.
                    .filter_map(lexically_scoped_statement_list_item_function_declaration)
                    .collect::<Vec<_>>();
                self.record_annex_b_direct_functions(
                    owner_id,
                    &direct_functions,
                    eligible_keys,
                    interner,
                );
                for case in statement.cases() {
                    self.collect_annex_b_nested_items(
                        owner_id,
                        case.body().statements(),
                        eligible_keys,
                        interner,
                        AnnexBDirectFunctionCollection::Skip,
                    );
                }
            }
            Statement::Labelled(statement) => {
                if let Some(statement) = labelled_base_statement(statement) {
                    self.collect_annex_b_nested_statement(
                        owner_id,
                        statement,
                        eligible_keys,
                        interner,
                    );
                }
            }
            Statement::Try(statement) => {
                self.collect_annex_b_nested_items(
                    owner_id,
                    statement.block().statement_list().statements(),
                    eligible_keys,
                    interner,
                    AnnexBDirectFunctionCollection::Record,
                );
                if let Some(catch) = statement.catch() {
                    self.collect_annex_b_nested_items(
                        owner_id,
                        catch.block().statement_list().statements(),
                        eligible_keys,
                        interner,
                        AnnexBDirectFunctionCollection::Record,
                    );
                }
                if let Some(finally) = statement.finally() {
                    self.collect_annex_b_nested_items(
                        owner_id,
                        finally.block().statement_list().statements(),
                        eligible_keys,
                        interner,
                        AnnexBDirectFunctionCollection::Record,
                    );
                }
            }
            Statement::With(statement) => self.collect_annex_b_nested_statement(
                owner_id,
                statement.statement(),
                eligible_keys,
                interner,
            ),
            Statement::Var(_)
            | Statement::Expression(_)
            | Statement::Empty
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Debugger
            | Statement::Return(_)
            | Statement::Throw(_) => {}
        }
    }

    fn collect_root_functions(
        &mut self,
        interner: &Interner,
        source_text: CompilerSourceText<'_>,
        items: &'a [StatementListItem],
        owner_strict: bool,
        capture_aliases: &BTreeMap<String, String>,
    ) -> Vec<PendingFunction<'a>> {
        let mut functions = Vec::new();
        for item in items {
            let async_generator = match item {
                StatementListItem::Declaration(declaration) => match declaration.as_ref() {
                    Declaration::AsyncGeneratorDeclaration(function) => Some(function),
                    _ => None,
                },
                StatementListItem::Statement(_) => None,
            };
            if let Some(function) = async_generator {
                let id = self.alloc_function_id();
                self.function_declaration_ids
                    .insert(async_generator_declaration_key(function), id.clone());
                functions.push(PendingFunction {
                    id,
                    name: interner.resolve_expect(function.name().sym()).to_string(),
                    to_string_representation: CallableToStringRepresentation::ExactSource(
                        async_generator_declaration_source_slice(function, source_text),
                    ),
                    protocol: FunctionProtocolIr::AsyncGenerator,
                    strict: owner_strict || function.body().strict(),
                    self_binding_name: None,
                    parameters: function.parameters(),
                    body: function.body(),
                    is_expression: false,
                    capture_aliases: capture_aliases.clone(),
                });
                continue;
            }

            let async_function = match item {
                StatementListItem::Declaration(declaration) => match declaration.as_ref() {
                    Declaration::AsyncFunctionDeclaration(function) => Some(function),
                    _ => None,
                },
                StatementListItem::Statement(_) => None,
            };
            if let Some(function) = async_function {
                let id = self.alloc_function_id();
                self.function_declaration_ids
                    .insert(async_function_declaration_key(function), id.clone());
                functions.push(PendingFunction {
                    id,
                    name: interner.resolve_expect(function.name().sym()).to_string(),
                    to_string_representation: CallableToStringRepresentation::ExactSource(
                        async_function_declaration_source_slice(function, source_text),
                    ),
                    protocol: FunctionProtocolIr::Async,
                    strict: owner_strict || function.body().strict(),
                    self_binding_name: None,
                    parameters: function.parameters(),
                    body: function.body(),
                    is_expression: false,
                    capture_aliases: capture_aliases.clone(),
                });
                continue;
            }

            let generator = match item {
                StatementListItem::Declaration(declaration) => match declaration.as_ref() {
                    Declaration::GeneratorDeclaration(function) => Some(function),
                    _ => None,
                },
                StatementListItem::Statement(_) => None,
            };
            if let Some(function) = generator.filter(|function| {
                generator_function_is_aot_supported(function.body(), function.parameters())
            }) {
                let id = self.alloc_function_id();
                self.function_declaration_ids
                    .insert(generator_declaration_key(function), id.clone());
                functions.push(PendingFunction {
                    id,
                    name: interner.resolve_expect(function.name().sym()).to_string(),
                    to_string_representation: CallableToStringRepresentation::ExactSource(
                        generator_declaration_source_slice(function, source_text),
                    ),
                    protocol: FunctionProtocolIr::Generator,
                    strict: owner_strict || function.body().strict(),
                    self_binding_name: None,
                    parameters: function.parameters(),
                    body: function.body(),
                    is_expression: false,
                    capture_aliases: capture_aliases.clone(),
                });
                continue;
            }

            let function = match item {
                StatementListItem::Declaration(declaration) => {
                    let Declaration::FunctionDeclaration(function) = declaration.as_ref() else {
                        continue;
                    };
                    function
                }
                StatementListItem::Statement(statement) => {
                    let Statement::Labelled(labelled) = statement.as_ref() else {
                        continue;
                    };
                    let Some(function) = labelled_function_declaration(labelled) else {
                        continue;
                    };
                    function
                }
            };
            let name = function_name(interner, function, None);
            let id = self.alloc_function_id();
            self.function_declaration_ids
                .insert(function_declaration_key(function), id.clone());
            functions.push(PendingFunction {
                id,
                name,
                to_string_representation: CallableToStringRepresentation::ExactSource(
                    function_source_slice(function, source_text),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallAndConstruct,
                strict: owner_strict || function.body().strict(),
                self_binding_name: None,
                parameters: function.parameters(),
                body: function.body(),
                is_expression: false,
                capture_aliases: capture_aliases.clone(),
            });
        }
        functions
    }

    fn collect_function_plan(
        &mut self,
        function: PendingFunction<'a>,
        parent_owner_id: String,
        definition_environment_cursor: EnvironmentCursor,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
    ) {
        let owner_id = function.id.clone();
        // Source function allocation always creates the immutable self-name
        // record. Count that physical parent even when no direct eval observes it.
        let definition_environment_cursor = if let Some(name) = &function.self_binding_name {
            self.register_lexical_environment_with_modes(
                &owner_id,
                EnvironmentKind::NamedFunctionExpression,
                definition_environment_cursor,
                BTreeSet::from([name.clone()]),
                BTreeMap::from([(name.clone(), BindingMode::Const)]),
            )
        } else {
            definition_environment_cursor
        };

        let root_functions = self.collect_root_functions(
            interner,
            source_text,
            function.body.statements(),
            function.strict,
            &function.capture_aliases,
        );
        let annex_b_candidates = annex_b_function_declarations(function.body);
        self.collect_owner_annex_b_function_plans(
            &owner_id,
            annex_b_candidates,
            lexically_declared_names(function.body),
            function.body.statements(),
            function.strict,
            function.parameters.as_ref(),
            interner,
        );
        // Only mapped arguments require every simple parameter to own a cell.
        // Strict parameters are retained later when capture, eval visibility,
        // a separate body environment or resumable execution actually needs
        // them, before physical environments and capture hops are finalized.
        let simple_parameter_names =
            if !function.strict && function.protocol.flavor() == FunctionFlavor::Ordinary {
                collect_simple_parameter_names(interner, function.parameters)
            } else {
                Vec::new()
            };
        let lexical_super_owner_role = function.protocol.lexical_super_owner_role();
        let parameter_environment_bindings = self.collect_parameter_environment_bindings(
            interner,
            function.parameters.as_ref(),
            function.self_binding_name.as_deref(),
            function.protocol.flavor() == FunctionFlavor::Ordinary,
            lexical_super_owner_role,
        );
        self.parameter_environment_bindings
            .insert(owner_id.clone(), parameter_environment_bindings);
        let mut owned_env_slots = BTreeMap::new();
        for (slot, name) in simple_parameter_names.iter().enumerate() {
            owned_env_slots.insert(name.clone(), slot as u32);
        }
        let root_bindings = {
            let mut bindings = self.collect_owner_bindings(
                interner,
                function.parameters.as_ref(),
                function.self_binding_name.as_deref(),
                function.protocol.flavor() == FunctionFlavor::Ordinary,
                function.protocol.flavor() == FunctionFlavor::Ordinary,
                function.protocol.flavor() == FunctionFlavor::Ordinary,
                function.body.statements(),
                &root_functions,
            );
            bindings.extend(
                self.annex_b_function_plans
                    .values()
                    .filter(|plan| plan.owner_id == owner_id)
                    .map(|plan| plan.block_storage_name.clone()),
            );
            self.remove_ineligible_block_function_owner_bindings(
                &owner_id,
                &mut bindings,
                self.collect_independent_owner_binding_names(
                    interner,
                    function.parameters.as_ref(),
                    function.body.statements(),
                    &root_functions,
                ),
            );
            if lexical_super_owner_role == LexicalSuperOwnerRole::HomeObject {
                bindings.insert(LEXICAL_HOME_OBJECT_NAME.to_string());
            }
            bindings
        };
        let mut activation_binding_modes = self.activation_binding_modes(
            interner,
            function.parameters.as_ref(),
            function.self_binding_name.as_deref(),
            function.body.statements(),
            &root_bindings,
        );
        self.apply_annex_b_variable_environment_binding_modes(
            &owner_id,
            &mut activation_binding_modes,
        );
        let activation_environment_id =
            self.register_activation_environment(&owner_id, root_bindings.clone());
        self.set_environment_binding_modes(activation_environment_id, activation_binding_modes);
        self.set_environment_parent_cursor(
            activation_environment_id,
            definition_environment_cursor.clone(),
        );
        self.owner_plans.insert(
            owner_id.clone(),
            OwnerPlan {
                parameter_external_refs: BTreeSet::new(),
                body_environment_id: None,
                parameter_eval_environment_id: None,
                parameter_names: eval_environment::parameter_names(
                    interner,
                    function.parameters.as_ref(),
                ),
                eval_invocation: if function.protocol.flavor() == FunctionFlavor::Arrow {
                    self.owner_plans[&parent_owner_id].eval_invocation
                } else {
                    match lexical_super_owner_role {
                        LexicalSuperOwnerRole::None => {
                            scriptw_front::EvalInvocationContext::Function
                        }
                        LexicalSuperOwnerRole::HomeObject => {
                            scriptw_front::EvalInvocationContext::Method
                        }
                        LexicalSuperOwnerRole::DerivedConstructorActivation => {
                            scriptw_front::EvalInvocationContext::DerivedConstructor
                        }
                    }
                },
                flavor: function.protocol.flavor(),
                execution_kind: function.protocol.execution_kind(),
                lexical_super_owner_role,
                strict: function.strict,
                parent_owner_id: Some(parent_owner_id.clone()),
                activation_environment_id,
                definition_environment_cursor: definition_environment_cursor.clone(),
                root_bindings,
                function_bindings: root_functions
                    .iter()
                    .map(|nested| (nested.name.clone(), nested.id.clone()))
                    .collect(),
                owned_env_slots,
                private_environment_id: self.current_private_environment_id(),
            },
        );
        self.scan_owner_items(
            &owner_id,
            function.parameters.as_ref(),
            function.body.statements(),
            interner,
            source_text,
            Some(function.name.as_str()),
            &function.capture_aliases,
        );
        self.function_plans.insert(
            owner_id.clone(),
            FunctionPlan {
                id: owner_id.clone(),
                name: function.name.clone(),
                to_string_representation: function.to_string_representation.clone(),
                protocol: function.protocol,
                strict: function.strict,
                self_binding_name: function.self_binding_name.clone(),
                parent_owner_id,
                parameters: function.parameters,
                body: function.body,
                is_expression: function.is_expression,
                root_functions: root_functions.clone(),
                captures: BTreeMap::new(),
                lexical_derived_activation_owner: None,
            },
        );
        for nested in root_functions {
            self.collect_function_plan(
                nested,
                owner_id.clone(),
                self.body_environment_cursor(&owner_id),
                interner,
                source_text,
            );
        }
        self.function_order.push(owner_id);
    }

    fn collect_owner_bindings(
        &self,
        interner: &Interner,
        params: &[FormalParameter],
        self_name: Option<&str>,
        has_own_this: bool,
        has_own_arguments: bool,
        has_own_new_target: bool,
        items: &'a [StatementListItem],
        root_functions: &[PendingFunction<'a>],
    ) -> BTreeSet<String> {
        let mut bindings = BTreeSet::new();
        if let Some(self_name) = self_name {
            bindings.insert(self_name.to_string());
        }
        if has_own_this {
            bindings.insert(LEXICAL_THIS_NAME.to_string());
        }
        if has_own_arguments {
            bindings.insert(LEXICAL_ARGUMENTS_NAME.to_string());
        }
        if has_own_new_target {
            bindings.insert(LEXICAL_NEW_TARGET_NAME.to_string());
        }
        for parameter in params {
            let mut parameter_names = Vec::new();
            collect_binding_names(
                interner,
                parameter.variable().binding(),
                &mut parameter_names,
            );
            bindings.extend(parameter_names);
        }
        for function in root_functions {
            bindings.insert(function.name.clone());
        }
        self.collect_owner_root_bindings_from_items(interner, items, &mut bindings);
        bindings
    }

    fn collect_parameter_environment_bindings(
        &self,
        interner: &Interner,
        params: &[FormalParameter],
        self_name: Option<&str>,
        has_own_this: bool,
        lexical_super_owner_role: LexicalSuperOwnerRole,
    ) -> BTreeSet<String> {
        let mut bindings = params
            .iter()
            .flat_map(|parameter| {
                let mut names = Vec::new();
                collect_binding_names(interner, parameter.variable().binding(), &mut names);
                names
            })
            .collect::<BTreeSet<_>>();
        if let Some(self_name) = self_name {
            bindings.insert(self_name.to_string());
        }
        if has_own_this {
            bindings.extend([
                LEXICAL_THIS_NAME.to_string(),
                LEXICAL_ARGUMENTS_NAME.to_string(),
                LEXICAL_NEW_TARGET_NAME.to_string(),
            ]);
        }
        match lexical_super_owner_role {
            LexicalSuperOwnerRole::None => {}
            LexicalSuperOwnerRole::HomeObject => {
                bindings.insert(LEXICAL_HOME_OBJECT_NAME.to_string());
            }
            LexicalSuperOwnerRole::DerivedConstructorActivation => {
                bindings.extend([
                    DERIVED_ACTIVATION_THIS_NAME.to_string(),
                    DERIVED_ACTIVATION_THIS_STATUS_NAME.to_string(),
                    DERIVED_ACTIVATION_NEW_TARGET_NAME.to_string(),
                    DERIVED_ACTIVATION_FUNCTION_NAME.to_string(),
                ]);
            }
        }
        bindings
    }

    fn activation_binding_modes(
        &self,
        interner: &Interner,
        params: &[FormalParameter],
        self_name: Option<&str>,
        items: &'a [StatementListItem],
        root_bindings: &BTreeSet<String>,
    ) -> BTreeMap<String, BindingMode> {
        let mut binding_modes = root_bindings
            .iter()
            .map(|name| (name.clone(), BindingMode::Let))
            .collect::<BTreeMap<_, _>>();
        if let Some(self_name) = self_name {
            binding_modes.insert(self_name.to_string(), BindingMode::Const);
        }
        for parameter in params {
            let mut names = Vec::new();
            collect_binding_names(interner, parameter.variable().binding(), &mut names);
            for name in names {
                binding_modes.insert(name, BindingMode::Let);
            }
        }
        self.collect_activation_binding_modes_from_items(interner, items, &mut binding_modes);
        binding_modes
    }

    fn apply_annex_b_variable_environment_binding_modes(
        &self,
        owner_id: &str,
        binding_modes: &mut BTreeMap<String, BindingMode>,
    ) {
        for plan in self
            .annex_b_function_plans
            .values()
            .filter(|plan| plan.owner_id == owner_id && plan.copy_to_variable_environment)
        {
            binding_modes.insert(plan.source_name.clone(), BindingMode::Var);
        }
    }

    fn collect_activation_binding_modes_from_items(
        &self,
        interner: &Interner,
        items: &'a [StatementListItem],
        binding_modes: &mut BTreeMap<String, BindingMode>,
    ) {
        for item in items {
            match item {
                StatementListItem::Statement(statement) => self
                    .collect_activation_binding_modes_from_statement(
                        interner,
                        statement,
                        binding_modes,
                    ),
                StatementListItem::Declaration(declaration) => match declaration.as_ref() {
                    Declaration::Lexical(lexical) => {
                        let mode = match lexical {
                            LexicalDeclaration::Let(_) => BindingMode::Let,
                            LexicalDeclaration::Const(_)
                            | LexicalDeclaration::Using(_)
                            | LexicalDeclaration::AwaitUsing(_) => BindingMode::Const,
                        };
                        let list = match lexical {
                            LexicalDeclaration::Let(list)
                            | LexicalDeclaration::Const(list)
                            | LexicalDeclaration::Using(list)
                            | LexicalDeclaration::AwaitUsing(list) => list,
                        };
                        for declarator in list.as_ref() {
                            let Some(bound_names) =
                                supported_bound_names(interner, declarator.binding())
                            else {
                                continue;
                            };
                            for bound in bound_names {
                                binding_modes.insert(bound.source_name, mode);
                            }
                        }
                    }
                    Declaration::ClassDeclaration(class) => {
                        binding_modes.insert(
                            interner.resolve_expect(class.name().sym()).to_string(),
                            BindingMode::Let,
                        );
                    }
                    Declaration::FunctionDeclaration(function) => {
                        binding_modes
                            .insert(function_name(interner, function, None), BindingMode::Let);
                    }
                    _ => {}
                },
            }
        }
    }

    fn collect_activation_binding_modes_from_statement(
        &self,
        interner: &Interner,
        statement: &'a Statement,
        binding_modes: &mut BTreeMap<String, BindingMode>,
    ) {
        match statement {
            Statement::Block(block) => self.collect_activation_var_binding_modes_from_items(
                interner,
                block.statement_list().statements(),
                binding_modes,
            ),
            Statement::If(if_statement) => {
                self.collect_activation_binding_modes_from_statement(
                    interner,
                    if_statement.body(),
                    binding_modes,
                );
                if let Some(else_node) = if_statement.else_node() {
                    self.collect_activation_binding_modes_from_statement(
                        interner,
                        else_node,
                        binding_modes,
                    );
                }
            }
            Statement::WhileLoop(while_loop) => self
                .collect_activation_binding_modes_from_statement(
                    interner,
                    while_loop.body(),
                    binding_modes,
                ),
            Statement::DoWhileLoop(do_while) => self
                .collect_activation_binding_modes_from_statement(
                    interner,
                    do_while.body(),
                    binding_modes,
                ),
            Statement::ForLoop(for_loop) => {
                if let Some(ForLoopInitializer::Var(declaration)) = for_loop.init() {
                    self.collect_var_binding_modes(interner, declaration, binding_modes);
                }
                self.collect_activation_binding_modes_from_statement(
                    interner,
                    for_loop.body(),
                    binding_modes,
                );
            }
            Statement::ForOfLoop(for_of) => {
                if let IterableLoopInitializer::Var(variable) = for_of.initializer() {
                    self.collect_variable_binding_modes(interner, variable, binding_modes);
                }
                self.collect_activation_binding_modes_from_statement(
                    interner,
                    for_of.body(),
                    binding_modes,
                );
            }
            Statement::ForInLoop(for_in) => {
                if let IterableLoopInitializer::Var(variable) = for_in.initializer() {
                    self.collect_variable_binding_modes(interner, variable, binding_modes);
                }
                self.collect_activation_binding_modes_from_statement(
                    interner,
                    for_in.body(),
                    binding_modes,
                );
            }
            Statement::Switch(switch) => {
                for case in switch.cases() {
                    self.collect_activation_var_binding_modes_from_items(
                        interner,
                        case.body().statements(),
                        binding_modes,
                    );
                }
            }
            Statement::Labelled(labelled) => {
                if let Some(function) = labelled_function_declaration(labelled) {
                    binding_modes.insert(function_name(interner, function, None), BindingMode::Let);
                } else if let Some(statement) = labelled_base_statement(labelled) {
                    self.collect_activation_binding_modes_from_statement(
                        interner,
                        statement,
                        binding_modes,
                    );
                }
            }
            Statement::Try(try_statement) => {
                self.collect_activation_var_binding_modes_from_items(
                    interner,
                    try_statement.block().statement_list().statements(),
                    binding_modes,
                );
                if let Some(catch) = try_statement.catch() {
                    self.collect_activation_var_binding_modes_from_items(
                        interner,
                        catch.block().statement_list().statements(),
                        binding_modes,
                    );
                }
                if let Some(finally_block) = try_statement.finally() {
                    self.collect_activation_var_binding_modes_from_items(
                        interner,
                        finally_block.block().statement_list().statements(),
                        binding_modes,
                    );
                }
            }
            Statement::Var(declaration) => {
                self.collect_var_binding_modes(interner, declaration, binding_modes)
            }
            Statement::With(with) => self.collect_activation_binding_modes_from_statement(
                interner,
                with.statement(),
                binding_modes,
            ),
            Statement::Expression(_)
            | Statement::Empty
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Debugger
            | Statement::Return(_)
            | Statement::Throw(_) => {}
        }
    }

    fn collect_activation_var_binding_modes_from_items(
        &self,
        interner: &Interner,
        items: &'a [StatementListItem],
        binding_modes: &mut BTreeMap<String, BindingMode>,
    ) {
        for item in items {
            if let StatementListItem::Statement(statement) = item {
                self.collect_activation_binding_modes_from_statement(
                    interner,
                    statement,
                    binding_modes,
                );
            }
        }
    }

    fn collect_var_binding_modes(
        &self,
        interner: &Interner,
        declaration: &'a VarDeclaration,
        binding_modes: &mut BTreeMap<String, BindingMode>,
    ) {
        for variable in declaration.0.as_ref() {
            self.collect_variable_binding_modes(interner, variable, binding_modes);
        }
    }

    fn collect_variable_binding_modes(
        &self,
        interner: &Interner,
        variable: &'a Variable,
        binding_modes: &mut BTreeMap<String, BindingMode>,
    ) {
        let Some(bound_names) = supported_bound_names(interner, variable.binding()) else {
            return;
        };
        for bound in bound_names {
            binding_modes.insert(bound.source_name, BindingMode::Var);
        }
    }

    fn collect_owner_root_bindings_from_items(
        &self,
        interner: &Interner,
        items: &'a [StatementListItem],
        bindings: &mut BTreeSet<String>,
    ) {
        for item in items {
            match item {
                StatementListItem::Statement(statement) => {
                    self.collect_owner_root_bindings_from_statement(interner, statement, bindings);
                }
                StatementListItem::Declaration(declaration) => match declaration.as_ref() {
                    Declaration::Lexical(lexical) => {
                        self.collect_root_lexical_declaration_bindings(interner, lexical, bindings);
                    }
                    Declaration::ClassDeclaration(class) => {
                        bindings.insert(interner.resolve_expect(class.name().sym()).to_string());
                    }
                    Declaration::FunctionDeclaration(function) => {
                        bindings.insert(function_name(interner, function, None));
                    }
                    _ => {}
                },
            }
        }
    }

    fn collect_owner_root_bindings_from_statement(
        &self,
        interner: &Interner,
        statement: &'a Statement,
        bindings: &mut BTreeSet<String>,
    ) {
        match statement {
            Statement::Block(block) => self.collect_scoped_bindings_from_items(
                interner,
                block.statement_list().statements(),
                bindings,
            ),
            Statement::If(if_statement) => {
                self.collect_owner_root_bindings_from_statement(
                    interner,
                    if_statement.body(),
                    bindings,
                );
                if let Some(else_node) = if_statement.else_node() {
                    self.collect_owner_root_bindings_from_statement(interner, else_node, bindings);
                }
            }
            Statement::WhileLoop(while_loop) => {
                self.collect_owner_root_bindings_from_statement(
                    interner,
                    while_loop.body(),
                    bindings,
                );
            }
            Statement::DoWhileLoop(do_while) => {
                self.collect_owner_root_bindings_from_statement(
                    interner,
                    do_while.body(),
                    bindings,
                );
            }
            Statement::ForLoop(for_loop) => {
                if let Some(init) = for_loop.init() {
                    match init {
                        ForLoopInitializer::Var(var) => {
                            self.collect_var_declaration_bindings(interner, var, bindings);
                        }
                        ForLoopInitializer::Lexical(lexical) => {
                            let declaration = lexical.declaration();
                            let list = match declaration {
                                LexicalDeclaration::Let(list)
                                | LexicalDeclaration::Const(list)
                                | LexicalDeclaration::Using(list)
                                | LexicalDeclaration::AwaitUsing(list) => list,
                            };
                            for declarator in list.as_ref() {
                                let Some(bound_names) =
                                    supported_bound_names(interner, declarator.binding())
                                else {
                                    continue;
                                };
                                for bound in bound_names {
                                    bindings.insert(scoped_lexical_binding_storage_name(
                                        &bound.source_name,
                                        bound.span,
                                    ));
                                }
                            }
                        }
                        ForLoopInitializer::Expression(_) => {}
                    }
                }
                self.collect_owner_root_bindings_from_statement(
                    interner,
                    for_loop.body(),
                    bindings,
                );
            }
            Statement::ForOfLoop(for_of) => {
                match for_of.initializer() {
                    IterableLoopInitializer::Let(binding)
                    | IterableLoopInitializer::Const(binding)
                    | IterableLoopInitializer::Using(binding)
                    | IterableLoopInitializer::AwaitUsing(binding) => {
                        if let Some(bound_names) = supported_bound_names(interner, binding) {
                            for bound in bound_names {
                                bindings.insert(
                                    TdzPlaceholderName::for_source_name(&bound.source_name)
                                        .into_string(),
                                );
                            }
                        }
                    }
                    _ => {}
                }
                match for_of.initializer() {
                    IterableLoopInitializer::Let(binding)
                    | IterableLoopInitializer::Const(binding)
                    | IterableLoopInitializer::Using(binding)
                    | IterableLoopInitializer::AwaitUsing(binding) => {
                        if let Some(bound_names) = supported_bound_names(interner, binding) {
                            for bound in bound_names {
                                bindings.insert(for_of_loop_binding_storage_name(
                                    for_of,
                                    &bound.source_name,
                                ));
                            }
                        }
                    }
                    IterableLoopInitializer::Var(variable) => {
                        if let Some(bound_names) =
                            supported_bound_names(interner, variable.binding())
                        {
                            bindings.extend(bound_names.into_iter().map(|bound| bound.source_name));
                        }
                    }
                    _ => {}
                }
                self.collect_owner_root_bindings_from_statement(interner, for_of.body(), bindings);
            }
            Statement::ForInLoop(for_in) => {
                match for_in.initializer() {
                    IterableLoopInitializer::Let(binding)
                    | IterableLoopInitializer::Const(binding) => {
                        if let Some(bound_names) = supported_bound_names(interner, binding) {
                            for bound in bound_names {
                                bindings.insert(
                                    TdzPlaceholderName::for_source_name(&bound.source_name)
                                        .into_string(),
                                );
                            }
                        }
                    }
                    _ => {}
                }
                match for_in.initializer() {
                    IterableLoopInitializer::Let(binding)
                    | IterableLoopInitializer::Const(binding) => {
                        if let Some(bound_names) = supported_bound_names(interner, binding) {
                            for bound in bound_names {
                                bindings.insert(for_in_loop_binding_storage_name(
                                    for_in,
                                    &bound.source_name,
                                ));
                            }
                        }
                    }
                    IterableLoopInitializer::Var(variable) => {
                        if let Some(bound_names) =
                            supported_bound_names(interner, variable.binding())
                        {
                            bindings.extend(bound_names.into_iter().map(|bound| bound.source_name));
                        }
                    }
                    _ => {}
                }
                self.collect_owner_root_bindings_from_statement(interner, for_in.body(), bindings);
            }
            Statement::Switch(switch) => {
                for case in switch.cases() {
                    self.collect_scoped_bindings_from_items(
                        interner,
                        case.body().statements(),
                        bindings,
                    );
                }
            }
            Statement::Labelled(labelled) => {
                if let Some(function) = labelled_function_declaration(labelled) {
                    bindings.insert(function_name(interner, function, None));
                } else if let Some(statement) = labelled_base_statement(labelled) {
                    self.collect_owner_root_bindings_from_statement(interner, statement, bindings);
                }
            }
            Statement::Try(try_statement) => {
                self.collect_scoped_bindings_from_items(
                    interner,
                    try_statement.block().statement_list().statements(),
                    bindings,
                );
                if let Some(catch) = try_statement.catch() {
                    if let Some(bound_names) = catch
                        .parameter()
                        .and_then(|binding| supported_bound_names(interner, binding))
                    {
                        for bound in bound_names {
                            bindings.insert(scoped_lexical_binding_storage_name(
                                &bound.source_name,
                                bound.span,
                            ));
                        }
                    }
                    self.collect_scoped_bindings_from_items(
                        interner,
                        catch.block().statement_list().statements(),
                        bindings,
                    );
                }
                if let Some(finally_block) = try_statement.finally() {
                    self.collect_scoped_bindings_from_items(
                        interner,
                        finally_block.block().statement_list().statements(),
                        bindings,
                    );
                }
            }
            Statement::Var(var) => self.collect_var_declaration_bindings(interner, var, bindings),
            Statement::With(with) => self.collect_owner_root_bindings_from_statement(
                interner,
                with.statement(),
                bindings,
            ),
            Statement::Expression(_)
            | Statement::Empty
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Debugger
            | Statement::Return(_)
            | Statement::Throw(_) => {}
        }
    }

    fn collect_scoped_bindings_from_items(
        &self,
        interner: &Interner,
        items: &'a [StatementListItem],
        bindings: &mut BTreeSet<String>,
    ) {
        let aliases = self.scoped_capture_aliases(interner, items, &BTreeMap::new());
        bindings.extend(aliases.into_values());

        for item in items {
            if let StatementListItem::Statement(statement) = item {
                self.collect_scoped_bindings_from_statement(interner, statement, bindings);
            }
        }
    }

    fn collect_scoped_bindings_from_statement(
        &self,
        interner: &Interner,
        statement: &'a Statement,
        bindings: &mut BTreeSet<String>,
    ) {
        match statement {
            Statement::Block(block) => self.collect_scoped_bindings_from_items(
                interner,
                block.statement_list().statements(),
                bindings,
            ),
            Statement::If(if_statement) => {
                self.collect_scoped_bindings_from_statement(
                    interner,
                    if_statement.body(),
                    bindings,
                );
                if let Some(else_node) = if_statement.else_node() {
                    self.collect_scoped_bindings_from_statement(interner, else_node, bindings);
                }
            }
            Statement::WhileLoop(while_loop) => {
                self.collect_scoped_bindings_from_statement(interner, while_loop.body(), bindings)
            }
            Statement::DoWhileLoop(do_while) => {
                self.collect_scoped_bindings_from_statement(interner, do_while.body(), bindings)
            }
            Statement::ForLoop(for_loop) => {
                if let Some(init) = for_loop.init() {
                    match init {
                        ForLoopInitializer::Var(var) => {
                            self.collect_var_declaration_bindings(interner, var, bindings);
                        }
                        ForLoopInitializer::Lexical(lexical) => {
                            let declaration = lexical.declaration();
                            let list = match declaration {
                                LexicalDeclaration::Let(list)
                                | LexicalDeclaration::Const(list)
                                | LexicalDeclaration::Using(list)
                                | LexicalDeclaration::AwaitUsing(list) => list,
                            };
                            for declarator in list.as_ref() {
                                let Some(bound_names) =
                                    supported_bound_names(interner, declarator.binding())
                                else {
                                    continue;
                                };
                                for bound in bound_names {
                                    bindings.insert(scoped_lexical_binding_storage_name(
                                        &bound.source_name,
                                        bound.span,
                                    ));
                                }
                            }
                        }
                        ForLoopInitializer::Expression(_) => {}
                    }
                }
                self.collect_scoped_bindings_from_statement(interner, for_loop.body(), bindings);
            }
            Statement::ForOfLoop(for_of) => {
                match for_of.initializer() {
                    IterableLoopInitializer::Var(variable) => {
                        if let Some(bound_names) =
                            supported_bound_names(interner, variable.binding())
                        {
                            bindings.extend(bound_names.into_iter().map(|bound| bound.source_name));
                        }
                    }
                    IterableLoopInitializer::Let(binding)
                    | IterableLoopInitializer::Const(binding)
                    | IterableLoopInitializer::Using(binding)
                    | IterableLoopInitializer::AwaitUsing(binding) => {
                        if let Some(bound_names) = supported_bound_names(interner, binding) {
                            for bound in bound_names {
                                bindings.insert(
                                    TdzPlaceholderName::for_source_name(&bound.source_name)
                                        .into_string(),
                                );
                                bindings.insert(for_of_loop_binding_storage_name(
                                    for_of,
                                    &bound.source_name,
                                ));
                            }
                        }
                    }
                    _ => {}
                }
                self.collect_scoped_bindings_from_statement(interner, for_of.body(), bindings);
            }
            Statement::ForInLoop(_) => {
                // A lexical head nested in a block still belongs to this
                // invocation. Collect its original TDZ and iteration names so
                // uncaptured resumable keys receive activation cells; captured
                // keys retain their separately discovered lexical records.
                self.collect_owner_root_bindings_from_statement(interner, statement, bindings);
            }
            Statement::Switch(switch) => {
                for case in switch.cases() {
                    self.collect_scoped_bindings_from_items(
                        interner,
                        case.body().statements(),
                        bindings,
                    );
                }
            }
            Statement::Labelled(labelled) => {
                if let Some(statement) = labelled_base_statement(labelled) {
                    self.collect_scoped_bindings_from_statement(interner, statement, bindings);
                }
            }
            Statement::Try(try_statement) => {
                self.collect_scoped_bindings_from_items(
                    interner,
                    try_statement.block().statement_list().statements(),
                    bindings,
                );
                if let Some(catch) = try_statement.catch() {
                    if let Some(bound_names) = catch
                        .parameter()
                        .and_then(|binding| supported_bound_names(interner, binding))
                    {
                        for bound in bound_names {
                            bindings.insert(scoped_lexical_binding_storage_name(
                                &bound.source_name,
                                bound.span,
                            ));
                        }
                    }
                    self.collect_scoped_bindings_from_items(
                        interner,
                        catch.block().statement_list().statements(),
                        bindings,
                    );
                }
                if let Some(finally_block) = try_statement.finally() {
                    self.collect_scoped_bindings_from_items(
                        interner,
                        finally_block.block().statement_list().statements(),
                        bindings,
                    );
                }
            }
            Statement::Var(var) => self.collect_var_declaration_bindings(interner, var, bindings),
            Statement::With(with) => {
                self.collect_scoped_bindings_from_statement(interner, with.statement(), bindings)
            }
            Statement::Expression(_)
            | Statement::Empty
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Debugger
            | Statement::Return(_)
            | Statement::Throw(_) => {}
        }
    }

    fn collect_var_declaration_bindings(
        &self,
        interner: &Interner,
        declaration: &'a VarDeclaration,
        bindings: &mut BTreeSet<String>,
    ) {
        for declarator in declaration.0.as_ref() {
            if let Some(bound_names) = supported_bound_names(interner, declarator.binding()) {
                bindings.extend(bound_names.into_iter().map(|bound| bound.source_name));
            }
        }
    }

    fn collect_root_lexical_declaration_bindings(
        &self,
        interner: &Interner,
        declaration: &'a LexicalDeclaration,
        bindings: &mut BTreeSet<String>,
    ) {
        let list = match declaration {
            LexicalDeclaration::Let(list)
            | LexicalDeclaration::Const(list)
            | LexicalDeclaration::Using(list)
            | LexicalDeclaration::AwaitUsing(list) => list,
        };
        for declarator in list.as_ref() {
            if let Some(bound_names) = supported_bound_names(interner, declarator.binding()) {
                bindings.extend(bound_names.into_iter().map(|bound| bound.source_name));
            }
        }
    }

    fn scoped_hoistable_binding(
        &self,
        interner: &Interner,
        function: BlockHoistableDeclaration<'_>,
    ) -> (String, String) {
        let source_name = function.source_name(interner);
        let storage_name = function
            .ordinary()
            .and_then(|function| {
                self.annex_b_function_plans
                    .get(&function_declaration_key(function))
            })
            .map(|plan| plan.block_storage_name.clone())
            .unwrap_or_else(|| {
                scoped_lexical_binding_storage_name(&source_name, function.name_span())
            });
        (source_name, storage_name)
    }

    fn scoped_capture_aliases(
        &self,
        interner: &Interner,
        items: &'a [StatementListItem],
        capture_aliases: &BTreeMap<String, String>,
    ) -> BTreeMap<String, String> {
        let mut aliases = capture_aliases.clone();
        for item in items {
            if let Some(function) = BlockHoistableDeclaration::from_item(item) {
                let (source_name, storage_name) = self.scoped_hoistable_binding(interner, function);
                aliases.insert(source_name, storage_name);
                continue;
            }
            if let StatementListItem::Declaration(declaration) = item {
                match declaration.as_ref() {
                    Declaration::Lexical(lexical) => {
                        self.add_scoped_lexical_aliases(interner, lexical, &mut aliases)
                    }
                    Declaration::ClassDeclaration(class) => {
                        let source_name = interner.resolve_expect(class.name().sym()).to_string();
                        aliases.insert(
                            source_name.clone(),
                            scoped_lexical_binding_storage_name(&source_name, class.name().span()),
                        );
                    }
                    Declaration::FunctionDeclaration(_)
                    | Declaration::GeneratorDeclaration(_)
                    | Declaration::AsyncFunctionDeclaration(_)
                    | Declaration::AsyncGeneratorDeclaration(_) => {}
                }
            }
        }
        aliases
    }

    fn scoped_environment_binding_storage_names(
        &self,
        interner: &Interner,
        items: &'a [StatementListItem],
    ) -> BTreeSet<String> {
        self.scoped_capture_aliases(interner, items, &BTreeMap::new())
            .into_values()
            .collect()
    }

    fn scoped_environment_binding_modes(
        &self,
        interner: &Interner,
        items: &'a [StatementListItem],
    ) -> BTreeMap<String, BindingMode> {
        let mut binding_modes = BTreeMap::new();
        for item in items {
            if let Some(function) = BlockHoistableDeclaration::from_item(item) {
                let (_, storage_name) = self.scoped_hoistable_binding(interner, function);
                binding_modes.insert(storage_name, BindingMode::Let);
                continue;
            }
            if let StatementListItem::Declaration(declaration) = item {
                match declaration.as_ref() {
                    Declaration::Lexical(lexical) => {
                        let (mode, variables) = match lexical {
                            LexicalDeclaration::Let(variables) => (BindingMode::Let, variables),
                            LexicalDeclaration::Const(variables)
                            | LexicalDeclaration::Using(variables)
                            | LexicalDeclaration::AwaitUsing(variables) => {
                                (BindingMode::Const, variables)
                            }
                        };
                        for variable in variables.as_ref() {
                            let Some(bound_names) =
                                supported_bound_names(interner, variable.binding())
                            else {
                                continue;
                            };
                            for bound in bound_names {
                                binding_modes.insert(
                                    scoped_lexical_binding_storage_name(
                                        &bound.source_name,
                                        bound.span,
                                    ),
                                    mode,
                                );
                            }
                        }
                    }
                    Declaration::ClassDeclaration(class) => {
                        let source_name = interner.resolve_expect(class.name().sym()).to_string();
                        binding_modes.insert(
                            scoped_lexical_binding_storage_name(&source_name, class.name().span()),
                            BindingMode::Let,
                        );
                    }
                    Declaration::FunctionDeclaration(_)
                    | Declaration::GeneratorDeclaration(_)
                    | Declaration::AsyncFunctionDeclaration(_)
                    | Declaration::AsyncGeneratorDeclaration(_) => {}
                }
            }
        }
        binding_modes
    }

    fn classic_for_lexical_binding_storage_names(
        &self,
        interner: &Interner,
        for_loop: &'a ForLoop,
    ) -> BTreeSet<String> {
        let Some(ForLoopInitializer::Lexical(lexical)) = for_loop.init() else {
            return BTreeSet::new();
        };
        let list = match lexical.declaration() {
            LexicalDeclaration::Let(list)
            | LexicalDeclaration::Const(list)
            | LexicalDeclaration::Using(list)
            | LexicalDeclaration::AwaitUsing(list) => list,
        };
        list.as_ref()
            .iter()
            .filter_map(|declarator| supported_bound_names(interner, declarator.binding()))
            .flatten()
            .map(|bound| scoped_lexical_binding_storage_name(&bound.source_name, bound.span))
            .collect()
    }

    fn classic_for_lexical_binding_modes(
        &self,
        interner: &Interner,
        for_loop: &'a ForLoop,
    ) -> BTreeMap<String, BindingMode> {
        let Some(ForLoopInitializer::Lexical(lexical)) = for_loop.init() else {
            return BTreeMap::new();
        };
        let (mode, list) = match lexical.declaration() {
            LexicalDeclaration::Let(list) => (BindingMode::Let, list),
            LexicalDeclaration::Const(list)
            | LexicalDeclaration::Using(list)
            | LexicalDeclaration::AwaitUsing(list) => (BindingMode::Const, list),
        };
        list.as_ref()
            .iter()
            .filter_map(|declarator| supported_bound_names(interner, declarator.binding()))
            .flatten()
            .map(|bound| {
                (
                    scoped_lexical_binding_storage_name(&bound.source_name, bound.span),
                    mode,
                )
            })
            .collect()
    }

    fn for_of_tdz_binding_storage_names(
        &self,
        interner: &Interner,
        for_of: &'a ForOfLoop,
    ) -> BTreeSet<String> {
        match for_of.initializer() {
            IterableLoopInitializer::Let(binding)
            | IterableLoopInitializer::Const(binding)
            | IterableLoopInitializer::Using(binding)
            | IterableLoopInitializer::AwaitUsing(binding) => {
                supported_bound_names(interner, binding)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|bound| {
                        TdzPlaceholderName::for_source_name(&bound.source_name).into_string()
                    })
                    .collect()
            }
            _ => BTreeSet::new(),
        }
    }

    fn for_of_tdz_binding_modes(
        &self,
        interner: &Interner,
        for_of: &'a ForOfLoop,
    ) -> BTreeMap<String, BindingMode> {
        let (mode, binding) = match for_of.initializer() {
            IterableLoopInitializer::Let(binding) => (BindingMode::Let, binding),
            IterableLoopInitializer::Const(binding)
            | IterableLoopInitializer::Using(binding)
            | IterableLoopInitializer::AwaitUsing(binding) => (BindingMode::Const, binding),
            _ => return BTreeMap::new(),
        };
        supported_bound_names(interner, binding)
            .unwrap_or_default()
            .into_iter()
            .map(|bound| {
                (
                    TdzPlaceholderName::for_source_name(&bound.source_name).into_string(),
                    mode,
                )
            })
            .collect()
    }

    fn for_of_iteration_binding_storage_names(
        &self,
        interner: &Interner,
        for_of: &'a ForOfLoop,
    ) -> BTreeSet<String> {
        match for_of.initializer() {
            IterableLoopInitializer::Let(binding)
            | IterableLoopInitializer::Const(binding)
            | IterableLoopInitializer::Using(binding)
            | IterableLoopInitializer::AwaitUsing(binding) => {
                supported_bound_names(interner, binding)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|bound| for_of_loop_binding_storage_name(for_of, &bound.source_name))
                    .collect()
            }
            _ => BTreeSet::new(),
        }
    }

    fn for_of_iteration_binding_modes(
        &self,
        interner: &Interner,
        for_of: &'a ForOfLoop,
    ) -> BTreeMap<String, BindingMode> {
        let (mode, binding) = match for_of.initializer() {
            IterableLoopInitializer::Let(binding) => (BindingMode::Let, binding),
            IterableLoopInitializer::Const(binding)
            | IterableLoopInitializer::Using(binding)
            | IterableLoopInitializer::AwaitUsing(binding) => (BindingMode::Const, binding),
            _ => return BTreeMap::new(),
        };
        supported_bound_names(interner, binding)
            .unwrap_or_default()
            .into_iter()
            .map(|bound| {
                (
                    for_of_loop_binding_storage_name(for_of, &bound.source_name),
                    mode,
                )
            })
            .collect()
    }

    fn for_in_tdz_binding_storage_names(
        &self,
        interner: &Interner,
        for_in: &'a boa_ast::statement::iteration::ForInLoop,
    ) -> BTreeSet<String> {
        match for_in.initializer() {
            IterableLoopInitializer::Let(binding) | IterableLoopInitializer::Const(binding) => {
                supported_bound_names(interner, binding)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|bound| {
                        TdzPlaceholderName::for_source_name(&bound.source_name).into_string()
                    })
                    .collect()
            }
            _ => BTreeSet::new(),
        }
    }

    fn for_in_tdz_binding_modes(
        &self,
        interner: &Interner,
        for_in: &'a boa_ast::statement::iteration::ForInLoop,
    ) -> BTreeMap<String, BindingMode> {
        let (mode, binding) = match for_in.initializer() {
            IterableLoopInitializer::Let(binding) => (BindingMode::Let, binding),
            IterableLoopInitializer::Const(binding) => (BindingMode::Const, binding),
            _ => return BTreeMap::new(),
        };
        supported_bound_names(interner, binding)
            .unwrap_or_default()
            .into_iter()
            .map(|bound| {
                (
                    TdzPlaceholderName::for_source_name(&bound.source_name).into_string(),
                    mode,
                )
            })
            .collect()
    }

    fn for_in_iteration_binding_storage_names(
        &self,
        interner: &Interner,
        for_in: &'a boa_ast::statement::iteration::ForInLoop,
    ) -> BTreeSet<String> {
        match for_in.initializer() {
            IterableLoopInitializer::Let(binding) | IterableLoopInitializer::Const(binding) => {
                supported_bound_names(interner, binding)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|bound| for_in_loop_binding_storage_name(for_in, &bound.source_name))
                    .collect()
            }
            _ => BTreeSet::new(),
        }
    }

    fn for_in_iteration_binding_modes(
        &self,
        interner: &Interner,
        for_in: &'a boa_ast::statement::iteration::ForInLoop,
    ) -> BTreeMap<String, BindingMode> {
        let (mode, binding) = match for_in.initializer() {
            IterableLoopInitializer::Let(binding) => (BindingMode::Let, binding),
            IterableLoopInitializer::Const(binding) => (BindingMode::Const, binding),
            _ => return BTreeMap::new(),
        };
        supported_bound_names(interner, binding)
            .unwrap_or_default()
            .into_iter()
            .map(|bound| {
                (
                    for_in_loop_binding_storage_name(for_in, &bound.source_name),
                    mode,
                )
            })
            .collect()
    }

    fn add_scoped_lexical_aliases(
        &self,
        interner: &Interner,
        declaration: &'a LexicalDeclaration,
        aliases: &mut BTreeMap<String, String>,
    ) {
        let list = match declaration {
            LexicalDeclaration::Let(list)
            | LexicalDeclaration::Const(list)
            | LexicalDeclaration::Using(list)
            | LexicalDeclaration::AwaitUsing(list) => list,
        };
        for declarator in list.as_ref() {
            let Some(bound_names) = supported_bound_names(interner, declarator.binding()) else {
                continue;
            };
            for bound in bound_names {
                aliases.insert(
                    bound.source_name.clone(),
                    scoped_lexical_binding_storage_name(&bound.source_name, bound.span),
                );
            }
        }
    }

    fn scan_owner_items(
        &mut self,
        owner_id: &str,
        parameters: &'a [FormalParameter],
        items: &'a [StatementListItem],
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
    ) {
        self.source_names_by_storage.extend(
            capture_aliases
                .iter()
                .map(|(source, storage)| (storage.clone(), source.clone())),
        );
        self.prepare_function_body_environment(owner_id, parameters, items, interner);
        let activation_cursor = self.activation_environment_cursor(owner_id);
        self.environment_cursor_stack
            .push(activation_cursor.clone());
        let mut refs = BTreeMap::new();
        for parameter in parameters {
            if let Binding::Pattern(pattern) = parameter.variable().binding() {
                self.scan_pattern_expressions(
                    owner_id,
                    pattern,
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    &mut refs,
                );
            }
            if let Some(initializer) = parameter.init() {
                self.scan_expression(
                    owner_id,
                    initializer,
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    &mut refs,
                );
            }
        }
        let activation_bindings =
            &self.environment_plans[&activation_cursor.environment_id].binding_storage_names;
        let parameter_external_refs = refs
            .keys()
            .filter(|name| !activation_bindings.contains(*name))
            .cloned()
            .collect();
        self.owner_plans
            .get_mut(owner_id)
            .expect("parameter owner exists")
            .parameter_external_refs = parameter_external_refs;
        let body_cursor = self.body_environment_cursor(owner_id);
        *self
            .environment_cursor_stack
            .last_mut()
            .expect("owner cursor is active") = body_cursor.clone();
        for item in items {
            self.scan_item(
                owner_id,
                item,
                interner,
                source_text,
                self_name,
                capture_aliases,
                &mut refs,
            );
        }
        if owner_id != SCRIPT_OWNER_ID {
            self.function_free_refs.insert(owner_id.to_string(), refs);
        }
        let cursor = self
            .environment_cursor_stack
            .pop()
            .expect("owner scan must restore its lexical environment cursor");
        debug_assert_eq!(cursor, body_cursor);
        self.finalize_activation_environment_bindings(owner_id);
    }

    fn scan_item(
        &mut self,
        owner_id: &str,
        item: &'a StatementListItem,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        self.source_names_by_storage.extend(
            capture_aliases
                .iter()
                .map(|(source, storage)| (storage.clone(), source.clone())),
        );
        match item {
            StatementListItem::Statement(statement) => {
                self.scan_statement(
                    owner_id,
                    statement,
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            StatementListItem::Declaration(declaration) => match declaration.as_ref() {
                Declaration::Lexical(lexical) => {
                    let list = match lexical {
                        LexicalDeclaration::Let(list)
                        | LexicalDeclaration::Const(list)
                        | LexicalDeclaration::Using(list)
                        | LexicalDeclaration::AwaitUsing(list) => list,
                    };
                    for declarator in list.as_ref() {
                        if let Binding::Pattern(pattern) = declarator.binding() {
                            self.scan_pattern_expressions(
                                owner_id,
                                pattern,
                                interner,
                                source_text,
                                self_name,
                                capture_aliases,
                                refs,
                            );
                        }
                        if let Some(init) = declarator.init() {
                            self.scan_expression(
                                owner_id,
                                init,
                                interner,
                                source_text,
                                self_name,
                                capture_aliases,
                                refs,
                            );
                        }
                    }
                }
                Declaration::FunctionDeclaration(function) => {
                    self.collect_non_root_function_declaration(
                        owner_id,
                        function,
                        interner,
                        source_text,
                        capture_aliases,
                    );
                }
                Declaration::GeneratorDeclaration(function)
                    if generator_function_is_aot_supported(
                        function.body(),
                        function.parameters(),
                    ) =>
                {
                    self.collect_non_root_generator_declaration(
                        owner_id,
                        function,
                        interner,
                        source_text,
                        capture_aliases,
                    );
                }
                Declaration::AsyncFunctionDeclaration(function) => {
                    self.collect_non_root_async_function_declaration(
                        owner_id,
                        function,
                        interner,
                        source_text,
                        capture_aliases,
                    );
                }
                Declaration::AsyncGeneratorDeclaration(function) => {
                    self.collect_non_root_async_generator_declaration(
                        owner_id,
                        function,
                        interner,
                        source_text,
                        capture_aliases,
                    );
                }
                Declaration::ClassDeclaration(class) => {
                    let constructor_execution_key = class
                        .constructor()
                        .map(class_constructor_key)
                        .unwrap_or_else(|| class_default_constructor_key(class.linear_span()));
                    self.scan_class_definition(
                        owner_id,
                        SourceClassName::from_declaration(class, interner),
                        constructor_execution_key,
                        class.super_ref(),
                        class.constructor(),
                        class.elements(),
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
                _ => {}
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_is_strict(&self, owner_id: &str) -> bool {
        self.lexical_strictness == Some(Strictness::Strict)
            || self
                .owner_plans
                .get(owner_id)
                .is_some_and(|owner| owner.strict)
    }

    fn scan_class_definition(
        &mut self,
        owner_id: &str,
        class_name: SourceClassName,
        constructor_execution_key: String,
        heritage: Option<&'a Expression>,
        constructor: Option<&'a FunctionExpression>,
        elements: &'a [ClassElement],
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        let mut class_capture_aliases = capture_aliases.clone();
        let class_cursor = class_name.binding_name().map(|source_name| {
            let span = class_name
                .binding_span()
                .expect("a source class binding retains its original span");
            let storage_name = class_name_binding_storage_name(source_name, span);
            class_capture_aliases.insert(source_name.to_owned(), storage_name.clone());
            self.register_class_name_environment(
                owner_id,
                constructor_execution_key.clone(),
                self.current_environment_cursor(),
                storage_name,
            )
        });
        if let Some(cursor) = &class_cursor {
            self.environment_cursor_stack.push(cursor.clone());
        }

        let enclosing_strictness = self.lexical_strictness.replace(Strictness::Strict);
        if let Some(heritage) = heritage {
            self.scan_expression(
                owner_id,
                heritage,
                interner,
                source_text,
                self_name,
                &class_capture_aliases,
                refs,
            );
        }
        let class_private_environment_id = self.register_class_private_environment(
            constructor_execution_key.clone(),
            elements,
            interner,
        );
        if let Some(private_environment_id) = class_private_environment_id {
            self.private_environment_stack.push(private_environment_id);
        }
        for element in elements {
            let property_name = match element {
                ClassElement::MethodDefinition(method) => match method.name() {
                    ClassElementName::PropertyName(name) => Some(name),
                    ClassElementName::PrivateName(_) => None,
                },
                ClassElement::FieldDefinition(field)
                | ClassElement::AccessorFieldDefinition(field)
                | ClassElement::StaticFieldDefinition(field)
                | ClassElement::StaticAccessorFieldDefinition(field) => Some(field.name()),
                ClassElement::PrivateFieldDefinition(_)
                | ClassElement::PrivateStaticFieldDefinition(_)
                | ClassElement::StaticBlock(_) => None,
            };
            if let Some(property_name) = property_name {
                self.scan_property_name(
                    owner_id,
                    property_name,
                    interner,
                    source_text,
                    self_name,
                    &class_capture_aliases,
                    refs,
                );
            }
        }

        let definition_environment_cursor = self.current_environment_cursor();
        if let Some(constructor) = constructor {
            self.collect_class_constructor_owner_plan(
                owner_id,
                constructor,
                definition_environment_cursor.clone(),
                interner,
                source_text,
                heritage.is_some(),
                &class_capture_aliases,
            );
        } else {
            self.collect_default_class_constructor_owner_plan(
                owner_id,
                constructor_execution_key,
                definition_environment_cursor.clone(),
                heritage.is_some(),
            );
        }
        self.collect_class_element_owner_plans(
            owner_id,
            elements,
            definition_environment_cursor,
            interner,
            source_text,
            &class_capture_aliases,
        );

        if let Some(expected_private_environment_id) = class_private_environment_id {
            let private_environment_id = self
                .private_environment_stack
                .pop()
                .expect("class definition must restore its private environment");
            debug_assert_eq!(private_environment_id, expected_private_environment_id);
        }

        if let Some(expected_cursor) = class_cursor {
            let cursor = self
                .environment_cursor_stack
                .pop()
                .expect("class definition must restore its lexical environment cursor");
            debug_assert_eq!(cursor, expected_cursor);
        }
        self.lexical_strictness = enclosing_strictness;
    }

    fn collect_non_root_function_declaration(
        &mut self,
        owner_id: &str,
        function: &'a FunctionDeclaration,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        capture_aliases: &BTreeMap<String, String>,
    ) {
        let key = function_declaration_key(function);
        if self.function_declaration_ids.contains_key(&key) {
            return;
        }
        let id = self.alloc_function_id();
        self.function_declaration_ids
            .insert(key.clone(), id.clone());
        let name = function_name(interner, function, None);
        let mut function_capture_aliases = capture_aliases.clone();
        if let Some(plan) = self.annex_b_function_plans.get(&key) {
            function_capture_aliases.insert(name.clone(), plan.block_storage_name.clone());
        }
        let pending = PendingFunction {
            id,
            name: name.clone(),
            to_string_representation: CallableToStringRepresentation::ExactSource(
                function_source_slice(function, source_text),
            ),
            protocol: FunctionProtocolIr::OrdinaryCallAndConstruct,
            strict: self.scan_is_strict(owner_id) || function.body().strict(),
            self_binding_name: None,
            parameters: function.parameters(),
            body: function.body(),
            is_expression: false,
            capture_aliases: function_capture_aliases,
        };
        self.collect_function_plan(
            pending,
            owner_id.to_string(),
            self.current_environment_cursor(),
            interner,
            source_text,
        );
    }

    fn collect_non_root_generator_declaration(
        &mut self,
        owner_id: &str,
        function: &'a GeneratorDeclaration,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        capture_aliases: &BTreeMap<String, String>,
    ) {
        let key = generator_declaration_key(function);
        if self.function_declaration_ids.contains_key(&key) {
            return;
        }
        let id = self.alloc_function_id();
        self.function_declaration_ids.insert(key, id.clone());
        let pending = PendingFunction {
            id,
            name: interner.resolve_expect(function.name().sym()).to_string(),
            to_string_representation: CallableToStringRepresentation::ExactSource(
                generator_declaration_source_slice(function, source_text),
            ),
            protocol: FunctionProtocolIr::Generator,
            strict: self.scan_is_strict(owner_id) || function.body().strict(),
            self_binding_name: None,
            parameters: function.parameters(),
            body: function.body(),
            is_expression: false,
            capture_aliases: capture_aliases.clone(),
        };
        self.collect_function_plan(
            pending,
            owner_id.to_string(),
            self.current_environment_cursor(),
            interner,
            source_text,
        );
    }

    fn collect_non_root_async_function_declaration(
        &mut self,
        owner_id: &str,
        function: &'a AsyncFunctionDeclaration,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        capture_aliases: &BTreeMap<String, String>,
    ) {
        let key = async_function_declaration_key(function);
        if self.function_declaration_ids.contains_key(&key) {
            return;
        }
        let id = self.alloc_function_id();
        self.function_declaration_ids.insert(key, id.clone());
        let pending = PendingFunction {
            id,
            name: interner.resolve_expect(function.name().sym()).to_string(),
            to_string_representation: CallableToStringRepresentation::ExactSource(
                async_function_declaration_source_slice(function, source_text),
            ),
            protocol: FunctionProtocolIr::Async,
            strict: self.scan_is_strict(owner_id) || function.body().strict(),
            self_binding_name: None,
            parameters: function.parameters(),
            body: function.body(),
            is_expression: false,
            capture_aliases: capture_aliases.clone(),
        };
        self.collect_function_plan(
            pending,
            owner_id.to_string(),
            self.current_environment_cursor(),
            interner,
            source_text,
        );
    }

    fn collect_non_root_async_generator_declaration(
        &mut self,
        owner_id: &str,
        function: &'a AsyncGeneratorDeclaration,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        capture_aliases: &BTreeMap<String, String>,
    ) {
        let key = async_generator_declaration_key(function);
        if self.function_declaration_ids.contains_key(&key) {
            return;
        }
        let id = self.alloc_function_id();
        self.function_declaration_ids.insert(key, id.clone());
        let pending = PendingFunction {
            id,
            name: interner.resolve_expect(function.name().sym()).to_string(),
            to_string_representation: CallableToStringRepresentation::ExactSource(
                async_generator_declaration_source_slice(function, source_text),
            ),
            protocol: FunctionProtocolIr::AsyncGenerator,
            strict: self.scan_is_strict(owner_id) || function.body().strict(),
            self_binding_name: None,
            parameters: function.parameters(),
            body: function.body(),
            is_expression: false,
            capture_aliases: capture_aliases.clone(),
        };
        self.collect_function_plan(
            pending,
            owner_id.to_string(),
            self.current_environment_cursor(),
            interner,
            source_text,
        );
    }

    fn collect_class_element_owner_plans(
        &mut self,
        parent_owner_id: &str,
        elements: &'a [ClassElement],
        definition_environment_cursor: EnvironmentCursor,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        capture_aliases: &BTreeMap<String, String>,
    ) {
        for element in elements {
            match element {
                ClassElement::MethodDefinition(method) => {
                    let key = class_method_key(method);
                    if self.class_execution_ids.contains_key(&key) {
                        continue;
                    }
                    let id = self.alloc_function_id();
                    self.class_execution_ids.insert(key, id.clone());
                    self.class_method_bodies.insert(id.clone(), method.body());
                    // ClassBody is unconditionally strict: every method,
                    // getter and setter is strict code regardless of the
                    // surrounding script.
                    let strict = true;
                    let root_functions = self.collect_root_functions(
                        interner,
                        source_text,
                        method.body().statements(),
                        strict,
                        capture_aliases,
                    );
                    let name = match method.name() {
                        ClassElementName::PropertyName(PropertyName::Literal(name)) => {
                            encode_js_string_utf16(interner.resolve_expect(name.sym()).utf16())
                        }
                        ClassElementName::PrivateName(name) => private_name_key(interner, *name),
                        _ => "<class-method>".to_string(),
                    };
                    let mut root_bindings = self.collect_owner_bindings(
                        interner,
                        method.parameters().as_ref(),
                        None,
                        true,
                        true,
                        true,
                        method.body().statements(),
                        &root_functions,
                    );
                    root_bindings.insert(LEXICAL_HOME_OBJECT_NAME.to_string());
                    self.parameter_environment_bindings.insert(
                        id.clone(),
                        self.collect_parameter_environment_bindings(
                            interner,
                            method.parameters().as_ref(),
                            None,
                            true,
                            LexicalSuperOwnerRole::HomeObject,
                        ),
                    );
                    let activation_binding_modes = self.activation_binding_modes(
                        interner,
                        method.parameters().as_ref(),
                        None,
                        method.body().statements(),
                        &root_bindings,
                    );
                    let activation_environment_id =
                        self.register_activation_environment(&id, root_bindings.clone());
                    self.set_environment_binding_modes(
                        activation_environment_id,
                        activation_binding_modes,
                    );
                    self.set_environment_parent_cursor(
                        activation_environment_id,
                        definition_environment_cursor.clone(),
                    );
                    let owned_env_slots = matches!(
                        method.kind(),
                        MethodDefinitionKind::Generator
                            | MethodDefinitionKind::Async
                            | MethodDefinitionKind::AsyncGenerator
                    )
                    .then(|| {
                        root_bindings
                            .iter()
                            .enumerate()
                            .map(|(slot, name)| (name.clone(), slot as u32))
                            .collect()
                    })
                    .unwrap_or_default();
                    self.owner_plans.insert(
                        id.clone(),
                        OwnerPlan {
                            parameter_external_refs: BTreeSet::new(),
                            body_environment_id: None,
                            parameter_eval_environment_id: None,
                            parameter_names: eval_environment::parameter_names(
                                interner,
                                method.parameters().as_ref(),
                            ),
                            eval_invocation: scriptw_front::EvalInvocationContext::Method,
                            flavor: FunctionFlavor::Ordinary,
                            execution_kind: object_method_protocol(method.kind())
                                .function_protocol()
                                .execution_kind(),
                            lexical_super_owner_role: LexicalSuperOwnerRole::HomeObject,
                            strict,
                            parent_owner_id: Some(parent_owner_id.to_string()),
                            activation_environment_id,
                            definition_environment_cursor: definition_environment_cursor.clone(),
                            root_bindings,
                            function_bindings: root_functions
                                .iter()
                                .map(|nested| (nested.name.clone(), nested.id.clone()))
                                .collect(),
                            owned_env_slots,
                            private_environment_id: self.current_private_environment_id(),
                        },
                    );
                    self.scan_owner_items(
                        &id,
                        method.parameters().as_ref(),
                        method.body().statements(),
                        interner,
                        source_text,
                        Some(name.as_str()),
                        capture_aliases,
                    );
                    for nested in root_functions {
                        self.collect_function_plan(
                            nested,
                            id.clone(),
                            self.body_environment_cursor(&id),
                            interner,
                            source_text,
                        );
                    }
                }
                ClassElement::FieldDefinition(field)
                | ClassElement::AccessorFieldDefinition(field)
                | ClassElement::StaticFieldDefinition(field)
                | ClassElement::StaticAccessorFieldDefinition(field) => {
                    let Some(initializer) = field.initializer() else {
                        continue;
                    };
                    self.collect_class_field_initializer_owner_plan(
                        parent_owner_id,
                        initializer,
                        definition_environment_cursor.clone(),
                        interner,
                        source_text,
                        capture_aliases,
                    );
                }
                ClassElement::PrivateFieldDefinition(field)
                | ClassElement::PrivateStaticFieldDefinition(field) => {
                    let Some(initializer) = field.initializer() else {
                        continue;
                    };
                    self.collect_class_field_initializer_owner_plan(
                        parent_owner_id,
                        initializer,
                        definition_environment_cursor.clone(),
                        interner,
                        source_text,
                        capture_aliases,
                    );
                }
                ClassElement::StaticBlock(block) => self.collect_class_static_block_owner_plan(
                    parent_owner_id,
                    block,
                    definition_environment_cursor.clone(),
                    interner,
                    source_text,
                    capture_aliases,
                ),
            }
        }
    }

    fn collect_class_field_initializer_owner_plan(
        &mut self,
        class_parent_owner_id: &str,
        initializer: &'a Expression,
        definition_environment_cursor: EnvironmentCursor,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        capture_aliases: &BTreeMap<String, String>,
    ) {
        let key = class_field_initializer_key(initializer);
        if self.class_execution_ids.contains_key(&key) {
            return;
        }
        let id = self.alloc_function_id();
        self.class_execution_ids.insert(key, id.clone());
        let root_bindings = BTreeSet::from([
            LEXICAL_THIS_NAME.to_string(),
            LEXICAL_HOME_OBJECT_NAME.to_string(),
        ]);
        let activation_environment_id =
            self.register_activation_environment(&id, root_bindings.clone());
        self.set_environment_binding_modes(
            activation_environment_id,
            root_bindings
                .iter()
                .map(|name| (name.clone(), BindingMode::Let))
                .collect(),
        );
        self.set_environment_parent_cursor(
            activation_environment_id,
            definition_environment_cursor.clone(),
        );
        self.owner_plans.insert(
            id.clone(),
            OwnerPlan {
                parameter_external_refs: BTreeSet::new(),
                body_environment_id: None,
                parameter_eval_environment_id: None,
                parameter_names: BTreeSet::new(),
                eval_invocation: scriptw_front::EvalInvocationContext::ClassFieldInitializer,
                flavor: FunctionFlavor::Ordinary,
                execution_kind: FunctionExecutionKind::Ordinary,
                lexical_super_owner_role: LexicalSuperOwnerRole::HomeObject,
                strict: true,
                parent_owner_id: Some(class_parent_owner_id.to_string()),
                activation_environment_id,
                definition_environment_cursor,
                root_bindings,
                function_bindings: BTreeMap::new(),
                owned_env_slots: BTreeMap::new(),
                private_environment_id: self.current_private_environment_id(),
            },
        );
        self.scan_owner_expression(&id, initializer, interner, source_text, capture_aliases);
    }

    fn collect_class_static_block_owner_plan(
        &mut self,
        parent_owner_id: &str,
        block: &'a StaticBlockBody,
        definition_environment_cursor: EnvironmentCursor,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        capture_aliases: &BTreeMap<String, String>,
    ) {
        let key = class_static_block_key(block);
        if self.class_execution_ids.contains_key(&key) {
            return;
        }
        let id = self.alloc_function_id();
        self.class_execution_ids.insert(key, id.clone());
        let strict = true;
        let root_functions = self.collect_root_functions(
            interner,
            source_text,
            block.statements().statements(),
            strict,
            capture_aliases,
        );
        let mut root_bindings = self.collect_owner_bindings(
            interner,
            &[],
            None,
            true,
            false,
            false,
            block.statements().statements(),
            &root_functions,
        );
        root_bindings.insert(LEXICAL_HOME_OBJECT_NAME.to_string());
        let activation_binding_modes = self.activation_binding_modes(
            interner,
            &[],
            None,
            block.statements().statements(),
            &root_bindings,
        );
        let activation_environment_id =
            self.register_activation_environment(&id, root_bindings.clone());
        self.set_environment_binding_modes(activation_environment_id, activation_binding_modes);
        self.set_environment_parent_cursor(
            activation_environment_id,
            definition_environment_cursor.clone(),
        );
        self.owner_plans.insert(
            id.clone(),
            OwnerPlan {
                parameter_external_refs: BTreeSet::new(),
                body_environment_id: None,
                parameter_eval_environment_id: None,
                parameter_names: BTreeSet::new(),
                eval_invocation: scriptw_front::EvalInvocationContext::ClassFieldInitializer,
                flavor: FunctionFlavor::Ordinary,
                execution_kind: FunctionExecutionKind::Ordinary,
                lexical_super_owner_role: LexicalSuperOwnerRole::HomeObject,
                strict,
                parent_owner_id: Some(parent_owner_id.to_string()),
                activation_environment_id,
                definition_environment_cursor: definition_environment_cursor.clone(),
                root_bindings,
                function_bindings: root_functions
                    .iter()
                    .map(|nested| (nested.name.clone(), nested.id.clone()))
                    .collect(),
                owned_env_slots: BTreeMap::new(),
                private_environment_id: self.current_private_environment_id(),
            },
        );
        self.scan_owner_items(
            &id,
            &[],
            block.statements().statements(),
            interner,
            source_text,
            Some("<static>"),
            capture_aliases,
        );
        for nested in root_functions {
            self.collect_function_plan(
                nested,
                id.clone(),
                self.body_environment_cursor(&id),
                interner,
                source_text,
            );
        }
    }

    fn collect_class_constructor_owner_plan(
        &mut self,
        parent_owner_id: &str,
        constructor: &'a FunctionExpression,
        definition_environment_cursor: EnvironmentCursor,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        is_derived_constructor: bool,
        capture_aliases: &BTreeMap<String, String>,
    ) -> FunctionId {
        let key = class_constructor_key(constructor);
        if let Some(id) = self.class_execution_ids.get(&key) {
            return id.clone();
        }
        let id = self.alloc_function_id();
        self.class_execution_ids.insert(key, id.clone());
        let lexical_super_owner_role = if is_derived_constructor {
            LexicalSuperOwnerRole::DerivedConstructorActivation
        } else {
            LexicalSuperOwnerRole::HomeObject
        };
        // ClassBody is unconditionally strict: the constructor is strict
        // code regardless of the surrounding script.
        let root_functions = self.collect_root_functions(
            interner,
            source_text,
            constructor.body().statements(),
            true,
            capture_aliases,
        );
        let mut root_bindings = self.collect_owner_bindings(
            interner,
            constructor.parameters().as_ref(),
            None,
            true,
            true,
            true,
            constructor.body().statements(),
            &root_functions,
        );
        if is_derived_constructor {
            root_bindings.extend([
                DERIVED_ACTIVATION_THIS_NAME.to_string(),
                DERIVED_ACTIVATION_THIS_STATUS_NAME.to_string(),
                DERIVED_ACTIVATION_NEW_TARGET_NAME.to_string(),
                DERIVED_ACTIVATION_FUNCTION_NAME.to_string(),
            ]);
        }
        root_bindings.insert(LEXICAL_HOME_OBJECT_NAME.to_string());
        self.parameter_environment_bindings.insert(
            id.clone(),
            self.collect_parameter_environment_bindings(
                interner,
                constructor.parameters().as_ref(),
                None,
                true,
                lexical_super_owner_role,
            ),
        );
        let activation_binding_modes = self.activation_binding_modes(
            interner,
            constructor.parameters().as_ref(),
            None,
            constructor.body().statements(),
            &root_bindings,
        );
        let activation_environment_id =
            self.register_activation_environment(&id, root_bindings.clone());
        self.set_environment_binding_modes(activation_environment_id, activation_binding_modes);
        self.set_environment_parent_cursor(
            activation_environment_id,
            definition_environment_cursor.clone(),
        );
        self.owner_plans.insert(
            id.clone(),
            OwnerPlan {
                parameter_external_refs: BTreeSet::new(),
                body_environment_id: None,
                parameter_eval_environment_id: None,
                parameter_names: eval_environment::parameter_names(
                    interner,
                    constructor.parameters().as_ref(),
                ),
                eval_invocation: if is_derived_constructor {
                    scriptw_front::EvalInvocationContext::DerivedConstructor
                } else {
                    scriptw_front::EvalInvocationContext::Method
                },
                flavor: FunctionFlavor::Ordinary,
                execution_kind: FunctionExecutionKind::Ordinary,
                lexical_super_owner_role,
                strict: true,
                parent_owner_id: Some(parent_owner_id.to_string()),
                activation_environment_id,
                definition_environment_cursor: definition_environment_cursor.clone(),
                root_bindings,
                function_bindings: root_functions
                    .iter()
                    .map(|nested| (nested.name.clone(), nested.id.clone()))
                    .collect(),
                owned_env_slots: BTreeMap::new(),
                private_environment_id: self.current_private_environment_id(),
            },
        );
        self.scan_owner_items(
            &id,
            constructor.parameters().as_ref(),
            constructor.body().statements(),
            interner,
            source_text,
            Some("constructor"),
            capture_aliases,
        );
        for nested in root_functions {
            self.collect_function_plan(
                nested,
                id.clone(),
                self.body_environment_cursor(&id),
                interner,
                source_text,
            );
        }
        id
    }

    fn collect_default_class_constructor_owner_plan(
        &mut self,
        parent_owner_id: &str,
        key: String,
        definition_environment_cursor: EnvironmentCursor,
        is_derived_constructor: bool,
    ) -> FunctionId {
        if let Some(id) = self.class_execution_ids.get(&key) {
            return id.clone();
        }
        let id = self.alloc_function_id();
        self.class_execution_ids.insert(key, id.clone());
        let mut root_bindings = BTreeSet::from([
            LEXICAL_THIS_NAME.to_string(),
            LEXICAL_ARGUMENTS_NAME.to_string(),
            LEXICAL_NEW_TARGET_NAME.to_string(),
            LEXICAL_HOME_OBJECT_NAME.to_string(),
        ]);
        if is_derived_constructor {
            root_bindings.extend([
                DERIVED_ACTIVATION_THIS_NAME.to_string(),
                DERIVED_ACTIVATION_THIS_STATUS_NAME.to_string(),
                DERIVED_ACTIVATION_NEW_TARGET_NAME.to_string(),
                DERIVED_ACTIVATION_FUNCTION_NAME.to_string(),
            ]);
        }
        let activation_environment_id =
            self.register_activation_environment(&id, root_bindings.clone());
        self.set_environment_binding_modes(
            activation_environment_id,
            root_bindings
                .iter()
                .map(|name| (name.clone(), BindingMode::Let))
                .collect(),
        );
        self.set_environment_parent_cursor(
            activation_environment_id,
            definition_environment_cursor.clone(),
        );
        self.owner_plans.insert(
            id.clone(),
            OwnerPlan {
                parameter_external_refs: BTreeSet::new(),
                body_environment_id: None,
                parameter_eval_environment_id: None,
                parameter_names: BTreeSet::new(),
                eval_invocation: if is_derived_constructor {
                    scriptw_front::EvalInvocationContext::DerivedConstructor
                } else {
                    scriptw_front::EvalInvocationContext::Method
                },
                flavor: FunctionFlavor::Ordinary,
                execution_kind: FunctionExecutionKind::Ordinary,
                lexical_super_owner_role: if is_derived_constructor {
                    LexicalSuperOwnerRole::DerivedConstructorActivation
                } else {
                    LexicalSuperOwnerRole::HomeObject
                },
                strict: true,
                parent_owner_id: Some(parent_owner_id.to_string()),
                activation_environment_id,
                definition_environment_cursor: definition_environment_cursor.clone(),
                root_bindings,
                function_bindings: BTreeMap::new(),
                owned_env_slots: BTreeMap::new(),
                private_environment_id: self.current_private_environment_id(),
            },
        );
        self.function_free_refs.insert(id.clone(), BTreeMap::new());
        self.finalize_activation_environment_bindings(&id);
        id
    }

    fn scan_owner_expression(
        &mut self,
        owner_id: &str,
        expression: &'a Expression,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        capture_aliases: &BTreeMap<String, String>,
    ) {
        let activation_cursor = self.activation_environment_cursor(owner_id);
        self.environment_cursor_stack
            .push(activation_cursor.clone());
        let mut refs = BTreeMap::new();
        self.scan_expression(
            owner_id,
            expression,
            interner,
            source_text,
            None,
            capture_aliases,
            &mut refs,
        );
        self.function_free_refs.insert(owner_id.to_string(), refs);
        let cursor = self
            .environment_cursor_stack
            .pop()
            .expect("owner expression scan must restore its lexical environment cursor");
        debug_assert_eq!(cursor, activation_cursor);
        self.finalize_activation_environment_bindings(owner_id);
    }

    fn record_ref(
        &self,
        owner_id: &str,
        source_name: String,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        let alias = capture_aliases.get(&source_name);
        let parameter_phase = self.owner_plans.get(owner_id).is_some_and(|owner| {
            owner.body_environment_id.is_some()
                && self.current_environment_cursor().environment_id
                    == owner.activation_environment_id
        });
        let owner_bindings = self.owner_plans.get(owner_id).map(|owner| {
            if parameter_phase {
                &self.environment_plans[&owner.activation_environment_id].binding_storage_names
            } else {
                &owner.root_bindings
            }
        });
        let owner_has_alias = alias
            .is_some_and(|alias| owner_bindings.is_some_and(|bindings| bindings.contains(alias)));
        let storage_name = if owner_has_alias
            || owner_bindings.is_none_or(|bindings| !bindings.contains(&source_name))
        {
            alias.cloned().unwrap_or_else(|| source_name.clone())
        } else {
            source_name.clone()
        };
        refs.entry(storage_name).or_insert(source_name);
    }

    fn scan_block(
        &mut self,
        owner_id: &str,
        block: &'a Block,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        let items = block.statement_list().statements();
        // Block : { } returns NormalCompletion(empty) directly (14.2.2).
        // Only a Block with a StatementList creates a declarative environment.
        // In particular, eval visibility cannot make an absent scope observable.
        if items.is_empty() {
            return;
        }
        let block_aliases = self.scoped_capture_aliases(interner, items, capture_aliases);
        let block_cursor = self.register_lexical_environment_with_modes(
            owner_id,
            EnvironmentKind::Block,
            self.current_environment_cursor(),
            self.scoped_environment_binding_storage_names(interner, items),
            self.scoped_environment_binding_modes(interner, items),
        );
        self.block_environment_ids
            .insert(block as *const Block as usize, block_cursor.environment_id);
        self.environment_cursor_stack.push(block_cursor.clone());
        for item in items {
            self.scan_item(
                owner_id,
                item,
                interner,
                source_text,
                self_name,
                &block_aliases,
                refs,
            );
        }
        let cursor = self
            .environment_cursor_stack
            .pop()
            .expect("block scan must restore its lexical environment cursor");
        debug_assert_eq!(cursor, block_cursor);
    }

    fn scan_statement(
        &mut self,
        owner_id: &str,
        statement: &'a Statement,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        self.source_names_by_storage.extend(
            capture_aliases
                .iter()
                .map(|(source, storage)| (storage.clone(), source.clone())),
        );
        match statement {
            Statement::Expression(expression) => {
                self.scan_expression(
                    owner_id,
                    expression,
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Statement::Block(block) => self.scan_block(
                owner_id,
                block,
                interner,
                source_text,
                self_name,
                capture_aliases,
                refs,
            ),
            Statement::If(if_statement) => {
                self.scan_expression(
                    owner_id,
                    if_statement.cond(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                self.scan_statement(
                    owner_id,
                    if_statement.body(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                if let Some(else_node) = if_statement.else_node() {
                    self.scan_statement(
                        owner_id,
                        else_node,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            Statement::WhileLoop(while_loop) => {
                self.scan_expression(
                    owner_id,
                    while_loop.condition(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                self.scan_statement(
                    owner_id,
                    while_loop.body(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Statement::DoWhileLoop(do_while) => {
                self.scan_statement(
                    owner_id,
                    do_while.body(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                self.scan_expression(
                    owner_id,
                    do_while.cond(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Statement::ForLoop(for_loop) => {
                let mut loop_aliases = capture_aliases.clone();
                let lexical_head_cursor = match for_loop.init() {
                    Some(ForLoopInitializer::Lexical(lexical))
                        if matches!(
                            lexical.declaration(),
                            LexicalDeclaration::Let(_)
                                | LexicalDeclaration::Const(_)
                                | LexicalDeclaration::Using(_)
                                | LexicalDeclaration::AwaitUsing(_)
                        ) =>
                    {
                        Some(self.register_lexical_environment_with_modes(
                            owner_id,
                            EnvironmentKind::ForLexicalHead,
                            self.current_environment_cursor(),
                            self.classic_for_lexical_binding_storage_names(interner, for_loop),
                            self.classic_for_lexical_binding_modes(interner, for_loop),
                        ))
                    }
                    _ => None,
                };
                if let Some(cursor) = &lexical_head_cursor {
                    self.for_lexical_environment_ids
                        .insert(for_loop as *const ForLoop as usize, cursor.environment_id);
                }
                if let Some(cursor) = &lexical_head_cursor {
                    self.environment_cursor_stack.push(cursor.clone());
                }
                if let Some(init) = for_loop.init() {
                    match init {
                        ForLoopInitializer::Expression(expr) => {
                            self.scan_expression(
                                owner_id,
                                expr,
                                interner,
                                source_text,
                                self_name,
                                capture_aliases,
                                refs,
                            );
                        }
                        ForLoopInitializer::Var(var) => {
                            for declarator in var.0.as_ref() {
                                if let Binding::Pattern(pattern) = declarator.binding() {
                                    self.scan_pattern_expressions(
                                        owner_id,
                                        pattern,
                                        interner,
                                        source_text,
                                        self_name,
                                        capture_aliases,
                                        refs,
                                    );
                                }
                                if let Some(init) = declarator.init() {
                                    self.scan_expression(
                                        owner_id,
                                        init,
                                        interner,
                                        source_text,
                                        self_name,
                                        capture_aliases,
                                        refs,
                                    );
                                }
                            }
                        }
                        ForLoopInitializer::Lexical(lexical) => {
                            let declaration = lexical.declaration();
                            let list = match declaration {
                                LexicalDeclaration::Let(list)
                                | LexicalDeclaration::Const(list)
                                | LexicalDeclaration::Using(list)
                                | LexicalDeclaration::AwaitUsing(list) => list,
                            };
                            for declarator in list.as_ref() {
                                let Some(bound_names) =
                                    supported_bound_names(interner, declarator.binding())
                                else {
                                    continue;
                                };
                                for bound in bound_names {
                                    loop_aliases.insert(
                                        bound.source_name.clone(),
                                        scoped_lexical_binding_storage_name(
                                            &bound.source_name,
                                            bound.span,
                                        ),
                                    );
                                }
                            }
                            let head_aliases = loop_aliases.clone();
                            for declarator in list.as_ref() {
                                if let Binding::Pattern(pattern) = declarator.binding() {
                                    self.scan_pattern_expressions(
                                        owner_id,
                                        pattern,
                                        interner,
                                        source_text,
                                        self_name,
                                        &head_aliases,
                                        refs,
                                    );
                                }
                                if let Some(init) = declarator.init() {
                                    self.scan_expression(
                                        owner_id,
                                        init,
                                        interner,
                                        source_text,
                                        self_name,
                                        &head_aliases,
                                        refs,
                                    );
                                }
                            }
                        }
                    }
                }
                if let Some(condition) = for_loop.condition() {
                    self.scan_expression(
                        owner_id,
                        condition,
                        interner,
                        source_text,
                        self_name,
                        &loop_aliases,
                        refs,
                    );
                }
                if let Some(update) = for_loop.final_expr() {
                    self.scan_expression(
                        owner_id,
                        update,
                        interner,
                        source_text,
                        self_name,
                        &loop_aliases,
                        refs,
                    );
                }
                self.scan_statement(
                    owner_id,
                    for_loop.body(),
                    interner,
                    source_text,
                    self_name,
                    &loop_aliases,
                    refs,
                );
                if let Some(expected_cursor) = lexical_head_cursor {
                    let cursor = self
                        .environment_cursor_stack
                        .pop()
                        .expect("for lexical head must restore its environment cursor");
                    debug_assert_eq!(cursor, expected_cursor);
                }
            }
            Statement::Switch(switch) => {
                self.scan_expression(
                    owner_id,
                    switch.val(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                let mut switch_aliases = capture_aliases.clone();
                let mut switch_bindings = BTreeSet::new();
                let mut switch_binding_modes = BTreeMap::new();
                for case in switch.cases() {
                    switch_aliases = self.scoped_capture_aliases(
                        interner,
                        case.body().statements(),
                        &switch_aliases,
                    );
                    switch_bindings.extend(self.scoped_environment_binding_storage_names(
                        interner,
                        case.body().statements(),
                    ));
                    switch_binding_modes.extend(
                        self.scoped_environment_binding_modes(interner, case.body().statements()),
                    );
                }
                let switch_cursor = self.register_lexical_environment_with_modes(
                    owner_id,
                    EnvironmentKind::SwitchCaseBlock,
                    self.current_environment_cursor(),
                    switch_bindings,
                    switch_binding_modes,
                );
                self.switch_environment_ids.insert(
                    switch as *const AstSwitch as usize,
                    switch_cursor.environment_id,
                );
                self.environment_cursor_stack.push(switch_cursor.clone());
                for case in switch.cases() {
                    if let Some(condition) = case.condition() {
                        self.scan_expression(
                            owner_id,
                            condition,
                            interner,
                            source_text,
                            self_name,
                            &switch_aliases,
                            refs,
                        );
                    }
                    for item in case.body().statements() {
                        self.scan_item(
                            owner_id,
                            item,
                            interner,
                            source_text,
                            self_name,
                            &switch_aliases,
                            refs,
                        );
                    }
                }
                let cursor = self
                    .environment_cursor_stack
                    .pop()
                    .expect("switch scan must restore its lexical environment cursor");
                debug_assert_eq!(cursor, switch_cursor);
            }
            Statement::Labelled(labelled) => {
                if let Some(function) = labelled_function_declaration(labelled) {
                    self.collect_non_root_function_declaration(
                        owner_id,
                        function,
                        interner,
                        source_text,
                        capture_aliases,
                    );
                } else if let Some(statement) = labelled_base_statement(labelled) {
                    self.scan_statement(
                        owner_id,
                        statement,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            Statement::Var(var) => {
                for declarator in var.0.as_ref() {
                    if let Binding::Pattern(pattern) = declarator.binding() {
                        self.scan_pattern_expressions(
                            owner_id,
                            pattern,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                    if let Some(init) = declarator.init() {
                        self.scan_expression(
                            owner_id,
                            init,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                }
            }
            Statement::Return(ret) => {
                if let Some(target) = ret.target() {
                    self.scan_expression(
                        owner_id,
                        target,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            Statement::Throw(throw) => {
                self.scan_expression(
                    owner_id,
                    throw.target(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Statement::Try(try_statement) => {
                self.scan_block(
                    owner_id,
                    try_statement.block(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                if let Some(catch) = try_statement.catch() {
                    let mut catch_parameter_aliases = capture_aliases.clone();
                    if let Some(bound_names) = catch
                        .parameter()
                        .and_then(|binding| supported_bound_names(interner, binding))
                    {
                        for bound in bound_names {
                            catch_parameter_aliases.insert(
                                bound.source_name.clone(),
                                scoped_lexical_binding_storage_name(&bound.source_name, bound.span),
                            );
                        }
                    }
                    let catch_parameter_bindings = catch
                        .parameter()
                        .and_then(|binding| supported_bound_names(interner, binding))
                        .unwrap_or_default()
                        .into_iter()
                        .map(|bound| {
                            scoped_lexical_binding_storage_name(&bound.source_name, bound.span)
                        })
                        .collect();
                    let catch_parameter_modes = catch
                        .parameter()
                        .and_then(|binding| supported_bound_names(interner, binding))
                        .unwrap_or_default()
                        .into_iter()
                        .map(|bound| {
                            (
                                scoped_lexical_binding_storage_name(&bound.source_name, bound.span),
                                BindingMode::Let,
                            )
                        })
                        .collect();
                    let catch_parameter_cursor = self.register_lexical_environment_with_modes(
                        owner_id,
                        if matches!(catch.parameter(), Some(Binding::Identifier(_))) {
                            EnvironmentKind::SimpleCatchParameter
                        } else {
                            EnvironmentKind::CatchParameter
                        },
                        self.current_environment_cursor(),
                        catch_parameter_bindings,
                        catch_parameter_modes,
                    );
                    if let Some(parameter) = catch.parameter() {
                        self.catch_parameter_environment_ids.insert(
                            parameter as *const Binding as usize,
                            catch_parameter_cursor.environment_id,
                        );
                    }
                    self.environment_cursor_stack
                        .push(catch_parameter_cursor.clone());
                    if let Some(Binding::Pattern(pattern)) = catch.parameter() {
                        self.scan_pattern_expressions(
                            owner_id,
                            pattern,
                            interner,
                            source_text,
                            self_name,
                            &catch_parameter_aliases,
                            refs,
                        );
                    }
                    self.scan_block(
                        owner_id,
                        catch.block(),
                        interner,
                        source_text,
                        self_name,
                        &catch_parameter_aliases,
                        refs,
                    );
                    let cursor = self
                        .environment_cursor_stack
                        .pop()
                        .expect("catch parameter scan must restore its lexical environment cursor");
                    debug_assert_eq!(cursor, catch_parameter_cursor);
                }
                if let Some(finally_block) = try_statement.finally() {
                    self.scan_block(
                        owner_id,
                        finally_block.block(),
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            Statement::ForOfLoop(for_of) => {
                let outer_cursor = self.current_environment_cursor();
                let mut head_aliases = capture_aliases.clone();
                let lexical_loop = matches!(
                    for_of.initializer(),
                    IterableLoopInitializer::Let(_)
                        | IterableLoopInitializer::Const(_)
                        | IterableLoopInitializer::Using(_)
                        | IterableLoopInitializer::AwaitUsing(_)
                );
                let tdz_head_cursor = lexical_loop.then(|| {
                    self.register_lexical_environment_with_modes(
                        owner_id,
                        EnvironmentKind::ForInOfTdzHead,
                        outer_cursor.clone(),
                        self.for_of_tdz_binding_storage_names(interner, for_of),
                        self.for_of_tdz_binding_modes(interner, for_of),
                    )
                });
                if let Some(cursor) = &tdz_head_cursor {
                    self.for_in_of_tdz_environment_ids
                        .insert(for_of as *const ForOfLoop as usize, cursor.environment_id);
                }
                if let Some(cursor) = &tdz_head_cursor {
                    self.environment_cursor_stack.push(cursor.clone());
                }
                match for_of.initializer() {
                    IterableLoopInitializer::Let(binding)
                    | IterableLoopInitializer::Const(binding)
                    | IterableLoopInitializer::Using(binding)
                    | IterableLoopInitializer::AwaitUsing(binding) => {
                        if let Some(bound_names) = supported_bound_names(interner, binding) {
                            for bound in bound_names {
                                head_aliases.insert(
                                    bound.source_name.clone(),
                                    TdzPlaceholderName::for_source_name(&bound.source_name)
                                        .into_string(),
                                );
                            }
                        }
                    }
                    _ => {}
                }
                self.scan_expression(
                    owner_id,
                    for_of.iterable(),
                    interner,
                    source_text,
                    self_name,
                    &head_aliases,
                    refs,
                );
                if let Some(expected_cursor) = &tdz_head_cursor {
                    let cursor = self
                        .environment_cursor_stack
                        .pop()
                        .expect("for-of TDZ head must restore its environment cursor");
                    debug_assert_eq!(&cursor, expected_cursor);
                }
                if let IterableLoopInitializer::Identifier(identifier) = for_of.initializer() {
                    self.record_ref(
                        owner_id,
                        interner.resolve_expect(identifier.sym()).to_string(),
                        capture_aliases,
                        refs,
                    );
                }
                let mut body_aliases = capture_aliases.clone();
                match for_of.initializer() {
                    IterableLoopInitializer::Let(binding)
                    | IterableLoopInitializer::Const(binding)
                    | IterableLoopInitializer::Using(binding)
                    | IterableLoopInitializer::AwaitUsing(binding) => {
                        if let Some(bound_names) = supported_bound_names(interner, binding) {
                            for bound in bound_names {
                                body_aliases.insert(
                                    bound.source_name.clone(),
                                    for_of_loop_binding_storage_name(for_of, &bound.source_name),
                                );
                            }
                        }
                    }
                    IterableLoopInitializer::Var(variable) => {
                        if let Some(bound_names) =
                            supported_bound_names(interner, variable.binding())
                        {
                            for bound in bound_names {
                                body_aliases.insert(bound.source_name.clone(), bound.source_name);
                            }
                        }
                    }
                    _ => {}
                }
                let iteration_cursor = lexical_loop.then(|| {
                    self.register_lexical_environment_with_modes(
                        owner_id,
                        EnvironmentKind::ForInOfIteration,
                        outer_cursor,
                        self.for_of_iteration_binding_storage_names(interner, for_of),
                        self.for_of_iteration_binding_modes(interner, for_of),
                    )
                });
                let complete_resumable_iteration_environment =
                    self.owner_plans.get(owner_id).is_some_and(|owner| {
                        match owner.execution_kind {
                        FunctionExecutionKind::Async => {
                            contains(for_of.body(), ContainsSymbol::AwaitExpression)
                        }
                        FunctionExecutionKind::Generator => contains(
                            for_of.body(),
                            ContainsSymbol::YieldExpression,
                        )
                            && crate::lowering_helpers::generator_for_of_source_shape_is_supported(
                                for_of,
                            ),
                        FunctionExecutionKind::Ordinary | FunctionExecutionKind::AsyncGenerator => {
                            false
                        }
                    }
                    }) && !for_of.r#await()
                        && matches!(
                            for_of.initializer(),
                            IterableLoopInitializer::Let(Binding::Pattern(_))
                                | IterableLoopInitializer::Const(Binding::Pattern(_))
                        );
                if let Some(cursor) = &iteration_cursor {
                    self.for_in_of_iteration_environment_ids
                        .insert(for_of as *const ForOfLoop as usize, cursor.environment_id);
                    if complete_resumable_iteration_environment {
                        self.complete_resumable_for_of_iteration_environment_ids
                            .insert(cursor.environment_id);
                    }
                }
                if let Some(cursor) = &iteration_cursor {
                    self.environment_cursor_stack.push(cursor.clone());
                }
                match for_of.initializer() {
                    IterableLoopInitializer::Let(Binding::Pattern(pattern))
                    | IterableLoopInitializer::Const(Binding::Pattern(pattern)) => {
                        self.scan_pattern_expressions(
                            owner_id,
                            pattern,
                            interner,
                            source_text,
                            self_name,
                            &body_aliases,
                            refs,
                        );
                    }
                    IterableLoopInitializer::Pattern(pattern) => {
                        self.scan_assignment_pattern_expressions(
                            owner_id,
                            pattern,
                            interner,
                            source_text,
                            self_name,
                            &body_aliases,
                            refs,
                        );
                    }
                    IterableLoopInitializer::Var(variable) => {
                        if let Binding::Pattern(pattern) = variable.binding() {
                            self.scan_pattern_expressions(
                                owner_id,
                                pattern,
                                interner,
                                source_text,
                                self_name,
                                &body_aliases,
                                refs,
                            );
                        }
                    }
                    IterableLoopInitializer::Access(access) => {
                        self.scan_property_access(
                            owner_id,
                            access,
                            interner,
                            source_text,
                            self_name,
                            &body_aliases,
                            refs,
                        );
                    }
                    _ => {}
                }
                self.scan_statement(
                    owner_id,
                    for_of.body(),
                    interner,
                    source_text,
                    self_name,
                    &body_aliases,
                    refs,
                );
                if let Some(expected_cursor) = iteration_cursor {
                    let cursor = self
                        .environment_cursor_stack
                        .pop()
                        .expect("for-of iteration must restore its environment cursor");
                    debug_assert_eq!(cursor, expected_cursor);
                }
            }
            Statement::ForInLoop(for_in) => {
                let outer_cursor = self.current_environment_cursor();
                self.for_in_continuation_owners.insert(
                    for_in as *const boa_ast::statement::iteration::ForInLoop as usize,
                    ForInContinuationOwner::ImmediateOrLinear,
                );
                let mut head_aliases = capture_aliases.clone();
                let lexical_loop = matches!(
                    for_in.initializer(),
                    IterableLoopInitializer::Let(_) | IterableLoopInitializer::Const(_)
                );
                let tdz_head_cursor = lexical_loop.then(|| {
                    self.register_lexical_environment_with_modes(
                        owner_id,
                        EnvironmentKind::ForInOfTdzHead,
                        outer_cursor.clone(),
                        self.for_in_tdz_binding_storage_names(interner, for_in),
                        self.for_in_tdz_binding_modes(interner, for_in),
                    )
                });
                if let Some(cursor) = &tdz_head_cursor {
                    self.for_in_of_tdz_environment_ids.insert(
                        for_in as *const boa_ast::statement::iteration::ForInLoop as usize,
                        cursor.environment_id,
                    );
                }
                if let Some(cursor) = &tdz_head_cursor {
                    self.environment_cursor_stack.push(cursor.clone());
                }
                if let IterableLoopInitializer::Let(binding)
                | IterableLoopInitializer::Const(binding) = for_in.initializer()
                {
                    if let Some(bound_names) = supported_bound_names(interner, binding) {
                        for bound in bound_names {
                            head_aliases.insert(
                                bound.source_name.clone(),
                                TdzPlaceholderName::for_source_name(&bound.source_name)
                                    .into_string(),
                            );
                        }
                    }
                }
                // Annex B permits a `var` declaration initializer in a
                // for-in head. Lowering evaluates that initializer before the
                // enumeration target, so analysis must discover its callable
                // and lexical owners in the same order. In particular, a
                // named class initializer owns a class-name environment keyed
                // by its default constructor execution key.
                if let IterableLoopInitializer::Var(variable) = for_in.initializer() {
                    if let Some(initializer) = variable.init() {
                        self.scan_expression(
                            owner_id,
                            initializer,
                            interner,
                            source_text,
                            self_name,
                            &head_aliases,
                            refs,
                        );
                    }
                }
                self.scan_expression(
                    owner_id,
                    for_in.target(),
                    interner,
                    source_text,
                    self_name,
                    &head_aliases,
                    refs,
                );
                if let Some(expected_cursor) = &tdz_head_cursor {
                    let cursor = self
                        .environment_cursor_stack
                        .pop()
                        .expect("for-in TDZ head must restore its environment cursor");
                    debug_assert_eq!(&cursor, expected_cursor);
                }
                // A bare `for (x in …)` head PutValues the key into `x` every
                // iteration, exactly like the for-of head above: without this
                // the write is invisible to capture analysis and lowers to a
                // global-object write instead of the captured binding.
                if let IterableLoopInitializer::Identifier(identifier) = for_in.initializer() {
                    self.record_ref(
                        owner_id,
                        interner.resolve_expect(identifier.sym()).to_string(),
                        capture_aliases,
                        refs,
                    );
                }
                let mut body_aliases = capture_aliases.clone();
                if let IterableLoopInitializer::Let(binding)
                | IterableLoopInitializer::Const(binding) = for_in.initializer()
                {
                    if let Some(bound_names) = supported_bound_names(interner, binding) {
                        for bound in bound_names {
                            body_aliases.insert(
                                bound.source_name.clone(),
                                for_in_loop_binding_storage_name(for_in, &bound.source_name),
                            );
                        }
                    }
                }
                let iteration_cursor = lexical_loop.then(|| {
                    self.register_lexical_environment_with_modes(
                        owner_id,
                        EnvironmentKind::ForInOfIteration,
                        outer_cursor,
                        self.for_in_iteration_binding_storage_names(interner, for_in),
                        self.for_in_iteration_binding_modes(interner, for_in),
                    )
                });
                if let Some(cursor) = &iteration_cursor {
                    self.for_in_of_iteration_environment_ids.insert(
                        for_in as *const boa_ast::statement::iteration::ForInLoop as usize,
                        cursor.environment_id,
                    );
                }
                if let Some(cursor) = &iteration_cursor {
                    self.environment_cursor_stack.push(cursor.clone());
                }
                match for_in.initializer() {
                    IterableLoopInitializer::Let(Binding::Pattern(pattern))
                    | IterableLoopInitializer::Const(Binding::Pattern(pattern)) => {
                        self.scan_pattern_expressions(
                            owner_id,
                            pattern,
                            interner,
                            source_text,
                            self_name,
                            &body_aliases,
                            refs,
                        );
                    }
                    IterableLoopInitializer::Pattern(pattern) => {
                        self.scan_assignment_pattern_expressions(
                            owner_id,
                            pattern,
                            interner,
                            source_text,
                            self_name,
                            &body_aliases,
                            refs,
                        );
                    }
                    IterableLoopInitializer::Var(variable) => {
                        if let Binding::Pattern(pattern) = variable.binding() {
                            self.scan_pattern_expressions(
                                owner_id,
                                pattern,
                                interner,
                                source_text,
                                self_name,
                                &body_aliases,
                                refs,
                            );
                        }
                    }
                    IterableLoopInitializer::Access(access) => {
                        self.scan_property_access(
                            owner_id,
                            access,
                            interner,
                            source_text,
                            self_name,
                            &body_aliases,
                            refs,
                        );
                    }
                    _ => {}
                }
                self.scan_statement(
                    owner_id,
                    for_in.body(),
                    interner,
                    source_text,
                    self_name,
                    &body_aliases,
                    refs,
                );
                if let Some(expected_cursor) = iteration_cursor {
                    let cursor = self
                        .environment_cursor_stack
                        .pop()
                        .expect("for-in iteration must restore its environment cursor");
                    debug_assert_eq!(cursor, expected_cursor);
                }
            }
            Statement::With(with) => {
                // 14.11.2 evaluates the expression before NewObjectEnvironment
                // creates the Object Environment Record. A function created by
                // the expression therefore sees the outer cursor, while a
                // function created by the body records the WithObject cursor.
                self.scan_expression(
                    owner_id,
                    with.expression(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                let with_cursor = self
                    .register_with_object_environment(owner_id, self.current_environment_cursor());
                self.with_environment_ids.insert(
                    with as *const boa_ast::statement::With as usize,
                    with_cursor.environment_id,
                );
                self.environment_cursor_stack.push(with_cursor.clone());
                self.scan_statement(
                    owner_id,
                    with.statement(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                let cursor = self
                    .environment_cursor_stack
                    .pop()
                    .expect("with statement must restore its environment cursor");
                debug_assert_eq!(cursor, with_cursor);
            }
            Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Debugger
            | Statement::Empty => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_array_pattern_expressions(
        &mut self,
        owner_id: &str,
        pattern: &'a ArrayPattern,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        for element in pattern.bindings() {
            match element {
                ArrayPatternElement::Elision | ArrayPatternElement::SingleNameRest { .. } => {}
                ArrayPatternElement::SingleName { default_init, .. } => {
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                }
                ArrayPatternElement::PropertyAccess {
                    access,
                    default_init,
                } => {
                    self.scan_property_access(
                        owner_id,
                        access,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                }
                ArrayPatternElement::Pattern {
                    pattern,
                    default_init,
                } => {
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                    self.scan_pattern_expressions(
                        owner_id,
                        pattern,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
                ArrayPatternElement::PropertyAccessRest { access } => {
                    self.scan_property_access(
                        owner_id,
                        access,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
                ArrayPatternElement::PatternRest { pattern } => {
                    self.scan_pattern_expressions(
                        owner_id,
                        pattern,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_object_pattern_expressions(
        &mut self,
        owner_id: &str,
        pattern: &'a ObjectPattern,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        for element in pattern.bindings() {
            match element {
                ObjectPatternElement::SingleName {
                    name, default_init, ..
                } => {
                    self.scan_property_name(
                        owner_id,
                        name,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                }
                ObjectPatternElement::Pattern {
                    name,
                    pattern,
                    default_init,
                } => {
                    self.scan_property_name(
                        owner_id,
                        name,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                    self.scan_pattern_expressions(
                        owner_id,
                        pattern,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
                ObjectPatternElement::AssignmentPropertyAccess {
                    name,
                    access,
                    default_init,
                } => {
                    self.scan_property_name(
                        owner_id,
                        name,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                    self.scan_property_access(
                        owner_id,
                        access,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                }
                ObjectPatternElement::AssignmentRestPropertyAccess { access } => {
                    self.scan_property_access(
                        owner_id,
                        access,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
                ObjectPatternElement::RestProperty { .. } => {}
            }
        }
    }

    fn scan_pattern_expressions(
        &mut self,
        owner_id: &str,
        pattern: &'a Pattern,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        match pattern {
            Pattern::Array(pattern) => self.scan_array_pattern_expressions(
                owner_id,
                pattern,
                interner,
                source_text,
                self_name,
                capture_aliases,
                refs,
            ),
            Pattern::Object(pattern) => self.scan_object_pattern_expressions(
                owner_id,
                pattern,
                interner,
                source_text,
                self_name,
                capture_aliases,
                refs,
            ),
        }
    }

    fn scan_array_assignment_pattern_expressions(
        &mut self,
        owner_id: &str,
        pattern: &'a ArrayPattern,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        for element in pattern.bindings() {
            match element {
                ArrayPatternElement::Elision => {}
                ArrayPatternElement::SingleName {
                    ident,
                    default_init,
                } => {
                    self.record_ref(
                        owner_id,
                        interner.resolve_expect(ident.sym()).to_string(),
                        capture_aliases,
                        refs,
                    );
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                }
                ArrayPatternElement::SingleNameRest { ident } => {
                    self.record_ref(
                        owner_id,
                        interner.resolve_expect(ident.sym()).to_string(),
                        capture_aliases,
                        refs,
                    );
                }
                ArrayPatternElement::PropertyAccess {
                    access,
                    default_init,
                } => {
                    self.scan_property_access(
                        owner_id,
                        access,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                }
                ArrayPatternElement::Pattern {
                    pattern,
                    default_init,
                } => {
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                    match pattern {
                        Pattern::Array(pattern) => self.scan_array_assignment_pattern_expressions(
                            owner_id,
                            pattern,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        ),
                        Pattern::Object(pattern) => self
                            .scan_object_assignment_pattern_expressions(
                                owner_id,
                                pattern,
                                interner,
                                source_text,
                                self_name,
                                capture_aliases,
                                refs,
                            ),
                    }
                }
                ArrayPatternElement::PropertyAccessRest { access } => {
                    self.scan_property_access(
                        owner_id,
                        access,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
                ArrayPatternElement::PatternRest { pattern } => match pattern {
                    Pattern::Array(pattern) => self.scan_array_assignment_pattern_expressions(
                        owner_id,
                        pattern,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    ),
                    Pattern::Object(pattern) => self.scan_object_assignment_pattern_expressions(
                        owner_id,
                        pattern,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    ),
                },
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_object_assignment_pattern_expressions(
        &mut self,
        owner_id: &str,
        pattern: &'a ObjectPattern,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        for element in pattern.bindings() {
            match element {
                ObjectPatternElement::SingleName {
                    name,
                    ident,
                    default_init,
                } => {
                    self.scan_property_name(
                        owner_id,
                        name,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                    self.record_ref(
                        owner_id,
                        interner.resolve_expect(ident.sym()).to_string(),
                        capture_aliases,
                        refs,
                    );
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                }
                ObjectPatternElement::Pattern {
                    name,
                    pattern,
                    default_init,
                } => {
                    self.scan_property_name(
                        owner_id,
                        name,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                    match pattern {
                        Pattern::Array(pattern) => self.scan_array_assignment_pattern_expressions(
                            owner_id,
                            pattern,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        ),
                        Pattern::Object(pattern) => self
                            .scan_object_assignment_pattern_expressions(
                                owner_id,
                                pattern,
                                interner,
                                source_text,
                                self_name,
                                capture_aliases,
                                refs,
                            ),
                    }
                }
                ObjectPatternElement::AssignmentPropertyAccess {
                    name,
                    access,
                    default_init,
                } => {
                    self.scan_property_name(
                        owner_id,
                        name,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                    self.scan_property_access(
                        owner_id,
                        access,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                    if let Some(default) = default_init {
                        self.scan_expression(
                            owner_id,
                            default,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                }
                ObjectPatternElement::RestProperty { ident } => {
                    self.record_ref(
                        owner_id,
                        interner.resolve_expect(ident.sym()).to_string(),
                        capture_aliases,
                        refs,
                    );
                }
                ObjectPatternElement::AssignmentRestPropertyAccess { access } => {
                    self.scan_property_access(
                        owner_id,
                        access,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
        }
    }

    fn scan_assignment_pattern_expressions(
        &mut self,
        owner_id: &str,
        pattern: &'a Pattern,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        match pattern {
            Pattern::Array(pattern) => self.scan_array_assignment_pattern_expressions(
                owner_id,
                pattern,
                interner,
                source_text,
                self_name,
                capture_aliases,
                refs,
            ),
            Pattern::Object(pattern) => self.scan_object_assignment_pattern_expressions(
                owner_id,
                pattern,
                interner,
                source_text,
                self_name,
                capture_aliases,
                refs,
            ),
        }
    }

    fn scan_expression(
        &mut self,
        owner_id: &str,
        expression: &'a Expression,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        self.source_names_by_storage.extend(
            capture_aliases
                .iter()
                .map(|(source, storage)| (storage.clone(), source.clone())),
        );
        match expression {
            Expression::Identifier(identifier) => {
                let name = interner.resolve_expect(identifier.sym()).to_string();
                if name == "arguments" {
                    let owner = self.owner_plans.get(owner_id);
                    let source_binds_arguments =
                        owner.is_some_and(|owner| owner.root_bindings.contains("arguments"));
                    if !source_binds_arguments {
                        if owner.is_some_and(|owner| owner.flavor == FunctionFlavor::Arrow) {
                            self.record_ref(
                                owner_id,
                                LEXICAL_ARGUMENTS_NAME.to_string(),
                                capture_aliases,
                                refs,
                            );
                        } else if owner_id != SCRIPT_OWNER_ID {
                            self.record_ref(
                                owner_id,
                                LEXICAL_ARGUMENTS_NAME.to_string(),
                                capture_aliases,
                                refs,
                            );
                        } else {
                            self.record_ref(owner_id, name, capture_aliases, refs);
                        }
                        return;
                    }
                }
                self.record_ref(owner_id, name, capture_aliases, refs);
            }
            Expression::Parenthesized(expression) => {
                self.scan_expression(
                    owner_id,
                    expression.expression(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Expression::ArrayLiteral(array) => {
                for element in array.as_ref().iter().flatten() {
                    self.scan_expression(
                        owner_id,
                        element,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            Expression::ObjectLiteral(object) => {
                for property in object.properties() {
                    match property {
                        PropertyDefinition::Property(name, value) => {
                            self.scan_property_name(
                                owner_id,
                                name,
                                interner,
                                source_text,
                                self_name,
                                capture_aliases,
                                refs,
                            );
                            self.scan_expression(
                                owner_id,
                                value,
                                interner,
                                source_text,
                                self_name,
                                capture_aliases,
                                refs,
                            );
                        }
                        PropertyDefinition::SpreadObject(value) => {
                            self.scan_expression(
                                owner_id,
                                value,
                                interner,
                                source_text,
                                self_name,
                                capture_aliases,
                                refs,
                            );
                        }
                        PropertyDefinition::MethodDefinition(method) => {
                            self.scan_property_name(
                                owner_id,
                                method.name(),
                                interner,
                                source_text,
                                self_name,
                                capture_aliases,
                                refs,
                            );
                            let key = object_method_key(method);
                            if !self.function_expr_ids.contains_key(&key) {
                                let id = self.alloc_function_id();
                                self.function_expr_ids.insert(key, id.clone());
                                let name = method
                                    .name()
                                    .prop_name()
                                    .map(|identifier| {
                                        encode_js_string_utf16(
                                            interner.resolve_expect(identifier.sym()).utf16(),
                                        )
                                    })
                                    .unwrap_or_else(|| "<method>".to_string());
                                let pending = PendingFunction {
                                    id,
                                    name,
                                    to_string_representation:
                                        CallableToStringRepresentation::ExactSource(
                                            object_method_source_slice(method, source_text),
                                        ),
                                    protocol: object_method_protocol(method.kind())
                                        .function_protocol(),
                                    strict: self.scan_is_strict(owner_id)
                                        || method.body().strict(),
                                    self_binding_name: None,
                                    parameters: method.parameters(),
                                    body: method.body(),
                                    is_expression: true,
                                    capture_aliases: capture_aliases.clone(),
                                };
                                self.collect_function_plan(
                                    pending,
                                    owner_id.to_string(),
                                    self.current_environment_cursor(),
                                    interner,
                                    source_text,
                                );
                            }
                        }
                        PropertyDefinition::IdentifierReference(identifier) => {
                            self.record_ref(
                                owner_id,
                                interner.resolve_expect(identifier.sym()).to_string(),
                                capture_aliases,
                                refs,
                            );
                        }
                        PropertyDefinition::CoverInitializedName(_, value) => {
                            self.scan_expression(
                                owner_id,
                                value,
                                interner,
                                source_text,
                                self_name,
                                capture_aliases,
                                refs,
                            );
                        }
                    }
                }
            }
            Expression::Unary(unary) => {
                self.scan_expression(
                    owner_id,
                    unary.target(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Expression::Binary(binary) => {
                self.scan_expression(
                    owner_id,
                    binary.lhs(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                self.scan_expression(
                    owner_id,
                    binary.rhs(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Expression::Assign(assign) => {
                match assign.lhs() {
                    AssignTarget::Identifier(identifier) => {
                        self.record_ref(
                            owner_id,
                            interner.resolve_expect(identifier.sym()).to_string(),
                            capture_aliases,
                            refs,
                        );
                    }
                    AssignTarget::Access(access) => {
                        self.scan_property_access(
                            owner_id,
                            access,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                    AssignTarget::Pattern(pattern) => {
                        self.scan_assignment_pattern_expressions(
                            owner_id,
                            pattern,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                    _ => {}
                }
                self.scan_expression(
                    owner_id,
                    assign.rhs(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Expression::Update(update) => match update.target() {
                UpdateTarget::Identifier(identifier) => {
                    self.record_ref(
                        owner_id,
                        interner.resolve_expect(identifier.sym()).to_string(),
                        capture_aliases,
                        refs,
                    );
                }
                UpdateTarget::PropertyAccess(access) => self.scan_property_access(
                    owner_id,
                    access,
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                ),
                UpdateTarget::WebCompatCall(call) => {
                    self.scan_expression(
                        owner_id,
                        call.function(),
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                    for argument in call.args() {
                        self.scan_expression(
                            owner_id,
                            argument,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                }
            },
            Expression::Call(call) => {
                if matches!(call.function().flatten(), Expression::Identifier(identifier)
                    if identifier.sym() == boa_interner::Sym::EVAL)
                {
                    self.record_arrow_direct_eval_context_refs(owner_id, capture_aliases, refs);
                }
                self.scan_expression(
                    owner_id,
                    call.function(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                for arg in call.args() {
                    self.scan_expression(
                        owner_id,
                        arg,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            Expression::PropertyAccess(access) => {
                self.scan_property_access(
                    owner_id,
                    access,
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Expression::Optional(optional) => {
                self.scan_expression(
                    owner_id,
                    optional.target(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                for operation in optional.chain() {
                    match operation.kind() {
                        OptionalOperationKind::SimplePropertyAccess {
                            field: PropertyAccessField::Expr(expr),
                        } => self.scan_expression(
                            owner_id,
                            expr,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        ),
                        OptionalOperationKind::Call { args } => {
                            for arg in args {
                                self.scan_expression(
                                    owner_id,
                                    arg,
                                    interner,
                                    source_text,
                                    self_name,
                                    capture_aliases,
                                    refs,
                                );
                            }
                        }
                        OptionalOperationKind::SimplePropertyAccess {
                            field: PropertyAccessField::Const(_),
                        }
                        | OptionalOperationKind::PrivatePropertyAccess { .. } => {}
                    }
                }
            }
            Expression::FunctionExpression(function) => {
                let key = function_expression_key(function);
                if !self.function_expr_ids.contains_key(&key) {
                    let id = self.alloc_function_id();
                    self.function_expr_ids.insert(key, id.clone());
                    let name = function
                        .name()
                        .map(|identifier| interner.resolve_expect(identifier.sym()).to_string())
                        .unwrap_or_default();
                    let self_binding_name = function.has_binding_identifier().then(|| name.clone());
                    let pending = PendingFunction {
                        id,
                        name,
                        to_string_representation: CallableToStringRepresentation::ExactSource(
                            function_expression_source_slice(function, source_text),
                        ),
                        protocol: FunctionProtocolIr::OrdinaryCallAndConstruct,
                        strict: self.scan_is_strict(owner_id)
                            || function.body().strict(),
                        self_binding_name,
                        parameters: function.parameters(),
                        body: function.body(),
                        is_expression: true,
                        capture_aliases: capture_aliases.clone(),
                    };
                    self.collect_function_plan(
                        pending,
                        owner_id.to_string(),
                        self.current_environment_cursor(),
                        interner,
                        source_text,
                    );
                }
            }
            Expression::AsyncFunctionExpression(function) => {
                let key = async_function_expression_key(function);
                if !self.function_expr_ids.contains_key(&key) {
                    let id = self.alloc_function_id();
                    self.function_expr_ids.insert(key, id.clone());
                    let name = function
                        .name()
                        .map(|identifier| interner.resolve_expect(identifier.sym()).to_string())
                        .unwrap_or_default();
                    let self_binding_name = function.has_binding_identifier().then(|| name.clone());
                    let pending = PendingFunction {
                        id,
                        name,
                        to_string_representation: CallableToStringRepresentation::ExactSource(
                            async_function_expression_source_slice(function, source_text),
                        ),
                        protocol: FunctionProtocolIr::Async,
                        strict: self.scan_is_strict(owner_id)
                            || function.body().strict(),
                        self_binding_name,
                        parameters: function.parameters(),
                        body: function.body(),
                        is_expression: true,
                        capture_aliases: capture_aliases.clone(),
                    };
                    self.collect_function_plan(
                        pending,
                        owner_id.to_string(),
                        self.current_environment_cursor(),
                        interner,
                        source_text,
                    );
                }
            }
            Expression::AsyncGeneratorExpression(function) => {
                let key = async_generator_expression_key(function);
                if !self.function_expr_ids.contains_key(&key) {
                    let id = self.alloc_function_id();
                    self.function_expr_ids.insert(key, id.clone());
                    let name = function
                        .name()
                        .map(|identifier| interner.resolve_expect(identifier.sym()).to_string())
                        .unwrap_or_default();
                    let self_binding_name = function.has_binding_identifier().then(|| name.clone());
                    let pending = PendingFunction {
                        id,
                        name,
                        to_string_representation: CallableToStringRepresentation::ExactSource(
                            async_generator_expression_source_slice(function, source_text),
                        ),
                        protocol: FunctionProtocolIr::AsyncGenerator,
                        strict: self.scan_is_strict(owner_id)
                            || function.body().strict(),
                        self_binding_name,
                        parameters: function.parameters(),
                        body: function.body(),
                        is_expression: true,
                        capture_aliases: capture_aliases.clone(),
                    };
                    self.collect_function_plan(
                        pending,
                        owner_id.to_string(),
                        self.current_environment_cursor(),
                        interner,
                        source_text,
                    );
                }
            }
            Expression::GeneratorExpression(function)
                if generator_function_is_aot_supported(function.body(), function.parameters()) =>
            {
                let key = generator_expression_key(function);
                if !self.function_expr_ids.contains_key(&key) {
                    let id = self.alloc_function_id();
                    self.function_expr_ids.insert(key, id.clone());
                    let name = function
                        .name()
                        .map(|identifier| interner.resolve_expect(identifier.sym()).to_string())
                        .unwrap_or_default();
                    let self_binding_name = function.has_binding_identifier().then(|| name.clone());
                    let pending = PendingFunction {
                        id,
                        name,
                        to_string_representation: CallableToStringRepresentation::ExactSource(
                            generator_expression_source_slice(function, source_text),
                        ),
                        protocol: FunctionProtocolIr::Generator,
                        strict: self.scan_is_strict(owner_id)
                            || function.body().strict(),
                        self_binding_name,
                        parameters: function.parameters(),
                        body: function.body(),
                        is_expression: true,
                        capture_aliases: capture_aliases.clone(),
                    };
                    self.collect_function_plan(
                        pending,
                        owner_id.to_string(),
                        self.current_environment_cursor(),
                        interner,
                        source_text,
                    );
                }
            }
            Expression::ArrowFunction(function) => {
                let key = arrow_function_key(function);
                if !self.function_expr_ids.contains_key(&key) {
                    let id = self.alloc_function_id();
                    self.function_expr_ids.insert(key, id.clone());
                    let pending = PendingFunction {
                        id,
                        name: function
                            .name()
                            .map(|identifier| interner.resolve_expect(identifier.sym()).to_string())
                            .unwrap_or_default(),
                        to_string_representation: CallableToStringRepresentation::ExactSource(
                            arrow_function_source_slice(function, source_text),
                        ),
                        protocol: FunctionProtocolIr::Arrow,
                        strict: self.scan_is_strict(owner_id)
                            || function.body().strict(),
                        self_binding_name: None,
                        parameters: function.parameters(),
                        body: function.body(),
                        is_expression: true,
                        capture_aliases: capture_aliases.clone(),
                    };
                    self.collect_function_plan(
                        pending,
                        owner_id.to_string(),
                        self.current_environment_cursor(),
                        interner,
                        source_text,
                    );
                }
            }
            Expression::AsyncArrowFunction(function) => {
                let key = async_arrow_function_key(function);
                if !self.function_expr_ids.contains_key(&key) {
                    let id = self.alloc_function_id();
                    self.function_expr_ids.insert(key, id.clone());
                    let pending = PendingFunction {
                        id,
                        name: function
                            .name()
                            .map(|identifier| interner.resolve_expect(identifier.sym()).to_string())
                            .unwrap_or_default(),
                        to_string_representation: CallableToStringRepresentation::ExactSource(
                            async_arrow_function_source_slice(function, source_text),
                        ),
                        protocol: FunctionProtocolIr::AsyncArrow,
                        strict: self.scan_is_strict(owner_id)
                            || function.body().strict(),
                        self_binding_name: None,
                        parameters: function.parameters(),
                        body: function.body(),
                        is_expression: true,
                        capture_aliases: capture_aliases.clone(),
                    };
                    self.collect_function_plan(
                        pending,
                        owner_id.to_string(),
                        self.current_environment_cursor(),
                        interner,
                        source_text,
                    );
                }
            }
            Expression::ClassExpression(class) => {
                let constructor_execution_key = class
                    .constructor()
                    .map(class_constructor_key)
                    .unwrap_or_else(|| class_default_constructor_key(class.linear_span()));
                self.scan_class_definition(
                    owner_id,
                    SourceClassName::from_expression(class, interner),
                    constructor_execution_key,
                    class.super_ref(),
                    class.constructor(),
                    class.elements(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Expression::SuperCall(call) => {
                self.record_derived_activation_refs(owner_id, capture_aliases, refs);
                for arg in call.arguments() {
                    self.scan_expression(
                        owner_id,
                        arg,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            Expression::This(_) => {
                if self
                    .owner_plans
                    .get(owner_id)
                    .is_some_and(|owner| owner.flavor == FunctionFlavor::Arrow)
                {
                    self.record_ref(
                        owner_id,
                        LEXICAL_THIS_NAME.to_string(),
                        capture_aliases,
                        refs,
                    );
                    self.record_derived_activation_refs(owner_id, capture_aliases, refs);
                }
            }
            Expression::NewTarget(_) => {
                if self
                    .owner_plans
                    .get(owner_id)
                    .is_some_and(|owner| owner.flavor == FunctionFlavor::Arrow)
                {
                    self.record_ref(
                        owner_id,
                        LEXICAL_NEW_TARGET_NAME.to_string(),
                        capture_aliases,
                        refs,
                    );
                    self.record_derived_activation_refs(owner_id, capture_aliases, refs);
                }
            }
            Expression::New(new_expr) => {
                self.scan_expression(
                    owner_id,
                    new_expr.constructor(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                for arg in new_expr.arguments() {
                    self.scan_expression(
                        owner_id,
                        arg,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            Expression::TemplateLiteral(template) => {
                for element in template.elements() {
                    if let TemplateElement::Expr(expr) = element {
                        self.scan_expression(
                            owner_id,
                            expr,
                            interner,
                            source_text,
                            self_name,
                            capture_aliases,
                            refs,
                        );
                    }
                }
            }
            Expression::TaggedTemplate(template) => {
                self.scan_expression(
                    owner_id,
                    template.tag(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                for expr in template.exprs() {
                    self.scan_expression(
                        owner_id,
                        expr,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            Expression::Conditional(conditional) => {
                self.scan_expression(
                    owner_id,
                    conditional.condition(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                self.scan_expression(
                    owner_id,
                    conditional.if_true(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                self.scan_expression(
                    owner_id,
                    conditional.if_false(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Expression::BinaryInPrivate(binary) => {
                self.scan_expression(
                    owner_id,
                    binary.rhs(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
            Expression::Await(await_expression) => self.scan_expression(
                owner_id,
                await_expression.target(),
                interner,
                source_text,
                self_name,
                capture_aliases,
                refs,
            ),
            Expression::Yield(yield_expression) => {
                if let Some(target) = yield_expression.target() {
                    self.scan_expression(
                        owner_id,
                        target,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            Expression::Literal(_)
            | Expression::RegExpLiteral(_)
            | Expression::GeneratorExpression(_)
            // `import.meta` has no operands; `import(x)` does, and they are
            // ordinary expressions whose identifiers must be registered or the
            // capture sets come out wrong.
            | Expression::ImportMeta(_)
            | Expression::FormalParameterList(_)
            | Expression::Debugger => {}
            Expression::ImportCall(call) => {
                self.scan_expression(
                    owner_id,
                    call.argument(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                if let Some(options) = call.options() {
                    self.scan_expression(
                        owner_id,
                        options,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            Expression::Spread(spread) => {
                self.scan_expression(
                    owner_id,
                    spread.target(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
        }
        let _ = self_name;
    }

    fn scan_property_name(
        &mut self,
        owner_id: &str,
        name: &'a PropertyName,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        if let PropertyName::Computed(expr) = name {
            self.scan_expression(
                owner_id,
                expr,
                interner,
                source_text,
                self_name,
                capture_aliases,
                refs,
            );
        }
    }

    fn scan_property_access(
        &mut self,
        owner_id: &str,
        access: &'a PropertyAccess,
        interner: &'a Interner,
        source_text: CompilerSourceText<'a>,
        self_name: Option<&str>,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        match access {
            PropertyAccess::Simple(access) => {
                self.scan_expression(
                    owner_id,
                    access.target(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
                if let PropertyAccessField::Expr(expr) = access.field() {
                    self.scan_expression(
                        owner_id,
                        expr,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            PropertyAccess::Super(access) => {
                self.record_lexical_super_property_refs(owner_id, capture_aliases, refs);
                if let PropertyAccessField::Expr(expr) = access.field() {
                    self.scan_expression(
                        owner_id,
                        expr,
                        interner,
                        source_text,
                        self_name,
                        capture_aliases,
                        refs,
                    );
                }
            }
            PropertyAccess::Private(access) => {
                self.scan_expression(
                    owner_id,
                    access.target(),
                    interner,
                    source_text,
                    self_name,
                    capture_aliases,
                    refs,
                );
            }
        }
    }

    fn record_arrow_direct_eval_context_refs(
        &self,
        owner_id: &str,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        let Some(owner) = self.owner_plans.get(owner_id) else {
            return;
        };
        if owner.flavor != FunctionFlavor::Arrow {
            return;
        }
        // A potential direct eval can observe lexical context without this,
        // new.target or super appearing in the arrow's own syntax. Capture the
        // same context as those expressions; the runtime intrinsic check still
        // distinguishes a shadowed eval from the actual direct-eval operation.
        self.record_ref(
            owner_id,
            LEXICAL_THIS_NAME.to_string(),
            capture_aliases,
            refs,
        );
        let mut lexical_owner = owner;
        while lexical_owner.flavor == FunctionFlavor::Arrow {
            let parent = lexical_owner
                .parent_owner_id
                .as_deref()
                .expect("an arrow has a lexical owner");
            lexical_owner = &self.owner_plans[parent];
        }
        if lexical_owner
            .root_bindings
            .contains(LEXICAL_NEW_TARGET_NAME)
        {
            self.record_ref(
                owner_id,
                LEXICAL_NEW_TARGET_NAME.to_string(),
                capture_aliases,
                refs,
            );
        }
        self.record_lexical_super_property_refs(owner_id, capture_aliases, refs);
    }

    fn record_lexical_super_property_refs(
        &self,
        owner_id: &str,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        if !self
            .owner_plans
            .get(owner_id)
            .is_some_and(|owner| owner.flavor == FunctionFlavor::Arrow)
        {
            return;
        }
        match self.lexical_super_owner_role(owner_id) {
            LexicalSuperOwnerRole::None => {}
            LexicalSuperOwnerRole::HomeObject => {
                for name in [LEXICAL_THIS_NAME, LEXICAL_HOME_OBJECT_NAME] {
                    self.record_ref(owner_id, name.to_string(), capture_aliases, refs);
                }
            }
            LexicalSuperOwnerRole::DerivedConstructorActivation => {
                self.record_derived_activation_binding_refs(owner_id, capture_aliases, refs);
            }
        }
    }

    fn record_derived_activation_refs(
        &self,
        owner_id: &str,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        if !self
            .owner_plans
            .get(owner_id)
            .is_some_and(|owner| owner.flavor == FunctionFlavor::Arrow)
            || self.lexical_super_owner_role(owner_id)
                != LexicalSuperOwnerRole::DerivedConstructorActivation
        {
            return;
        }
        self.record_derived_activation_binding_refs(owner_id, capture_aliases, refs);
    }

    fn record_derived_activation_binding_refs(
        &self,
        owner_id: &str,
        capture_aliases: &BTreeMap<String, String>,
        refs: &mut BTreeMap<String, String>,
    ) {
        for name in [
            DERIVED_ACTIVATION_THIS_NAME,
            DERIVED_ACTIVATION_THIS_STATUS_NAME,
            DERIVED_ACTIVATION_NEW_TARGET_NAME,
            DERIVED_ACTIVATION_FUNCTION_NAME,
        ] {
            self.record_ref(owner_id, name.to_string(), capture_aliases, refs);
        }
    }

    fn surrounding_with_object_environment_ids(&self, owner_id: &str) -> Vec<EnvironmentId> {
        let mut cursor = self
            .owner_plans
            .get(owner_id)
            .map(|owner| owner.definition_environment_cursor.clone());
        let mut environments = Vec::new();
        while let Some(current) = cursor {
            let environment = self
                .environment_plans
                .get(&current.environment_id)
                .expect("definition environment cursor must name a planned environment");
            if environment.kind == EnvironmentKind::WithObject {
                environments.push(environment.id);
            }
            cursor = environment.parent_cursor.clone();
        }
        environments
    }

    fn finalize_capture_plans(&mut self, interner: &Interner) {
        let mut owned_names = BTreeMap::<EnvironmentId, BTreeSet<String>>::new();
        self.prepare_direct_eval_context_captures(&mut owned_names);
        for environment in self.environment_plans.values().filter(|environment| {
            environment.eval_visible
                || (environment.kind == EnvironmentKind::WithObject
                    && matches!(
                        self.with_object_environment_plans[&environment.id].continuation_owner,
                        WithContinuationOwner::OrdinaryWhole
                            | WithContinuationOwner::PlainAsyncWhole
                            | WithContinuationOwner::MixedAsyncGeneratorWhole(_)
                    ))
        }) {
            owned_names.entry(environment.id).or_default().extend(
                environment
                    .binding_storage_names
                    .iter()
                    .filter(|name| {
                        !self
                            .module_local_candidates
                            .get(&environment.owner_id)
                            .is_some_and(|locals| locals.contains(*name))
                    })
                    .cloned(),
            );
        }

        for owner in self.owner_plans.values() {
            if let Some(body_id) = owner.body_environment_id {
                owned_names.entry(body_id).or_default().extend(
                    self.environment_plans[&body_id]
                        .binding_storage_names
                        .iter()
                        .cloned(),
                );
                owned_names
                    .entry(owner.activation_environment_id)
                    .or_default()
                    .extend(
                        self.environment_plans[&owner.activation_environment_id]
                            .binding_storage_names
                            .iter()
                            .cloned(),
                    );
            }
        }

        for environment_id in &self.complete_resumable_for_of_iteration_environment_ids {
            let environment = self
                .environment_plans
                .get(environment_id)
                .expect("a complete resumable for-of environment must be planned");
            owned_names
                .entry(*environment_id)
                .or_default()
                .extend(environment.binding_storage_names.iter().cloned());
        }
        // Every derived constructor has a canonical per-invocation activation,
        // even when no nested arrow currently captures it. Direct `super()`,
        // derived `this`, and completion normalization all share these slots.
        for owner in self.owner_plans.values() {
            if owner.lexical_super_owner_role == LexicalSuperOwnerRole::DerivedConstructorActivation
            {
                owned_names
                    .entry(owner.activation_environment_id)
                    .or_default()
                    .extend([
                        DERIVED_ACTIVATION_THIS_NAME.to_string(),
                        DERIVED_ACTIVATION_THIS_STATUS_NAME.to_string(),
                        DERIVED_ACTIVATION_NEW_TARGET_NAME.to_string(),
                        DERIVED_ACTIVATION_FUNCTION_NAME.to_string(),
                    ]);
            }
        }
        let function_ids = self.function_order.clone();
        for function_id in function_ids {
            let Some(function) = self.function_plans.get(&function_id).cloned() else {
                continue;
            };
            let local_bindings = self
                .owner_plans
                .get(&function.id)
                .map(|owner| owner.root_bindings.clone())
                .unwrap_or_default();
            let free_refs = self
                .function_free_refs
                .get(&function.id)
                .cloned()
                .unwrap_or_default();
            let mut captures = BTreeMap::new();
            for (name, source_name) in free_refs {
                if local_bindings.contains(&name)
                    && !self.owner_plans[&function.id]
                        .parameter_external_refs
                        .contains(&name)
                {
                    continue;
                }
                let Some(environment_id) = self.resolve_capture_environment(&function.id, &name)
                else {
                    continue;
                };
                let environment = &self.environment_plans[&environment_id];
                let owner_id = environment.owner_id.clone();
                let mode = *environment
                    .binding_modes
                    .get(&name)
                    .expect("captured binding mode must be planned on its physical environment");
                owned_names
                    .entry(environment_id)
                    .or_default()
                    .insert(name.clone());
                captures.insert(
                    name,
                    CaptureBindingPlan {
                        owner_id,
                        environment_id,
                        source_name,
                        mode,
                        slot: 0,
                        hops: 0,
                    },
                );
            }
            // An Object Environment Record can supply any identifier name at
            // run time, so source-name free-reference analysis cannot decide
            // whether the function needs it. Carry every surrounding binding
            // object unconditionally. The hidden binding is source-unspellable
            // and therefore cannot collide with an ordinary capture.
            for environment_id in self.surrounding_with_object_environment_ids(&function.id) {
                let environment = &self.environment_plans[&environment_id];
                let binding_name = self.with_object_environment_plans[&environment_id]
                    .binding_name
                    .as_str()
                    .to_string();
                owned_names
                    .entry(environment_id)
                    .or_default()
                    .insert(binding_name.clone());
                captures.insert(
                    binding_name.clone(),
                    CaptureBindingPlan {
                        owner_id: environment.owner_id.clone(),
                        environment_id,
                        source_name: binding_name,
                        mode: *environment
                            .binding_modes
                            .get(
                                self.with_object_environment_plans[&environment_id]
                                    .binding_name
                                    .as_str(),
                            )
                            .expect("with-object binding mode must be planned"),
                        slot: 0,
                        hops: 0,
                    },
                );
            }
            if let Some(plan) = self.function_plans.get_mut(&function.id) {
                plan.captures = captures;
            }
        }

        let mut class_owner_ids = self
            .class_execution_ids
            .values()
            .cloned()
            .collect::<Vec<_>>();
        class_owner_ids.sort_by(|left, right| {
            self.owner_depth(right)
                .cmp(&self.owner_depth(left))
                .then_with(|| left.cmp(right))
        });
        for owner_id in class_owner_ids {
            let local_bindings = self
                .owner_plans
                .get(&owner_id)
                .unwrap_or_else(|| panic!("class execution owner `{owner_id}` must be planned"))
                .root_bindings
                .clone();
            let mut free_refs = self
                .function_free_refs
                .get(&owner_id)
                .cloned()
                .unwrap_or_else(|| {
                    panic!("class execution owner `{owner_id}` must have scanned references")
                });
            for function in self.function_plans.values() {
                if !self.owner_descends_from(&function.id, &owner_id) {
                    continue;
                }
                for (name, capture) in &function.captures {
                    if self.resolve_capture_environment(&owner_id, name)
                        != Some(capture.environment_id)
                    {
                        continue;
                    }
                    free_refs
                        .entry(name.clone())
                        .or_insert_with(|| capture.source_name.clone());
                }
            }
            let child_owner_refs = self
                .class_execution_ids
                .values()
                .filter(|child_id| {
                    self.owner_plans
                        .get(*child_id)
                        .and_then(|owner| owner.parent_owner_id.as_deref())
                        == Some(owner_id.as_str())
                })
                .map(|child_id| {
                    let refs = self.function_free_refs.get(child_id).unwrap_or_else(|| {
                        panic!("class execution child `{child_id}` must have finalized references")
                    });
                    (child_id.clone(), refs.clone())
                })
                .collect::<Vec<_>>();
            for (child_id, child_refs) in child_owner_refs {
                for (name, source_name) in child_refs {
                    let target_environment = self.resolve_capture_environment(&owner_id, &name);
                    if target_environment.is_none()
                        || target_environment != self.resolve_capture_environment(&child_id, &name)
                    {
                        continue;
                    }
                    free_refs.entry(name).or_insert(source_name);
                }
            }
            free_refs.retain(|name, _| {
                !local_bindings.contains(name)
                    || self.owner_plans[&owner_id]
                        .parameter_external_refs
                        .contains(name)
            });
            for name in free_refs.keys() {
                if let Some(environment_id) = self.resolve_capture_environment(&owner_id, name) {
                    owned_names
                        .entry(environment_id)
                        .or_default()
                        .insert(name.clone());
                }
            }
            self.function_free_refs.insert(owner_id, free_refs);
        }

        for function in self.function_plans.values() {
            if !matches!(
                function.protocol.execution_kind(),
                FunctionExecutionKind::Generator
                    | FunctionExecutionKind::Async
                    | FunctionExecutionKind::AsyncGenerator
            ) {
                continue;
            }
            let owner = self
                .owner_plans
                .get(&function.id)
                .expect("generator owner must be planned");
            // Uncaptured lexical cells can live in the resumable activation,
            // including block and loop-head cells. A cell captured in a real
            // Environment Record must remain there: duplicating it would make
            // resumed reads disagree with closure capture hops. Decide only
            // after capture discovery has established those physical owners.
            let activation_binding_names = owner
                .root_bindings
                .iter()
                .filter(|name| {
                    self.physical_binding_environments
                        .get(*name)
                        .is_some_and(|environments| {
                            environments.iter().any(|environment_id| {
                                self.environment_plans[environment_id].owner_id == function.id
                            }) && environments.iter().all(|environment_id| {
                                let environment = &self.environment_plans[environment_id];
                                environment.owner_id != function.id
                                    || *environment_id == owner.activation_environment_id
                                    || !owned_names
                                        .get(environment_id)
                                        .is_some_and(|names| names.contains(*name))
                            })
                        })
                })
                .cloned()
                .collect::<Vec<_>>();
            owned_names
                .entry(owner.activation_environment_id)
                .or_default()
                .extend(activation_binding_names);
        }
        for owner in self.owner_plans.values() {
            let activation = self
                .environment_plans
                .get_mut(&owner.activation_environment_id)
                .expect("activation environment must be planned");
            activation.owned_env_slots = owner.owned_env_slots.clone();
        }

        for (environment_id, names) in owned_names {
            let owner_id = self.environment_plans[&environment_id].owner_id.clone();
            let is_activation =
                self.owner_plans[&owner_id].activation_environment_id == environment_id;
            let is_derived_constructor = self.owner_plans[&owner_id].lexical_super_owner_role
                == LexicalSuperOwnerRole::DerivedConstructorActivation;
            let environment = self
                .environment_plans
                .get_mut(&environment_id)
                .expect("captured binding environment must be planned");
            let mut next_slot = environment
                .owned_env_slots
                .values()
                .copied()
                .max()
                .map(|slot| slot + 1)
                .unwrap_or(0);
            if is_activation && is_derived_constructor {
                for name in [
                    DERIVED_ACTIVATION_FUNCTION_NAME,
                    DERIVED_ACTIVATION_NEW_TARGET_NAME,
                    DERIVED_ACTIVATION_THIS_NAME,
                    DERIVED_ACTIVATION_THIS_STATUS_NAME,
                ] {
                    environment
                        .owned_env_slots
                        .entry(name.to_string())
                        .or_insert_with(|| {
                            let slot = next_slot;
                            next_slot += 1;
                            slot
                        });
                }
            }
            next_slot = environment
                .owned_env_slots
                .values()
                .copied()
                .max()
                .map(|slot| slot + 1)
                .unwrap_or(next_slot);
            for name in names {
                environment.owned_env_slots.entry(name).or_insert_with(|| {
                    let slot = next_slot;
                    next_slot += 1;
                    slot
                });
            }
        }

        // Capture discovery has now retained every actual cell. Remove empty
        // local records before named roles and capture hops are finalized.
        // Nonempty records keep their named bindings for nested closures.
        for environment in self.environment_plans.values_mut() {
            if self
                .module_local_candidates
                .contains_key(&environment.owner_id)
                && environment.owned_env_slots.is_empty()
            {
                environment.eval_visible = false;
            }
        }
        self.finalize_eval_environment_roles(interner);

        for owner in self.owner_plans.values_mut() {
            owner.owned_env_slots = self.environment_plans[&owner.activation_environment_id]
                .owned_env_slots
                .clone();
        }

        let function_ids = self.function_order.clone();
        for function_id in function_ids {
            let Some(function) = self.function_plans.get(&function_id).cloned() else {
                continue;
            };
            let mut captures = function.captures;
            let names: Vec<String> = captures.keys().cloned().collect();
            for name in names {
                if let Some(capture) = captures.get_mut(&name) {
                    capture.hops = self.capture_hops(&function.id, capture.environment_id);
                    capture.slot =
                        self.environment_plans[&capture.environment_id].owned_env_slots[&name];
                }
            }
            if let Some(plan) = self.function_plans.get_mut(&function_id) {
                plan.captures = captures;
                plan.lexical_derived_activation_owner = plan
                    .captures
                    .get(DERIVED_ACTIVATION_FUNCTION_NAME)
                    .map(|capture| capture.owner_id.clone());
            }
        }
    }

    fn owner_depth(&self, owner_id: &str) -> usize {
        let mut depth = 0;
        let mut current = Some(owner_id);
        while let Some(id) = current {
            let owner = self
                .owner_plans
                .get(id)
                .unwrap_or_else(|| panic!("owner `{id}` must be planned"));
            depth += 1;
            current = owner.parent_owner_id.as_deref();
        }
        depth
    }

    fn owner_descends_from(&self, owner_id: &str, ancestor_owner_id: &str) -> bool {
        let mut current = self
            .owner_plans
            .get(owner_id)
            .and_then(|owner| owner.parent_owner_id.as_deref());
        while let Some(id) = current {
            if id == ancestor_owner_id {
                return true;
            }
            current = self
                .owner_plans
                .get(id)
                .and_then(|owner| owner.parent_owner_id.as_deref());
        }
        false
    }

    fn lexical_super_owner_role(&self, owner_id: &str) -> LexicalSuperOwnerRole {
        let mut current = Some(owner_id);
        while let Some(id) = current {
            let owner = self
                .owner_plans
                .get(id)
                .unwrap_or_else(|| panic!("lexical super owner `{id}` must be planned"));
            if owner.flavor != FunctionFlavor::Arrow {
                return owner.lexical_super_owner_role;
            }
            current = owner.parent_owner_id.as_deref();
        }
        LexicalSuperOwnerRole::None
    }

    fn resolve_capture_environment(
        &self,
        function_owner_id: &str,
        name: &str,
    ) -> Option<EnvironmentId> {
        let mut cursor = Some(
            self.owner_plans
                .get(function_owner_id)?
                .definition_environment_cursor
                .clone(),
        );
        while let Some(current) = cursor {
            let environment = self.environment_plans.get(&current.environment_id)?;
            if self
                .physical_binding_environments
                .get(name)
                .is_some_and(|environments| environments.contains(&environment.id))
            {
                return Some(if environment.kind.is_materialized() {
                    environment.id
                } else {
                    self.owner_plans[&environment.owner_id].activation_environment_id
                });
            }
            cursor = environment.parent_cursor.clone();
        }
        None
    }

    fn capture_hops(&self, current_owner_id: &str, target_environment_id: EnvironmentId) -> u32 {
        let owner = &self.owner_plans[current_owner_id];
        let activation = &self.environment_plans[&owner.activation_environment_id];
        let mut cursor = if !environment_has_runtime_storage(activation, owner.execution_kind) {
            owner
                .parent_owner_id
                .as_ref()
                .map(|_| owner.definition_environment_cursor.clone())
        } else {
            Some(EnvironmentCursor {
                owner_id: current_owner_id.to_string(),
                environment_id: owner.activation_environment_id,
            })
        };
        let mut hops = 0;
        while let Some(current) = cursor {
            let environment = &self.environment_plans[&current.environment_id];
            if environment_has_runtime_storage(
                environment,
                self.owner_plans[&environment.owner_id].execution_kind,
            ) {
                if environment.id == target_environment_id {
                    return hops;
                }
                hops += 1;
            }
            cursor = environment.parent_cursor.clone();
        }
        panic!("captured binding environment must be reachable from its function definition")
    }
}

#[cfg(test)]
mod private_environment_tests;
