//! Grow only unpublished UTF-16 storage; never mutate a published StringValue.
//! Appending at each original concatenation point preserves getter/coercion and
//! overflow order without repeatedly copying the complete accumulated prefix.
use super::*;
use crate::gc_types::{RuntimeSchema, StringConstruction, StringValue, StringValueSchema};

#[must_use]
pub(super) struct ArrayStringBuffer {
    construction: StringConstruction,
    length: I32Local,
    capacity: I32Local,
}

impl ArrayStringBuffer {
    pub(super) fn new(schema: &RuntimeSchema, f: &mut Function) -> Self {
        let length = schema.reserve_i32_local(f);
        let capacity = schema.reserve_i32_local(f);
        length.set_constant(0, f);
        capacity.set_constant(0, f);
        let construction =
            StringConstruction::allocate(schema, schema.reserve_gc_local(f), capacity, f);
        Self {
            construction,
            length,
            capacity,
        }
    }

    /// Leaves 1 when appending `part` would make the result longer than
    /// `MAX_STRING_LENGTH`; the caller throws the RangeError before
    /// appending, ahead of any later observable element read.
    pub(super) fn append_exceeds_limit_i32(
        &self,
        part: &GcLocal<StringValue>,
        schema: &RuntimeSchema,
        f: &mut Function,
    ) {
        let units = schema.reserve_gc_local(f).initialize(
            schema
                .field(StringValueSchema::CODE_UNITS)
                .read(part, schema, f)
                .reference(),
            f,
        );
        self.length.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        schema
            .array_type::<crate::gc_types::CodeUnitArray>()
            .length(&units, schema, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
        f.instruction(&Instruction::I64GtU);
        units.clear(f);
    }

    pub(super) fn append(
        &self,
        part: &GcLocal<StringValue>,
        schema: &RuntimeSchema,
        f: &mut Function,
    ) {
        let units = schema.reserve_gc_local(f).initialize(
            schema
                .field(StringValueSchema::CODE_UNITS)
                .read(part, schema, f)
                .reference(),
            f,
        );
        let part_length = schema.reserve_i32_local(f);
        let required = schema.reserve_i32_local(f);
        schema
            .array_type::<crate::gc_types::CodeUnitArray>()
            .length(&units, schema, f);
        part_length.store(f);
        self.length.load(f);
        part_length.load(f);
        f.instruction(&Instruction::I32Add);
        required.store(f);
        required.load(f);
        self.length.load(f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        // Unreachable: callers check `append_exceeds_limit_i32` first.
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        required.load(f);
        self.capacity.load(f);
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        let grown = schema.reserve_i32_local(f);
        self.capacity.load(f);
        f.instruction(&Instruction::I32Const(2));
        f.instruction(&Instruction::I32Mul);
        grown.store(f);
        grown.load(f);
        self.capacity.load(f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        // At the final Wasm32 capacity boundary, grow exactly as required.
        required.load(f);
        grown.store(f);
        f.instruction(&Instruction::End);
        grown.load(f);
        f.instruction(&Instruction::I32Const(16));
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        grown.set_constant(16, f);
        f.instruction(&Instruction::End);
        grown.load(f);
        required.load(f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        required.load(f);
        grown.store(f);
        f.instruction(&Instruction::End);
        // Never reserve past the longest String a join may produce: the
        // caller checked `required` against it, and a doubled backing above
        // it could exhaust the heap a result within the limit fits.
        grown.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
        f.instruction(&Instruction::I64GtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::I32Const(
            i32::try_from(crate::gc_types::MAX_STRING_LENGTH).expect("the String limit is an i32"),
        ));
        grown.store(f);
        f.instruction(&Instruction::End);
        self.construction.grow(grown, self.length, schema, f);
        grown.load(f);
        self.capacity.store(f);
        schema.release_i32_local(grown, f);
        f.instruction(&Instruction::End);
        self.construction
            .copy_units_from(self.length, &units, part_length, schema, f);
        required.load(f);
        self.length.store(f);
        schema.release_i32_local(required, f);
        schema.release_i32_local(part_length, f);
        units.clear(f);
    }

    pub(super) fn finish(self, schema: &RuntimeSchema, f: &mut Function) -> GcLocal<StringValue> {
        let result = schema
            .reserve_gc_local(f)
            .initialize(self.construction.publish_prefix(self.length, schema, f), f);
        schema.release_i32_local(self.capacity, f);
        schema.release_i32_local(self.length, f);
        result
    }
}
