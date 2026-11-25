mod expression;

use super::*;

impl<'a> ScriptLowerer<'a> {
    pub(super) fn lower_statement(&mut self, statement: &Statement) -> (StatementIr, ValueKind) {
        if let Some(boundary) = self.lower_module_instantiation_boundary(statement) {
            return boundary;
        }
        let mut lowered = match statement {
            Statement::Expression(expression) => self.lower_expression_statement(expression),
            Statement::Empty => (StatementIr::Empty, ValueKind::Undefined),
            Statement::Block(block) if block.statement_list().statements().is_empty() => {
                // Block : { } has an empty completion and creates no environment.
                // Keeping it as Empty also preserves the enclosing UpdateEmpty
                // value rather than manufacturing an `undefined` expression.
                (StatementIr::Empty, ValueKind::Undefined)
            }
            Statement::Block(block) => {
                // The Block's declarative Environment Record (14.3.1.2 step 1)
                // is pushed and popped by `lower_block`'s
                // `LexicalScopeInstantiation`; pushing a second, empty frame
                // here would only give the sweep somewhere else it could have
                // landed.
                let block_ir = self.lower_block(block);
                let kind = block_ir.result_kind;
                (StatementIr::Block(block_ir), kind)
            }
            // Statement heads that are evaluated exactly once stage their
            // suspensions ahead of the statement. The guards check that the
            // head is the *only* suspending part, because the prefix runs
            // unconditionally and once: an `await` from a loop body or a
            // branch hoisted out here would run at the wrong time, or only
            // once for a body that runs many times.
            Statement::If(if_statement)
                if self.current_generator_resume_state.is_some()
                    && self.head_await_is_stageable(
                        if_statement.cond(),
                        [Some(if_statement.body()), if_statement.else_node()],
                    ) =>
            {
                self.lower_with_async_head_prefix(if_statement.cond(), |this| {
                    this.lower_if_statement(if_statement)
                })
            }
            Statement::If(if_statement) => self.lower_if_statement(if_statement),
            Statement::WhileLoop(while_loop) => self.lower_while_loop(while_loop),
            Statement::DoWhileLoop(do_while) => self.lower_do_while_loop(do_while),
            Statement::ForLoop(for_loop) => self.lower_for_loop(for_loop),
            Statement::ForOfLoop(for_of)
                if !self
                    .analysis
                    .complete_for_of_owners
                    .contains_key(&(&**for_of as *const ForOfLoop as usize))
                    && self.head_await_is_stageable(for_of.iterable(), [Some(for_of.body())])
                    && !contains(for_of.initializer(), ContainsSymbol::AwaitExpression) =>
            {
                self.lower_with_async_head_prefix(for_of.iterable(), |this| {
                    this.lower_for_of_loop(for_of)
                })
            }
            Statement::ForOfLoop(for_of) => self.lower_for_of_loop(for_of),
            Statement::Switch(switch)
                if self.plain_async_entry_state().is_none()
                    && self.async_generator_entry_state().is_none()
                    && self.head_await_is_stageable(switch.val(), [])
                    && !switch
                        .cases()
                        .iter()
                        .any(|case| contains(case, ContainsSymbol::AwaitExpression)) =>
            {
                self.lower_with_async_head_prefix(switch.val(), |this| this.lower_switch(switch))
            }
            Statement::Switch(switch) => self.lower_switch(switch),
            Statement::Labelled(labelled) => self.lower_labelled(labelled),
            Statement::Break(brk) => self.lower_break(brk),
            Statement::Continue(cont) => self.lower_continue(cont),
            Statement::Debugger => (StatementIr::Debugger, ValueKind::Undefined),
            Statement::Throw(throw) if self.head_await_is_stageable(throw.target(), []) => {
                self.lower_with_async_head_prefix(throw.target(), |this| this.lower_throw(throw))
            }
            Statement::Throw(throw) => self.lower_throw(throw),
            Statement::Try(try_statement) => self.lower_try(try_statement),
            Statement::Var(var) => self.lower_var_statement(var),
            Statement::Return(ret) => self.lower_return(ret),
            Statement::ForInLoop(for_in)
                if !matches!(
                    self.analysis.for_in_continuation_owners
                        [&(&**for_in as *const boa_ast::statement::iteration::ForInLoop as usize)],
                    crate::analysis::ForInContinuationOwner::CompleteWhole(_)
                ) && self.head_await_is_stageable(for_in.target(), [Some(for_in.body())])
                    && !contains(for_in.initializer(), ContainsSymbol::AwaitExpression) =>
            {
                self.lower_with_async_head_prefix(for_in.target(), |this| {
                    this.lower_for_in_loop(for_in)
                })
            }
            Statement::ForInLoop(for_in) => self.lower_for_in_loop(for_in),
            Statement::With(with) => self.lower_with_statement(with),
        };
        if self.ordinary_generator_switch_depth > 0
            || self.ordinary_generator_region_depth > 0
            || self.ordinary_generator_for_in_depth > 0
            || self.plain_async_for_in_depth > 0
            || self.plain_async_for_of_depth > 0
            || self.plain_async_classic_depth > 0
            || self.plain_async_resource_depth > 0
            || self.mixed_async_generator_region_depth > 0
        {
            if let Some(source) = CheckedEmptyStatementCompletionSource::from_statement(statement) {
                lowered.0 = StatementIr::EmptyStatementCompletion(Box::new(
                    EmptyStatementCompletionIr::new(source, lowered.0),
                ));
            }
        }
        // Existing ordinary/direct-await loop dispatchers do not own an async
        // switch's case-selection segment. This also catches an eager try that
        // allocates child states without any source Await expression.
        let complete_async_classic = matches!(&lowered.0, StatementIr::AsyncGeneratorLoop(plan)
            if plan.execution() == ResumableRegionProtocolIr::Async);
        let complete_async_iterator = matches!(&lowered.0,StatementIr::AsyncGeneratorForOf(plan)
            if plan.execution()==ResumableRegionProtocolIr::Async);
        let complete_async_for_in = matches!(&lowered.0,StatementIr::AsyncGeneratorForIn(plan)
            if plan.execution()==ResumableRegionProtocolIr::Async);
        if matches!(
            statement,
            Statement::WhileLoop(_)
                | Statement::DoWhileLoop(_)
                | Statement::ForLoop(_)
                | Statement::ForOfLoop(_)
                | Statement::ForInLoop(_)
        ) && !complete_async_for_in
            && !complete_async_classic
            && !complete_async_iterator
            && crate::ir::statement_contains_async_switch(&lowered.0)
        {
            self.unsupported(
                "async switch inside an enclosing loop requires a composed loop owner",
            );
            return (StatementIr::Empty, ValueKind::Undefined);
        }
        if matches!(
            statement,
            Statement::WhileLoop(_)
                | Statement::DoWhileLoop(_)
                | Statement::ForLoop(_)
                | Statement::ForOfLoop(_)
                | Statement::ForInLoop(_)
        ) && !complete_async_for_in
            && !complete_async_classic
            && !complete_async_iterator
            && crate::ir::statement_contains_async_with(&lowered.0)
        {
            self.unsupported("async with inside an enclosing loop requires a composed loop owner");
            return (StatementIr::Empty, ValueKind::Undefined);
        }
        if matches!(
            statement,
            Statement::WhileLoop(_)
                | Statement::DoWhileLoop(_)
                | Statement::ForLoop(_)
                | Statement::ForOfLoop(_)
                | Statement::ForInLoop(_)
        ) && !complete_async_for_in
            && !complete_async_classic
            && !complete_async_iterator
            && crate::ir::statement_contains_async_for_in(&lowered.0)
        {
            self.unsupported(
                "async for-in inside an enclosing loop requires a composed loop owner",
            );
            return (StatementIr::Empty, ValueKind::Undefined);
        }
        lowered
    }
}
