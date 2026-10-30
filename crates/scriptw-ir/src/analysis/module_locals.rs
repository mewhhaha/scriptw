//! Private activation locals do not require reusable module lookup cells.
use super::*;

impl AnalysisBuilder<'_> {
    /// Called only when module reuse is the sole reason for named lookup.
    /// Keep lexical, function, self and synthetic bindings on their original
    /// paths. Actual captures are discovered afterwards and still own cells.
    pub(super) fn plan_module_local_candidates(&mut self) {
        for function in self.function_plans.values() {
            let owner = &self.owner_plans[&function.id];
            if !function.strict
                || function.protocol != FunctionProtocolIr::OrdinaryCallAndConstruct
                || owner.body_environment_id.is_some()
                || !self
                    .surrounding_with_object_environment_ids(&function.id)
                    .is_empty()
                || !function.parameters.as_ref().iter().all(|parameter| {
                    !parameter.is_rest_param()
                        && parameter.init().is_none()
                        && matches!(parameter.variable().binding(), Binding::Identifier(_))
                })
            {
                continue;
            }
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
