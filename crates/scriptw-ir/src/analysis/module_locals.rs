//! Named storage does not imply dynamically interceptable binding lookup.
use super::*;

/// Module reuse may expose names without permitting dynamic binding creation.
/// Arguments storage alone does not invalidate unrelated cell identities.
#[derive(Debug, Clone, Default)]
pub(crate) struct ModuleCellOwners(BTreeSet<FunctionId>);

impl ModuleCellOwners {
    /// A cell's identity needs static scoping around its reader, not the
    /// reader's own local storage: an arrow function with the same plain
    /// parameter list reads its own and its captured cells like an ordinary
    /// function. It has no `this`, `arguments` or `new.target` cell of its
    /// own, and the synthetic ones it captures are never source names here.
    ///
    /// A binding pattern that holds no expression changes none of this: the
    /// owner keeps its one activation record, which holds the names the
    /// pattern binds, and BindingInitialization evaluates nothing of the
    /// owner's but those initializations. A pattern with an initializer or
    /// a computed key gives the owner a separate body record and is refused
    /// with it.
    pub(super) fn collect(builder: &AnalysisBuilder<'_>) -> Self {
        Self(builder.plain_module_owner_ids(
            &[
                FunctionProtocolIr::OrdinaryCallAndConstruct,
                FunctionProtocolIr::Arrow,
            ],
            PlainParameters::ExpressionFreePatterns,
        ))
    }
}

/// Both forms use the existing BindingCell; neither permits a new value fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenBindingCellOrigin {
    Owned,
    Captured { entry_hops: u32 },
}

/// A finalized cell whose identity cannot be intercepted by runtime lookup.
/// The source owner and storage name authenticate the same analyzed binding at
/// emission. The value remains mutable and may still be uninitialized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenBindingCellIr {
    owner: FunctionId,
    source_name: String,
    storage_name: String,
    slot: u32,
    origin: ProvenBindingCellOrigin,
}

impl ProvenBindingCellIr {
    pub(crate) fn analyze(
        analysis: &Analysis<'_>,
        owner_id: &str,
        output_owner_id: &str,
        storage_name: &str,
        source_name: &str,
    ) -> Option<Self> {
        // Collected before Module reuse is folded into eval_visible; actual
        // eval and Annex-B binding creation never receive this evidence.
        if !analysis.module_cell_owners.0.contains(owner_id)
            || source_name == "arguments"
            || source_name == LEXICAL_ARGUMENTS_NAME
            || storage_name == LEXICAL_ARGUMENTS_NAME
        {
            return None;
        }
        let function = analysis.function_plans.get(owner_id)?;
        if !matches!(
            function.protocol,
            FunctionProtocolIr::OrdinaryCallAndConstruct | FunctionProtocolIr::Arrow
        ) {
            return None;
        }
        let Some(capture) = function.captures.get(storage_name) else {
            // A lowered lexical storage name must identify one real current-
            // owner cell. In particular, do not guess between parameter/body
            // records or the distinct TDZ/iteration records of a loop head.
            let mut environments = analysis
                .physical_binding_environments
                .get(storage_name)?
                .iter()
                .filter_map(|id| analysis.environment_plans.get(id))
                .filter(|environment| {
                    environment.owner_id == owner_id
                        && environment.owned_env_slots.contains_key(storage_name)
                });
            let environment = environments.next()?;
            if environments.next().is_some() || !Self::declarative(environment.kind) {
                return None;
            }
            return Some(Self {
                owner: output_owner_id.to_string(),
                source_name: source_name.to_string(),
                storage_name: storage_name.to_string(),
                slot: *environment.owned_env_slots.get(storage_name)?,
                origin: ProvenBindingCellOrigin::Owned,
            });
        };
        let environment = analysis.environment_plans.get(&capture.environment_id)?;
        let owner = analysis.function_plans.get(&capture.owner_id)?;
        if capture.owner_id == SCRIPT_OWNER_ID
            || capture.owner_id == owner_id
            || environment.owner_id != capture.owner_id
            || capture.source_name != source_name
            || environment.owned_env_slots.get(storage_name) != Some(&capture.slot)
            || !Self::declarative(environment.kind)
        {
            return None;
        }
        let module_kind = match owner.protocol {
            FunctionProtocolIr::ModuleActivation => {
                Some(crate::ModuleActivationKindIr::Synchronous)
            }
            FunctionProtocolIr::AsyncModuleActivation => Some(crate::ModuleActivationKindIr::Async),
            _ => None,
        };
        if let Some(kind) = module_kind {
            let module_owner = analysis.owner_plans.get(&capture.owner_id)?;
            // The graph retains this one invocation environment before linking
            // imports or evaluating source. Block/iteration records and split
            // parameter/body records do not inherit that lifetime proof.
            if capture.environment_id != module_owner.activation_environment_id
                || environment.kind != EnvironmentKind::Activation
                || module_owner.body_environment_id.is_some()
                || module_owner.parameter_eval_environment_id.is_some()
                || module_owner.owned_env_slots.get(storage_name) != Some(&capture.slot)
                || !analysis.module_execution.graphs.values().any(|graph| {
                    graph.activations().iter().any(|activation| {
                        activation.function() == &capture.owner_id && activation.kind() == kind
                    })
                })
            {
                return None;
            }
        }
        // Only the authenticated Module activation may terminate this walk
        // across a resumable protocol. User async/generator ancestors still miss.
        // The exact owner ancestry, rather than a matching spelling, must lead
        // to the source environment that owns the finalized capture cell.
        let mut cursor = analysis
            .owner_plans
            .get(owner_id)?
            .parent_owner_id
            .as_deref();
        loop {
            let ancestor_id = cursor?;
            let ancestor = analysis.function_plans.get(ancestor_id)?;
            if ancestor_id == capture.owner_id && module_kind.is_some() {
                break;
            }
            if ancestor.protocol.execution_kind() != FunctionExecutionKind::Ordinary {
                return None;
            }
            if ancestor_id == capture.owner_id {
                break;
            }
            cursor = analysis
                .owner_plans
                .get(ancestor_id)?
                .parent_owner_id
                .as_deref();
        }
        Some(Self {
            owner: output_owner_id.to_string(),
            source_name: source_name.to_string(),
            storage_name: storage_name.to_string(),
            slot: capture.slot,
            origin: ProvenBindingCellOrigin::Captured {
                entry_hops: capture.hops,
            },
        })
    }

