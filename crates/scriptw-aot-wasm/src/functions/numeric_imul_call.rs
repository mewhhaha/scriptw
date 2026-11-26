//! Numeric Math (`imul`, unary, `min`/`max`) and 1-arg `String`/`Number`
//! inline fast paths avoid argument storage after actual callee proof.
use super::*;
use crate::gc_types::{ScalarValue, StringValue, ValueLocals};

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
        // A tail continuation admits the same route: a proper tail call to
        // %Math.imul% is observable only through stack depth, which the
        // inline arm never consumes. Its value then returns normally; a guard
        // miss keeps the exact tail call with these once-evaluated operands.
        if [left, right].iter().any(|argument| {
            matches!(
                argument.expr,
                ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
            )
        }) {
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

    /// Numeric-only unary Math builtins (`floor`, `ceil`, `trunc`, `abs`,
    /// `sqrt`) lower directly to a single Wasm f64 instruction after proving
    /// the current Realm's canonical builtin and a Number operand.
    pub(super) fn try_emit_numeric_math_unary_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        builtin: StandardBuiltinId,
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let [arg_expr] = arguments else {
            return Ok(false);
        };
        if matches!(
            arg_expr.expr,
            ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
        ) {
            return Ok(false);
        }
        let op = match builtin {
            StandardBuiltinId::MathFloor => Instruction::F64Floor,
            StandardBuiltinId::MathCeil => Instruction::F64Ceil,
            StandardBuiltinId::MathTrunc => Instruction::F64Trunc,
            StandardBuiltinId::MathAbs => Instruction::F64Abs,
            StandardBuiltinId::MathSqrt => Instruction::F64Sqrt,
            _ => return Ok(false),
        };
        let schema = self.runtime_schema();
        let arg = schema.reserve_value_local(function);
        self.compile_expr_to_value(arg_expr, &arg, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        let handled = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        handled.store(function);

        self.emit_with_current_native_builtin(callee, builtin, function, |builder, function| {
            arg.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
            function.instruction(&Instruction::I32Eq);
            builder.open_frame(ControlFrameKind::If, function);
            let bits = schema.reserve_i64_local(function);
            arg.scalar().load(function);
            function.instruction(&Instruction::F64ReinterpretI64);
            function.instruction(&op);
            function.instruction(&Instruction::I64ReinterpretF64);
            bits.store(function);
            output.set_number(bits, function);
            schema.release_i64_local(bits, function);
            function.instruction(&Instruction::I32Const(1));
            handled.store(function);
            builder.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            Ok(())
        })?;

        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let list = self.emit_pre_evaluated_arg_vector(&[&arg], function);
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
        arg.clear(function);
        Ok(true)
    }

    /// Numeric-only binary `Math.min(a, b)` and `Math.max(a, b)` lower
    /// directly to Wasm `f64.min` / `f64.max` after proving the current
    /// Realm's canonical builtin and two Number operands. Wasm `f64.min` and
    /// `f64.max` match ECMAScript on NaN, signed zero (`-0` vs `+0`), and
    /// infinities.
    pub(super) fn try_emit_numeric_math_binary_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        builtin: StandardBuiltinId,
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let [left, right] = arguments else {
            return Ok(false);
        };
        if [left, right].iter().any(|argument| {
            matches!(
                argument.expr,
                ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
            )
        }) {
            return Ok(false);
        }
        let op = match builtin {
            StandardBuiltinId::MathMin => Instruction::F64Min,
            StandardBuiltinId::MathMax => Instruction::F64Max,
            _ => return Ok(false),
        };
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

        self.emit_with_current_native_builtin(callee, builtin, function, |builder, function| {
            lhs.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
            function.instruction(&Instruction::I32Eq);
            rhs.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32And);
            builder.open_frame(ControlFrameKind::If, function);
            let bits = schema.reserve_i64_local(function);
            lhs.scalar().load(function);
            function.instruction(&Instruction::F64ReinterpretI64);
            rhs.scalar().load(function);
            function.instruction(&Instruction::F64ReinterpretI64);
            function.instruction(&op);
            function.instruction(&Instruction::I64ReinterpretF64);
            bits.store(function);
            output.set_number(bits, function);
            schema.release_i64_local(bits, function);
            function.instruction(&Instruction::I32Const(1));
            handled.store(function);
            builder.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            Ok(())
        })?;

        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
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

    /// Ordinary (non-construct) 1-argument `String(x)` inline fast path for
    /// String (identity) and Number (`Number::toString`) operands after
    /// proving the current Realm's `%String%`. Symbols, Objects, and other
    /// values fall through to the canonical builtin.
    pub(super) fn try_emit_string_constructor_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let [arg_expr] = arguments else {
            return Ok(false);
        };
        if matches!(
            arg_expr.expr,
            ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
        ) {
            return Ok(false);
        }
        let schema = self.runtime_schema();
        let arg = schema.reserve_value_local(function);
        self.compile_expr_to_value(arg_expr, &arg, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        let handled = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        handled.store(function);

        self.emit_with_current_native_builtin(
            callee,
            StandardBuiltinId::StringConstructor,
            function,
            |builder, function| {
                arg.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
                function.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, function);
                output.copy_from(&arg, function);
                function.instruction(&Instruction::I32Const(1));
                handled.store(function);
                function.instruction(&Instruction::Else);
                arg.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
                function.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, function);
                let formatted = schema.reserve_gc_local(function).initialize(
                    builder.emit_number_to_string_payload(arg.scalar(), function)?,
                    function,
                );
                output.set_reference(&formatted, schema, function);
                formatted.clear(function);
                function.instruction(&Instruction::I32Const(1));
                handled.store(function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                Ok(())
            },
        )?;

        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let list = self.emit_pre_evaluated_arg_vector(&[&arg], function);
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
        arg.clear(function);
        Ok(true)
    }

    /// Ordinary (non-construct) 1-argument `Number(x)` inline fast path for
    /// Number, String, Boolean, null, and undefined operands after proving
    /// the current Realm's `%Number%`. BigInts, Symbols, and Objects fall
    /// through to the canonical builtin.
    pub(super) fn try_emit_number_constructor_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let [arg_expr] = arguments else {
            return Ok(false);
        };
        if matches!(
            arg_expr.expr,
            ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
        ) {
            return Ok(false);
        }
        let schema = self.runtime_schema();
        let arg = schema.reserve_value_local(function);
        self.compile_expr_to_value(arg_expr, &arg, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        let handled = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        handled.store(function);

        self.emit_with_current_native_builtin(
            callee,
            StandardBuiltinId::NumberConstructor,
            function,
            |builder, function| {
                arg.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
                function.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, function);
                output.copy_from(&arg, function);
                function.instruction(&Instruction::I32Const(1));
                handled.store(function);
                function.instruction(&Instruction::Else);
                arg.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
                function.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, function);
                let text = schema.reserve_gc_local(function).initialize(
                    arg.cast_reference::<StringValue>(schema, function),
                    function,
                );
                let bits = schema.reserve_i64_local(function);
                builder.emit_string_to_number_payload(&text, bits, function)?;
                text.clear(function);
                output.set_number(bits, function);
                schema.release_i64_local(bits, function);
                function.instruction(&Instruction::I32Const(1));
                handled.store(function);
                function.instruction(&Instruction::Else);
                arg.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Boolean.tag()));
                function.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, function);
                let bits = schema.reserve_i64_local(function);
                arg.scalar().load(function);
                function.instruction(&Instruction::F64ConvertI64U);
                function.instruction(&Instruction::I64ReinterpretF64);
                bits.store(function);
                output.set_number(bits, function);
                schema.release_i64_local(bits, function);
                function.instruction(&Instruction::I32Const(1));
                handled.store(function);
                function.instruction(&Instruction::Else);
                arg.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
                function.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, function);
                output.set_scalar(ScalarValue::NumberBits(0), function);
                function.instruction(&Instruction::I32Const(1));
                handled.store(function);
                function.instruction(&Instruction::Else);
                arg.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
                function.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, function);
                let bits = schema.reserve_i64_local(function);
                builder.emit_nan_payload(function);
                bits.store(function);
                output.set_number(bits, function);
                schema.release_i64_local(bits, function);
                function.instruction(&Instruction::I32Const(1));
                handled.store(function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                Ok(())
            },
        )?;

        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let list = self.emit_pre_evaluated_arg_vector(&[&arg], function);
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
        arg.clear(function);
        Ok(true)
    }
}
