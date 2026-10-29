use super::*;
use crate::runtime_helpers::{
    HelperParameters, RelationalComparisonArguments, RelationalComparisonParameters,
};

/// A result emitted as Number, rather than a mutable read/call whose inferred
/// kind merely suggests Number. Arithmetic arms match the actual Number
/// producers in the ordinary emitter; conversion operations retain their
/// complete evaluation and abrupt behavior before the payload is consumed.
fn emits_number_result(expression: &TypedExpr, depth: u8) -> bool {
    let Some(depth) = depth.checked_sub(1) else {
        return false;
    };
    match &expression.expr {
        ExprIr::Number(_) => true,
        ExprIr::SpecOperation {
            operation:
                SpecOperationIr::ToNumber
                | SpecOperationIr::ToIntegerOrInfinity
                | SpecOperationIr::ToLength
                | SpecOperationIr::ToIndex,
            ..
        } => true,
        ExprIr::BinaryNumber { lhs, rhs, .. }
        | ExprIr::CoerciveAdd { lhs, rhs }
        | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
        | ExprIr::BitwiseNumeric { lhs, rhs, .. } => {
            emits_number_result(lhs, depth) && emits_number_result(rhs, depth)
        }
        ExprIr::UnaryMinusNumeric { expr } | ExprIr::UnaryBitwiseNumeric { expr, .. } => {
            emits_number_result(expr, depth)
        }
        ExprIr::UnaryPlus { expr } => {
            expr.kind != ValueKind::Number || emits_number_result(expr, depth)
        }
        // In particular, singleton hints on identifiers, property reads,
        // environment reads or calls cannot bypass their runtime tags.
        _ => false,
    }
}

