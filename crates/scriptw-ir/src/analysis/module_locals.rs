//! Named storage does not imply dynamically interceptable binding lookup.
use super::*;

/// Module reuse may expose names without permitting dynamic binding creation.
/// Arguments storage alone does not invalidate unrelated cell identities.
#[derive(Debug, Clone, Default)]
pub(crate) struct ModuleCellOwners(BTreeSet<FunctionId>);

impl ModuleCellOwners {
    pub(super) fn collect(builder: &AnalysisBuilder<'_>) -> Self {
        Self(builder.ordinary_module_owner_ids())
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
        // Collected before arguments-only exposure is folded into eval_visible;
        // actual eval and Annex-B binding creation never receive this evidence.
        if !analysis.module_cell_owners.0.contains(owner_id)
            || source_name == "arguments"
            || source_name == LEXICAL_ARGUMENTS_NAME
            || storage_name == LEXICAL_ARGUMENTS_NAME
        {
            return None;
        }
        let function = analysis.function_plans.get(owner_id)?;
        if function.protocol != FunctionProtocolIr::OrdinaryCallAndConstruct {
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
            || matches!(
                owner.protocol,
                FunctionProtocolIr::ModuleActivation | FunctionProtocolIr::AsyncModuleActivation
            )
            || environment.owner_id != capture.owner_id
            || capture.source_name != source_name
            || environment.owned_env_slots.get(storage_name) != Some(&capture.slot)
            || !Self::declarative(environment.kind)
        {
            return None;
        }
        // Do not extend this first admission across a resumable invocation.
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

impl AnalysisBuilder<'_> {
    fn ordinary_module_owner_ids(&self) -> BTreeSet<FunctionId> {
        self.function_plans
            .values()
            .filter(|function| {
                let owner = &self.owner_plans[&function.id];
                function.strict
                    && function.protocol == FunctionProtocolIr::OrdinaryCallAndConstruct
                    && owner.body_environment_id.is_none()
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
    /// Keep their live binding, lexical/function/self/synthetic storage and all
    /// actual captures; only private vars and simple parameters become locals.
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
                        "arguments"
                            | LEXICAL_ARGUMENTS_NAME
                            | LEXICAL_THIS_NAME
                            | LEXICAL_NEW_TARGET_NAME
                            | LEXICAL_HOME_OBJECT_NAME
                    ) {
                        return None;
                    }
                    ((*mode == BindingMode::Var || owner.parameter_names.contains(name))
                        && self.source_identifiers.contains(name)
                        && !owner.function_bindings.contains_key(name)
                        && function.self_binding_name.as_ref() != Some(name))
                    .then(|| name.clone())
                })
                .collect();
            self.module_local_candidates
                .insert(function.id.clone(), locals);
        }
    }
}
