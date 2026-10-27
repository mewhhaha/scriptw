use super::*;
use crate::data::RuntimeStringDescriptors;

pub(crate) struct RuntimeStringDescriptorRowLocals {
    ordinal: I32Local,
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
            ordinal: schema.reserve_i32_local(function),
            byte_offset: schema.reserve_i32_local(function),
            length: schema.reserve_i32_local(function),
        };
        for (offset, output) in [
            (0, result.ordinal),
            (4, result.byte_offset),
            (8, result.length),
        ] {
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
        for local in [self.length, self.byte_offset, self.ordinal] {
            schema.release_i32_local(local, function);
        }
    }
}
impl StringConstruction {
    pub(crate) fn from_runtime_pooled_row(
        schema: &RuntimeSchema,
        slot: GcLocalSlot<CodeUnitArray>,
        row: &RuntimeStringDescriptorRowLocals,
        function: &mut Function,
    ) -> Self {
        row.byte_offset.load(function);
        row.length.load(function);
        function.instruction(&Instruction::ArrayNewData {
            array_type_index: schema.array_type::<CodeUnitArray>().type_index().raw(),
            array_data_index: crate::data::PooledCodeUnits::DATA_SEGMENT.index(),
        });
        Self {
            units: slot.initialize(GcStackReference::new(), function),
        }
    }
}