    fn declarative(kind: EnvironmentKind) -> bool {
        matches!(
            kind,
            EnvironmentKind::Activation
                | EnvironmentKind::FunctionParameters
                | EnvironmentKind::FunctionBody
                | EnvironmentKind::Block
                | EnvironmentKind::ClassName
                | EnvironmentKind::NamedFunctionExpression
                | EnvironmentKind::SwitchCaseBlock
                | EnvironmentKind::CatchParameter
                | EnvironmentKind::SimpleCatchParameter
                | EnvironmentKind::ForLexicalHead
                | EnvironmentKind::ForInOfTdzHead
                | EnvironmentKind::ForInOfIteration
        )
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn storage_name(&self) -> &str {
        &self.storage_name
    }

    pub fn source_name(&self) -> &str {
        &self.source_name
    }

    pub fn slot(&self) -> u32 {
        self.slot
    }

    /// Codegen retains active-scope hop adjustments through the same binding
    /// storage as other lexical reads; captured entry hops only authenticate it.
    pub fn origin(&self) -> ProvenBindingCellOrigin {
        self.origin
    }
}

/// The parameter lists a plain owner may have.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlainParameters {
    /// Identifiers only: every parameter name is an entry value.
    Identifiers,
    /// Identifiers and binding patterns that hold no expression.
    ExpressionFreePatterns,
}

