//! Exact BigInt arithmetic on rooted GC values and private base-2^32 digits.
//!
//! Published values have one canonical sign/magnitude representation. Working
//! digits are private construction owners; no semantic reference is an integer
//! or a linear-memory address. The registered helper returns a whole completion.

use super::*;
use crate::gc_types::{
    BigIntConstruction, BigIntLimbArray, BigIntValue, BigIntValueSchema, CompletionLocals,
    I64Local, ValueLocals,
};
use crate::runtime_helpers::{
    BigIntArithmeticArguments, BigIntArithmeticParameters, HelperParameters,
};

mod helper_op;
pub(crate) use helper_op::BigIntHelperOp;

pub(crate) const BIGINT_DIVISION_BY_ZERO_MESSAGE: RuntimeErrorMessage =
    RuntimeErrorMessage::BIGINT_DIVISION_BY_ZERO;
pub(crate) const BIGINT_EXPONENT_MESSAGE: RuntimeErrorMessage =
    RuntimeErrorMessage::BIGINT_EXPONENT_MUST_BE_NON_NEGATIVE;
pub(crate) const BIGINT_SHIFT_RESOURCE_MESSAGE: RuntimeErrorMessage =
    RuntimeErrorMessage::BIGINT_SHIFT_RESULT_EXCEEDS_THE_ENGINE_RESOURCE_LIMIT;
