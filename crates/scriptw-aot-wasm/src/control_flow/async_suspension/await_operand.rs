use super::*;

enum AwaitedCall<'a> {
    Resolve(Option<&'a TypedExpr>),
    Read,
}

enum AwaitedMethod<'a> {
    Materialized {
        name: &'a str,
        base: &'a TypedExpr,
        callee: &'a TypedExpr,
    },
    Direct {
        base: &'a TypedExpr,
        key: &'a PropertyKeyIr,
    },
}

impl<'a> AwaitedCall<'a> {
    fn of(operand: &'a TypedExpr) -> Option<(Self, AwaitedMethod<'a>)> {
        let (name, args, method) = match &operand.expr {
            ExprIr::CallMethod { receiver, key, args } => {
                let PropertyKeyIr::StaticString(name) = key else {
                    return None;
                };
                (
                    name.as_str(),
                    args,
                    AwaitedMethod::Direct { base: receiver, key },
                )
            }
            ExprIr::MaterializeBinding { name, value, body } => {
                let ExprIr::CallIndirect {
                    direct_eval: None,
                    callee,
                    this_arg: Some(this),
                    args,
                    static_regexp_compilation: None,
                } = &body.expr
                else {
                    return None;
                };
                let is_receiver = |expr: &TypedExpr| {
                    matches!(&expr.expr, ExprIr::Identifier(id) if id == name)
                };
                if !is_receiver(this) {
                    return None;
                }
                let key = match &callee.expr {
                    ExprIr::PropertyRead {
                        target,
                        key: PropertyKeyIr::StaticString(key),
                    } if is_receiver(target) => key,
                    ExprIr::SpecOperation {
                        operation: SpecOperationIr::GetV,
                        operands,
                    } => match operands.as_slice() {
                        [target, TypedExpr { expr: ExprIr::String(key), .. }]
                            if is_receiver(target) => key,
                        _ => return None,
                    },
                    _ => return None,
                };
                (
                    key.as_str(),
                    args,
                    AwaitedMethod::Materialized { name, base: value, callee },
                )
            }
            _ => return None,
        };
        let base = match &method {
            AwaitedMethod::Materialized { base, .. } | AwaitedMethod::Direct { base, .. } => base,
        };
        let call = match (name, args.as_slice()) {
            ("resolve", []) => Self::Resolve(None),
            ("resolve", [arg])
                if !matches!(arg.expr, ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)) =>
            {
                Self::Resolve(Some(arg))
            }
            ("read", []) => Self::Read,
            _ => return None,
        };
        if matches!(call, Self::Resolve(_)) {
            let name = match &base.expr {
                ExprIr::Identifier(name)
                | ExprIr::GlobalIdentifierRead { name }
                | ExprIr::GlobalPropertyRead { name } => Some(name),
                ExprIr::EnvironmentIdentifier(identifier) => Some(&identifier.name),
                _ => None,
            };
            if !name.is_some_and(|name| name == "Promise") {
                return None;
            }
        }
        Some((call, method))
    }
}

