use super::*;

/// Branches can restart only within the currently lowered checked condition.
/// A nested function starts from ScriptLowerer::new's Ordinary context; the
/// condition context is never transferred with the enclosing flow facts.
#[derive(Clone, Copy)]
pub(super) enum AsyncValueBranchContext {
    Ordinary,
    WhileCondition { loop_depth: usize },
    Pattern { loop_depth: usize },
    WithHead { loop_depth: usize },
    WithBody { loop_depth: usize },
    ForInHead { loop_depth: usize },
    ForInBody { loop_depth: usize },
    ClassicLoop { loop_depth: usize },
    ForOf { loop_depth: usize },
    ResourceScope { loop_depth: usize },
    Switch { loop_depth: usize },
}

impl<'a> ScriptLowerer<'a> {
    pub(super) fn has_plain_async_value_branch_owner(&self) -> bool {
        if self.plain_async_entry_state().is_none() {
            return false;
        }
        match self.async_value_branch_context {
            AsyncValueBranchContext::Ordinary => self.loop_depth == 0,
            AsyncValueBranchContext::WhileCondition { loop_depth }
            | AsyncValueBranchContext::Pattern { loop_depth }
            | AsyncValueBranchContext::WithHead { loop_depth }
            | AsyncValueBranchContext::WithBody { loop_depth }
            | AsyncValueBranchContext::ForInHead { loop_depth }
            | AsyncValueBranchContext::ForInBody { loop_depth }
            | AsyncValueBranchContext::ClassicLoop { loop_depth }
            | AsyncValueBranchContext::ForOf { loop_depth }
            | AsyncValueBranchContext::ResourceScope { loop_depth }
            | AsyncValueBranchContext::Switch { loop_depth } => self.loop_depth == loop_depth,
        }
    }

    pub(super) fn has_checked_async_source_value_branch_owner(&self) -> bool {
        self.plain_async_entry_state().is_some()
            && match self.async_value_branch_context {
                AsyncValueBranchContext::WhileCondition { loop_depth }
                | AsyncValueBranchContext::Pattern { loop_depth }
                | AsyncValueBranchContext::WithHead { loop_depth }
                | AsyncValueBranchContext::WithBody { loop_depth }
                | AsyncValueBranchContext::ForInHead { loop_depth }
                | AsyncValueBranchContext::ForInBody { loop_depth }
                | AsyncValueBranchContext::ClassicLoop { loop_depth }
                | AsyncValueBranchContext::ForOf { loop_depth }
                | AsyncValueBranchContext::ResourceScope { loop_depth }
                | AsyncValueBranchContext::Switch { loop_depth } => self.loop_depth == loop_depth,
                AsyncValueBranchContext::Ordinary => false,
            }
    }

    pub(super) fn lower_awaited_while_condition(
        &mut self,
        while_loop: &WhileLoop,
        entry_state: u32,
    ) -> (StatementIr, ValueKind) {
        let Some(body) = super::eager_async_while_body::EagerAsyncWhileBody::new(while_loop.body())
        else {
            self.unsupported("awaited while condition requires a body without suspension");
            return (StatementIr::Empty, ValueKind::Undefined);
        };
        self.loop_depth += 1;
        let enclosing_context = std::mem::replace(
            &mut self.async_value_branch_context,
            AsyncValueBranchContext::WhileCondition {
                loop_depth: self.loop_depth,
            },
        );
        let condition = self.lower_async_prefixed_expression(while_loop.condition());
        // Restore even when source admission fails, and before the eager body
        // temporarily removes continuation authority from ordinary lowering.
        self.async_value_branch_context = enclosing_context;
        self.loop_depth -= 1;
        let Some((prefix, condition)) = condition else {
            self.unsupported("conditionally reached await in a while condition");
            return (StatementIr::Empty, ValueKind::Undefined);
        };
        let ready_state = self
            .plain_async_entry_state()
            .expect("plain async condition retains its continuation state");
        let before_vars = self.var_bindings.clone();
        let before_globals = self.global_properties.clone();
        // The eagerly evaluated body source has no suspension. Lower ordinary
        // try/finally and nested eager loops without allocating async clause
        // states that could escape the condition's back edge.
        let (body, kind) = body.lower(self);
        self.var_bindings = self.merge_var_bindings(&before_vars, &self.var_bindings.clone());
        self.global_properties =
            self.merge_global_properties(&before_globals, &self.global_properties.clone());
        match crate::AsyncFunctionWhileConditionIr::new(
            prefix,
            condition,
            body,
            entry_state,
            ready_state,
        ) {
            Ok(plan) => {
                self.current_async_resume_state = Some(plan.exit_state());
                (StatementIr::AsyncFunctionWhile(plan), kind)
            }
            Err(error) => {
                self.unsupported_with_message(format!(
                    "unsupported in scriptw wasm-aot: invalid awaited while condition: {error:?}",
                ));
                (StatementIr::Empty, ValueKind::Undefined)
            }
        }
    }
}
