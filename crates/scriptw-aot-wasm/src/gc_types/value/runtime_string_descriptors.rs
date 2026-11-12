use super::*;
use crate::data::RuntimeStringDescriptors;

pub(crate) struct RuntimeStringDescriptorRowLocals {
    raw_hash: I64Local,
    byte_offset: I32Local,
    length: I32Local,
}
impl RuntimeStringDescriptorRowLocals {
    /// Caller bounds row by the sealed table count before reading. Both the
    /// linear-memory row and passive UTF16 span were checked at pool sealing.
    pub(crate) fn read(
        table: RuntimeStringDescriptors,
        schema: &RuntimeSchema,
        row: I32Local,
        function: &mut Function,
    ) -> Self {
        let result = Self {
            raw_hash: schema.reserve_i64_local(function),
            byte_offset: schema.reserve_i32_local(function),
            length: schema.reserve_i32_local(function),
        };
        row.load(function);
        function.instruction(&Instruction::I32Const(
            RuntimeStringDescriptors::ROW_BYTES as i32,
        ));
        function.instruction(&Instruction::I32Mul);
        function.instruction(&Instruction::I32Const(table.address() as i32));
        function.instruction(&Instruction::I32Add);
        function.instruction(&Instruction::I64Load(wasm_encoder::MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }));
        result.raw_hash.store(function);
        for (offset, output) in [(8, result.byte_offset), (12, result.length)] {
            row.load(function);
            function.instruction(&Instruction::I32Const(
                RuntimeStringDescriptors::ROW_BYTES as i32,
            ));
            function.instruction(&Instruction::I32Mul);
            function.instruction(&Instruction::I32Const(table.address() as i32));
            function.instruction(&Instruction::I32Add);
            function.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                offset,
                align: 0,
                memory_index: 0,
            }));
            output.store(function);
        }
        result
    }
    pub(crate) fn release(self, schema: &RuntimeSchema, function: &mut Function) {
        for local in [self.length, self.byte_offset] {
            schema.release_i32_local(local, function);
        }
        schema.release_i64_local(self.raw_hash, function);
    }

    /// Publish the checked span and its matching raw hash together. No caller
    /// can pair another string's contents with this descriptor's prehash.
    pub(crate) fn publish_string(
        &self,
        schema: &RuntimeSchema,
        slot: GcLocalSlot<CodeUnitArray>,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        self.byte_offset.load(function);
        self.length.load(function);
        function.instruction(&Instruction::ArrayNewData {
            array_type_index: schema.array_type::<CodeUnitArray>().type_index().raw(),
            array_data_index: crate::data::PooledCodeUnits::DATA_SEGMENT.index(),
        });
        let units = slot.initialize(GcStackReference::new(), function);
        let result = schema.struct_type::<StringValue>().construct(
            (
                GcOperand::reference(&units, schema),
                GcOperand::i64_local(self.raw_hash),
                GcOperand::i32(-1),
            ),
            function,
        );
        units.clear(function);
        result
    }
}
