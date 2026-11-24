use super::*;
use crate::data::{ProgramStringDescriptorImage, ProgramStringDescriptorWord};

/// Only this read-only wrapper retains the published immutable descriptor
/// bytes until ProgramInstance publication takes them as LITERAL_DESCRIPTORS.
pub(crate) struct ProgramStringDescriptorTable {
    bytes: GcLocal<ImmutableByteArray>,
}

/// Bounds read from a compiler-validated program descriptor. Construction of
/// String values never accepts unrelated integer locals as passive spans.
struct ProgramPooledCodeUnitLocals {
    byte_offset: I32Local,
    length: I32Local,
}

/// One literal's row, read from the actual P's published descriptor bytes.
pub(crate) struct ProgramStringDescriptorRowLocals {
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
        }
    }

    pub(crate) fn bytes(&self) -> &GcLocal<ImmutableByteArray> {
        &self.bytes
    }

    pub(crate) fn clear(self, function: &mut Function) {
        self.bytes.clear(function);
    }
}

impl ProgramStringDescriptorRowLocals {
    /// Row `ordinal` of a P's LITERAL_DESCRIPTORS. The caller has bounded the
    /// ordinal by that P's literal table, whose length is the row count; each
    /// byte read is also bounds-checked by `array.get_u` itself.
    pub(crate) fn read(
        schema: &RuntimeSchema,
        descriptors: &GcLocal<ImmutableByteArray>,
        ordinal: I32Local,
        function: &mut Function,
    ) -> Self {
        let result = Self {
            literal: ProgramPooledCodeUnitLocals {
                byte_offset: schema.reserve_i32_local(function),
                length: schema.reserve_i32_local(function),
            },
        };
        let index = schema.reserve_i32_local(function);
        let byte = schema.reserve_i32_local(function);
        for (word, output) in [
            (
                ProgramStringDescriptorWord::Utf16ByteOffset,
                result.literal.byte_offset,
            ),
            (
                ProgramStringDescriptorWord::UnitLength,
                result.literal.length,
            ),
        ] {
            for lane in 0..4 {
                ordinal.load(function);
                function.instruction(&Instruction::I32Const(
                    ProgramStringDescriptorWord::ROW_BYTES as i32,
                ));
                function.instruction(&Instruction::I32Mul);
                function.instruction(&Instruction::I32Const((word.byte_offset() + lane) as i32));
                function.instruction(&Instruction::I32Add);
                index.store(function);
                schema
                    .array_type::<ImmutableByteArray>()
                    .read(descriptors, index, schema, function)
                    .store(byte, function);
                byte.load(function);
                if lane != 0 {
                    function.instruction(&Instruction::I32Const((lane * 8) as i32));
                    function.instruction(&Instruction::I32Shl);
                    function.instruction(&Instruction::I32Or);
                }
            }
            output.store(function);
        }
        schema.release_i32_local(byte, function);
        schema.release_i32_local(index, function);
        result
    }

    pub(crate) fn release(self, schema: &RuntimeSchema, function: &mut Function) {
        for local in [self.literal.length, self.literal.byte_offset] {
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
