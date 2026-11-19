use super::*;

impl ScriptLowerer<'_> {
    pub(super) fn root_this_binding_for_function_owner(
        &self,
        function_id: &FunctionId,
    ) -> RootThisBinding {
        // Function plans are lowered from one flat registry, rather than by
        // recursively lowering their source parents. Recover the source-goal
        // boundary from those parents so lexical arrows below a Module owner
        // cannot inherit the outer Script's global this policy.
        let mut owner_id = Some(function_id.as_str());
        while let Some(id) = owner_id {
            if let Some(owner) = self.analysis.function_plans.get(id) {
                match owner.protocol {
                    FunctionProtocolIr::ModuleActivation
                    | FunctionProtocolIr::AsyncModuleActivation => {
                        return RootThisBinding::Undefined;
                    }
                    FunctionProtocolIr::OrdinaryCallOnly
                    | FunctionProtocolIr::OrdinaryCallAndConstruct
                    | FunctionProtocolIr::Arrow
                    | FunctionProtocolIr::Generator
                    | FunctionProtocolIr::Async
                    | FunctionProtocolIr::AsyncArrow
                    | FunctionProtocolIr::AsyncGenerator
                    | FunctionProtocolIr::ObjectMethod(_)
                    | FunctionProtocolIr::ObjectGetter
                    | FunctionProtocolIr::ObjectSetter
                    | FunctionProtocolIr::ClassConstructor
                    | FunctionProtocolIr::ClassMethod(_)
                    | FunctionProtocolIr::ClassGetter
                    | FunctionProtocolIr::ClassSetter => {}
                }
            }
            owner_id = self
                .analysis
                .owner_plans
                .get(id)
                .and_then(|owner| owner.parent_owner_id.as_deref());
        }
        self.root_this_binding
    }

    /// FunctionDeclarationInstantiation binds the source name `arguments` to
    /// the implicit arguments object in the owner's own record unless a
    /// parameter or declaration of that name occupies it. An arrow reaches its
    /// enclosing function's binding through the captured storage instead.
    /// Analysis resolves every reference the same way
    /// (`analysis/arguments_reference.rs`), so reads, writes, destructuring
    /// targets and closures all share the one `LEXICAL_ARGUMENTS_NAME` cell, and
    /// no Script global of that name can precede it.
    pub(super) fn declare_source_arguments_binding(&mut self) {
        let Some(binding) = self
            .scopes
            .last()
            .and_then(|scope| scope.get(LEXICAL_ARGUMENTS_NAME))
            .cloned()
        else {
            return;
        };
        let owner = &self.analysis.owner_plans[&self.current_owner_id];
        if self.analysis.environment_plans[&owner.activation_environment_id]
            .binding_storage_names
            .contains("arguments")
        {
            return;
        }
        self.declare_binding("arguments".to_string(), binding);
    }

    pub(super) fn begin_function_body_environment(&mut self) -> Option<LexicalEnvironmentIr> {
        let owner = &self.analysis.owner_plans[&self.current_owner_id];
        let body_id = owner.body_environment_id?;
        let body = &self.analysis.environment_plans[&body_id];
        let parameter = &self.analysis.environment_plans[&owner.activation_environment_id];
        let mut initializations = Vec::new();
        let mut source_bindings = Vec::new();
        for (name, slot) in &body.owned_env_slots {
            if body.binding_modes[name] != BindingMode::Var
                && !owner.function_bindings.contains_key(name)
            {
                continue;
            }
            let parameter_name = if name == "arguments" && !owner.parameter_names.contains(name) {
                LEXICAL_ARGUMENTS_NAME
            } else {
                name.as_str()
            };
            let is_parameter_binding = owner.parameter_names.contains(name)
                || (name == "arguments"
                    && parameter
                        .owned_env_slots
                        .contains_key(LEXICAL_ARGUMENTS_NAME));
            let parameter_slot = (is_parameter_binding
                && !owner.function_bindings.contains_key(name))
            .then(|| parameter.owned_env_slots.get(parameter_name))
            .flatten();
            let value = match parameter_slot {
                Some(slot) => FunctionBodyBindingValueIr::Parameter { slot: *slot },
                None => FunctionBodyBindingValueIr::Undefined,
            };
            let info = if parameter_slot.is_some() {
                self.lookup_binding(name)
                    .map(|binding| ValueInfo {
                        kind: binding.kind,
                        possible_kinds: binding.possible_kinds,
                        heap_shape: binding.heap_shape,
                        function_targets: binding.function_targets,
                    })
                    .unwrap_or_else(|| ValueInfo::new(ValueKind::Dynamic))
            } else {
                ValueInfo::undefined()
            };
            initializations.push(FunctionBodyBindingInitializationIr { slot: *slot, value });
            let source_candidates = parameter_slot
                .and_then(|_| self.finite_binding_source_candidates(parameter_name))
                .unwrap_or_default()
                .to_vec();
            source_bindings.push((name.clone(), info, source_candidates));
        }
        let mut environment = self
            .lower_runtime_lexical_environment(Some(body_id))
            .expect("a function body owns its complete declaration cells");
        environment.initialization = LexicalEnvironmentInitializationIr::FunctionBody {
            bindings: initializations,
        };
        self.finite_source_body_scope = Some(self.scopes.len());
        self.push_scope();
        for (name, info, source_candidates) in source_bindings {
            self.declare_binding(
                name.clone(),
                BindingInfo::initialized(BindingMode::Var, name.clone(), info),
            );
            if !source_candidates.is_empty() {
                self.set_finite_source_binding_candidates(&name, source_candidates);
            }
        }
        Some(environment)
    }

    pub(super) fn finish_function_body_environment(
        &mut self,
        mut body: BlockIr,
        environment: Option<LexicalEnvironmentIr>,
    ) -> Vec<StatementIr> {
        if let Some(environment) = environment {
            self.pop_scope();
            body.lexical_environment = Some(environment);
            vec![StatementIr::Block(body)]
        } else {
            body.statements
        }
    }
}
