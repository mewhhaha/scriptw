//! Number arithmetic keeps intermediate payloads in scalar Wasm locals.
//!
//! Ordinary Number proofs and checked private-counter reads select arithmetic
//! here. Other leaves retain their full expression evaluation,
//! including coercions and abrupt completion, before their payload is copied.
use super::*;
use crate::control_flow::BoundedInductionRead;

/// The literal divisor cannot coerce or run source code. Its constructor also
/// excludes zero, fractions and values outside the unsigned Wasm operand range.
enum UnsignedRemainderDivisor {
    Mask(u32),
    Remainder(u32),
}

impl UnsignedRemainderDivisor {
    fn new(expression: &TypedExpr) -> Option<Self> {
        let ExprIr::Number(bits) = expression.expr else {
            return None;
        };
        let number = f64::from_bits(bits);
        if !(1.0..=f64::from(u32::MAX)).contains(&number) || number.fract() != 0.0 {
            return None;
        }
        let divisor = number as u32;
        if divisor.is_power_of_two() {
            return Some(Self::Mask(divisor - 1));
        }
        Some(Self::Remainder(divisor))
    }

    fn emit(self, function: &mut Function) {
        match self {
            Self::Mask(mask) => {
                function.instruction(&Instruction::I32Const(mask as i32));
                function.instruction(&Instruction::I32And);
            }
            Self::Remainder(divisor) => {
                function.instruction(&Instruction::I32Const(divisor as i32));
                function.instruction(&Instruction::I32RemU);
            }
        }
    }
}

impl FunctionBuilder<'_> {
    /// Admission belongs to the ordinary loop's private binding proof. A miss
    /// occurs before evaluating either operand and retains Number::remainder.
    pub(super) fn try_compile_bounded_remainder(
        &mut self,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        output: I64Local,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(read) = self.bounded_induction_read(lhs) else {
            return Ok(false);
        };
        let Some(divisor) = UnsignedRemainderDivisor::new(rhs) else {
            return Ok(false);
        };
        self.compile_bounded_remainder(read, divisor, output, function)?;
        Ok(true)
    }

    fn compile_bounded_remainder(
        &mut self,
        read: BoundedInductionRead<'_>,
        divisor: UnsignedRemainderDivisor,
        output: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        // Keep the canonical binding and its initialized read. The witness
        // proves this exact expression is a nonnegative integral Number in u32,
        // so conversion cannot trap and the remainder has positive zero sign.
        self.compile_number_payload(read.expression(), output, function)?;
        output.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::I32TruncF64U);
        divisor.emit(function);
        function.instruction(&Instruction::F64ConvertI32U);
        function.instruction(&Instruction::I64ReinterpretF64);
        output.store(function);
        Ok(())
    }

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
        if matches!(op, ArithmeticBinaryOp::Mod)
            && self.try_compile_bounded_remainder(lhs, rhs, output, function)?
        {
            return Ok(());
        }
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
