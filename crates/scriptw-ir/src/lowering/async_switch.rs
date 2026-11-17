use super::*;
use crate::async_switch::AsyncSwitchError;

/// Source admission precedes allocating any CaseBlock continuation. Consuming
/// this carrier prevents calling the lowerer with a branch-sensitive selector
/// or a body whose suspension phases lack a complete source owner.
struct CheckedAsyncSwitchSource<'ast> {
    source: &'ast AstSwitch,
    selectors_suspend: bool,
}

impl<'ast> CheckedAsyncSwitchSource<'ast> {
    fn new(source: &'ast AstSwitch, lowerer: &ScriptLowerer<'_>) -> Result<Self, &'static str> {
        for case in source.cases() {
            let mut cursor = 0;
            let mut points = Vec::new();
            append_async_statement_protocol_items(
                case.body().statements(),
                &mut cursor,
                &mut points,
                false,
            )
            .ok_or("async switch body requires an owned continuation")?;
        }
        let mut selectors_suspend = false;
        for condition in source.cases().iter().filter_map(|case| case.condition()) {
            if synchronous_resource_loop::source_expression_suspends(condition) {
                if lowerer.has_branch_sensitive_await(condition)
                    || contains(condition, ContainsSymbol::YieldExpression)
                {
                    return Err(
                        "conditionally reached switch selector suspension requires an owner",
                    );
                }
                selectors_suspend = true;
            }
        }
        Ok(Self {
            source,
            selectors_suspend,
        })
    }

    fn lower(self, lowerer: &mut ScriptLowerer<'_>) -> (StatementIr, ValueKind) {
        lowerer.lower_checked_async_switch(self)
    }
}

impl<'a> ScriptLowerer<'a> {
    pub(super) fn lower_async_function_switch(
        &mut self,
        source: &AstSwitch,
    ) -> (StatementIr, ValueKind) {
        if crate::async_generator_source::switch_has_async_operands(source) {
            return self.lower_complete_async_switch(source);
        }
        match CheckedAsyncSwitchSource::new(source, self) {
            Ok(source) => source.lower(self),
            Err(reason) => {
                self.unsupported(reason);
                (StatementIr::Empty, ValueKind::Undefined)
            }
        }
    }

