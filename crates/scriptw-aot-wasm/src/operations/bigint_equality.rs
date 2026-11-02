//! The pure BigInt limb comparator has one runtime body. Ordinary Number,
//! String and reference equality retain their existing inline paths.

use super::*;
use crate::runtime_helpers::{BigIntEqualityParameters, HelperParameters};

impl FunctionBuilder<'_> {
    pub(crate) fn compile_bigint_equality_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::BigIntEquality);
        let parameters = self.helper_parameters::<BigIntEqualityParameters>(&mut function);
        self.emit_bigint_equality_kernel_i32(&parameters.left, &parameters.right, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_bigint_equality_kernel_i32(
        &self,
        left: &GcLocal<BigIntValue>,
        right: &GcLocal<BigIntValue>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let left_sign = schema.reserve_i32_local(function);
        let right_sign = schema.reserve_i32_local(function);
        let bigint = schema.struct_type::<BigIntValue>();
        bigint
            .field(BigIntValueSchema::NEGATIVE)
            .read(left, schema, function)
            .store(left_sign, function);
        bigint
            .field(BigIntValueSchema::NEGATIVE)
            .read(right, schema, function)
            .store(right_sign, function);
        let left_limbs_slot = schema.reserve_gc_local(function);
        let left_limbs = left_limbs_slot.initialize(
            bigint
                .field(BigIntValueSchema::LIMBS)
                .read(left, schema, function)
                .reference(),
            function,
        );
        let right_limbs_slot = schema.reserve_gc_local(function);
        let right_limbs = right_limbs_slot.initialize(
            bigint
                .field(BigIntValueSchema::LIMBS)
                .read(right, schema, function)
                .reference(),
            function,
        );
        let left_length = schema.reserve_i32_local(function);
        let right_length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let equal = schema.reserve_i32_local(function);
        let left_limb = schema.reserve_i64_local(function);
        let right_limb = schema.reserve_i64_local(function);
        let limbs = schema.array_type::<BigIntLimbArray>();
        limbs.length(&left_limbs, schema, function);
        left_length.store(function);
        limbs.length(&right_limbs, schema, function);
        right_length.store(function);
        left_sign.load(function);
        right_sign.load(function);
        function.instruction(&Instruction::I32Eq);
        left_length.load(function);
        right_length.load(function);
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        equal.store(function);
        equal.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        left_length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        limbs
            .read(&left_limbs, index, schema, function)
            .store_i64(left_limb, function);
        limbs
            .read(&right_limbs, index, schema, function)
            .store_i64(right_limb, function);
        left_limb.load(function);
        right_limb.load(function);
        function.instruction(&Instruction::I64Eq);
        equal.store(function);
        equal.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        equal.load(function);
        schema.release_i64_local(right_limb, function);
        schema.release_i64_local(left_limb, function);
        schema.release_i32_local(equal, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(right_length, function);
        schema.release_i32_local(left_length, function);
        right_limbs.clear(function);
        left_limbs.clear(function);
        schema.release_i32_local(right_sign, function);
        schema.release_i32_local(left_sign, function);
    }
}
