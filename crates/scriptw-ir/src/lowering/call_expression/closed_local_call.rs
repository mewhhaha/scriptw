use super::super::*;

impl ScriptLowerer<'_> {
    pub(super) fn prove_closed_local_call(
        &self,
        source: &Expression,
        callee: &TypedExpr,
    ) -> Option<ClosedLocalCallTargetIr> {
        let (Expression::Identifier(identifier), ExprIr::Identifier(storage)) =
            (source.flatten(), &callee.expr)
        else {
            return None;
        };
        let name = self.interner.resolve_expect(identifier.sym()).to_string();
        self.prove_closed_local_binding(&name, storage)
    }

    fn prove_closed_local_binding(
        &self,
        name: &str,
        storage: &str,
    ) -> Option<ClosedLocalCallTargetIr> {
        if self.is_prepass || !self.with_environment_chain.is_empty() {
            return None;
        }
        let environment = self.inline_binding_environment(name, storage)?;
        let binding = self
            .analysis
            .closed_inline_bindings
            .get(&(environment, storage.to_owned()))?;
        let target = &binding.target;
        if self.current_function_id.as_ref() == Some(target) || binding.name != name {
            return None;
        }
        let function = self.analysis.function_plans.get(target)?;
        if !matches!(
            function.protocol,
            FunctionProtocolIr::OrdinaryCallAndConstruct | FunctionProtocolIr::Arrow
        ) || function.parameters.as_ref().is_empty()
            || (function.parent_owner_id == SCRIPT_OWNER_ID && !function.is_expression)
        {
            return None;
        }

        // The actual identifier lookup must select the same Environment
        // Record and physical name as the callee's definition. Comparing
        // spellings or parent function IDs would miss intervening shadows.
        for (storage, capture) in &function.captures {
            if self.inline_binding_environment(&capture.source_name, storage)
                != Some(capture.environment_id)
            {
                return None;
            }
        }
        Some(ClosedLocalCallTargetIr::new(target.clone()))
    }

    fn inline_binding_environment(&self, name: &str, storage: &str) -> Option<EnvironmentId> {
        let (binding, location) = self.lookup_binding_with_location(name)?;
        if binding.storage_name != storage {
            return None;
        }
        let owner = self.analysis.owner_plans.get(&self.current_owner_id)?;
        if owner.body_environment_id.is_some() {
            return None;
        }

        if storage == name && owner.root_bindings.contains(name) {
            return Some(owner.activation_environment_id);
        }
        if let BindingLookupLocation::Scope(index) = location {
            if self.direct_lexical_scopes[index] {
                let mut environments = self
                    .analysis
                    .physical_binding_environments
                    .get(storage)?
                    .iter()
                    .filter(|id| {
                        self.analysis.environment_plans[*id].owner_id == self.current_owner_id
                    });
                let environment = environments.next()?;
                return environments.next().is_none().then_some(*environment);
            }
        }
        let capture = self
            .analysis
            .function_plans
            .get(&self.current_owner_id)?
            .captures
            .get(storage)?;
        (capture.source_name == name).then_some(capture.environment_id)
    }
}
