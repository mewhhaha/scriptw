//! Private locals and proven captures retain their existing storage identities.
use super::*;

/// Module reuse may expose names without permitting dynamic binding creation.
/// Arguments storage alone does not invalidate unrelated capture identities.
#[derive(Debug, Clone, Default)]
pub(crate) struct ModuleCaptureOwners(BTreeSet<FunctionId>);

impl ModuleCaptureOwners {
    pub(super) fn collect(builder: &AnalysisBuilder<'_>) -> Self {
        Self(builder.ordinary_module_owner_ids())
    }
}

/// A finalized capture whose binding identity cannot be intercepted by runtime
/// name resolution. The value remains mutable and may still be uninitialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCapturedCellIr {
    slot: u32,
    hops: u32,
}

impl ModuleCapturedCellIr {
    pub(crate) fn analyze(
        analysis: &Analysis<'_>,
        owner_id: &str,
        storage_name: &str,
        source_name: &str,
    ) -> Option<Self> {
        // Collected before arguments-only exposure is folded into eval_visible;
        // actual eval and Annex-B binding creation never receive this evidence.
        if !analysis.module_capture_owners.0.contains(owner_id)
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
        let capture = function.captures.get(storage_name)?;
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
            || !matches!(
                environment.kind,
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
            slot: capture.slot,
            hops: capture.hops,
        })
    }

    pub fn slot(self) -> u32 {
        self.slot
    }

    /// Parent hops at function entry; codegen retains its ordinary active-scope
    /// adjustment through the same captured binding storage as lexical reads.
    pub fn hops(self) -> u32 {
        self.hops
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

    /// Called only when module reuse is the sole reason for named lookup.
    /// Keep lexical, function, self and synthetic bindings on their original
    /// paths. Actual captures are discovered afterwards and still own cells.
    pub(super) fn plan_module_local_candidates(&mut self) {
        for id in self.ordinary_module_owner_ids() {
            let function = &self.function_plans[&id];
            let owner = &self.owner_plans[&function.id];
            let activation = &self.environment_plans[&owner.activation_environment_id];
            let locals = activation
                .binding_modes
                .iter()
                .filter_map(|(name, mode)| {
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