impl FunctionBuilder<'_> {
    pub(in crate::control_flow) fn emit_plain_async_await_suspend(
        &mut self,
        activation: &GcLocal<AsyncActivation>,
        operand: &TypedExpr,
        resume_state: u32,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let awaited = schema.reserve_value_local(function);
        let direct = self.emit_plain_async_await_operand(operand, &awaited, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_set_resumable_resume_point(resume_state, function)?;
        if let Some(direct) = direct {
            direct.load(function);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_fulfilled_async_await(activation, &awaited, function)?;
            function.instruction(&Instruction::Else);
            self.emit_async_await_reactions(activation, &awaited, function)?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            schema.release_i32_local(direct, function);
        } else if matches!(
            operand.expr,
            ExprIr::ModuleEvaluate(_) | ExprIr::ModuleDeferredImportEvaluate(_)
        ) {
            let promise = schema.reserve_gc_local(function).initialize(
                awaited.cast_reference::<PromiseObject>(schema, function),
                function,
            );
            self.emit_module_import_await_reactions(activation, &promise, function)?;
            promise.clear(function);
        } else {
            self.emit_async_await_reactions(activation, &awaited, function)?;
        }
        awaited.clear(function);
        Ok(())
    }

    fn emit_plain_async_await_operand(
        &mut self,
        operand: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<Option<I32Local>, EmitError> {
        let Some((call, method)) = AwaitedCall::of(operand) else {
            self.compile_expr_to_value(operand, output, function)?;
            return Ok(None);
        };
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let callee = schema.reserve_value_local(function);
        let binding = match method {
            AwaitedMethod::Materialized { name, base, callee: source } => {
                self.compile_expr_to_value(base, &receiver, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.push_scope();
                let (_, binding) = self.retain_expression_operand(name, &receiver, function);
                self.compile_expr_to_value(source, &callee, function)?;
                Some(binding)
            }
            AwaitedMethod::Direct { base, key } => {
                self.compile_expr_to_value(base, &receiver, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.compile_property_read_from_locals(base, key, &receiver, &callee, function)?;
                None
            }
        };
        self.emit_propagate_current_throw_if_needed(function);

        // Evaluate the selected argument after Get. Any fallback calls the
        // same captured method and receiver with the same evaluated argument.
        let input = if let AwaitedCall::Resolve(argument) = call {
            let input = schema.reserve_value_local(function);
            if let Some(argument) = argument {
                self.compile_expr_to_value(argument, &input, function)?;
                self.emit_propagate_current_throw_if_needed(function);
            } else {
                input.set_undefined(function);
            }
            Some(input)
        } else {
            None
        };
        let fulfilled = schema.reserve_i32_local(function);
        fulfilled.set_constant(0, function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let slow = self.open_frame(ControlFrameKind::Block, function);
        match (&call, &input) {
            (AwaitedCall::Resolve(_), Some(input)) => {
                self.emit_is_primitive_tag_i32(input.tag(), function);
                function.instruction(&Instruction::I32Eqz);
                self.emit_branch_if_to_target(slow, function);
                self.emit_with_current_native_builtin(
                    &callee,
                    StandardBuiltinId::PromiseResolve,
                    function,
                    |builder, function| {
                        let realm = builder.load_current_realm(function);
                        builder.emit_is_intrinsic_promise_constructor_i32(
                            &realm, &receiver, function,
                        );
                        function.instruction(&Instruction::I32Eqz);
                        builder.emit_branch_if_to_target(slow, function);
                        builder.emit_intrinsic_fresh_promise_prototype_or_branch(
                            &realm, slow, function,
                        )?;
                        realm.clear(function);
                        output.copy_from(input, function);
                        fulfilled.set_constant(1, function);
                        builder.emit_branch_to_target(done, function);
                        Ok(())
                    },
                )?;
            }
            (AwaitedCall::Read, None) => {
                self.emit_await_stream_reader_read_or_branch(
                    &callee, &receiver, output, fulfilled, slow, function,
                )?;
                self.emit_branch_to_target(done, function);
            }
            _ => unreachable!("captured await argument matches its call"),
        }
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let arguments = match (&call, &input) {
            (AwaitedCall::Resolve(Some(_)), Some(input)) => {
                self.emit_pre_evaluated_arg_vector(&[input], function)
            }
            _ => self.emit_pre_evaluated_arg_vector(&[], function),
        };
        let called = schema.reserve_completion(function);
        self.emit_function_or_proxy_call_with_argv(
            &callee, &receiver, &arguments, &called, function,
        )?;
        self.emit_propagate_pending_throw(&called, &[&callee, &receiver], function);
        output.copy_from(called.value(), function);
        called.clear(function);
        arguments.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if let Some(input) = input {
            input.clear(function);
        }
        if let Some(binding) = binding {
            self.pop_scope();
            self.release_local_binding(binding, function);
        }
        callee.clear(function);
        receiver.clear(function);
        Ok(Some(fulfilled))
    }
}
