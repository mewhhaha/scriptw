//! The actual callable's published typed entry is the guard, including across P.
use super::*;
use crate::function_entry::{LocalArgumentCallInputs, LocalArgumentsCallableRole};
use crate::gc_types::{
    CompletionLocals, ExecutableCodeKind, ExecutableCodeSchema, FunctionContextSchema,
    FunctionObject, FunctionObjectSchema, GcI32Constant, GcNullability, I32Local,
    LocalArgumentsCallable, ValueLocals, WideLocalArgumentsCallable,
};

impl FunctionBuilder<'_> {
    pub(super) fn try_emit_local_argument_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        known_target: Option<&WasmFunctionMeta>,
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        // An ineligible source hint does not justify another inline guard.
        // Full dispatch still handles an eligible replacement correctly.
        if known_target.is_some_and(|meta| meta.entry.local_arguments().is_none()) {
            return Ok(false);
        }
        if arguments.iter().any(|argument| {
            matches!(
                argument.expr,
                ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
            )
        }) {
            return Ok(false);
        }
        match arguments.len() {
            0..=2 => self.emit_local_argument_call::<LocalArgumentsCallable>(
                callee,
                receiver,
                arguments,
                known_target,
                continuation,
                output,
                function,
            ),
            3..=5 => self.emit_local_argument_call::<WideLocalArgumentsCallable>(
                callee,
                receiver,
                arguments,
                known_target,
                continuation,
                output,
                function,
            ),
            _ => Ok(false),
        }
    }

    fn emit_local_argument_call<R: LocalArgumentsCallableRole>(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        known_target: Option<&WasmFunctionMeta>,
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let schema = self.runtime_schema();
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = schema.reserve_value_local(function);
            self.compile_expr_to_value(argument, &value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            // A later operand can call, allocate, or replace the source binding.
            // Keep both the actual callee and all preceding arguments rooted.
            values.push(value);
        }
        let result = schema.reserve_completion(function);
        let references: Vec<_> = values.iter().collect();
        let handled = self.emit_local_argument_call_values::<R>(
            callee,
            receiver,
            &references,
            known_target,
            continuation,
            &result,
            function,
        )?;

        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let list = self.emit_pre_evaluated_arg_vector(&references, function);
        match continuation {
            CallContinuation::Continue => self.emit_function_or_proxy_call_with_argv(
                callee, receiver, &list, &result, function,
            )?,
            CallContinuation::Return => {
                self.emit_prepared_tail_call(callee, receiver, &list, function)?;
            }
        }
        list.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_propagate_current_throw_if_needed(function);
        output.copy_from(result.value(), function);
        schema.release_i32_local(handled, function);
        result.clear(function);
        for value in values.into_iter().rev() {
            value.clear(function);
        }
        Ok(true)
    }

    /// Borrow once-evaluated values. The owner chooses fallback allocation and
    /// abrupt routing after this guard has preserved the whole call result.
    pub(crate) fn emit_local_argument_call_from_values(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[&ValueLocals],
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        match arguments.len() {
            0..=2 => self.emit_local_argument_call_values::<LocalArgumentsCallable>(
                callee,
                receiver,
                arguments,
                None,
                &CallContinuation::Continue,
                result,
                function,
            ),
            3..=5 => self.emit_local_argument_call_values::<WideLocalArgumentsCallable>(
                callee,
                receiver,
                arguments,
                None,
                &CallContinuation::Continue,
                result,
                function,
            ),
            _ => {
                let handled = self.runtime_schema().reserve_i32_local(function);
                handled.set_constant(0, function);
                Ok(handled)
            }
        }
    }

    fn emit_local_argument_call_values<R: LocalArgumentsCallableRole>(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[&ValueLocals],
        known_target: Option<&WasmFunctionMeta>,
        continuation: &CallContinuation,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(function);
        handled.set_constant(0, function);
        callee.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
        function.instruction(&Instruction::I32Eq);
        callee.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let callable = schema.reserve_gc_local(function).initialize(
            callee.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CODE)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        let optional = R::load_optional_entry(&code, schema, function);
        optional.load(function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let entry = schema
            .reserve_function_local::<R, _>(function)
            .initialize(optional.load(function).require_non_null(function), function);
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CONTEXT)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionContextSchema::REALM)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        let caller_realm = self.load_current_realm(function);
        // Cross-P source IDs can collide. The actual code and its defining
        // program must both agree before a source hint becomes a direct call.
        let direct = if let Some(meta) = known_target.filter(|meta| {
            meta.entry
                .local_arguments()
                .is_some_and(|entry| entry.is_source() && entry.arity() == R::ARITY)
        }) {
            let owner = schema.reserve_gc_local(function).initialize(
                schema
                    .field(FunctionContextSchema::DEFINING_PROGRAM)
                    .read(&context, schema, function)
                    .reference(),
                function,
            );
            let source = schema.reserve_i64_local(function);
            let kind = schema.reserve_i32_local(function);
            let matches = schema.reserve_i32_local(function);
            schema
                .field(ExecutableCodeSchema::SOURCE_ID)
                .read(&code, schema, function)
                .store_i64(source, function);
            schema
                .field(ExecutableCodeSchema::KIND)
                .read(&code, schema, function)
                .store(kind, function);
            source.load(function);
            function.instruction(&Instruction::I64Const(meta.entry.source_id()));
            function.instruction(&Instruction::I64Eq);
            kind.load(function);
            function.instruction(&Instruction::I32Const(
                ExecutableCodeKind::JavaScript.encode(),
            ));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32And);
            owner.load(schema, function);
            self.current_program().reference(schema, function);
            function.instruction(&Instruction::RefEq);
            function.instruction(&Instruction::I32And);
            matches.store(function);
            schema.release_i32_local(kind, function);
            schema.release_i64_local(source, function);
            owner.clear(function);
            Some((
                matches,
                meta.entry
                    .local_arguments()
                    .expect("selected canonical entry"),
            ))
        } else {
            None
        };
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        match continuation {
            CallContinuation::Continue => {
                self.replace_current_realm(&realm, function);
                match direct {
                    Some((guard, planned)) => {
                        guard.load(function);
                        self.open_frame(ControlFrameKind::If, function);
                        result.store_call(
                            LocalArgumentCallInputs::<R>::call(
                                receiver,
                                arguments,
                                &undefined,
                                &caller_realm,
                                self.current_program(),
                            )
                            .emit_planned_call(&callable, planned, schema, function),
                            function,
                        );
                        function.instruction(&Instruction::Else);
                        result.store_call(
                            LocalArgumentCallInputs::<R>::call(
                                receiver,
                                arguments,
                                &undefined,
                                &caller_realm,
                                self.current_program(),
                            )
                            .emit_call(&callable, &entry, schema, function),
                            function,
                        );
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                    }
                    None => result.store_call(
                        LocalArgumentCallInputs::<R>::call(
                            receiver,
                            arguments,
                            &undefined,
                            &caller_realm,
                            self.current_program(),
                        )
                        .emit_call(&callable, &entry, schema, function),
                        function,
                    ),
                }
                self.replace_current_realm(&caller_realm, function);
            }
            CallContinuation::Return => {
                // The P owner and Realm survive lexical unwind. Operand work
                // and the actual typed-entry guard precede caller retirement.
                let calling_program = self.capture_current_program(function);
                self.emit_retire_tail_call_activation(function);
                self.replace_current_realm(&realm, function);
                let inputs = LocalArgumentCallInputs::<R>::call(
                    receiver,
                    arguments,
                    &undefined,
                    &caller_realm,
                    &calling_program,
                );
                match direct {
                    Some((guard, planned)) => {
                        guard.load(function);
                        self.open_frame(ControlFrameKind::If, function);
                        inputs.emit_planned_return_call(&callable, planned, schema, function);
                        function.instruction(&Instruction::Else);
                        inputs.emit_return_call(&callable, &entry, schema, function);
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                    }
                    None => inputs.emit_return_call(&callable, &entry, schema, function),
                }
                calling_program.clear(function);
            }
        }
        // Only Throw or the returned value leaves this private scratch result.
        handled.set_constant(1, function);
        if let Some((guard, _)) = direct {
            schema.release_i32_local(guard, function);
        }
        undefined.clear(function);
        caller_realm.clear(function);
        realm.clear(function);
        context.clear(function);
        entry.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        optional.clear(function);
        code.clear(function);
        callable.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        Ok(handled)
    }
}
