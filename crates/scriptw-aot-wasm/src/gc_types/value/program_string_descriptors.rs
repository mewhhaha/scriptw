use super::*;
use crate::data::{ProgramStringDescriptorImage, ProgramStringDescriptorWord};

/// Only this read-only wrapper retains the published immutable descriptor bytes.
pub(crate) struct ProgramStringDescriptorTable {
    bytes: GcLocal<ImmutableByteArray>,
    count: u32,
}

/// Bounds read from a compiler-validated program descriptor. Construction of
/// String values never accepts unrelated integer locals as passive spans.
struct ProgramPooledCodeUnitLocals {
    byte_offset: I32Local,
    length: I32Local,
}

pub(crate) struct ProgramStringDescriptorRowLocals {
    literal_index: I32Local,
    literal: ProgramPooledCodeUnitLocals,
}

impl ProgramStringDescriptorTable {
    pub(crate) fn from_image(
        schema: &RuntimeSchema,
        image: &ProgramStringDescriptorImage,
        function: &mut Function,
    ) -> Self {
        image.emit_bounds(function);
        function.instruction(&Instruction::ArrayNewData {
            array_type_index: schema.array_type::<ImmutableByteArray>().type_index().raw(),
            array_data_index: ProgramStringDescriptorImage::DATA_SEGMENT,
        });
        Self {
            bytes: schema
                .reserve_gc_local(function)
                .initialize(GcStackReference::new(), function),
            count: image.count(),
        }
    }

    pub(crate) fn count(&self) -> u32 {
        self.count
    }

    fn load_word(
        &self,
        schema: &RuntimeSchema,
        row: I32Local,
        word: ProgramStringDescriptorWord,
        output: I32Local,
        function: &mut Function,
    ) {
        let index = schema.reserve_i32_local(function);
        let byte = schema.reserve_i32_local(function);
        for lane in 0..4 {
            row.load(function);
            function.instruction(&Instruction::I32Const(
                ProgramStringDescriptorWord::ROW_BYTES as i32,
            ));
            function.instruction(&Instruction::I32Mul);
            function.instruction(&Instruction::I32Const((word.byte_offset() + lane) as i32));
            function.instruction(&Instruction::I32Add);
            index.store(function);
            schema
                .array_type::<ImmutableByteArray>()
                .read(&self.bytes, index, schema, function)
                .store(byte, function);
            byte.load(function);
            if lane != 0 {
                function.instruction(&Instruction::I32Const((lane * 8) as i32));
                function.instruction(&Instruction::I32Shl);
                function.instruction(&Instruction::I32Or);
            }
        }
        output.store(function);
        schema.release_i32_local(byte, function);
        schema.release_i32_local(index, function);
    }

    /// Caller emits one row < count loop guard. The validated image bounds all
    /// row multiplication, byte accesses, P-local indexes and UTF-16 spans.
    pub(crate) fn read_row(
        &self,
        schema: &RuntimeSchema,
        row: I32Local,
        function: &mut Function,
    ) -> ProgramStringDescriptorRowLocals {
        let result = ProgramStringDescriptorRowLocals {
            literal_index: schema.reserve_i32_local(function),
            literal: ProgramPooledCodeUnitLocals {
                byte_offset: schema.reserve_i32_local(function),
                length: schema.reserve_i32_local(function),
            },
        };
        for (word, output) in [
            (
                ProgramStringDescriptorWord::LiteralIndex,
                result.literal_index,
            ),
            (
                ProgramStringDescriptorWord::Utf16ByteOffset,
                result.literal.byte_offset,
            ),
            (
                ProgramStringDescriptorWord::UnitLength,
                result.literal.length,
            ),
        ] {
            self.load_word(schema, row, word, output, function);
        }
        result
    }

    pub(crate) fn clear(self, function: &mut Function) {
        self.bytes.clear(function);
    }
}

impl ProgramStringDescriptorRowLocals {
    pub(crate) fn literal_index(&self) -> I32Local {
        self.literal_index
    }

    pub(crate) fn release(self, schema: &RuntimeSchema, function: &mut Function) {
        for local in [
            self.literal.length,
            self.literal.byte_offset,
            self.literal_index,
        ] {
            schema.release_i32_local(local, function);
        }
    }
}

impl StringConstruction {
    pub(crate) fn from_program_pooled_row(
        schema: &RuntimeSchema,
        slot: GcLocalSlot<CodeUnitArray>,
        row: &ProgramStringDescriptorRowLocals,
        function: &mut Function,
    ) -> Self {
        row.literal.byte_offset.load(function);
        row.literal.length.load(function);
        function.instruction(&Instruction::ArrayNewData {
            array_type_index: schema.array_type::<CodeUnitArray>().type_index().raw(),
            array_data_index: crate::data::PooledCodeUnits::DATA_SEGMENT.index(),
        });
        Self {
            units: slot.initialize(GcStackReference::new(), function),
        }
    }
}
