use super::*;

mod decimal_format;
mod integer_decimal;
mod ryu;

pub(super) use decimal_format::{NumberDecimalFormat, NumberExponentialFormat};

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn emit_number_to_string_payload(
        &mut self,
        bits: crate::gc_types::I64Local,
        function: &mut Function,
    ) -> Result<crate::gc_types::GcStackReference<StringValue>, EmitError> {
        if self.outline_number_to_string {
            let schema = self.runtime_schema();
            let slot = schema.reserve_gc_local(function);
            let string = schema
                .call_helper(
                    crate::runtime_helpers::NumberToStringArguments::new(bits),
                    self.runtime_helper_base()?,
                    function,
                )
                .bind(schema, slot, function);
            let result = string.load(schema, function);
            string.clear(function);
            return Ok(result);
        }
        self.emit_ryu_number_to_string_payload(bits, function)
    }
}

/// Private numeric digit storage. It cannot be published as a JavaScript
/// value; publication copies its ASCII digits into immutable UTF-16 storage.
pub(in crate::operations) struct FormattingBuffer {
    bytes: crate::gc_types::GcLocal<crate::gc_types::ByteArray>,
}
impl FormattingBuffer {
    pub(in crate::operations) fn empty(
        schema: &crate::gc_types::RuntimeSchema,
        function: &mut Function,
    ) -> Self {
        let length = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        length.store(function);
        let bytes = schema.reserve_gc_local(function).initialize(
            schema.array_type::<crate::gc_types::ByteArray>().filled(
                crate::gc_types::GcOperand::i32(0),
                length,
                function,
            ),
            function,
        );
        schema.release_i32_local(length, function);
        Self { bytes }
    }
    pub(in crate::operations) fn resize(
        &self,
        length: crate::gc_types::I64Local,
        schema: &crate::gc_types::RuntimeSchema,
        function: &mut Function,
    ) {
        let count = schema.reserve_i32_local(function);
        length.load(function);
        function.instruction(&Instruction::I64Const(i64::from(u32::MAX)));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        length.load(function);
        function.instruction(&Instruction::I32WrapI64);
        count.store(function);
        self.bytes.replace(
            schema.array_type::<crate::gc_types::ByteArray>().filled(
                crate::gc_types::GcOperand::i32(0),
                count,
                function,
            ),
            function,
        );
        schema.release_i32_local(count, function);
    }
    pub(in crate::operations) fn read_i32_from_stack(
        &self,
        schema: &crate::gc_types::RuntimeSchema,
        function: &mut Function,
    ) {
        let index = schema.reserve_i32_local(function);
        let value = schema.reserve_i32_local(function);
        index.store(function);
        schema
            .array_type::<crate::gc_types::ByteArray>()
            .read(&self.bytes, index, schema, function)
            .store(value, function);
        value.load(function);
        schema.release_i32_local(value, function);
        schema.release_i32_local(index, function);
    }
    pub(in crate::operations) fn read_i64_from_stack(
        &self,
        schema: &crate::gc_types::RuntimeSchema,
        function: &mut Function,
    ) {
        self.read_i32_from_stack(schema, function);
        function.instruction(&Instruction::I64ExtendI32U);
    }
    pub(in crate::operations) fn write_from_stack(
        &self,
        schema: &crate::gc_types::RuntimeSchema,
        function: &mut Function,
    ) {
        let index = schema.reserve_i32_local(function);
        let value = schema.reserve_i32_local(function);
        value.store(function);
        index.store(function);
        schema.array_type::<crate::gc_types::ByteArray>().write(
            &self.bytes,
            index,
            crate::gc_types::GcOperand::i32_local(value),
            schema,
            function,
        );
        schema.release_i32_local(value, function);
        schema.release_i32_local(index, function);
    }
    pub(in crate::operations) fn publish_string(
        &self,
        schema: &crate::gc_types::RuntimeSchema,
        function: &mut Function,
    ) -> crate::gc_types::GcStackReference<StringValue> {
        let length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let unit = schema.reserve_i32_local(function);
        schema
            .array_type::<crate::gc_types::ByteArray>()
            .length(&self.bytes, schema, function);
        length.store(function);
        let construction = crate::gc_types::StringConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        schema
            .array_type::<crate::gc_types::ByteArray>()
            .read(&self.bytes, index, schema, function)
            .store(unit, function);
        construction.write(index, unit, schema, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        let output = construction.publish(schema, function);
        schema.release_i32_local(unit, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
        output
    }
    pub(in crate::operations) fn clear(self, function: &mut Function) {
        self.bytes.clear(function);
    }
}
