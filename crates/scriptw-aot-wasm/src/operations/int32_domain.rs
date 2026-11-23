//! Number bitwise results stay in the int32 domain until a consumer needs a
//! Number.
//!
//! ECMA-262 ToInt32 and ToUint32 are total on Numbers, and every Number
//! bitwise operator (`& | ^ ~ << >>`) returns an integral Number in
//! [-2^31, 2^31), `>>>` one in [0, 2^32); none returns -0 or NaN. The 32
//! result bits and the operator's signedness therefore determine the Number
//! exactly. Another bitwise operator, ToBoolean, an integer comparison, a
//! switch table or an array-index probe consumes those bits directly; only a
//! consumer of a general Number converts them, once.
//!
//! Admission is syntactic over the typed IR and needs no range analysis: an
//! operand's `ValueKind::Number` is the same authority the binary64 path
//! already uses to skip ToNumeric. When one operand may need ToNumeric, the
//! other is a Number, so a BigInt ToNumeric result is the mixed-type
//! TypeError and the operator has no BigInt completion. `>>>` has none for
//! any operands. Every other pair keeps the general Number/BigInt owner.
use super::*;

/// How the 32 result bits denote the operator's Number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Int32Signedness {
    /// ToInt32 range: every operator except `>>>`.
    Signed,
    /// ToUint32 range: `>>>`, whose results at or above 2^31 are Numbers
    /// that do not fit the signed range.
    Unsigned,
}

impl Int32Signedness {
    /// Converts the bits on the stack to the exact binary64 Number.
    pub(crate) fn emit_to_f64(self, function: &mut Function) {
        function.instruction(&match self {
            Self::Signed => Instruction::F64ConvertI32S,
            Self::Unsigned => Instruction::F64ConvertI32U,
        });
    }

    fn emit_to_i64(self, function: &mut Function) {
        function.instruction(&match self {
            Self::Signed => Instruction::I64ExtendI32S,
            Self::Unsigned => Instruction::I64ExtendI32U,
        });
    }
}

#[derive(Clone, Copy)]
enum CoercedOperand {
    Left,
    Right,
}

#[derive(Clone, Copy)]
enum Int32Form<'a> {
    /// Both operands are Numbers: ToNumeric is the identity on each.
    Numbers {
        op: BitwiseBinaryOp,
        lhs: &'a TypedExpr,
        rhs: &'a TypedExpr,
    },
    /// The other operand is a Number, so this one's BigInt ToNumeric result
    /// is the mixed-type TypeError.
    OneCoerced {
        op: BitwiseBinaryOp,
        lhs: &'a TypedExpr,
        rhs: &'a TypedExpr,
        coerced: CoercedOperand,
    },
    /// `>>>` of two values that may each need ToNumeric: mixed operands are
    /// the mixed-type TypeError and two BigInts are `>>>`'s own TypeError.
    UnsignedShiftCoerced {
        lhs: &'a TypedExpr,
        rhs: &'a TypedExpr,
    },
    /// `~` of a Number.
    Complement(&'a TypedExpr),
}

/// A bitwise expression whose every normal completion is a Number in the
/// int32 (`>>>`: uint32) domain. Construction is the only admission.
#[derive(Clone, Copy)]
pub(crate) struct Int32Expression<'a> {
    form: Int32Form<'a>,
}

impl<'a> Int32Expression<'a> {
    pub(crate) fn of(expression: &'a TypedExpr) -> Option<Self> {
        match &expression.expr {
            ExprIr::BitwiseNumeric { op, lhs, rhs } => Self::binary(*op, lhs, rhs),
            ExprIr::UnaryBitwiseNumeric {
                op: UnaryBitwiseOp::Complement,
                expr,
            } if expr.kind == ValueKind::Number => Some(Self::of_complement(expr)),
            _ => None,
        }
    }

    /// `~operand` of a Number operand.
    pub(crate) fn of_complement(operand: &'a TypedExpr) -> Self {
        assert_eq!(
            operand.kind,
            ValueKind::Number,
            "only a Number complement is int32-domain"
        );
        Self {
            form: Int32Form::Complement(operand),
        }
    }

