use super::*;
use boa_ast::visitor::{VisitWith, Visitor};
use core::ops::ControlFlow;

struct ContainsExpression;
impl<'ast> Visitor<'ast> for ContainsExpression {
    type BreakTy = ();
    fn visit_expression(&mut self, _: &'ast Expression) -> ControlFlow<()> {
        ControlFlow::Break(())
    }
}

/// Whether a binding pattern holds no initializer and no computed key: the
/// same test that decides below whether an owner needs a separate body
/// record for its parameter expressions.
pub(super) fn pattern_is_expression_free(pattern: &Pattern) -> bool {
    pattern.visit_with(&mut ContainsExpression).is_continue()
}

fn parameters_contain_expression(parameters: &[FormalParameter]) -> bool {
    parameters.iter().any(|parameter| {
        parameter.init().is_some()
            || matches!(parameter.variable().binding(), Binding::Pattern(pattern)
                if !pattern_is_expression_free(pattern))
    })
}

impl<'a> AnalysisBuilder<'a> {
    pub(super) fn body_environment_cursor(&self, owner_id: &str) -> EnvironmentCursor {
        let owner = &self.owner_plans[owner_id];
        EnvironmentCursor {
            owner_id: owner_id.to_string(),
            environment_id: owner
                .body_environment_id
                .unwrap_or(owner.activation_environment_id),
        }
    }

    pub(super) fn prepare_function_body_environment(
        &mut self,
        owner_id: &str,
        parameters: &[FormalParameter],
        items: &'a [StatementListItem],
        interner: &Interner,
    ) {
        if !parameters_contain_expression(parameters) {
            return;
        }
        let owner = self.owner_plans[owner_id].clone();
        let activation_id = owner.activation_environment_id;
        self.environment_plans
            .get_mut(&activation_id)
            .expect("parameter activation exists")
            .kind = EnvironmentKind::FunctionParameters;
        let parameter_bindings = self.parameter_environment_bindings[owner_id].clone();
        let mut body_bindings = owner
            .root_bindings
            .difference(&parameter_bindings)
            .cloned()
            .collect();
        self.collect_owner_root_bindings_from_items(interner, items, &mut body_bindings);
        body_bindings.extend(owner.function_bindings.keys().cloned());
        let activation_modes = self.environment_plans[&activation_id].binding_modes.clone();
        let definition_environment =
            &self.environment_plans[&owner.definition_environment_cursor.environment_id];
        let named_self_modes = (definition_environment.owner_id == owner_id
            && definition_environment.kind == EnvironmentKind::NamedFunctionExpression)
            .then(|| definition_environment.binding_modes.clone())
            .unwrap_or_default();
        let body_id = self.alloc_environment_id();
        self.register_environment_plan(
            body_id,
            owner_id,
            EnvironmentKind::FunctionBody,
            Some(self.activation_environment_cursor(owner_id)),
            body_bindings,
        );
        self.set_environment_binding_modes(body_id, activation_modes);
        let activation = self
            .environment_plans
            .get_mut(&activation_id)
            .expect("parameter activation exists");
        let removed = activation
            .binding_storage_names
            .difference(&parameter_bindings)
            .cloned()
            .collect::<Vec<_>>();
        activation.binding_storage_names = parameter_bindings.clone();
        activation
            .binding_modes
            .retain(|name, _| parameter_bindings.contains(name));
        // A body declaration may shadow the function expression's name only
        // in the separate body record. Parameter defaults still resolve its
        // immutable self binding unless a real parameter shadows that name.
        activation.binding_modes.extend(named_self_modes);
        for name in &owner.parameter_names {
            activation
                .binding_modes
                .insert(name.clone(), BindingMode::Let);
        }
        for name in removed {
            self.physical_binding_environments
                .get_mut(&name)
                .expect("binding has a physical owner")
                .remove(&activation_id);
        }
        for name in &parameter_bindings {
            self.physical_binding_environments
                .entry(name.clone())
                .or_default()
                .insert(activation_id);
        }
        let parameter_eval_id = if self.eval_visible && !owner.strict {
            let id = self.alloc_environment_id();
            self.register_environment_plan(
                id,
                owner_id,
                EnvironmentKind::ParameterEvalVariable,
                Some(owner.definition_environment_cursor.clone()),
                BTreeSet::new(),
            );
            self.set_environment_parent_cursor(
                activation_id,
                EnvironmentCursor {
                    owner_id: owner_id.to_string(),
                    environment_id: id,
                },
            );
            Some(id)
        } else {
            None
        };
        let owner = self
            .owner_plans
            .get_mut(owner_id)
            .expect("function owner exists");
        owner.body_environment_id = Some(body_id);
        owner.parameter_eval_environment_id = parameter_eval_id;
        owner.owned_env_slots = owner
            .owned_env_slots
            .keys()
            .filter(|name| parameter_bindings.contains(*name))
            .enumerate()
            .map(|(slot, name)| (name.clone(), slot as u32))
            .collect();
    }
}
