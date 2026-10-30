//! A saved actual native push consumes one to three rooted operands directly.
//! Argument storage and ordinary JS dispatch exist only on the decline arm.
use super::*;
use crate::gc_types::ValueLocals;
use crate::runtime_helpers::{
    ArrayPackedPush1Arguments, ArrayPackedPush2Arguments, ArrayPackedPush3Arguments,
};

impl FunctionBuilder<'_> {
    pub(super) fn try_emit_packed_array_push_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        if !matches!(continuation, CallContinuation::Continue)
            || !(1..=3).contains(&arguments.len())
            || arguments.iter().any(|argument| {
                matches!(
                    argument.expr,
                    ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
                )
            })
        {
            return Ok(false);
        }
        let s = self.runtime_schema();
        let mut saved = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = s.reserve_value_local(function);
            self.compile_expr_to_value(argument, &value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            saved.push(value);
        }
        let handled = s.reserve_i32_local(function);
        handled.set_constant(0, function);
        self.emit_with_current_native_builtin(
            callee,
            StandardBuiltinId::ArrayPrototypePush,
            function,
            |builder, function| {
                let pending = s.reserve_completion(function);
                let base = builder.runtime_helper_base()?;
                match saved.as_slice() {
                    [first] => s
                        .call_helper(
                            ArrayPackedPush1Arguments::new(receiver, first),
                            base,
                            function,
                        )
                        .store(&pending, function),
                    [first, second] => s
                        .call_helper(
                            ArrayPackedPush2Arguments::new(receiver, first, second),
                            base,
                            function,
                        )
                        .store(&pending, function),
                    [first, second, third] => s
                        .call_helper(
                            ArrayPackedPush3Arguments::new(receiver, first, second, third),
                            base,
                            function,
                        )
                        .store(&pending, function),
                    _ => unreachable!("fixed push admission checked its arity"),
                }
                pending.kind().load(function);
                function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
                function.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, function);
                builder.completion().copy_from(&pending, function);
                builder.emit_propagate_current_throw(function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                // A completed Number is the helper's publication capability;
                // Undefined means a pure miss before any mutation or user code.
                pending.value().tag().load(function);
                function.instruction(&Instruction::I32Const(ValueKind::Number.tag() as i32));
                function.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, function);
                output.copy_from(pending.value(), function);
                handled.set_constant(1, function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                pending.clear(function);
                Ok(())
            },
        )?;
        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let references = saved.iter().collect::<Vec<_>>();
        let list = self.emit_pre_evaluated_arg_vector(&references, function);
        self.emit_indirect_call_with_argv_to_value(
            callee,
            receiver,
            &list,
            continuation,
            output,
            function,
        )?;
        list.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        s.release_i32_local(handled, function);
        for value in saved.into_iter().rev() {
            value.clear(function);
        }
        Ok(true)
    }
}
