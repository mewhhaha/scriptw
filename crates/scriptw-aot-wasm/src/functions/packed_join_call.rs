//! Actual native join can consume rooted primitive operands without argv or an
//! opaque callback boundary. Any unsupported case uses the saved ordinary call.
use super::*;
use crate::gc_types::{ArrayObject, ArrayObjectSchema, GcNullability, StringValue, ValueLocals};
use crate::runtime_helpers::ArrayPackedJoinArguments;

impl FunctionBuilder<'_> {
    pub(super) fn try_emit_packed_array_join_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        // Decline before any source argument evaluation. Once accepted here,
        // this emitter owns both the pure route and saved-operand fallback.
        if !matches!(continuation, CallContinuation::Continue)
            || arguments.len() > 1
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
        let separator_value = s.reserve_value_local(function);
        match arguments.first() {
            Some(argument) => {
                self.compile_expr_to_value(argument, &separator_value, function)?;
                self.emit_propagate_current_throw_if_needed(function);
            }
            None => separator_value.set_undefined(function),
        }
        let handled = s.reserve_i32_local(function);
        handled.set_constant(0, function);
        self.emit_with_current_native_builtin(
            callee,
            StandardBuiltinId::ArrayPrototypeJoin,
            function,
            |builder, function| {
                receiver.reference().load(function);
                function.instruction(&Instruction::RefTestNonNull(
                    s.reference_type::<ArrayObject>(GcNullability::NonNullable)
                        .heap_type,
                ));
                separator_value.tag().load(function);
                function.instruction(&Instruction::I32Const(ValueKind::String.tag() as i32));
                function.instruction(&Instruction::I32Eq);
                separator_value.tag().load(function);
                function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
                function.instruction(&Instruction::I32Eq);
                function.instruction(&Instruction::I32Or);
                function.instruction(&Instruction::I32And);
                builder.open_frame(ControlFrameKind::If, function);
                // Both admitted arms retain a completed String root. Resolving the
                // default comma belongs only to Undefined, including omitted arguments.
                separator_value.tag().load(function);
                function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
                function.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, function);
                let separator = s.reserve_gc_local(function).initialize(
                    builder.emit_interned_string_reference(",", function)?,
                    function,
                );
                function.instruction(&Instruction::Else);
                separator.replace(
                    separator_value.cast_reference::<StringValue>(s, function),
                    function,
                );
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                let array = s.reserve_gc_local(function).initialize(
                    receiver.cast_reference::<ArrayObject>(s, function),
                    function,
                );
                let length = s.reserve_i64_local(function);
                // Source argument effects have all completed. Actual Array length is
                // an own intrinsic field, with no getter or observable boxing.
                s.field(ArrayObjectSchema::LENGTH)
                    .read(&array, s, function)
                    .store_i64(length, function);
                // Native entries do not acquire source activation depth. The opaque
                // native caller boundary only affects source callbacks; this helper
                // performs no user code, and declines before any such callback.
                let text = s
                    .call_helper(
                        ArrayPackedJoinArguments::new(receiver, length, &separator),
                        builder.runtime_helper_base()?,
                        function,
                    )
                    .bind(s, s.reserve_gc_local(function), function);
                text.load(s, function).is_null(function);
                function.instruction(&Instruction::I32Eqz);
                builder.open_frame(ControlFrameKind::If, function);
                let present = s
                    .reserve_gc_local(function)
                    .initialize(text.load(s, function).require_non_null(function), function);
                output.set_reference(&present, s, function);
                present.clear(function);
                handled.set_constant(1, function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                text.clear(function);
                s.release_i64_local(length, function);
                array.clear(function);
                separator.clear(function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                Ok(())
            },
        )?;
        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let saved_arguments = [&separator_value];
        let list = self.emit_pre_evaluated_arg_vector(
            if arguments.is_empty() {
                &[]
            } else {
                &saved_arguments
            },
            function,
        );
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
        separator_value.clear(function);
        Ok(true)
    }
}