    pub(crate) fn binary(
        op: BitwiseBinaryOp,
        lhs: &'a TypedExpr,
        rhs: &'a TypedExpr,
    ) -> Option<Self> {
        let form = match (lhs.kind == ValueKind::Number, rhs.kind == ValueKind::Number) {
            (true, true) => Int32Form::Numbers { op, lhs, rhs },
            (true, false) => Int32Form::OneCoerced {
                op,
                lhs,
                rhs,
                coerced: CoercedOperand::Right,
            },
            (false, true) => Int32Form::OneCoerced {
                op,
                lhs,
                rhs,
                coerced: CoercedOperand::Left,
            },
            (false, false) if op == BitwiseBinaryOp::UShr => {
                Int32Form::UnsignedShiftCoerced { lhs, rhs }
            }
            (false, false) => return None,
        };
        Some(Self { form })
    }

    pub(crate) fn signedness(&self) -> Int32Signedness {
        match self.form {
            Int32Form::Numbers { op, .. } | Int32Form::OneCoerced { op, .. }
                if op == BitwiseBinaryOp::UShr =>
            {
                Int32Signedness::Unsigned
            }
            Int32Form::UnsignedShiftCoerced { .. } => Int32Signedness::Unsigned,
            Int32Form::Numbers { .. } | Int32Form::OneCoerced { .. } | Int32Form::Complement(_) => {
                Int32Signedness::Signed
            }
        }
    }
}

/// One side of an exact integer comparison: int32-domain bits, or a literal
/// integral Number whose magnitude is at most 2^53 (`-0` compares as `0`).
enum Int32Comparand<'a> {
    Bits(Int32Expression<'a>),
    Integer(i64),
}

impl<'a> Int32Comparand<'a> {
    fn of(expression: &'a TypedExpr) -> Option<Self> {
        if let Some(bits) = Int32Expression::of(expression) {
            return Some(Self::Bits(bits));
        }
        let ExprIr::Number(bits) = expression.expr else {
            return None;
        };
        let value = f64::from_bits(bits);
        (value.is_finite() && value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_992.0)
            .then(|| Self::Integer(value as i64))
    }

    /// Both sides are exact integers and at least one is int32-domain bits.
    fn pair(lhs: &'a TypedExpr, rhs: &'a TypedExpr) -> Option<(Self, Self)> {
        let left = Self::of(lhs)?;
        let right = Self::of(rhs)?;
        matches!((&left, &right), (Self::Bits(_), _) | (_, Self::Bits(_))).then_some((left, right))
    }
}

/// An evaluated operand's 32 bits.
enum Int32Bits {
    Constant(i32),
    Local(I32Local),
}

impl Int32Bits {
    fn load(&self, function: &mut Function) {
        match self {
            Self::Constant(value) => {
                function.instruction(&Instruction::I32Const(*value));
            }
            Self::Local(local) => local.load(function),
        }
    }

    fn release(self, schema: &RuntimeSchema, function: &mut Function) {
        if let Self::Local(local) = self {
            schema.release_i32_local(local, function);
        }
    }
}

fn emit_bitwise_instruction(op: BitwiseBinaryOp, function: &mut Function) {
    // Wasm shifts use the count modulo 32, exactly `shiftCount & 31`.
    function.instruction(&match op {
        BitwiseBinaryOp::And => Instruction::I32And,
        BitwiseBinaryOp::Or => Instruction::I32Or,
        BitwiseBinaryOp::Xor => Instruction::I32Xor,
        BitwiseBinaryOp::Shl => Instruction::I32Shl,
        BitwiseBinaryOp::Shr => Instruction::I32ShrS,
        BitwiseBinaryOp::UShr => Instruction::I32ShrU,
    });
}

