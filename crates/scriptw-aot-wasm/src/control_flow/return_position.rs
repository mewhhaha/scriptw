use super::*;
use crate::functions::CallContinuation;

impl FunctionBuilder<'_> {
    /// All observable operand work is complete before this activation retires.
    /// Preserve its Realm and Environment even after lexical unwind.
    pub(crate) fn emit_prepared_tail_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let caller_realm = self.load_current_realm(function);
        let calling_program = self.capture_current_program(function);
        let caller_environment = schema
            .reserve_gc_local(function)
            .initialize(self.current_environment().load(schema, function), function);
        self.emit_retire_abandoned_identifier_references(function);
        self.emit_unwind_environment_depth(self.environment_depth, function);
        // Proper tail calls retire the actual caller before dispatching the
        // target. Operand conversion and all user effects above precede this.
        self.emit_leave_source_activation(function);
        schema.return_call_helper(
            crate::runtime_helpers::ProxyCallArguments::new(
                callee,
                receiver,
                arguments,
                &caller_realm,
                &caller_environment,
                &calling_program,
            ),
            self.runtime_helper_base()?,
            function,
        );
        calling_program.clear(function);
        caller_environment.clear(function);
        caller_realm.clear(function);
        Ok(())
    }

    pub(crate) fn emit_return_position_value(
        &mut self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_propagate_current_throw_if_needed(function);
        self.completion().value().copy_from(value, function);
        self.emit_prepare_fresh_return(function)?;
        self.set_completion_kind(CompletionKind::Return, function);
        self.emit_derived_constructor_body_result(function)?;
        self.emit_return_current_completion(function);
        Ok(())
    }

    pub(super) fn compile_return_position_expr(
        &mut self,
        expression: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let tail = self.strict
            && !self.is_derived_constructor
            && self.throw_handler_stack.is_empty()
            && self.finally_stack.is_empty()
            && self.current_function_meta().is_some_and(|meta| {
                meta.protocol().execution_kind() == FunctionExecutionKind::Ordinary
            });
        if !tail {
            let value = self.runtime_schema().reserve_value_local(function);
            self.compile_expr_to_value(expression, &value, function)?;
            self.emit_return_position_value(&value, function)?;
            value.clear(function);
            return Ok(());
        }

        match &expression.expr {
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                self.compile_truthy_i32(condition, function)?;
                self.open_frame(ControlFrameKind::If, function);
                self.compile_return_position_expr(then_expr, function)?;
                function.instruction(&Instruction::Else);
                self.compile_return_position_expr(else_expr, function)?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            ExprIr::LogicalShortCircuit { op, lhs, rhs } => {
                // The left result still decides whether the RHS executes.
                let left = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(lhs, &left, function)?;
                match op {
                    LogicalBinaryOp::And | LogicalBinaryOp::Or => {
                        self.compile_truthy_tagged_i32(&left, function)?;
                        if *op == LogicalBinaryOp::Or {
                            function.instruction(&Instruction::I32Eqz);
                        }
                    }
                    LogicalBinaryOp::Coalesce => {
                        self.compile_nullish_tagged_i32(left.tag(), function)?;
                    }
                }
                self.open_frame(ControlFrameKind::If, function);
                self.compile_return_position_expr(rhs, function)?;
                function.instruction(&Instruction::Else);
                self.emit_return_position_value(&left, function)?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                left.clear(function);
            }
            ExprIr::Comma { lhs, rhs } => {
                let ignored = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(lhs, &ignored, function)?;
                ignored.clear(function);
                self.compile_return_position_expr(rhs, function)?;
            }
            ExprIr::MaterializeBinding { name, value, body } => {
                let initialized = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(value, &initialized, function)?;
                self.push_scope();
                let (_, id) = self.retain_expression_operand(name, &initialized, function);
                let result = self.compile_return_position_expr(body, function);
                self.pop_scope();
                self.release_local_binding(id, function);
                initialized.clear(function);
                result?;
            }
            _ => {
                let value = self.runtime_schema().reserve_value_local(function);
                match &expression.expr {
                    ExprIr::CallNamed { name, args } => {
                        self.emit_call(name, args, &CallContinuation::Return, &value, function)?
                    }
                    ExprIr::CallMethod {
                        receiver,
                        key,
                        args,
                    } => self.emit_method_call(
                        receiver,
                        key,
                        args,
                        &CallContinuation::Return,
                        &value,
                        function,
                    )?,
                    ExprIr::CallIndirect {
                        direct_eval,
                        callee,
                        this_arg,
                        args,
                        static_regexp_compilation,
                    } => self.emit_indirect_call(
                        callee,
                        this_arg.as_deref(),
                        args,
                        static_regexp_compilation.as_ref(),
                        direct_eval.as_ref(),
                        &CallContinuation::Return,
                        &value,
                        function,
                    )?,
                    ExprIr::EnvironmentIdentifier(identifier)
                        if matches!(
                            &identifier.operation,
                            scriptw_ir::EnvironmentIdentifierOperationIr::Call { .. }
                        ) =>
                    {
                        self.compile_environment_identifier_to_value(
                            identifier,
                            &CallContinuation::Return,
                            &value,
                            function,
                        )?
                    }
                    ExprIr::OptionalPropertyChain { target, chain } => self
                        .compile_optional_property_chain_to_value(
                            target,
                            chain,
                            None,
                            &CallContinuation::Return,
                            &value,
                            function,
                        )?,
                    _ => self.compile_expr_to_value(expression, &value, function)?,
                }
                // Genuine eval and shorted optional calls keep this Return;
                // ordinary calls have already left through a Wasm tail call.
                self.emit_return_position_value(&value, function)?;
                value.clear(function);
            }
        }
        Ok(())
    }
}