    fn lower_checked_async_switch(
        &mut self,
        source: CheckedAsyncSwitchSource<'_>,
    ) -> (StatementIr, ValueKind) {
        let selectors_suspend = source.selectors_suspend;
        let switch = source.source;
        // Discriminant evaluation belongs to the enclosing environment, before
        // the shared CaseBlock TDZ cells and functions are instantiated. Its
        // staged await prefix must not remain armed around the case bodies.
        let (mut prefix, discriminant) = match self.lower_async_prefixed_expression(switch.val()) {
            Some(staged) => staged,
            None if synchronous_resource_loop::source_expression_suspends(switch.val()) => {
                self.unsupported(
                    "conditionally reached switch discriminant suspension requires an owner",
                );
                return (StatementIr::Empty, ValueKind::Undefined);
            }
            None => (Vec::new(), self.lower_expression(switch.val())),
        };
        let discriminant = if selectors_suspend {
            let name = self.alloc_suspension_owned_binding(
                "async.switch.discriminant.",
                discriminant.value_info(),
            );
            prefix.push(StatementIr::Lexical {
                mode: BindingMode::Let,
                name: name.clone(),
                init: discriminant,
            });
            self.lower_identifier_name(name, false)
        } else {
            discriminant
        };
        let entry = self
            .plain_async_entry_state()
            .expect("checked plain async switch owner");
        let Some(selection_entry) = entry.checked_add(1) else {
            self.unsupported("async switch continuation state overflow");
            return (StatementIr::Empty, ValueKind::Undefined);
        };
        let before_vars = self.var_bindings.clone();
        let before_globals = self.global_properties.clone();
        self.breakable_depth += 1;
        let mut scope = LexicalScopeInstantiation::instantiate_switch(self, switch);
        let lexical_declarations = self.lower_block_function_declarations(
            switch
                .cases()
                .iter()
                .flat_map(|case| case.body().statements()),
        );
        let mut ownership_error = None;
        let mut next_selector = selection_entry;
        let mut case_conditions = Vec::with_capacity(switch.cases().len());
        for case in switch.cases() {
            // Reaching this selector means every earlier selector failed.
            // Their writes still happened, including writes before an await.
            let selector = match case.condition() {
                None => None,
                Some(expression) if selectors_suspend => {
                    self.current_async_resume_state = Some(next_selector);
                    let staged = self.lower_async_prefixed_expression(expression);
                    let (condition_prefix, condition) = match staged {
                        Some(staged) => staged,
                        None if synchronous_resource_loop::source_expression_suspends(
                            expression,
                        ) =>
                        {
                            ownership_error = Some(AsyncSwitchError::UnsupportedChildOwner);
                            break;
                        }
                        None => (Vec::new(), self.lower_expression(expression)),
                    };
                    let ready = self
                        .plain_async_entry_state()
                        .expect("selector lowering retains its activation");
                    match AsyncFunctionSwitchSelectorContinuationIr::new(
                        condition_prefix,
                        condition,
                        next_selector,
                        ready,
                    ) {
                        Ok(owner) => {
                            next_selector = owner.next_state();
                            Some(AsyncFunctionSwitchSelectorIr::Resumable(owner))
                        }
                        Err(error) => {
                            ownership_error = Some(error);
                            break;
                        }
                    }
                }
                Some(expression) => Some(AsyncFunctionSwitchSelectorIr::Eager(
                    self.lower_expression(expression),
                )),
            };
            case_conditions.push((
                selector,
                self.var_bindings.clone(),
                self.global_properties.clone(),
            ));
        }
        // Default is entered only after all tests, including those following
        // its source position, have failed. This is also the no-match exit.
        let fallback_vars = self.var_bindings.clone();
        let fallback_globals = self.global_properties.clone();
        // A fallback decision owns a state distinct from every body entry.
        // A first-case match must never be mistaken for the no-match fallback.
        let first_case_entry = if selectors_suspend {
            match next_selector.checked_add(1) {
                Some(state) => state,
                None => {
                    ownership_error = Some(AsyncSwitchError::StateOverflow);
                    next_selector
                }
            }
        } else {
            selection_entry
        };
        let mut cases = Vec::with_capacity(switch.cases().len());
        let mut merged_vars = self.merge_var_bindings(&before_vars, &fallback_vars);
        let mut merged_globals = self.merge_global_properties(&before_globals, &fallback_globals);
        let mut fallthrough_facts = None;
        let mut result_kind = None;
        let mut next_entry = first_case_entry;
        let mut owns_case_continuation = selectors_suspend;
        for (case, (condition, case_vars, case_globals)) in
            switch.cases().iter().zip(case_conditions)
        {
            if ownership_error.is_some() {
                break;
            }
            let (direct_vars, direct_globals) = if condition.is_none() {
                (fallback_vars.clone(), fallback_globals.clone())
            } else {
                (case_vars, case_globals)
            };
            // A body can be selected directly or entered after the preceding
            // body. Keep the conservative join even when an abrupt completion
            // might make that particular fallthrough path unreachable.
            if let Some((previous_vars, previous_globals)) = fallthrough_facts {
                self.var_bindings = self.merge_var_bindings(&direct_vars, &previous_vars);
                self.global_properties =
                    self.merge_global_properties(&direct_globals, &previous_globals);
            } else {
                self.var_bindings = direct_vars;
                self.global_properties = direct_globals;
            }
            self.widen_switch_scope_value_facts();
            self.current_async_resume_state = Some(next_entry);
            let body = self.lower_statement_items_without_function_initialization(
                case.body().statements(),
                &mut scope,
            );
            let body_exit = self
                .plain_async_entry_state()
                .expect("case lowering retains its async activation");
            owns_case_continuation |= body_exit != next_entry;
            merged_vars = self.merge_var_bindings(&merged_vars, &self.var_bindings);
            merged_globals = self.merge_global_properties(&merged_globals, &self.global_properties);
            fallthrough_facts = Some((self.var_bindings.clone(), self.global_properties.clone()));
            result_kind = Some(match result_kind {
                Some(kind) if kind != body.result_kind => ValueKind::Undefined,
                _ => body.result_kind,
            });
            match AsyncFunctionSwitchCaseIr::new(condition, body, next_entry, body_exit) {
                Ok(case) => {
                    next_entry = case.next_entry_state();
                    cases.push(case);
                }
                Err(error) => {
                    ownership_error = Some(error);
                    break;
                }
            }
        }
        self.breakable_depth -= 1;
        let lexical_environment = self.lower_materialized_lexical_environment(
            self.analysis
                .switch_environment_ids
                .get(&(switch as *const AstSwitch as usize))
                .copied(),
        );
        scope.finish(self);
        self.var_bindings = merged_vars;
        self.global_properties = merged_globals;
        self.widen_switch_scope_value_facts();
        if let Some(error) = ownership_error {
            self.unsupported(&format!(
                "async switch case continuation ownership: {error:?}"
            ));
            return (StatementIr::Empty, ValueKind::Undefined);
        }
        if !owns_case_continuation {
            // No child owns a state: preserve the eager switch, including eager
            // switches inside existing synchronous loop owners. A staged head
            // already owns its preceding states independently of the CaseBlock.
            let cases = match cases
                .into_iter()
                .map(AsyncFunctionSwitchCaseIr::into_eager_case)
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(cases) => cases,
                Err(error) => {
                    self.unsupported(&format!("async switch eager ownership: {error:?}"));
                    return (StatementIr::Empty, ValueKind::Undefined);
                }
            };
            self.current_async_resume_state = Some(entry);
            prefix.push(StatementIr::Switch {
                discriminant,
                lexical_environment,
                lexical_declarations,
                cases,
            });
            return (
                StatementIr::LexicalBlock(prefix),
                result_kind.unwrap_or(ValueKind::Undefined),
            );
        }
        let plan = match AsyncFunctionSwitchIr::new(
            discriminant,
            lexical_environment,
            lexical_declarations,
            cases,
            entry,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                self.unsupported(&format!("async switch continuation ownership: {error:?}"));
                return (StatementIr::Empty, ValueKind::Undefined);
            }
        };
        self.current_async_resume_state = Some(plan.exit_state());
        prefix.push(StatementIr::AsyncFunctionSwitch(plan));
        (
            StatementIr::LexicalBlock(prefix),
            result_kind.unwrap_or(ValueKind::Undefined),
        )
    }

    pub(super) fn widen_switch_scope_value_facts(&mut self) {
        // The shared scope owns declaration initialization independently of
        // selection. Value facts can instead come from a later selector or an
        // earlier body skipped by this entry. Keep lifecycle/storage intact;
        // widen values and caches without manufacturing an unknown call.
        let unknown = unknown_runtime_value_info();
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                if binding.mode != BindingMode::Const {
                    binding.kind = unknown.kind;
                    binding.possible_kinds = unknown.possible_kinds;
                    binding.function_targets.widen_for_possible_replacement();
                }
            }
        }
        self.discard_live_heap_shape_roots();
        self.static_boolean_bindings.clear();
        self.static_string_bindings.clear();
        self.static_to_string_regexp_object_bindings.clear();
        self.well_known_symbol_prototype_properties.clear();
        self.array_prototype_mutated = true;
        self.number_prototype_to_string_state = PrototypeToStringState::Unknown;
        self.boolean_prototype_to_string_state = PrototypeToStringState::Unknown;
    }
}