const DIGIT_MASK: i64 = 0xffff_ffff;
// Array indices are i32. Length checks occur before a narrowing conversion.
const MAX_BIGINT_DIGITS: i64 = i32::MAX as i64 - 1;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_number_to_bigint_locals(
        &mut self,
        bits: I64Local,
        output: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        output.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        bits.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        bits.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Ne);
        bits.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Abs);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::CANNOT_CONVERT_NUMBER_TO_BIGINT,
            output,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        bits.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        bits.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::F64Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::CANNOT_CONVERT_NON_INTEGER_NUMBER_TO_BIGINT,
            output,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let sign = schema.reserve_i64_local(function);
        let length = schema.reserve_i64_local(function);
        let fraction = schema.reserve_i64_local(function);
        let class = schema.reserve_i64_local(function);
        let digits = self.emit_bigint_empty_digits(function);
        self.emit_bigint_decode_number_operand(
            bits, sign, &digits, length, fraction, class, exit, function,
        )?;
        self.emit_bigint_pack_result(sign, &digits, length, output, function)?;
        digits.clear(function);
        schema.release_i64_local(class, function);
        schema.release_i64_local(fraction, function);
        schema.release_i64_local(length, function);
        schema.release_i64_local(sign, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_bigint_arithmetic_to_locals(
        &mut self,
        operation: BigIntHelperOp,
        left: &TypedExpr,
        right: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let lhs = schema.reserve_value_local(function);
        let rhs = schema.reserve_value_local(function);
        self.compile_expr_to_value(left, &lhs, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.compile_expr_to_value(right, &rhs, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        let pending = schema.reserve_completion(function);
        self.emit_bigint_binary_op_to_locals(operation, &lhs, &rhs, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        output.copy_from(pending.value(), function);
        pending.clear(function);
        rhs.clear(function);
        lhs.clear(function);
        Ok(())
    }

    pub(crate) fn emit_bigint_binary_op_to_locals(
        &mut self,
        operation: BigIntHelperOp,
        left: &ValueLocals,
        right: &ValueLocals,
        output: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let selector = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(operation.runtime_code() as i32));
        selector.store(function);
        schema
            .call_helper(
                BigIntArithmeticArguments::new(left, right, selector),
                self.runtime_helper_base()?,
                function,
            )
            .store(output, function);
        schema.release_i32_local(selector, function);
        Ok(())
    }

    fn emit_bigint_error(
        &mut self,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::RangeError,
            message,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        Ok(())
    }

    fn emit_bigint_empty_digits(&self, function: &mut Function) -> BigIntConstruction {
        let schema = self.runtime_schema();
        let length = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        length.store(function);
        let digits = BigIntConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        schema.release_i32_local(length, function);
        digits
    }

    fn emit_bigint_digits_alloc(
        &mut self,
        count: I64Local,
        digits: &BigIntConstruction,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        count.load(function);
        function.instruction(&Instruction::I64Const(MAX_BIGINT_DIGITS));
        function.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_bigint_error(BIGINT_SHIFT_RESOURCE_MESSAGE, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let length = schema.reserve_i32_local(function);
        count.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        length.store(function);
        digits.reallocate(length, schema, function);
        schema.release_i32_local(length, function);
        Ok(())
    }

    fn emit_bigint_digit_load(
        &self,
        digits: &BigIntConstruction,
        index: I64Local,
        output: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let checked = schema.reserve_i32_local(function);
        index.load(function);
        function.instruction(&Instruction::I32WrapI64);
        checked.store(function);
        digits.read(checked, output, schema, function);
        schema.release_i32_local(checked, function);
    }

    fn emit_bigint_digit_load_or_zero(
        &self,
        digits: &BigIntConstruction,
        length: I64Local,
        index: I64Local,
        output: I64Local,
        function: &mut Function,
    ) {
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_bigint_digit_load(digits, index, output, function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        output.store(function);
        function.instruction(&Instruction::End);
    }

    fn emit_bigint_digit_store(
        &self,
        digits: &BigIntConstruction,
        index: I64Local,
        value: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let checked = schema.reserve_i32_local(function);
        index.load(function);
        function.instruction(&Instruction::I32WrapI64);
        checked.store(function);
        digits.write(checked, value, schema, function);
        schema.release_i32_local(checked, function);
    }

    fn emit_bigint_decode_operand(
        &mut self,
        input: &ValueLocals,
        sign: I64Local,
        digits: &BigIntConstruction,
        length: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let bigint = schema.reserve_gc_local(function).initialize(
            input.cast_reference::<BigIntValue>(schema, function),
            function,
        );
        let limbs = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<BigIntValue>()
                .field(BigIntValueSchema::LIMBS)
                .read(&bigint, schema, function)
                .reference(),
            function,
        );
        let negative = schema.reserve_i32_local(function);
        schema
            .struct_type::<BigIntValue>()
            .field(BigIntValueSchema::NEGATIVE)
            .read(&bigint, schema, function)
            .store(negative, function);
        negative.load(function);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::End);
        sign.store(function);
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let digit_index = schema.reserve_i32_local(function);
        let limb = schema.reserve_i64_local(function);
        let digit = schema.reserve_i64_local(function);
        schema
            .array_type::<BigIntLimbArray>()
            .length(&limbs, schema, function);
        count.store(function);
        count.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Mul);
        length.store(function);
        self.emit_bigint_digits_alloc(length, digits, exit, function)?;
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        schema
            .array_type::<BigIntLimbArray>()
            .read(&limbs, index, schema, function)
            .store_i64(limb, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(2));
        function.instruction(&Instruction::I32Mul);
        digit_index.store(function);
        limb.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        digit.store(function);
        digits.write(digit_index, digit, schema, function);
        digit_index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        digit_index.store(function);
        limb.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        digit.store(function);
        digits.write(digit_index, digit, schema, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_bigint_normalize_len(digits, length, function);
        length.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        sign.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(digit, function);
        schema.release_i64_local(limb, function);
        schema.release_i32_local(digit_index, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        schema.release_i32_local(negative, function);
        limbs.clear(function);
        bigint.clear(function);
        Ok(())
    }

    fn emit_bigint_pack_result(
        &mut self,
        sign: I64Local,
        digits: &BigIntConstruction,
        length: I64Local,
        output: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_bigint_normalize_len(digits, length, function);
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let digit_index = schema.reserve_i64_local(function);
        let limb = schema.reserve_i64_local(function);
        let high = schema.reserve_i64_local(function);
        let negative = schema.reserve_i32_local(function);
        length.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I32WrapI64);
        count.store(function);
        let construction = BigIntConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            count,
            function,
        );
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Mul);
        digit_index.store(function);
        self.emit_bigint_digit_load_or_zero(digits, length, digit_index, limb, function);
        digit_index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        digit_index.store(function);
        self.emit_bigint_digit_load_or_zero(digits, length, digit_index, high, function);
        high.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Shl);
        limb.load(function);
        function.instruction(&Instruction::I64Or);
        limb.store(function);
        construction.write(index, limb, schema, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        sign.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        negative.store(function);
        let bigint = schema
            .reserve_gc_local(function)
            .initialize(construction.publish(negative, schema, function), function);
        let value = schema.reserve_value_local(function);
        value.set_reference(&bigint, schema, function);
        output.set_normal(&value, function);
        value.clear(function);
        bigint.clear(function);
        schema.release_i32_local(negative, function);
        schema.release_i64_local(high, function);
        schema.release_i64_local(limb, function);
        schema.release_i64_local(digit_index, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        Ok(())
    }

    fn emit_bigint_normalize_len(
        &mut self,
        ptr_local: &BigIntConstruction,
        len_local: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let digit = schema.reserve_i64_local(function);
        let index = schema.reserve_i64_local(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        len_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::BrIf(1));
        len_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        index.store(function);
        self.emit_bigint_digit_load(&ptr_local, index, digit, function);
        digit.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        len_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i64_local(index, function);
        schema.release_i64_local(digit, function);
    }

    fn emit_bigint_decode_number_operand(
        &mut self,
        payload_param: I64Local,
        sign_local: I64Local,
        ptr_local: &BigIntConstruction,
        len_local: I64Local,
        fraction_sign_local: I64Local,
        class_local: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let bits = schema.reserve_i64_local(function);
        let exponent = schema.reserve_i64_local(function);
        let mantissa = schema.reserve_i64_local(function);
        let scale = schema.reserve_i64_local(function);
        let words = schema.reserve_i64_local(function);
        let shift = schema.reserve_i64_local(function);
        let low = schema.reserve_i64_local(function);
        let high = schema.reserve_i64_local(function);
        let digit = schema.reserve_i64_local(function);
        let index = schema.reserve_i64_local(function);

        payload_param.load(function);
        bits.store(function);
        function.instruction(&Instruction::I64Const(0));
        class_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        fraction_sign_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        len_local.store(function);
        self.emit_bigint_digits_alloc(len_local, &ptr_local, exit, function)?;

        // sign := -1 for a set sign bit, else 1 (corrected to 0 for a zero).
        bits.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::End);
        sign_local.store(function);

        bits.load(function);
        function.instruction(&Instruction::I64Const(52));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(0x7ff));
        function.instruction(&Instruction::I64And);
        exponent.store(function);
        bits.load(function);
        function.instruction(&Instruction::I64Const(0x000f_ffff_ffff_ffff));
        function.instruction(&Instruction::I64And);
        mantissa.store(function);

        exponent.load(function);
        function.instruction(&Instruction::I64Const(0x7ff));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        mantissa.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        sign_local.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::End);
        class_local.store(function);
        function.instruction(&Instruction::Else);

        exponent.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        // Zero or subnormal: |value| < 1, so the integer part is zero and only
        // the fractional sign survives.
        mantissa.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::Else);
        sign_local.load(function);
        function.instruction(&Instruction::End);
        fraction_sign_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        sign_local.store(function);
        function.instruction(&Instruction::Else);

        // value == mantissa * 2^(exponent - 1075) with the implicit leading bit.
        mantissa.load(function);
        function.instruction(&Instruction::I64Const(1_i64 << 52));
        function.instruction(&Instruction::I64Or);
        mantissa.store(function);
        exponent.load(function);
        function.instruction(&Instruction::I64Const(1075));
        function.instruction(&Instruction::I64Sub);
        scale.store(function);

        scale.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::If(BlockType::Empty));
        // Exact integer: shift the 53-bit mantissa left by `scale` bits.
        scale.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64DivU);
        words.store(function);
        scale.load(function);
        function.instruction(&Instruction::I64Const(31));
        function.instruction(&Instruction::I64And);
        shift.store(function);
        words.load(function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Add);
        len_local.store(function);
        self.emit_bigint_digits_alloc(len_local, &ptr_local, exit, function)?;
        mantissa.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        low.store(function);
        mantissa.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        high.store(function);
        // digit[words] = low << shift
        low.load(function);
        shift.load(function);
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        digit.store(function);
        words.load(function);
        index.store(function);
        self.emit_bigint_digit_store(&ptr_local, index, digit, function);
        // digit[words + 1] = (low >> (32 - shift)) | (high << shift)
        low.load(function);
        function.instruction(&Instruction::I64Const(32));
        shift.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64ShrU);
        high.load(function);
        shift.load(function);
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Or);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        digit.store(function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        self.emit_bigint_digit_store(&ptr_local, index, digit, function);
        // digit[words + 2] = high >> (32 - shift)
        high.load(function);
        function.instruction(&Instruction::I64Const(32));
        shift.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        digit.store(function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        self.emit_bigint_digit_store(&ptr_local, index, digit, function);
        function.instruction(&Instruction::Else);
        // Fractional: the integer part is `mantissa >> -scale` and the shifted
        // out bits decide the fractional sign.
        function.instruction(&Instruction::I64Const(0));
        scale.load(function);
        function.instruction(&Instruction::I64Sub);
        scale.store(function);
        scale.load(function);
        function.instruction(&Instruction::I64Const(64));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        sign_local.load(function);
        fraction_sign_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        mantissa.store(function);
        function.instruction(&Instruction::Else);
        mantissa.load(function);
        function.instruction(&Instruction::I64Const(1));
        scale.load(function);
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::Else);
        sign_local.load(function);
        function.instruction(&Instruction::End);
        fraction_sign_local.store(function);
        mantissa.load(function);
        scale.load(function);
        function.instruction(&Instruction::I64ShrU);
        mantissa.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(2));
        len_local.store(function);
        self.emit_bigint_digits_alloc(len_local, &ptr_local, exit, function)?;
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        mantissa.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        digit.store(function);
        self.emit_bigint_digit_store(&ptr_local, index, digit, function);
        function.instruction(&Instruction::I64Const(1));
        index.store(function);
        mantissa.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        digit.store(function);
        self.emit_bigint_digit_store(&ptr_local, index, digit, function);
        function.instruction(&Instruction::End);

        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        self.emit_bigint_normalize_len(&ptr_local, len_local, function);
        len_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        sign_local.store(function);
        function.instruction(&Instruction::End);

        schema.release_i64_local(index, function);
        schema.release_i64_local(digit, function);
        schema.release_i64_local(high, function);
        schema.release_i64_local(low, function);
        schema.release_i64_local(shift, function);
        schema.release_i64_local(words, function);
        schema.release_i64_local(scale, function);
        schema.release_i64_local(mantissa, function);
        schema.release_i64_local(exponent, function);
        schema.release_i64_local(bits, function);
        Ok(())
    }

    fn emit_bigint_magnitude_compare(
        &mut self,
        a_ptr: &BigIntConstruction,
        a_len: I64Local,
        b_ptr: &BigIntConstruction,
        b_len: I64Local,
        out_local: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let index = schema.reserve_i64_local(function);
        let a_digit = schema.reserve_i64_local(function);
        let b_digit = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(0));
        out_local.store(function);
        a_len.load(function);
        b_len.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        a_len.load(function);
        function.instruction(&Instruction::Else);
        b_len.load(function);
        function.instruction(&Instruction::End);
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        index.store(function);
        self.emit_bigint_digit_load_or_zero(&a_ptr, a_len, index, a_digit, function);
        self.emit_bigint_digit_load_or_zero(&b_ptr, b_len, index, b_digit, function);
        a_digit.load(function);
        b_digit.load(function);
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        a_digit.load(function);
        b_digit.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::End);
        out_local.store(function);
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i64_local(b_digit, function);
        schema.release_i64_local(a_digit, function);
        schema.release_i64_local(index, function);
    }

    fn emit_bigint_signed_compare(
        &mut self,
        lhs_sign: I64Local,
        lhs_ptr: &BigIntConstruction,
        lhs_len: I64Local,
        rhs_sign: I64Local,
        rhs_ptr: &BigIntConstruction,
        rhs_len: I64Local,
        out_local: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let magnitude = schema.reserve_i64_local(function);
        lhs_sign.load(function);
        rhs_sign.load(function);
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        lhs_sign.load(function);
        rhs_sign.load(function);
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::End);
        out_local.store(function);
        function.instruction(&Instruction::Else);
        self.emit_bigint_magnitude_compare(
            &lhs_ptr, lhs_len, &rhs_ptr, rhs_len, magnitude, function,
        );
        magnitude.load(function);
        lhs_sign.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Mul);
        out_local.store(function);
        function.instruction(&Instruction::End);
        schema.release_i64_local(magnitude, function);
    }

    fn emit_bigint_magnitude_add(
        &mut self,
        a_ptr: &BigIntConstruction,
        a_len: I64Local,
        b_ptr: &BigIntConstruction,
        b_len: I64Local,
        out_ptr: &BigIntConstruction,
        out_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let index = schema.reserve_i64_local(function);
        let carry = schema.reserve_i64_local(function);
        let a_digit = schema.reserve_i64_local(function);
        let b_digit = schema.reserve_i64_local(function);
        let sum = schema.reserve_i64_local(function);

        a_len.load(function);
        b_len.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        a_len.load(function);
        function.instruction(&Instruction::Else);
        b_len.load(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        out_len.store(function);
        self.emit_bigint_digits_alloc(out_len, &out_ptr, exit, function)?;

        function.instruction(&Instruction::I64Const(0));
        carry.store(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        out_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_digit_load_or_zero(&a_ptr, a_len, index, a_digit, function);
        self.emit_bigint_digit_load_or_zero(&b_ptr, b_len, index, b_digit, function);
        a_digit.load(function);
        b_digit.load(function);
        function.instruction(&Instruction::I64Add);
        carry.load(function);
        function.instruction(&Instruction::I64Add);
        sum.store(function);
        sum.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        carry.store(function);
        sum.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        sum.store(function);
        self.emit_bigint_digit_store(&out_ptr, index, sum, function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_bigint_normalize_len(&out_ptr, out_len, function);

        schema.release_i64_local(sum, function);
        schema.release_i64_local(b_digit, function);
        schema.release_i64_local(a_digit, function);
        schema.release_i64_local(carry, function);
        schema.release_i64_local(index, function);
        Ok(())
    }

    fn emit_bigint_magnitude_sub(
        &mut self,
        a_ptr: &BigIntConstruction,
        a_len: I64Local,
        b_ptr: &BigIntConstruction,
        b_len: I64Local,
        out_ptr: &BigIntConstruction,
        out_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let index = schema.reserve_i64_local(function);
        let borrow = schema.reserve_i64_local(function);
        let a_digit = schema.reserve_i64_local(function);
        let b_digit = schema.reserve_i64_local(function);
        let diff = schema.reserve_i64_local(function);

        a_len.load(function);
        out_len.store(function);
        self.emit_bigint_digits_alloc(out_len, &out_ptr, exit, function)?;

        function.instruction(&Instruction::I64Const(0));
        borrow.store(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        out_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_digit_load_or_zero(&a_ptr, a_len, index, a_digit, function);
        self.emit_bigint_digit_load_or_zero(&b_ptr, b_len, index, b_digit, function);
        a_digit.load(function);
        b_digit.load(function);
        function.instruction(&Instruction::I64Sub);
        borrow.load(function);
        function.instruction(&Instruction::I64Sub);
        diff.store(function);
        diff.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        diff.load(function);
        function.instruction(&Instruction::I64Const(1 << 32));
        function.instruction(&Instruction::I64Add);
        diff.store(function);
        function.instruction(&Instruction::I64Const(1));
        borrow.store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        borrow.store(function);
        function.instruction(&Instruction::End);
        self.emit_bigint_digit_store(&out_ptr, index, diff, function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_bigint_normalize_len(&out_ptr, out_len, function);

        schema.release_i64_local(diff, function);
        schema.release_i64_local(b_digit, function);
        schema.release_i64_local(a_digit, function);
        schema.release_i64_local(borrow, function);
        schema.release_i64_local(index, function);
        Ok(())
    }

    fn emit_bigint_signed_add(
        &mut self,
        lhs_sign: I64Local,
        lhs_ptr: &BigIntConstruction,
        lhs_len: I64Local,
        rhs_sign: I64Local,
        rhs_ptr: &BigIntConstruction,
        rhs_len: I64Local,
        res_sign: I64Local,
        res_ptr: &BigIntConstruction,
        res_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let comparison = schema.reserve_i64_local(function);
        lhs_sign.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        rhs_sign.load(function);
        res_sign.store(function);
        res_ptr.copy_from(&rhs_ptr, schema, function);
        rhs_len.load(function);
        res_len.store(function);
        function.instruction(&Instruction::Else);
        rhs_sign.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        lhs_sign.load(function);
        res_sign.store(function);
        res_ptr.copy_from(&lhs_ptr, schema, function);
        lhs_len.load(function);
        res_len.store(function);
        function.instruction(&Instruction::Else);
        lhs_sign.load(function);
        rhs_sign.load(function);
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_bigint_magnitude_add(
            &lhs_ptr, lhs_len, &rhs_ptr, rhs_len, &res_ptr, res_len, exit, function,
        )?;
        lhs_sign.load(function);
        res_sign.store(function);
        function.instruction(&Instruction::Else);
        self.emit_bigint_magnitude_compare(
            &lhs_ptr, lhs_len, &rhs_ptr, rhs_len, comparison, function,
        );
        comparison.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        res_sign.store(function);
        function.instruction(&Instruction::I64Const(0));
        res_len.store(function);
        res_ptr.copy_from(&lhs_ptr, schema, function);
        function.instruction(&Instruction::Else);
        comparison.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_bigint_magnitude_sub(
            &lhs_ptr, lhs_len, &rhs_ptr, rhs_len, &res_ptr, res_len, exit, function,
        )?;
        lhs_sign.load(function);
        res_sign.store(function);
        function.instruction(&Instruction::Else);
        self.emit_bigint_magnitude_sub(
            &rhs_ptr, rhs_len, &lhs_ptr, lhs_len, &res_ptr, res_len, exit, function,
        )?;
        rhs_sign.load(function);
        res_sign.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i64_local(comparison, function);
        Ok(())
    }

    fn emit_bigint_bitwise(
        &mut self,
        op_local: I64Local,
        lhs_sign: I64Local,
        lhs_ptr: &BigIntConstruction,
        lhs_len: I64Local,
        rhs_sign: I64Local,
        rhs_ptr: &BigIntConstruction,
        rhs_len: I64Local,
        res_sign: I64Local,
        res_ptr: &BigIntConstruction,
        res_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let index = schema.reserve_i64_local(function);
        let lhs_digit = schema.reserve_i64_local(function);
        let rhs_digit = schema.reserve_i64_local(function);
        let lhs_carry = schema.reserve_i64_local(function);
        let rhs_carry = schema.reserve_i64_local(function);
        let digit = schema.reserve_i64_local(function);
        let carry = schema.reserve_i64_local(function);

        lhs_len.load(function);
        rhs_len.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        lhs_len.load(function);
        function.instruction(&Instruction::Else);
        rhs_len.load(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        res_len.store(function);
        self.emit_bigint_digits_alloc(res_len, &res_ptr, exit, function)?;

        lhs_sign.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::End);
        lhs_carry.store(function);
        rhs_sign.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::End);
        rhs_carry.store(function);

        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        res_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));

        self.emit_bigint_digit_load_or_zero(&lhs_ptr, lhs_len, index, lhs_digit, function);
        lhs_sign.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        lhs_digit.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64Xor);
        lhs_carry.load(function);
        function.instruction(&Instruction::I64Add);
        lhs_digit.store(function);
        lhs_digit.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        lhs_carry.store(function);
        lhs_digit.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        lhs_digit.store(function);
        function.instruction(&Instruction::End);

        self.emit_bigint_digit_load_or_zero(&rhs_ptr, rhs_len, index, rhs_digit, function);
        rhs_sign.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        rhs_digit.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64Xor);
        rhs_carry.load(function);
        function.instruction(&Instruction::I64Add);
        rhs_digit.store(function);
        rhs_digit.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        rhs_carry.store(function);
        rhs_digit.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        rhs_digit.store(function);
        function.instruction(&Instruction::End);

        op_local.load(function);
        function.instruction(&Instruction::I64Const(i64::from(
            BigIntHelperOp::BitAnd.runtime_code(),
        )));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        lhs_digit.load(function);
        rhs_digit.load(function);
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::Else);
        op_local.load(function);
        function.instruction(&Instruction::I64Const(i64::from(
            BigIntHelperOp::BitOr.runtime_code(),
        )));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        lhs_digit.load(function);
        rhs_digit.load(function);
        function.instruction(&Instruction::I64Or);
        function.instruction(&Instruction::Else);
        lhs_digit.load(function);
        rhs_digit.load(function);
        function.instruction(&Instruction::I64Xor);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        digit.store(function);
        self.emit_bigint_digit_store(&res_ptr, index, digit, function);

        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        res_len.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        index.store(function);
        self.emit_bigint_digit_load(&res_ptr, index, digit, function);
        digit.load(function);
        function.instruction(&Instruction::I64Const(0x8000_0000));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        res_sign.store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(-1));
        res_sign.store(function);
        function.instruction(&Instruction::I64Const(1));
        carry.store(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        res_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_digit_load(&res_ptr, index, digit, function);
        digit.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64Xor);
        carry.load(function);
        function.instruction(&Instruction::I64Add);
        digit.store(function);
        digit.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        carry.store(function);
        digit.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        digit.store(function);
        self.emit_bigint_digit_store(&res_ptr, index, digit, function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        self.emit_bigint_normalize_len(&res_ptr, res_len, function);
        res_len.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        res_sign.store(function);
        function.instruction(&Instruction::End);

        schema.release_i64_local(carry, function);
        schema.release_i64_local(digit, function);
        schema.release_i64_local(rhs_carry, function);
        schema.release_i64_local(lhs_carry, function);
        schema.release_i64_local(rhs_digit, function);
        schema.release_i64_local(lhs_digit, function);
        schema.release_i64_local(index, function);
        Ok(())
    }

    fn emit_bigint_shift(
        &mut self,
        op_local: I64Local,
        lhs_sign: I64Local,
        lhs_ptr: &BigIntConstruction,
        lhs_len: I64Local,
        rhs_sign: I64Local,
        rhs_ptr: &BigIntConstruction,
        rhs_len: I64Local,
        res_sign: I64Local,
        res_ptr: &BigIntConstruction,
        res_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        op_local.load(function);
        function.instruction(&Instruction::I64Const(i64::from(
            BigIntHelperOp::Shl.runtime_code(),
        )));
        function.instruction(&Instruction::I64Eq);
        rhs_sign.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::I32Xor);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_bigint_shift_left(
            lhs_sign, &lhs_ptr, lhs_len, &rhs_ptr, rhs_len, res_sign, &res_ptr, res_len, exit,
            function,
        )?;
        function.instruction(&Instruction::Else);
        self.emit_bigint_shift_right(
            lhs_sign, &lhs_ptr, lhs_len, &rhs_ptr, rhs_len, res_sign, &res_ptr, res_len, exit,
            function,
        )?;
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_bigint_shift_left(
        &mut self,
        lhs_sign: I64Local,
        lhs_ptr: &BigIntConstruction,
        lhs_len: I64Local,
        rhs_ptr: &BigIntConstruction,
        rhs_len: I64Local,
        res_sign: I64Local,
        res_ptr: &BigIntConstruction,
        res_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let count = schema.reserve_i64_local(function);
        let word_shift = schema.reserve_i64_local(function);
        let bit_shift = schema.reserve_i64_local(function);
        let index = schema.reserve_i64_local(function);
        let dest_index = schema.reserve_i64_local(function);
        let digit = schema.reserve_i64_local(function);
        let high = schema.reserve_i64_local(function);
        let carry = schema.reserve_i64_local(function);

        // Zero stays zero even for a count too large to materialise; no
        // allocation is required, so resource exhaustion is not observable.
        lhs_len.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        lhs_sign.load(function);
        res_sign.store(function);
        res_ptr.copy_from(&lhs_ptr, schema, function);
        function.instruction(&Instruction::I64Const(0));
        res_len.store(function);
        function.instruction(&Instruction::Else);

        // More than 64 count bits necessarily exceeds a GC array's index
        // domain for a non-zero left operand.
        rhs_len.load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_bigint_error(BIGINT_SHIFT_RESOURCE_MESSAGE, function)?;
        function.instruction(&Instruction::Else);

        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        self.emit_bigint_digit_load_or_zero(&rhs_ptr, rhs_len, index, count, function);
        function.instruction(&Instruction::I64Const(1));
        index.store(function);
        self.emit_bigint_digit_load_or_zero(&rhs_ptr, rhs_len, index, high, function);
        high.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Shl);
        count.load(function);
        function.instruction(&Instruction::I64Or);
        count.store(function);
        count.load(function);
        function.instruction(&Instruction::I64Const(5));
        function.instruction(&Instruction::I64ShrU);
        word_shift.store(function);
        count.load(function);
        function.instruction(&Instruction::I64Const(31));
        function.instruction(&Instruction::I64And);
        bit_shift.store(function);

        word_shift.load(function);
        function.instruction(&Instruction::I64Const(MAX_BIGINT_DIGITS));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_bigint_error(BIGINT_SHIFT_RESOURCE_MESSAGE, function)?;
        function.instruction(&Instruction::Else);

        lhs_len.load(function);
        word_shift.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        res_len.store(function);
        res_len.load(function);
        function.instruction(&Instruction::I64Const(MAX_BIGINT_DIGITS));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_bigint_error(BIGINT_SHIFT_RESOURCE_MESSAGE, function)?;
        function.instruction(&Instruction::Else);

        self.emit_bigint_digits_alloc(res_len, &res_ptr, exit, function)?;
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::I64Const(0));
        carry.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        lhs_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_digit_load(&lhs_ptr, index, digit, function);
        digit.load(function);
        bit_shift.load(function);
        function.instruction(&Instruction::I64Shl);
        high.store(function);
        high.load(function);
        carry.load(function);
        function.instruction(&Instruction::I64Or);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        digit.store(function);
        index.load(function);
        word_shift.load(function);
        function.instruction(&Instruction::I64Add);
        dest_index.store(function);
        self.emit_bigint_digit_store(&res_ptr, dest_index, digit, function);
        high.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        carry.store(function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        lhs_len.load(function);
        word_shift.load(function);
        function.instruction(&Instruction::I64Add);
        dest_index.store(function);
        self.emit_bigint_digit_store(&res_ptr, dest_index, carry, function);
        lhs_sign.load(function);
        res_sign.store(function);
        self.emit_bigint_normalize_len(&res_ptr, res_len, function);

        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        schema.release_i64_local(carry, function);
        schema.release_i64_local(high, function);
        schema.release_i64_local(digit, function);
        schema.release_i64_local(dest_index, function);
        schema.release_i64_local(index, function);
        schema.release_i64_local(bit_shift, function);
        schema.release_i64_local(word_shift, function);
        schema.release_i64_local(count, function);
        Ok(())
    }

    fn emit_bigint_shift_right(
        &mut self,
        lhs_sign: I64Local,
        lhs_ptr: &BigIntConstruction,
        lhs_len: I64Local,
        rhs_ptr: &BigIntConstruction,
        rhs_len: I64Local,
        res_sign: I64Local,
        res_ptr: &BigIntConstruction,
        res_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let count = schema.reserve_i64_local(function);
        let word_shift = schema.reserve_i64_local(function);
        let bit_shift = schema.reserve_i64_local(function);
        let base_len = schema.reserve_i64_local(function);
        let index = schema.reserve_i64_local(function);
        let source_index = schema.reserve_i64_local(function);
        let digit = schema.reserve_i64_local(function);
        let high = schema.reserve_i64_local(function);
        let discarded = schema.reserve_i64_local(function);
        let carry = schema.reserve_i64_local(function);

        lhs_len.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        lhs_sign.load(function);
        res_sign.store(function);
        res_ptr.copy_from(&lhs_ptr, schema, function);
        function.instruction(&Instruction::I64Const(0));
        res_len.store(function);
        function.instruction(&Instruction::Else);

        // A count with more than 64 significant bits is necessarily beyond
        // the finite lhs magnitude. Arithmetic right shift therefore
        // saturates without narrowing the count.
        rhs_len.load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_bigint_saturated_right_shift(
            lhs_sign, &lhs_ptr, res_sign, &res_ptr, res_len, exit, function,
        )?;
        function.instruction(&Instruction::Else);

        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        self.emit_bigint_digit_load_or_zero(&rhs_ptr, rhs_len, index, count, function);
        function.instruction(&Instruction::I64Const(1));
        index.store(function);
        self.emit_bigint_digit_load_or_zero(&rhs_ptr, rhs_len, index, high, function);
        high.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Shl);
        count.load(function);
        function.instruction(&Instruction::I64Or);
        count.store(function);
        count.load(function);
        function.instruction(&Instruction::I64Const(5));
        function.instruction(&Instruction::I64ShrU);
        word_shift.store(function);
        count.load(function);
        function.instruction(&Instruction::I64Const(31));
        function.instruction(&Instruction::I64And);
        bit_shift.store(function);

        word_shift.load(function);
        lhs_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_bigint_saturated_right_shift(
            lhs_sign, &lhs_ptr, res_sign, &res_ptr, res_len, exit, function,
        )?;
        function.instruction(&Instruction::Else);

        lhs_len.load(function);
        word_shift.load(function);
        function.instruction(&Instruction::I64Sub);
        base_len.store(function);
        base_len.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        res_len.store(function);
        self.emit_bigint_digits_alloc(res_len, &res_ptr, exit, function)?;

        // Track all complete discarded digits and the low partial digit.
        function.instruction(&Instruction::I64Const(0));
        discarded.store(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        word_shift.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_digit_load(&lhs_ptr, index, digit, function);
        discarded.load(function);
        digit.load(function);
        function.instruction(&Instruction::I64Or);
        discarded.store(function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_bigint_digit_load(&lhs_ptr, word_shift, digit, function);
        function.instruction(&Instruction::I64Const(1));
        bit_shift.load(function);
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        digit.load(function);
        function.instruction(&Instruction::I64And);
        discarded.load(function);
        function.instruction(&Instruction::I64Or);
        discarded.store(function);

        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        base_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        word_shift.load(function);
        function.instruction(&Instruction::I64Add);
        source_index.store(function);
        self.emit_bigint_digit_load(&lhs_ptr, source_index, digit, function);
        digit.load(function);
        bit_shift.load(function);
        function.instruction(&Instruction::I64ShrU);
        digit.store(function);
        source_index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        source_index.store(function);
        self.emit_bigint_digit_load_or_zero(&lhs_ptr, lhs_len, source_index, high, function);
        high.load(function);
        function.instruction(&Instruction::I64Const(32));
        bit_shift.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        digit.load(function);
        function.instruction(&Instruction::I64Or);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        digit.store(function);
        self.emit_bigint_digit_store(&res_ptr, index, digit, function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        // For negative values arithmetic shift rounds toward -infinity:
        // `-(m >> k)` becomes `-ceil(m / 2^k)` whenever a bit was discarded.
        lhs_sign.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        discarded.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        carry.store(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        res_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        carry.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_digit_load(&res_ptr, index, digit, function);
        digit.load(function);
        carry.load(function);
        function.instruction(&Instruction::I64Add);
        digit.store(function);
        digit.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        carry.store(function);
        digit.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        digit.store(function);
        self.emit_bigint_digit_store(&res_ptr, index, digit, function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        lhs_sign.load(function);
        res_sign.store(function);
        self.emit_bigint_normalize_len(&res_ptr, res_len, function);

        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        schema.release_i64_local(carry, function);
        schema.release_i64_local(discarded, function);
        schema.release_i64_local(high, function);
        schema.release_i64_local(digit, function);
        schema.release_i64_local(source_index, function);
        schema.release_i64_local(index, function);
        schema.release_i64_local(base_len, function);
        schema.release_i64_local(bit_shift, function);
        schema.release_i64_local(word_shift, function);
        schema.release_i64_local(count, function);
        Ok(())
    }

    fn emit_bigint_saturated_right_shift(
        &mut self,
        lhs_sign: I64Local,
        lhs_ptr: &BigIntConstruction,
        res_sign: I64Local,
        res_ptr: &BigIntConstruction,
        res_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let index = schema.reserve_i64_local(function);
        let one = schema.reserve_i64_local(function);
        lhs_sign.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(-1));
        res_sign.store(function);
        function.instruction(&Instruction::I64Const(1));
        res_len.store(function);
        self.emit_bigint_digits_alloc(res_len, &res_ptr, exit, function)?;
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::I64Const(1));
        one.store(function);
        self.emit_bigint_digit_store(&res_ptr, index, one, function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        res_sign.store(function);
        res_ptr.copy_from(&lhs_ptr, schema, function);
        function.instruction(&Instruction::I64Const(0));
        res_len.store(function);
        function.instruction(&Instruction::End);
        schema.release_i64_local(one, function);
        schema.release_i64_local(index, function);
        Ok(())
    }

    fn emit_bigint_magnitude_mul(
        &mut self,
        a_ptr: &BigIntConstruction,
        a_len: I64Local,
        b_ptr: &BigIntConstruction,
        b_len: I64Local,
        out_ptr: &BigIntConstruction,
        out_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let i = schema.reserve_i64_local(function);
        let j = schema.reserve_i64_local(function);
        let k = schema.reserve_i64_local(function);
        let carry = schema.reserve_i64_local(function);
        let a_digit = schema.reserve_i64_local(function);
        let b_digit = schema.reserve_i64_local(function);
        let acc = schema.reserve_i64_local(function);

        a_len.load(function);
        b_len.load(function);
        function.instruction(&Instruction::I64Add);
        out_len.store(function);
        self.emit_bigint_digits_alloc(out_len, &out_ptr, exit, function)?;

        function.instruction(&Instruction::I64Const(0));
        i.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        i.load(function);
        a_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_digit_load(&a_ptr, i, a_digit, function);
        function.instruction(&Instruction::I64Const(0));
        carry.store(function);
        function.instruction(&Instruction::I64Const(0));
        j.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        j.load(function);
        b_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        i.load(function);
        j.load(function);
        function.instruction(&Instruction::I64Add);
        k.store(function);
        self.emit_bigint_digit_load(&b_ptr, j, b_digit, function);
        // (2^32-1)^2 + 2*(2^32-1) == 2^64-1, so the accumulator never wraps.
        self.emit_bigint_digit_load(&out_ptr, k, acc, function);
        acc.load(function);
        a_digit.load(function);
        b_digit.load(function);
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        carry.load(function);
        function.instruction(&Instruction::I64Add);
        acc.store(function);
        acc.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        carry.store(function);
        acc.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        acc.store(function);
        self.emit_bigint_digit_store(&out_ptr, k, acc, function);
        j.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        j.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        // Flush the carry into the high digits.
        i.load(function);
        b_len.load(function);
        function.instruction(&Instruction::I64Add);
        k.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        carry.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::BrIf(1));
        k.load(function);
        out_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_digit_load(&out_ptr, k, acc, function);
        acc.load(function);
        carry.load(function);
        function.instruction(&Instruction::I64Add);
        acc.store(function);
        acc.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        carry.store(function);
        acc.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        acc.store(function);
        self.emit_bigint_digit_store(&out_ptr, k, acc, function);
        k.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        k.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        i.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        i.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.emit_bigint_normalize_len(&out_ptr, out_len, function);

        schema.release_i64_local(acc, function);
        schema.release_i64_local(b_digit, function);
        schema.release_i64_local(a_digit, function);
        schema.release_i64_local(carry, function);
        schema.release_i64_local(k, function);
        schema.release_i64_local(j, function);
        schema.release_i64_local(i, function);
        Ok(())
    }

    fn emit_bigint_magnitude_divmod(
        &mut self,
        a_ptr: &BigIntConstruction,
        a_len: I64Local,
        b_ptr: &BigIntConstruction,
        b_len: I64Local,
        q_ptr: &BigIntConstruction,
        q_len: I64Local,
        r_ptr: &BigIntConstruction,
        r_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let bit = schema.reserve_i64_local(function);
        let index = schema.reserve_i64_local(function);
        let digit = schema.reserve_i64_local(function);
        let carry = schema.reserve_i64_local(function);
        let shifted = schema.reserve_i64_local(function);
        let comparison = schema.reserve_i64_local(function);
        let borrow = schema.reserve_i64_local(function);
        let b_digit = schema.reserve_i64_local(function);
        let working_len = schema.reserve_i64_local(function);

        a_len.load(function);
        q_len.store(function);
        self.emit_bigint_digits_alloc(q_len, &q_ptr, exit, function)?;
        a_len.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        working_len.store(function);
        self.emit_bigint_digits_alloc(working_len, &r_ptr, exit, function)?;

        a_len.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Mul);
        bit.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        bit.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::BrIf(1));
        bit.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        bit.store(function);

        // carry := bit `bit` of the dividend
        bit.load(function);
        function.instruction(&Instruction::I64Const(5));
        function.instruction(&Instruction::I64ShrU);
        index.store(function);
        self.emit_bigint_digit_load(&a_ptr, index, digit, function);
        digit.load(function);
        bit.load(function);
        function.instruction(&Instruction::I64Const(31));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        carry.store(function);

        // remainder := remainder * 2 + carry
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        working_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_digit_load(&r_ptr, index, digit, function);
        digit.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Shl);
        carry.load(function);
        function.instruction(&Instruction::I64Or);
        shifted.store(function);
        shifted.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        carry.store(function);
        shifted.load(function);
        function.instruction(&Instruction::I64Const(DIGIT_MASK));
        function.instruction(&Instruction::I64And);
        shifted.store(function);
        self.emit_bigint_digit_store(&r_ptr, index, shifted, function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        // if remainder >= divisor { remainder -= divisor; set quotient bit }
        self.emit_bigint_magnitude_compare(
            &r_ptr,
            working_len,
            &b_ptr,
            b_len,
            comparison,
            function,
        );
        comparison.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        borrow.store(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        working_len.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_digit_load(&r_ptr, index, digit, function);
        self.emit_bigint_digit_load_or_zero(&b_ptr, b_len, index, b_digit, function);
        digit.load(function);
        b_digit.load(function);
        function.instruction(&Instruction::I64Sub);
        borrow.load(function);
        function.instruction(&Instruction::I64Sub);
        shifted.store(function);
        shifted.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        shifted.load(function);
        function.instruction(&Instruction::I64Const(1 << 32));
        function.instruction(&Instruction::I64Add);
        shifted.store(function);
        function.instruction(&Instruction::I64Const(1));
        borrow.store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        borrow.store(function);
        function.instruction(&Instruction::End);
        self.emit_bigint_digit_store(&r_ptr, index, shifted, function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        bit.load(function);
        function.instruction(&Instruction::I64Const(5));
        function.instruction(&Instruction::I64ShrU);
        index.store(function);
        self.emit_bigint_digit_load(&q_ptr, index, digit, function);
        digit.load(function);
        function.instruction(&Instruction::I64Const(1));
        bit.load(function);
        function.instruction(&Instruction::I64Const(31));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Or);
        digit.store(function);
        self.emit_bigint_digit_store(&q_ptr, index, digit, function);
        function.instruction(&Instruction::End);

        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        working_len.load(function);
        r_len.store(function);
        self.emit_bigint_normalize_len(&q_ptr, q_len, function);
        self.emit_bigint_normalize_len(&r_ptr, r_len, function);

        schema.release_i64_local(working_len, function);
        schema.release_i64_local(b_digit, function);
        schema.release_i64_local(borrow, function);
        schema.release_i64_local(comparison, function);
        schema.release_i64_local(shifted, function);
        schema.release_i64_local(carry, function);
        schema.release_i64_local(digit, function);
        schema.release_i64_local(index, function);
        schema.release_i64_local(bit, function);
        Ok(())
    }

    fn emit_bigint_divmod_op(
        &mut self,
        op_local: I64Local,
        lhs_sign: I64Local,
        lhs_ptr: &BigIntConstruction,
        lhs_len: I64Local,
        rhs_sign: I64Local,
        rhs_ptr: &BigIntConstruction,
        rhs_len: I64Local,
        res_sign: I64Local,
        res_ptr: &BigIntConstruction,
        res_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let q_ptr = self.emit_bigint_empty_digits(function);
        let q_len = schema.reserve_i64_local(function);
        let r_ptr = self.emit_bigint_empty_digits(function);
        let r_len = schema.reserve_i64_local(function);

        rhs_len.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_bigint_error(BIGINT_DIVISION_BY_ZERO_MESSAGE, function)?;
        function.instruction(&Instruction::Else);
        self.emit_bigint_magnitude_divmod(
            &lhs_ptr, lhs_len, &rhs_ptr, rhs_len, &q_ptr, q_len, &r_ptr, r_len, exit, function,
        )?;
        op_local.load(function);
        function.instruction(&Instruction::I64Const(i64::from(
            BigIntHelperOp::Div.runtime_code(),
        )));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        res_ptr.copy_from(&q_ptr, schema, function);
        q_len.load(function);
        res_len.store(function);
        lhs_sign.load(function);
        rhs_sign.load(function);
        function.instruction(&Instruction::I64Mul);
        res_sign.store(function);
        function.instruction(&Instruction::Else);
        res_ptr.copy_from(&r_ptr, schema, function);
        r_len.load(function);
        res_len.store(function);
        lhs_sign.load(function);
        res_sign.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        schema.release_i64_local(r_len, function);
        r_ptr.clear(function);
        schema.release_i64_local(q_len, function);
        q_ptr.clear(function);
        Ok(())
    }

    fn emit_bigint_pow(
        &mut self,
        lhs_sign: I64Local,
        lhs_ptr: &BigIntConstruction,
        lhs_len: I64Local,
        rhs_sign: I64Local,
        rhs_ptr: &BigIntConstruction,
        rhs_len: I64Local,
        res_sign: I64Local,
        res_ptr: &BigIntConstruction,
        res_len: I64Local,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let base_ptr = self.emit_bigint_empty_digits(function);
        let square_ptr = self.emit_bigint_empty_digits(function);
        let base_len = schema.reserve_i64_local(function);
        let square_len = schema.reserve_i64_local(function);
        let position = schema.reserve_i64_local(function);
        let bits = schema.reserve_i64_local(function);
        let index = schema.reserve_i64_local(function);
        let digit = schema.reserve_i64_local(function);
        let one = schema.reserve_i64_local(function);
        let odd = schema.reserve_i64_local(function);
        rhs_sign.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_bigint_error(BIGINT_EXPONENT_MESSAGE, function)?;
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        bits.store(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        self.emit_bigint_digit_load_or_zero(rhs_ptr, rhs_len, index, odd, function);
        odd.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        odd.store(function);
        rhs_len.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        rhs_len.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        index.store(function);
        self.emit_bigint_digit_load(rhs_ptr, index, digit, function);
        index.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Const(64));
        digit.load(function);
        function.instruction(&Instruction::I64Clz);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Add);
        bits.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(1));
        res_len.store(function);
        function.instruction(&Instruction::I64Const(1));
        one.store(function);
        self.emit_bigint_digits_alloc(res_len, res_ptr, exit, function)?;
        function.instruction(&Instruction::I64Const(0));
        position.store(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        self.emit_bigint_digit_store(res_ptr, index, one, function);
        base_ptr.copy_from(lhs_ptr, schema, function);
        lhs_len.load(function);
        base_len.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        position.load(function);
        bits.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        position.load(function);
        function.instruction(&Instruction::I64Const(5));
        function.instruction(&Instruction::I64ShrU);
        index.store(function);
        self.emit_bigint_digit_load(rhs_ptr, index, digit, function);
        digit.load(function);
        position.load(function);
        function.instruction(&Instruction::I64Const(31));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_bigint_magnitude_mul(
            res_ptr,
            res_len,
            &base_ptr,
            base_len,
            &square_ptr,
            square_len,
            exit,
            function,
        )?;
        res_ptr.copy_from(&square_ptr, schema, function);
        square_len.load(function);
        res_len.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        position.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        position.store(function);
        position.load(function);
        bits.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_magnitude_mul(
            &base_ptr,
            base_len,
            &base_ptr,
            base_len,
            &square_ptr,
            square_len,
            exit,
            function,
        )?;
        base_ptr.copy_from(&square_ptr, schema, function);
        square_len.load(function);
        base_len.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        res_len.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::Else);
        lhs_sign.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        odd.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        res_sign.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(odd, function);
        schema.release_i64_local(one, function);
        schema.release_i64_local(digit, function);
        schema.release_i64_local(index, function);
        schema.release_i64_local(bits, function);
        schema.release_i64_local(position, function);
        schema.release_i64_local(square_len, function);
        schema.release_i64_local(base_len, function);
        square_ptr.clear(function);
        base_ptr.clear(function);
        Ok(())
    }

    pub(crate) fn compile_bigint_arithmetic_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::BigIntArithmetic);
        let schema = self.runtime_schema();
        let parameters = self.helper_parameters::<BigIntArithmeticParameters>(&mut function);
        let lhs_ptr = self.emit_bigint_empty_digits(&mut function);
        let rhs_ptr = self.emit_bigint_empty_digits(&mut function);
        let res_ptr = self.emit_bigint_empty_digits(&mut function);
        let lhs_sign = schema.reserve_i64_local(&mut function);
        let lhs_len = schema.reserve_i64_local(&mut function);
        let rhs_sign = schema.reserve_i64_local(&mut function);
        let rhs_len = schema.reserve_i64_local(&mut function);
        let res_sign = schema.reserve_i64_local(&mut function);
        let res_len = schema.reserve_i64_local(&mut function);
        let op_local = schema.reserve_i64_local(&mut function);
        let fraction_sign = schema.reserve_i64_local(&mut function);
        let number_class = schema.reserve_i64_local(&mut function);
        let matched = schema.reserve_i32_local(&mut function);
        let comparison = schema.reserve_i32_local(&mut function);
        function.instruction(&Instruction::I32Const(0));
        matched.store(&mut function);
        function.instruction(&Instruction::I32Const(0));
        comparison.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        fraction_sign.store(&mut function);
        function.instruction(&Instruction::I64Const(0));
        number_class.store(&mut function);
        parameters.operation.load(&mut function);
        function.instruction(&Instruction::I64ExtendI32U);
        op_local.store(&mut function);
        let exit = self.open_frame(ControlFrameKind::Block, &mut function);
        self.emit_bigint_decode_operand(
            &parameters.left,
            lhs_sign,
            &lhs_ptr,
            lhs_len,
            exit,
            &mut function,
        )?;
        parameters.operation.load(&mut function);
        function.instruction(&Instruction::I32Const(
            BigIntHelperOp::CompareWithNumber.runtime_code() as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, &mut function);
        self.emit_bigint_decode_number_operand(
            parameters.right.scalar(),
            rhs_sign,
            &rhs_ptr,
            rhs_len,
            fraction_sign,
            number_class,
            exit,
            &mut function,
        )?;
        function.instruction(&Instruction::Else);
        self.emit_bigint_decode_operand(
            &parameters.right,
            rhs_sign,
            &rhs_ptr,
            rhs_len,
            exit,
            &mut function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for operation in BigIntHelperOp::ALL {
            parameters.operation.load(&mut function);
            function.instruction(&Instruction::I32Const(operation.runtime_code() as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, &mut function);
            function.instruction(&Instruction::I32Const(1));
            matched.store(&mut function);
            match operation {
                BigIntHelperOp::Add | BigIntHelperOp::Sub | BigIntHelperOp::Negate => {
                    match operation {
                        BigIntHelperOp::Sub => {
                            function.instruction(&Instruction::I64Const(0));
                            rhs_sign.load(&mut function);
                            function.instruction(&Instruction::I64Sub);
                            rhs_sign.store(&mut function);
                        }
                        BigIntHelperOp::Negate => {
                            function.instruction(&Instruction::I64Const(0));
                            lhs_sign.load(&mut function);
                            function.instruction(&Instruction::I64Sub);
                            lhs_sign.store(&mut function);
                            function.instruction(&Instruction::I64Const(0));
                            rhs_sign.store(&mut function);
                            function.instruction(&Instruction::I64Const(0));
                            rhs_len.store(&mut function);
                        }
                        BigIntHelperOp::Add => {}
                        BigIntHelperOp::Mul
                        | BigIntHelperOp::Div
                        | BigIntHelperOp::Rem
                        | BigIntHelperOp::Exp
                        | BigIntHelperOp::Compare
                        | BigIntHelperOp::CompareWithNumber
                        | BigIntHelperOp::BitAnd
                        | BigIntHelperOp::BitOr
                        | BigIntHelperOp::BitXor
                        | BigIntHelperOp::Shl
                        | BigIntHelperOp::Shr => unreachable!("arithmetic arm is closed"),
                    }
                    self.emit_bigint_signed_add(
                        lhs_sign,
                        &lhs_ptr,
                        lhs_len,
                        rhs_sign,
                        &rhs_ptr,
                        rhs_len,
                        res_sign,
                        &res_ptr,
                        res_len,
                        exit,
                        &mut function,
                    )?;
                }
                BigIntHelperOp::Mul => {
                    self.emit_bigint_magnitude_mul(
                        &lhs_ptr,
                        lhs_len,
                        &rhs_ptr,
                        rhs_len,
                        &res_ptr,
                        res_len,
                        exit,
                        &mut function,
                    )?;
                    lhs_sign.load(&mut function);
                    rhs_sign.load(&mut function);
                    function.instruction(&Instruction::I64Mul);
                    res_sign.store(&mut function);
                }
                BigIntHelperOp::Div | BigIntHelperOp::Rem => self.emit_bigint_divmod_op(
                    op_local,
                    lhs_sign,
                    &lhs_ptr,
                    lhs_len,
                    rhs_sign,
                    &rhs_ptr,
                    rhs_len,
                    res_sign,
                    &res_ptr,
                    res_len,
                    exit,
                    &mut function,
                )?,
                BigIntHelperOp::Exp => self.emit_bigint_pow(
                    lhs_sign,
                    &lhs_ptr,
                    lhs_len,
                    rhs_sign,
                    &rhs_ptr,
                    rhs_len,
                    res_sign,
                    &res_ptr,
                    res_len,
                    exit,
                    &mut function,
                )?,
                BigIntHelperOp::BitAnd | BigIntHelperOp::BitOr | BigIntHelperOp::BitXor => self
                    .emit_bigint_bitwise(
                        op_local,
                        lhs_sign,
                        &lhs_ptr,
                        lhs_len,
                        rhs_sign,
                        &rhs_ptr,
                        rhs_len,
                        res_sign,
                        &res_ptr,
                        res_len,
                        exit,
                        &mut function,
                    )?,
                BigIntHelperOp::Shl | BigIntHelperOp::Shr => self.emit_bigint_shift(
                    op_local,
                    lhs_sign,
                    &lhs_ptr,
                    lhs_len,
                    rhs_sign,
                    &rhs_ptr,
                    rhs_len,
                    res_sign,
                    &res_ptr,
                    res_len,
                    exit,
                    &mut function,
                )?,
                BigIntHelperOp::Compare | BigIntHelperOp::CompareWithNumber => {
                    function.instruction(&Instruction::I32Const(1));
                    comparison.store(&mut function);
                    self.emit_bigint_signed_compare(
                        lhs_sign,
                        &lhs_ptr,
                        lhs_len,
                        rhs_sign,
                        &rhs_ptr,
                        rhs_len,
                        res_sign,
                        &mut function,
                    );
                    res_sign.load(&mut function);
                    function.instruction(&Instruction::I64Eqz);
                    self.open_frame(ControlFrameKind::If, &mut function);
                    function.instruction(&Instruction::I64Const(0));
                    fraction_sign.load(&mut function);
                    function.instruction(&Instruction::I64Sub);
                    res_sign.store(&mut function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                    for (class, answer) in [(2, -1), (3, 1)] {
                        number_class.load(&mut function);
                        function.instruction(&Instruction::I64Const(class));
                        function.instruction(&Instruction::I64Eq);
                        self.open_frame(ControlFrameKind::If, &mut function);
                        function.instruction(&Instruction::I64Const(answer));
                        res_sign.store(&mut function);
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                    }
                    number_class.load(&mut function);
                    function.instruction(&Instruction::I64Const(1));
                    function.instruction(&Instruction::I64Eq);
                    function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
                    function.instruction(&Instruction::I64Const(f64::NAN.to_bits() as i64));
                    function.instruction(&Instruction::Else);
                    res_sign.load(&mut function);
                    function.instruction(&Instruction::F64ConvertI64S);
                    function.instruction(&Instruction::I64ReinterpretF64);
                    function.instruction(&Instruction::End);
                    res_sign.store(&mut function);
                    self.completion()
                        .value()
                        .set_number(res_sign, &mut function);
                    self.completion()
                        .set_normal(self.completion().value(), &mut function);
                }
            }
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        matched.load(&mut function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        comparison.load(&mut function);
        function.instruction(&Instruction::I32Eqz);
        self.completion().kind().load(&mut function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, &mut function);
        let packed = schema.reserve_completion(&mut function);
        self.emit_bigint_pack_result(res_sign, &res_ptr, res_len, &packed, &mut function)?;
        self.completion().copy_from(&packed, &mut function);
        packed.clear(&mut function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(comparison, &mut function);
        schema.release_i32_local(matched, &mut function);
        for local in [
            number_class,
            fraction_sign,
            op_local,
            res_len,
            res_sign,
            rhs_len,
            rhs_sign,
            lhs_len,
            lhs_sign,
        ] {
            schema.release_i64_local(local, &mut function);
        }
        res_ptr.clear(&mut function);
        rhs_ptr.clear(&mut function);
        lhs_ptr.clear(&mut function);
        parameters.release(&mut function);
        self.completion().emit(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
