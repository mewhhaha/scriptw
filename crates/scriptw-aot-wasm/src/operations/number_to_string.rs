use super::*;
use crate::data::PooledLiteralDomain;
use crate::gc_types::{GcNullability, GcOperand, GcStackReference, PooledStringTable};

mod decimal_format;
mod integer_decimal;
mod ryu;

pub(super) use decimal_format::{NumberDecimalFormat, NumberExponentialFormat};

const SMALL_INTEGER_COUNT: i32 = 128;
// These decimal spellings already have canonical descriptors in R.
const POOLED_INTEGER_COUNT: i32 = 32;

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn emit_number_to_string_payload(
        &mut self,
        bits: crate::gc_types::I64Local,
        function: &mut Function,
    ) -> Result<GcStackReference<StringValue>, EmitError> {
        if self.outline_number_to_string {
            let schema = self.runtime_schema();
            return Ok(schema.helper_reference_on_stack(schema.call_helper(
                crate::runtime_helpers::NumberToStringArguments::new(bits),
                self.runtime_helper_base()?,
                function,
            )));
        }
        if !self.uses_heap {
            return self.emit_ryu_number_to_string_payload(bits, function);
        }

        let schema = self.runtime_schema();
        let integer = schema.reserve_i32_local(function);
        bits.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::I32TruncSatF64U);
        integer.store(function);
        integer.load(function);
        function.instruction(&Instruction::I32Const(SMALL_INTEGER_COUNT));
        function.instruction(&Instruction::I32LtU);
        // The round trip accepts both signs of zero and excludes fractions,
        // NaN, negative numbers and values outside the exact integer range.
        integer.load(function);
        function.instruction(&Instruction::F64ConvertI32U);
        bits.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::I32And);
        let reference = schema.reference_type::<StringValue>(GcNullability::NonNullable);
        function.instruction(&Instruction::If(BlockType::Result(ValType::Ref(reference))));
        integer.load(function);
        function.instruction(&Instruction::I32Const(POOLED_INTEGER_COUNT));
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::Ref(reference))));
        let _ = self.emit_pooled_integer_string(integer, function)?;
        function.instruction(&Instruction::Else);
        let _ = self.emit_extended_integer_string(integer, function)?;
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        let result = self.emit_ryu_number_to_string_payload(bits, function)?;
        function.instruction(&Instruction::End);
        schema.release_i32_local(integer, function);
        Ok(result)
    }

    /// Keep R's descriptor prefix at its original ordinals. The first
    /// request for a larger small integer extends the same GC root with
    /// decimal-only slots that the descriptor resolver never indexes.
    fn emit_extended_integer_string(
        &self,
        integer: I32Local,
        function: &mut Function,
    ) -> Result<GcStackReference<StringValue>, EmitError> {
        let schema = self.runtime_schema();
        let prefix = self.strings.runtime_string_descriptors()?.count();
        let capacity = prefix + (SMALL_INTEGER_COUNT - POOLED_INTEGER_COUNT) as u32;
        let arrays = schema.array_type::<PooledStringTable>();
        let table = schema
            .reserve_gc_local(function)
            .initialize(schema.pooled_string_table_reference(function), function);
        arrays.length(&table, schema, function);
        function.instruction(&Instruction::I32Const(capacity as i32));
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.rarely_reached();
        let length = schema.reserve_i32_local(function);
        length.set_constant(capacity as i32, function);
        let extended = schema.reserve_gc_local(function).initialize(
            arrays.filled(GcOperand::null(schema), length, function),
            function,
        );
        length.set_constant(prefix as i32, function);
        arrays.copy_prefix_from(&extended, &table, length, schema, function);
        schema.release_i32_local(length, function);
        schema.publish_pooled_strings(&extended, function);
        table.replace(extended.load(schema, function), function);
        extended.clear(function);
        function.instruction(&Instruction::End);

        let ordinal = schema.reserve_i32_local(function);
        integer.load(function);
        function.instruction(&Instruction::I32Const(prefix as i32 - POOLED_INTEGER_COUNT));
        function.instruction(&Instruction::I32Add);
        ordinal.store(function);
        let reference = schema.reference_type::<StringValue>(GcNullability::NonNullable);
        function.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(
            reference,
        ))));
        let _ = arrays.read(&table, ordinal, schema, function);
        function.likely(&Instruction::BrOnNonNull(0));

        let magnitude = schema.reserve_i64_local(function);
        integer.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        magnitude.store(function);
        let negative = schema.reserve_i32_local(function);
        negative.set_constant(0, function);
        let text = schema.reserve_gc_local(function).initialize(
            schema.helper_reference_on_stack(schema.call_helper(
                crate::runtime_helpers::IntegerDecimalStringArguments::new(magnitude, negative),
                self.runtime_helper_base()?,
                function,
            )),
            function,
        );
        schema.release_i32_local(negative, function);
        schema.release_i64_local(magnitude, function);
        arrays.write(
            &table,
            ordinal,
            GcOperand::nullable_reference(&text, schema),
            schema,
            function,
        );
        let result = text.load(schema, function);
        text.clear(function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(ordinal, function);
        table.clear(function);
        Ok(result)
    }

    /// Resolve a checked integer through the same lazy GC roots as its literal
    /// spelling. The pool can deduplicate spellings before the numeric seed,
    /// so adjacent integers can have different ordinal offsets.
    fn emit_pooled_integer_string(
        &self,
        integer: I32Local,
        function: &mut Function,
    ) -> Result<GcStackReference<StringValue>, EmitError> {
        let mut ranges: Vec<(i32, i32)> = Vec::new();
        for value in 0..POOLED_INTEGER_COUNT {
            let index = self.strings.pooled_string_index(&value.to_string())?;
            let PooledLiteralDomain::Runtime(index) = self.strings.pooled_string_domain(index)?
            else {
                return Err(EmitError::unsupported(
                    "small integer string requires a compiler-owned pool entry",
                ));
            };
            let offset = (index.ordinal() as i32).wrapping_sub(value);
            match ranges.last_mut() {
                Some((end, previous_offset)) if *previous_offset == offset => *end = value + 1,
                _ => ranges.push((value + 1, offset)),
            }
        }

        let schema = self.runtime_schema();
        let ordinal = schema.reserve_i32_local(function);
        function.instruction(&Instruction::Block(BlockType::Result(ValType::I32)));
        let ordinal_result = function.label_depth();
        for (end, offset) in ranges {
            if end < POOLED_INTEGER_COUNT {
                integer.load(function);
                function.instruction(&Instruction::I32Const(end));
                function.instruction(&Instruction::I32LtU);
                function.instruction(&Instruction::If(BlockType::Empty));
            }
            integer.load(function);
            function.instruction(&Instruction::I32Const(offset));
            function.instruction(&Instruction::I32Add);
            if end < POOLED_INTEGER_COUNT {
                function.branch_to_label(ordinal_result);
                function.instruction(&Instruction::End);
            }
        }
        function.instruction(&Instruction::End);
        ordinal.store(function);

        let reference = schema.reference_type::<StringValue>(GcNullability::NonNullable);
        function.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(
            reference,
        ))));
        let table = schema
            .reserve_gc_local(function)
            .initialize(schema.pooled_string_table_reference(function), function);
        let _ = schema
            .array_type::<PooledStringTable>()
            .read(&table, ordinal, schema, function);
        table.clear(function);
        function.likely(&Instruction::BrOnNonNull(0));
        let result =
            schema.resolve_pooled_string_reference(ordinal, self.runtime_helper_base()?, function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(ordinal, function);
        Ok(result)
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