impl FunctionBuilder<'_> {
    /// GetValue remains left-to-right in P. Actual Number pairs need no
    /// coercion; R retains all other primitive conversions and comparisons.
    pub(crate) fn compile_compare_value_i32(
        &mut self,
        op: RelationalBinaryOp,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if emits_number_result(lhs, 64) && emits_number_result(rhs, 64) {
            return self.compile_number_comparison_i32(op, lhs, rhs, function);
        }
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let predicate = schema.reserve_i32_local(function);
        self.compile_expr_to_value(lhs, &left, function)?;
        self.compile_expr_to_value(rhs, &right, function)?;
        self.emit_value_tag_is_i32(&left, WasmRuntimeValueTag::Number, function);
        self.emit_value_tag_is_i32(&right, WasmRuntimeValueTag::Number, function);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        left.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        right.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        // Wasm ordered comparisons return false for either NaN operand and
        // compare the two zero signs as equal, exactly as all four JS operators.
        function.instruction(&match op {
            RelationalBinaryOp::LessThan => Instruction::F64Lt,
            RelationalBinaryOp::LessThanOrEqual => Instruction::F64Le,
            RelationalBinaryOp::GreaterThan => Instruction::F64Gt,
            RelationalBinaryOp::GreaterThanOrEqual => Instruction::F64Ge,
        });
        predicate.store(function);
        function.instruction(&Instruction::Else);
        let realm = self.emit_execution_realm(function);
        schema
            .call_helper(
                RelationalComparisonArguments::new(
                    &left,
                    &right,
                    &realm,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        realm.clear(function);
        // Keep release ownership until the branches merge, but remove the
        // saved roots before a Throw can branch to the caller's active handler.
        right.reference().clear(function);
        left.reference().clear(function);
        self.finish_to_primitive_operation(
            ToPrimitiveAbruptRoute::ActiveHandler,
            &pending,
            function,
        )?;
        pending.value().scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        match op {
            RelationalBinaryOp::LessThan => function.instruction(&Instruction::F64Lt),
            RelationalBinaryOp::LessThanOrEqual => function.instruction(&Instruction::F64Le),
            RelationalBinaryOp::GreaterThan => function.instruction(&Instruction::F64Gt),
            RelationalBinaryOp::GreaterThanOrEqual => function.instruction(&Instruction::F64Ge),
        };
        predicate.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        right.clear(function);
        left.clear(function);
        predicate.load(function);
        schema.release_i32_local(predicate, function);
        Ok(())
    }

    pub(crate) fn compile_relational_comparison_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RelationalComparison);
        let parameters = self.helper_parameters::<RelationalComparisonParameters>(&mut function);
        self.push_scope();
        self.completion().initialize(&mut function);
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(&mut function);
        let right = schema.reserve_value_local(&mut function);
        let pending = schema.reserve_completion(&mut function);
        for (input, output) in [(&parameters.left, &left), (&parameters.right, &right)] {
            self.emit_tagged_to_primitive_locals(
                ToPrimitiveHint::Number,
                input,
                &pending,
                ToPrimitiveAbruptRoute::ActiveHandler,
                &mut function,
            )?;
            output.copy_from(pending.value(), &mut function);
        }
        let output = schema.reserve_value_local(&mut function);
        self.emit_primitive_relational_comparison_to_value(&left, &right, &output, &mut function)?;
        self.completion().set_normal(&output, &mut function);
        output.clear(&mut function);
        pending.clear(&mut function);
        right.clear(&mut function);
        left.clear(&mut function);
        self.pop_scope();
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        self.completion().emit(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_primitive_relational_comparison_to_value(
        &mut self,
        lhs: &ValueLocals,
        rhs: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let comparison = schema.reserve_i32_local(function);
        let unordered = schema.reserve_i32_local(function);
        left.copy_from(lhs, function);
        right.copy_from(rhs, function);
        function.instruction(&Instruction::I32Const(0));
        unordered.store(function);
        function.instruction(&Instruction::I32Const(0));
        comparison.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_value_tag_is_i32(&left, WasmRuntimeValueTag::String, function);
        self.emit_value_tag_is_i32(&right, WasmRuntimeValueTag::String, function);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let l = schema.reserve_gc_local(function).initialize(
            left.cast_reference::<StringValue>(schema, function),
            function,
        );
        let r = schema.reserve_gc_local(function).initialize(
            right.cast_reference::<StringValue>(schema, function),
            function,
        );
        self.emit_string_payload_utf16_compare_i32(&l, &r, function);
        comparison.store(function);
        r.clear(function);
        l.clear(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for (string_side, other) in [(&left, &right), (&right, &left)] {
            self.emit_value_tag_is_i32(string_side, WasmRuntimeValueTag::String, function);
            self.emit_value_tag_is_i32(other, WasmRuntimeValueTag::BigInt, function);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            let text = schema.reserve_gc_local(function).initialize(
                string_side.cast_reference::<StringValue>(schema, function),
                function,
            );
            let parsed = schema.reserve_value_local(function);
            let valid = schema.reserve_i32_local(function);
            self.emit_string_to_bigint_locals(&text, &parsed, valid, function)?;
            string_side.copy_from(&parsed, function);
            valid.load(function);
            function.instruction(&Instruction::I32Eqz);
            unordered.store(function);
            schema.release_i32_local(valid, function);
            parsed.clear(function);
            text.clear(function);
            unordered.load(function);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_branch_to_target(exit, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for value in [&left, &right] {
            self.emit_primitive_to_number_or_bigint(value, &pending, function)?;
            self.finish_to_primitive_operation(
                ToPrimitiveAbruptRoute::ActiveHandler,
                &pending,
                function,
            )?;
            value.copy_from(pending.value(), function);
        }
        self.emit_value_tag_is_i32(&left, WasmRuntimeValueTag::BigInt, function);
        self.open_frame(ControlFrameKind::If, function);
        let l = schema.reserve_gc_local(function).initialize(
            left.cast_reference::<BigIntValue>(schema, function),
            function,
        );
        self.emit_value_tag_is_i32(&right, WasmRuntimeValueTag::BigInt, function);
        self.open_frame(ControlFrameKind::If, function);
        let r = schema.reserve_gc_local(function).initialize(
            right.cast_reference::<BigIntValue>(schema, function),
            function,
        );
        self.emit_bigint_compare(&l, &r, comparison, function);
        r.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_bigint_number_compare(&l, right.scalar(), comparison, unordered, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        l.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_value_tag_is_i32(&right, WasmRuntimeValueTag::BigInt, function);
        self.open_frame(ControlFrameKind::If, function);
        let r = schema.reserve_gc_local(function).initialize(
            right.cast_reference::<BigIntValue>(schema, function),
            function,
        );
        self.emit_bigint_number_compare(&r, left.scalar(), comparison, unordered, function)?;
        r.clear(function);
        function.instruction(&Instruction::I32Const(0));
        comparison.load(function);
        function.instruction(&Instruction::I32Sub);
        comparison.store(function);
        function.instruction(&Instruction::Else);
        for value in [&left, &right] {
            value.scalar().load(function);
            function.instruction(&Instruction::F64ReinterpretI64);
            value.scalar().load(function);
            function.instruction(&Instruction::F64ReinterpretI64);
            function.instruction(&Instruction::F64Ne);
        }
        function.instruction(&Instruction::I32Or);
        unordered.store(function);
        left.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        right.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Lt);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(-1));
        function.instruction(&Instruction::Else);
        left.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        right.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Gt);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        comparison.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        // NaN represents the specification's unordered comparison. Every
        // source relational operator then compares this normalized Number to 0.
        unordered.load(function);
        function.instruction(&Instruction::If(BlockType::Result(ValType::F64)));
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::NAN)));
        function.instruction(&Instruction::Else);
        comparison.load(function);
        function.instruction(&Instruction::F64ConvertI32S);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64ReinterpretF64);
        output.scalar().store(function);
        output.set_number(output.scalar(), function);
        schema.release_i32_local(unordered, function);
        schema.release_i32_local(comparison, function);
        pending.clear(function);
        right.clear(function);
        left.clear(function);
        Ok(())
    }
}
