//! `F.call(thisArg, ...args)` syntax only selects a candidate. When the actual
//! callee is the %Function.prototype.call% builtin of any Realm, its algorithm
//! is Call(F, thisArg, args) after IsCallable(F). A published local entry of F
//! proves F callable, so that call enters F's entry directly. Every other
//! callee, and any F without such an entry, keeps the complete dispatch with
//! the same once-evaluated operands.
use super::*;
use crate::gc_types::{
    ExecutableCodeKind, ExecutableCodeSchema, FunctionObject, FunctionObjectSchema, GcI32Constant,
    GcNullability, ValueLocals,
};

impl FunctionBuilder<'_> {
    pub(super) fn try_emit_function_call_forwarding(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        // The forwarded list omits thisArg and must fit a local entry.
        if arguments.is_empty()
            || arguments.len() > 6
            || arguments.iter().any(|argument| {
                matches!(
                    argument.expr,
                    ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
                )
            })
        {
            return Ok(false);
        }
        let schema = self.runtime_schema();
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = schema.reserve_value_local(function);
            self.compile_expr_to_value(argument, &value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            // A later operand can call, allocate, or replace the source binding.
            values.push(value);
        }
        let references: Vec<_> = values.iter().collect();
        let result = schema.reserve_completion(function);
        let handled = schema.reserve_i32_local(function);
        handled.set_constant(0, function);
        let forwards = schema.reserve_i32_local(function);
        forwards.set_constant(0, function);
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
        let kind = schema.reserve_i32_local(function);
        schema
            .field(ExecutableCodeSchema::KIND)
            .read(&code, schema, function)
            .store(kind, function);
        schema
            .field(ExecutableCodeSchema::STANDARD_BUILTIN)
            .read(&code, schema, function)
            .store(forwards, function);
        forwards.load(function);
        function.instruction(&Instruction::I32Const(
            StandardBuiltinId::FunctionPrototypeCall.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        kind.load(function);
        function.instruction(&Instruction::I32Const(
            ExecutableCodeKind::StandardBuiltin.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        forwards.store(function);
        schema.release_i32_local(kind, function);
        code.clear(function);
        callable.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        forwards.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let entered = self.emit_local_argument_call_values_for_count(
            receiver,
            references[0],
            &references[1..],
            continuation,
            &result,
            function,
        )?;
        entered.load(function);
        handled.store(function);
        schema.release_i32_local(entered, function);
        function.instruction(&Instruction::Else);
        let entered = self.emit_local_argument_call_values_for_count(
            callee,
            receiver,
            &references,
            continuation,
            &result,
            function,
        )?;
        entered.load(function);
        handled.store(function);
        schema.release_i32_local(entered, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(forwards, function);

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
        drop(references);
        for value in values.into_iter().rev() {
            value.clear(function);
        }
        Ok(true)
    }
}
