//! Number arithmetic keeps intermediate payloads in scalar Wasm locals.
//!
//! Only the same Number proofs used by the ordinary expression emitter select
//! arithmetic here. Other leaves retain their full expression evaluation,
//! including coercions and abrupt completion, before their payload is copied.
use super::*;

impl FunctionBuilder<'_> {
    pub(crate) fn compile_number_payload(
        &mut self,
        expression: &TypedExpr,
        output: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match &expression.expr {
            ExprIr::Number(bits) => {
                function.instruction(&Instruction::I64Const(*bits as i64));
                output.store(function);
            }
            ExprIr::Identifier(name)
                if expression.kind == ValueKind::Number
                    && name != LEXICAL_THIS_NAME
                    && matches!(self.lookup_binding(name), Some(BindingStorage::Local(_))) =>
            {
                let Some(BindingStorage::Local(id)) = self.lookup_binding(name) else {
                    unreachable!("the scalar read guard selects an actual local binding")
                };
                self.check_local_binding_initialized(id, function)?;
                self.local_binding(id).value().scalar().load(function);
                output.store(function);
            }
            ExprIr::BinaryNumber { op, lhs, rhs } if expression.kind == ValueKind::Number => {
                self.compile_number_arithmetic_payload(*op, lhs, rhs, output, function)?;
            }
            ExprIr::CoerciveBinaryNumber { op, lhs, rhs }
                if lhs.kind == ValueKind::Number && rhs.kind == ValueKind::Number =>
            {
                self.compile_number_arithmetic_payload(*op, lhs, rhs, output, function)?;
            }
            ExprIr::UnaryPlus { expr } if expr.kind == ValueKind::Number => {
                self.compile_number_payload(expr, output, function)?;
            }
            ExprIr::UnaryMinusNumeric { expr } if expr.kind == ValueKind::Number => {
                self.compile_number_payload(expr, output, function)?;
                output.load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                function.instruction(&Instruction::F64Neg);
                function.instruction(&Instruction::I64ReinterpretF64);
                output.store(function);
            }
            _ => {
                let value = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(expression, &value, function)?;
                value.scalar().load(function);
                output.store(function);
                value.clear(function);
            }
        }
        Ok(())
    }

    /// Consume an ordered Number comparison directly as a Wasm predicate.
    /// Value-producing comparisons and branch conditions use this same owner.
    pub(crate) fn compile_number_comparison_i32(
        &mut self,
        op: RelationalBinaryOp,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let left = schema.reserve_i64_local(function);
        let right = schema.reserve_i64_local(function);
        self.compile_number_payload(lhs, left, function)?;
        self.compile_number_payload(rhs, right, function)?;
        left.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        right.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&match op {
            RelationalBinaryOp::LessThan => Instruction::F64Lt,
            RelationalBinaryOp::LessThanOrEqual => Instruction::F64Le,
            RelationalBinaryOp::GreaterThan => Instruction::F64Gt,
            RelationalBinaryOp::GreaterThanOrEqual => Instruction::F64Ge,
        });
        schema.release_i64_local(right, function);
        schema.release_i64_local(left, function);
        Ok(())
    }

    pub(super) fn compile_number_arithmetic_payload(
        &mut self,
        op: ArithmeticBinaryOp,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        output: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let left = schema.reserve_i64_local(function);
        let right = schema.reserve_i64_local(function);
        // Keep the evaluated left value while evaluating the complete right
        // expression. In particular, no operand coercion moves across another
        // operand's evaluation and no floating-point operation is reassociated.
        self.compile_number_payload(lhs, left, function)?;
        self.compile_number_payload(rhs, right, function)?;
        self.emit_number_arithmetic(op, left, right, output, function)?;
        schema.release_i64_local(right, function);
        schema.release_i64_local(left, function);
        Ok(())
    }
}
