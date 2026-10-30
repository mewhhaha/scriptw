//! Numeric-only Math.imul avoids argument storage after actual callee proof.
use super::*;
use crate::gc_types::ValueLocals;

impl FunctionBuilder<'_> {
    /// Static target knowledge selects a candidate; immutable code and current Realm
    /// identity decide the runtime route after all observable argument effects.
    pub(super) fn try_emit_numeric_imul_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let [left, right] = arguments else {
            return Ok(false);
        };
        if !matches!(continuation, CallContinuation::Continue)
            || [left, right].iter().any(|argument| {
                matches!(
                    argument.expr,
                    ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
                )
            })
        {
            return Ok(false);
        }
        let schema = self.runtime_schema();
        let lhs = schema.reserve_value_local(function);
        let rhs = schema.reserve_value_local(function);
        self.compile_expr_to_value(left, &lhs, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.compile_expr_to_value(right, &rhs, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        let handled = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        handled.store(function);

        self.emit_with_current_native_builtin(
            callee,
            StandardBuiltinId::MathImul,
            function,
            |builder, function| {
                lhs.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
                function.instruction(&Instruction::I32Eq);
                rhs.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
                function.instruction(&Instruction::I32Eq);
                function.instruction(&Instruction::I32And);
                builder.open_frame(ControlFrameKind::If, function);
                // The inputs are already Numbers: this existing modulo authority runs
                // no user code, cannot throw, and needs no opaque native caller frame.
                let left_bits = schema.reserve_i64_local(function);
                let right_bits = schema.reserve_i64_local(function);
                builder.emit_to_uint32_i64_from_number_payload(lhs.scalar(), left_bits, function);
                builder.emit_to_uint32_i64_from_number_payload(rhs.scalar(), right_bits, function);
                left_bits.load(function);
                function.instruction(&Instruction::I32WrapI64);
                right_bits.load(function);
                function.instruction(&Instruction::I32WrapI64);
                function.instruction(&Instruction::I32Mul);
                function.instruction(&Instruction::F64ConvertI32S);
                function.instruction(&Instruction::I64ReinterpretF64);
                left_bits.store(function);
                output.set_number(left_bits, function);
                schema.release_i64_local(right_bits, function);
                schema.release_i64_local(left_bits, function);
                function.instruction(&Instruction::I32Const(1));
                handled.store(function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                Ok(())
            },
        )?;

        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        // Non-Numbers, wrappers, foreign Realms and patched methods use the
        // original dispatch with these exact once-evaluated, rooted operands.
        let list = self.emit_pre_evaluated_arg_vector(&[&lhs, &rhs], function);
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
        schema.release_i32_local(handled, function);
        rhs.clear(function);
        lhs.clear(function);
        Ok(true)
    }
}