impl AnalysisBuilder<'_> {
    fn ordinary_module_owner_ids(&self) -> BTreeSet<FunctionId> {
        self.plain_module_owner_ids(
            &[
                FunctionProtocolIr::OrdinaryCallAndConstruct,
                FunctionProtocolIr::Arrow,
            ],
            PlainParameters::Identifiers,
        )
    }

    fn has_body_with_object_environment(&self, owner_id: &str) -> bool {
        self.environment_plans.values().any(|environment| {
            environment.owner_id == owner_id && environment.kind == EnvironmentKind::WithObject
        })
    }

    /// Strict owners (or eval-visible sloppy arrows without `with`) of one of
    /// `protocols` with one activation record, no enclosing or body `with`, and
    /// a list of plain identifier parameters or, where `parameters` admits
    /// them, binding patterns without an expression.
    fn plain_module_owner_ids(
        &self,
        protocols: &[FunctionProtocolIr],
        parameters: PlainParameters,
    ) -> BTreeSet<FunctionId> {
        self.function_plans
            .values()
            .filter(|function| {
                let owner = &self.owner_plans[&function.id];
                let eval_visible = self.environment_plans[&owner.activation_environment_id]
                    .eval_visible;
                (function.strict
                    || (function.protocol == FunctionProtocolIr::Arrow
                        && eval_visible
                        && !self.has_body_with_object_environment(&function.id)))
                    && protocols.contains(&function.protocol)
                    && owner.body_environment_id.is_none()
                    && self
                        .surrounding_with_object_environment_ids(&function.id)
                        .is_empty()
                    && function.parameters.as_ref().iter().all(|parameter| {
                        !parameter.is_rest_param()
                            && parameter.init().is_none()
                            && match parameter.variable().binding() {
                                Binding::Identifier(_) => true,
                                Binding::Pattern(pattern) => {
                                    parameters == PlainParameters::ExpressionFreePatterns
                                        && function_environment::pattern_is_expression_free(pattern)
                                }
                            }
                    })
            })
            .map(|function| function.id.clone())
            .collect()
    }

    /// Eval-visible sloppy ordinary owners without enclosing or body `with`,
    /// split parameter/body records, or rest/default parameters. Mapped
    /// `arguments` can only alias formal parameters; non-parameter `var` locals
    /// cannot be aliased and may stay in Wasm locals when uncaptured.
    fn sloppy_ordinary_local_owner_ids(&self) -> BTreeSet<FunctionId> {
        self.function_plans
            .values()
            .filter(|function| {
                let owner = &self.owner_plans[&function.id];
                !function.strict
                    && self.environment_plans[&owner.activation_environment_id].eval_visible
                    && function.protocol == FunctionProtocolIr::OrdinaryCallAndConstruct
                    && owner.body_environment_id.is_none()
                    && !self.has_body_with_object_environment(&function.id)
                    && self
                        .surrounding_with_object_environment_ids(&function.id)
                        .is_empty()
                    && function.parameters.as_ref().iter().all(|parameter| {
                        !parameter.is_rest_param()
                            && parameter.init().is_none()
                            && matches!(parameter.variable().binding(), Binding::Identifier(_))
                    })
            })
            .map(|function| function.id.clone())
            .collect()
    }

    /// Actual eval and dynamic binding creation have already been excluded.
    /// Strict arguments snapshot the incoming list rather than alias parameters.
    /// Arrows (strict or sloppy) have no own `arguments`, `this` or `new.target`.
    /// Keep lexical/function/self storage and all actual captures. Ordinary
    /// this/new.target reads already use body-entry values; only a real child
    /// capture requires their synthetic environment cells.
    ///
    /// The implicit arguments binding is a candidate too, observed or not:
    /// without eval nothing resolves the name `arguments` at runtime, so the
    /// owner's own references read its storage directly (lowering selects the
    /// storage only while no environment slot holds it), and an arrow child's
    /// reference is a real capture that still retains a slot. The synthetic
    /// storage names are unspellable in source.
    pub(super) fn plan_module_local_candidates(&mut self) {
        for id in self.ordinary_module_owner_ids() {
            let function = &self.function_plans[&id];
            let owner = &self.owner_plans[&function.id];
            let activation = &self.environment_plans[&owner.activation_environment_id];
            let locals = activation
                .binding_modes
                .iter()
                .filter_map(|(name, mode)| {
                    if matches!(
                        name.as_str(),
                        LEXICAL_THIS_NAME | LEXICAL_NEW_TARGET_NAME | LEXICAL_ARGUMENTS_NAME
                    ) {
                        return Some(name.clone());
                    }
                    if matches!(name.as_str(), "arguments" | LEXICAL_HOME_OBJECT_NAME) {
                        return None;
                    }
                    ((*mode == BindingMode::Var || owner.parameter_names.contains(name))
                        && self.source_identifiers.contains(name)
                        && !owner.function_bindings.contains_key(name)
                        && function.activation_self_binding_name() != Some(name.as_str()))
                    .then(|| name.clone())
                })
                .collect();
            self.module_local_candidates
                .insert(function.id.clone(), locals);
        }
        // Sloppy ordinary functions keep formal parameters in environment slots
        // for mapped `arguments`, but uncaptured non-parameter `var` bindings
        // and synthetic `this`/`new.target`/`arguments` can still use Wasm locals.
        for id in self.sloppy_ordinary_local_owner_ids() {
            let function = &self.function_plans[&id];
            let owner = &self.owner_plans[&function.id];
            let activation = &self.environment_plans[&owner.activation_environment_id];
            let locals: BTreeSet<String> = activation
                .binding_modes
                .iter()
                .filter_map(|(name, mode)| {
                    if matches!(
                        name.as_str(),
                        LEXICAL_THIS_NAME | LEXICAL_NEW_TARGET_NAME | LEXICAL_ARGUMENTS_NAME
                    ) {
                        return Some(name.clone());
                    }
                    if matches!(name.as_str(), "arguments" | LEXICAL_HOME_OBJECT_NAME)
                        || owner.parameter_names.contains(name)
                    {
                        return None;
                    }
                    (*mode == BindingMode::Var
                        && self.source_identifiers.contains(name)
                        && !owner.function_bindings.contains_key(name)
                        && function.activation_self_binding_name() != Some(name.as_str()))
                    .then(|| name.clone())
                })
                .collect();
            if !locals.is_empty() {
                self.module_local_candidates
                    .insert(function.id.clone(), locals);
            }
        }
    }
}
