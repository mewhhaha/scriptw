//! Native BigInt boundaries consume one GC value representation and completions.

use super::super::*;
use crate::gc_types::{
    BigIntConstruction, BigIntLimbArray, BigIntValue, BigIntValueSchema, CompletionLocals,
    GcNullability, PrimitiveBox, PrimitiveBoxSchema, ValueLocals,
};
use crate::operations::BigIntNumberPolicy;

mod radix_formatting;

#[derive(Clone, Copy)]
enum BigIntFixedWidthOperation {
    Signed,
    Unsigned,
}
enum BigIntPrototypeOperation {
    ExactValue,
    RadixString,
    LocaleString,
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_bigint_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let new_target = schema.reserve_value_local(function);
        let argument = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        self.compile_new_target_to_locals(&new_target, function)?;
        new_target.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::BIGINT_IS_NOT_A_CONSTRUCTOR,
            &pending,
            function,
        )?;
        function.instruction(&Instruction::Else);
        self.emit_builtin_arg_to_value(0, &argument, function);
        self.emit_value_to_bigint_locals(
            &argument,
            BigIntNumberPolicy::NumberToBigInt,
            &pending,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        argument.clear(function);
        new_target.clear(function);
        Ok(())
    }

    pub(super) fn emit_bigint_as_int_n_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_bigint_fixed_width_builtin(BigIntFixedWidthOperation::Signed, function)
    }
    pub(super) fn emit_bigint_as_uint_n_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_bigint_fixed_width_builtin(BigIntFixedWidthOperation::Unsigned, function)
    }
    pub(super) fn emit_bigint_prototype_to_string_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_bigint_prototype_operation(BigIntPrototypeOperation::RadixString, function)
    }
    pub(super) fn emit_bigint_prototype_to_locale_string_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_bigint_prototype_operation(BigIntPrototypeOperation::LocaleString, function)
    }
    pub(super) fn emit_bigint_prototype_value_of_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_bigint_prototype_operation(BigIntPrototypeOperation::ExactValue, function)
    }

    fn emit_bigint_prototype_operation(
        &mut self,
        operation: BigIntPrototypeOperation,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let bigint = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        self.compile_this_to_locals(&receiver, function)?;
        bigint.copy_from(&receiver, function);
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<PrimitiveBox>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let boxed = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<PrimitiveBox>(schema, function),
            function,
        );
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PrimitiveBox>()
                .field(PrimitiveBoxSchema::PRIMITIVE)
                .read(&boxed, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<crate::gc_types::StoredValue>()
            .read_into(&stored, &bigint, schema, function);
        stored.clear(function);
        boxed.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        bigint.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::BigInt.tag()));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::CANNOT_CONVERT_VALUE_TO_BIGINT,
            &pending,
            function,
        )?;
        function.instruction(&Instruction::Else);
        match operation {
            BigIntPrototypeOperation::ExactValue => pending.set_normal(&bigint, function),
            BigIntPrototypeOperation::RadixString => {
                self.emit_bigint_radix_string_result(&bigint, &pending, function)?
            }
            BigIntPrototypeOperation::LocaleString => {
                self.emit_intrinsic_number_locale_format(&bigint, function)?;
                pending.copy_from(self.completion(), function);
            }
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        bigint.clear(function);
        receiver.clear(function);
        Ok(())
    }

    fn emit_bigint_fixed_width_builtin(
        &mut self,
        operation: BigIntFixedWidthOperation,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let width_arg = schema.reserve_value_local(function);
        let bigint_arg = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let width = schema.reserve_i64_local(function);
        let output = schema.reserve_completion(function);
        output.initialize(function);
        self.emit_builtin_arg_to_value(0, &width_arg, function);
        self.emit_builtin_arg_to_value(1, &bigint_arg, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_to_index_i64_from_value_locals(
            &width_arg,
            width,
            RuntimeErrorMessage::CANNOT_CONVERT_VALUE_TO_BIGINT,
            &pending,
            function,
        )?;
        output.copy_from(&pending, function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(exit, function);
        self.emit_value_to_bigint_locals(
            &bigint_arg,
            BigIntNumberPolicy::RejectNumber,
            &pending,
            function,
        )?;
        output.copy_from(&pending, function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(exit, function);
        self.emit_bigint_fixed_width_result(operation, pending.value(), width, &output, function)?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        output.clear(function);
        schema.release_i64_local(width, function);
        pending.clear(function);
        bigint_arg.clear(function);
        width_arg.clear(function);
        Ok(())
    }

    fn emit_bigint_fixed_width_result(
        &mut self,
        operation: BigIntFixedWidthOperation,
        input: &ValueLocals,
        width: crate::gc_types::I64Local,
        output: &CompletionLocals,
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
        let source_len = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let result_len = schema.reserve_i32_local(function);
        let result_negative = schema.reserve_i32_local(function);
        let count = schema.reserve_i64_local(function);
        let partial = schema.reserve_i64_local(function);
        let mask = schema.reserve_i64_local(function);
        let limb = schema.reserve_i64_local(function);
        let carry = schema.reserve_i64_local(function);
        let magnitude_bits = schema.reserve_i64_local(function);
        schema
            .struct_type::<BigIntValue>()
            .field(BigIntValueSchema::NEGATIVE)
            .read(&bigint, schema, function)
            .store(negative, function);
        schema
            .array_type::<BigIntLimbArray>()
            .length(&limbs, schema, function);
        source_len.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        source_len.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        output.set_normal(input, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        source_len.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        index.store(function);
        schema
            .array_type::<BigIntLimbArray>()
            .read(&limbs, index, schema, function)
            .store_i64(limb, function);
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(64));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Const(64));
        limb.load(function);
        function.instruction(&Instruction::I64Clz);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Add);
        magnitude_bits.store(function);
        width.load(function);
        magnitude_bits.load(function);
        function.instruction(&match operation {
            BigIntFixedWidthOperation::Signed => Instruction::I64GtU,
            BigIntFixedWidthOperation::Unsigned => Instruction::I64GeU,
        });
        if let BigIntFixedWidthOperation::Unsigned = operation {
            negative.load(function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
        }
        self.open_frame(ControlFrameKind::If, function);
        output.set_normal(input, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        width.load(function);
        function.instruction(&Instruction::I64Const(63));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(6));
        function.instruction(&Instruction::I64ShrU);
        count.store(function);
        count.load(function);
        function.instruction(&Instruction::I64Const(crate::bigint::MAX_BIGINT_BITS / 64));
        function.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_range_error(
            RuntimeErrorMessage::BIGINT_SIZE_EXCEEDS_IMPLEMENTATION_LIMIT,
            output,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        count.load(function);
        function.instruction(&Instruction::I32WrapI64);
        result_len.store(function);
        let work = BigIntConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            result_len,
            function,
        );
        width.load(function);
        function.instruction(&Instruction::I64Const(63));
        function.instruction(&Instruction::I64And);
        partial.store(function);
        function.instruction(&Instruction::I64Const(-1));
        mask.store(function);
        partial.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        partial.load(function);
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        mask.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        negative.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        carry.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        result_len.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        function.instruction(&Instruction::I64Const(0));
        limb.store(function);
        index.load(function);
        source_len.load(function);
        function.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .array_type::<BigIntLimbArray>()
            .read(&limbs, index, schema, function)
            .store_i64(limb, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        negative.load(function);
        self.open_frame(ControlFrameKind::If, function);
        limb.load(function);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Xor);
        carry.load(function);
        function.instruction(&Instruction::I64Add);
        limb.store(function);
        limb.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I64ExtendI32U);
        carry.load(function);
        function.instruction(&Instruction::I64And);
        carry.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        result_len.load(function);
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        limb.load(function);
        mask.load(function);
        function.instruction(&Instruction::I64And);
        limb.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        work.write(index, limb, schema, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I32Const(0));
        result_negative.store(function);
        if let BigIntFixedWidthOperation::Signed = operation {
            width.load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            result_len.load(function);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32Sub);
            index.store(function);
            work.read(index, limb, schema, function);
            limb.load(function);
            width.load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Sub);
            function.instruction(&Instruction::I64Const(63));
            function.instruction(&Instruction::I64And);
            function.instruction(&Instruction::I64ShrU);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64And);
            function.instruction(&Instruction::I32WrapI64);
            result_negative.store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        result_negative.load(function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        carry.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        result_len.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        work.read(index, limb, schema, function);
        limb.load(function);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Xor);
        carry.load(function);
        function.instruction(&Instruction::I64Add);
        limb.store(function);
        limb.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I64ExtendI32U);
        carry.load(function);
        function.instruction(&Instruction::I64And);
        carry.store(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        result_len.load(function);
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        limb.load(function);
        mask.load(function);
        function.instruction(&Instruction::I64And);
        limb.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        work.write(index, limb, schema, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let result = schema
            .reserve_gc_local(function)
            .initialize(work.publish(result_negative, schema, function), function);
        let value = schema.reserve_value_local(function);
        value.set_reference(&result, schema, function);
        output.set_normal(&value, function);
        value.clear(function);
        result.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        for local in [magnitude_bits, carry, limb, mask, partial, count] {
            schema.release_i64_local(local, function);
        }
        for local in [result_negative, result_len, index, source_len, negative] {
            schema.release_i32_local(local, function);
        }
        limbs.clear(function);
        bigint.clear(function);
        Ok(())
    }
}
