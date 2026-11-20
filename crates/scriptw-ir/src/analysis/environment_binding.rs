//! BindingCell policies belong to the analyzed source environment, including
//! cells retained in an activation whose names are not visible to direct eval.

use super::*;

impl Analysis<'_> {
    pub(crate) fn owned_environment_binding(
        &self,
        environment_id: EnvironmentId,
        name: &str,
        slot: u32,
        interner: &Interner,
    ) -> OwnedEnvBindingIr {
        let environment = &self.environment_plans[&environment_id];
        // An uncaptured block/head binding may be retained in the invocation
        // environment. Its write policy still comes from its source record.
        let source = if environment.binding_modes.contains_key(name) {
            environment
        } else {
            self.physical_binding_environments
                .get(name)
                .into_iter()
                .flatten()
                .map(|id| &self.environment_plans[id])
                .find(|source| {
                    source.owner_id == environment.owner_id
                        && source.binding_modes.contains_key(name)
                })
                .unwrap_or_else(|| {
                    panic!("owned source binding `{name}` must have an analyzed write policy")
                })
        };
        let mode = source.binding_modes[name];
        let named_self = source.kind == EnvironmentKind::NamedFunctionExpression
            || (matches!(
                environment.kind,
                EnvironmentKind::Activation | EnvironmentKind::FunctionParameters
            ) && mode == BindingMode::Const
                && self
                    .function_plans
                    .get(&environment.owner_id)
                    .is_some_and(|function| {
                        function.activation_self_binding_name() == Some(name)
                            && (self.owner_plans[&environment.owner_id]
                                .body_environment_id
                                .is_some()
                                || !lexically_declared_names(function.body).into_iter().any(
                                    |bound| interner.resolve_expect(bound).to_string() == name,
                                ))
                    }));
        let mutability = if mode == BindingMode::Const {
            EnvironmentBindingMutabilityIr::Immutable {
                strict: !named_self,
            }
        } else {
            EnvironmentBindingMutabilityIr::Mutable
        };
        OwnedEnvBindingIr {
            name: name.to_string(),
            slot,
            mutability,
        }
    }

    pub(crate) fn owner_environment_binding(
        &self,
        owner_id: &str,
        name: &str,
        slot: u32,
        interner: &Interner,
    ) -> OwnedEnvBindingIr {
        self.owned_environment_binding(
            self.owner_plans[owner_id].activation_environment_id,
            name,
            slot,
            interner,
        )
    }
}