impl FunctionBuilder<'_> {
    /// Evaluates the expression completely, with its ordered operand
    /// evaluation, ToNumeric conversions and abrupt completions, and leaves
    /// its result's 32 bits on the stack.
    pub(crate) fn compile_int32_bits(
        &mut self,
        value: &Int32Expression<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        match value.form {
            Int32Form::Complement(operand) => {
                let bits = self.compile_int32_number_operand(operand, function)?;
                bits.load(function);
                bits.release(schema, function);
                function.instruction(&Instruction::I32Const(-1));
                function.instruction(&Instruction::I32Xor);
            }
            Int32Form::Numbers { op, lhs, rhs } => {
                // ToUint32 of an evaluated Number is pure, so converting the
                // left value before evaluating the right one is unobservable.
                let left = self.compile_int32_number_operand(lhs, function)?;
                let right = self.compile_int32_number_operand(rhs, function)?;
                left.load(function);
                right.load(function);
                emit_bitwise_instruction(op, function);
                right.release(schema, function);
                left.release(schema, function);
            }
            Int32Form::OneCoerced {
                op,
                lhs,
                rhs,
                coerced,
            } => {
                let value = schema.reserve_value_local(function);
                let (left, right) = match coerced {
                    CoercedOperand::Left => {
                        self.compile_expr_to_value(lhs, &value, function)?;
                        let right = self.compile_int32_number_operand(rhs, function)?;
                        // ToNumeric(lval) follows the evaluation of the right
                        // operand; ToNumeric of the right Number is itself.
                        let left = self.emit_int32_coerced_operand(&value, function)?;
                        (left, right)
                    }
                    CoercedOperand::Right => {
                        let left = self.compile_int32_number_operand(lhs, function)?;
                        self.compile_expr_to_value(rhs, &value, function)?;
                        let right = self.emit_int32_coerced_operand(&value, function)?;
                        (left, right)
                    }
                };
                value.clear(function);
                left.load(function);
                right.load(function);
                emit_bitwise_instruction(op, function);
                right.release(schema, function);
                left.release(schema, function);
            }
            Int32Form::UnsignedShiftCoerced { lhs, rhs } => {
                let left = schema.reserve_value_local(function);
                let right = schema.reserve_value_local(function);
                // Both ToNumeric conversions, then the mixed-type check.
                self.compile_numeric_operand_pair(lhs, rhs, &left, &right, function)?;
                self.emit_int32_bigint_type_error(
                    left.tag(),
                    RuntimeErrorMessage::BIGINTS_DO_NOT_SUPPORT_UNSIGNED_RIGHT_SHIFT,
                    function,
                )?;
                emit_number_bitwise_shift_bits(schema, left.scalar(), right.scalar(), function);
                right.clear(function);
                left.clear(function);
            }
        }
        Ok(())
    }

    /// The exact Number payload of the expression.
    pub(crate) fn compile_int32_number_payload(
        &mut self,
        value: &Int32Expression<'_>,
        output: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_int32_bits(value, function)?;
        value.signedness().emit_to_f64(function);
        function.instruction(&Instruction::I64ReinterpretF64);
        output.store(function);
        Ok(())
    }

    /// ToBoolean of an int32-domain Number is false exactly for +0.
    pub(crate) fn try_compile_int32_truthy_i32(
        &mut self,
        expression: &TypedExpr,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(value) = Int32Expression::of(expression) else {
            return Ok(false);
        };
        self.compile_int32_bits(&value, function)?;
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32Ne);
        Ok(true)
    }

    /// An ordered relation between exact integers is the i64 relation of
    /// their values; no NaN can make it undefined.
    pub(crate) fn try_compile_int32_relation_i32(
        &mut self,
        op: RelationalBinaryOp,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(pair) = Int32Comparand::pair(lhs, rhs) else {
            return Ok(false);
        };
        self.compile_int32_comparison(pair, function)?;
        function.instruction(&match op {
            RelationalBinaryOp::LessThan => Instruction::I64LtS,
            RelationalBinaryOp::LessThanOrEqual => Instruction::I64LeS,
            RelationalBinaryOp::GreaterThan => Instruction::I64GtS,
            RelationalBinaryOp::GreaterThanOrEqual => Instruction::I64GeS,
        });
        Ok(true)
    }

    /// Strict and loose equality of two Numbers are both Number::equal; for
    /// exact integers that is equality of their values.
    pub(crate) fn try_compile_int32_equality_i32(
        &mut self,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(pair) = Int32Comparand::pair(lhs, rhs) else {
            return Ok(false);
        };
        self.compile_int32_comparison(pair, function)?;
        function.instruction(&Instruction::I64Eq);
        Ok(true)
    }

    /// Leaves both comparands' exact i64 values on the stack, left first.
    /// The left operand completes before the right one is evaluated.
    fn compile_int32_comparison(
        &mut self,
        (left, right): (Int32Comparand<'_>, Int32Comparand<'_>),
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let left = self.compile_int32_comparand(left, function)?;
        let right = self.compile_int32_comparand(right, function)?;
        for side in [&left, &right] {
            match side {
                Ok(local) => local.load(function),
                Err(value) => {
                    function.instruction(&Instruction::I64Const(*value));
                }
            }
        }
        for side in [right, left] {
            if let Ok(local) = side {
                schema.release_i64_local(local, function);
            }
        }
        Ok(())
    }

    /// An evaluated comparand's exact value: a local, or a literal.
    fn compile_int32_comparand(
        &mut self,
        comparand: Int32Comparand<'_>,
        function: &mut Function,
    ) -> Result<Result<I64Local, i64>, EmitError> {
        match comparand {
            Int32Comparand::Integer(value) => Ok(Err(value)),
            Int32Comparand::Bits(bits) => {
                let local = self.runtime_schema().reserve_i64_local(function);
                self.compile_int32_bits(&bits, function)?;
                bits.signedness().emit_to_i64(function);
                local.store(function);
                Ok(Ok(local))
            }
        }
    }

    /// A Number operand's ToUint32 bits, evaluated completely.
    fn compile_int32_number_operand(
        &mut self,
        operand: &TypedExpr,
        function: &mut Function,
    ) -> Result<Int32Bits, EmitError> {
        debug_assert_eq!(operand.kind, ValueKind::Number);
        // A literal is its own ToNumeric and evaluates nothing.
        if let Some(bits) = BitwiseOperand::literal(operand) {
            return Ok(Int32Bits::Constant(bits as i32));
        }
        let schema = self.runtime_schema();
        let local = schema.reserve_i32_local(function);
        if let Some(nested) = Int32Expression::of(operand) {
            // ToInt32/ToUint32 of an int32-domain Number are its own bits.
            self.compile_int32_bits(&nested, function)?;
        } else {
            let payload = schema.reserve_i64_local(function);
            self.compile_number_payload(operand, payload, function)?;
            BitwiseOperand::Payload(payload).emit_uint32(schema, function);
            schema.release_i64_local(payload, function);
        }
        local.store(function);
        Ok(Int32Bits::Local(local))
    }

    /// ToNumeric of an evaluated operand whose partner is a Number, then
    /// its ToUint32 bits. A BigInt result is the mixed-type TypeError.
    fn emit_int32_coerced_operand(
        &mut self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<Int32Bits, EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        self.emit_value_to_numeric_locals(value, &pending, function)?;
        self.finish_to_primitive_operation(
            ToPrimitiveAbruptRoute::ActiveHandler,
            &pending,
            function,
        )?;
        value.copy_from(pending.value(), function);
        pending.clear(function);
        self.emit_int32_bigint_type_error(
            value.tag(),
            RuntimeErrorMessage::CANNOT_MIX_BIGINT_AND_OTHER_TYPES,
            function,
        )?;
        let local = schema.reserve_i32_local(function);
        BitwiseOperand::Payload(value.scalar()).emit_uint32(schema, function);
        local.store(function);
        Ok(Int32Bits::Local(local))
    }

    fn emit_int32_bigint_type_error(
        &mut self,
        tag: I32Local,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_is_bigint_tag_i32(tag, function);
        self.open_frame(ControlFrameKind::If, function);
        let error = self.runtime_schema().reserve_completion(function);
        self.emit_throw_runtime_error(NativeErrorKind::TypeError, message, &error, function)?;
        self.completion().copy_from(&error, function);
        error.clear(function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }
}

/// `>>>` of two Number payloads, leaving the result bits.
fn emit_number_bitwise_shift_bits(
    schema: &RuntimeSchema,
    left: I64Local,
    right: I64Local,
    function: &mut Function,
) {
    BitwiseOperand::Payload(left).emit_uint32(schema, function);
    BitwiseOperand::Payload(right).emit_uint32(schema, function);
    emit_bitwise_instruction(BitwiseBinaryOp::UShr, function);
}
