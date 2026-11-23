//! Fresh JSON frames publish unique Array data properties without key conversion.
use super::*;

/// No constructor accepts a JavaScript Array. The parser's private frame owns
/// its unpublished container and next unique ordinal; consuming append advances
/// that ordinal after successful publication, so this insertion cannot repeat.
struct FreshJsonArrayElement<'a> {
    frame: &'a GcLocal<JsonParseFrame>,
    array: GcLocal<ArrayObject>,
    ordinal: I64Local,
}
impl<'a> FreshJsonArrayElement<'a> {
    fn from_frame(
        frame: &'a GcLocal<JsonParseFrame>,
        ordinal: I64Local,
        builder: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> Self {
        let s = builder.runtime_schema();
        let state = s.reserve_i32_local(f);
        s.struct_type::<JsonParseFrame>()
            .field(JsonParseFrameSchema::STATE)
            .read(frame, s, f)
            .store(state, f);
        state.load(f);
        f.instruction(&Instruction::I32Const(
            JsonParseFrameState::ArrayValue.wire_code(),
        ));
        f.instruction(&Instruction::I32Ne);
        builder.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(state, f);
        s.struct_type::<JsonParseFrame>()
            .field(JsonParseFrameSchema::INDEX)
            .read(frame, s, f)
            .store_i64(ordinal, f);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonParseFrame>()
                .field(JsonParseFrameSchema::CONTAINER)
                .read(frame, s, f)
                .reference(),
            f,
        );
        let container = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &container, s, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(container.cast_reference::<ArrayObject>(s, f), f);
        container.clear(f);
        stored.clear(f);
        Self {
            frame,
            array,
            ordinal,
        }
    }
    fn append(
        self,
        element: &ValueLocals,
        builder: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = builder.runtime_schema();
        let next = s.reserve_i64_local(f);
        self.ordinal.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        next.store(f);
        self.ordinal.load(f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        f.instruction(&Instruction::I64LtU);
        builder.open_frame(ControlFrameKind::If, f);
        // The private frame supplies an in-range unique Array ordinal. Storage
        // publication never substitutes for parser state or length validation.
        builder.emit_array_indexed_publish_default_data(&self.array, self.ordinal, element, f)?;
        s.struct_type::<ArrayObject>()
            .field(ArrayObjectSchema::LENGTH)
            .write(&self.array, GcOperand::i64_local(next), s, f);
        f.instruction(&Instruction::Else);
        // 2^32-1 and higher are ordinary String properties, not Array indices.
        // Retain the original generic definition path without narrowing them.
        let container = s.reserve_value_local(f);
        container.set_reference(&self.array, s, f);
        let key = builder.emit_json_index_key(self.ordinal, f)?;
        builder.emit_json_define(&container, &key, element, false, f)?;
        key.clear(f);
        container.clear(f);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.struct_type::<JsonParseFrame>()
            .field(JsonParseFrameSchema::INDEX)
            .write(self.frame, GcOperand::i64_local(next), s, f);
        s.release_i64_local(next, f);
        self.array.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_json_parse_array_append(
        &mut self,
        frame: &GcLocal<JsonParseFrame>,
        value: &ValueLocals,
        ordinal: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        FreshJsonArrayElement::from_frame(frame, ordinal, self, f).append(value, self, f)
    }
}
