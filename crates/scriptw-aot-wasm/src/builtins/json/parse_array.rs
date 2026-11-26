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
        // The private frame owns this unpublished Array with contiguous elements
        // 0..ordinal, so COUNT == ordinal and slot `ordinal` is unoccupied.
        // Fast inline append within existing PackedValues or PackedNumbers
        // capacity avoids the helper call; growth, promotion, and cold modes
        // fall through to `emit_array_indexed_publish_default_data`.
        let published_fast = s.reserve_i32_local(f);
        published_fast.set_constant(0, f);
        s.array_storage_mode_is(&self.array, ArrayIndexedStorageMode::PackedValues, f);
        builder.open_frame(ControlFrameKind::If, f);
        let values = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::VALUES)
                .read(&self.array, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        next.load(f);
        s.array_type::<PackedValueTable>().length(&values, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LeU);
        builder.open_frame(ControlFrameKind::If, f);
        let slot = s.reserve_i32_local(f);
        self.ordinal.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        let stored = s
            .reserve_gc_local::<StoredValue, Nullable>(f)
            .initialize_null(s, f);
        element.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        builder.open_frame(ControlFrameKind::If, f);
        stored.replace(s.stored_undefined(f).nullable(), f);
        f.instruction(&Instruction::Else);
        stored.replace(
            s.struct_type::<StoredValue>()
                .from_value(element, f)
                .nullable(),
            f,
        );
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.array_type::<PackedValueTable>().write(
            &values,
            slot,
            GcOperand::reference(&stored, s),
            s,
            f,
        );
        stored.clear(f);
        s.release_i32_local(slot, f);
        s.field(ArrayObjectSchema::COUNT)
            .write(&self.array, GcOperand::i64_local(next), s, f);
        s.field(ArrayObjectSchema::LENGTH)
            .write(&self.array, GcOperand::i64_local(next), s, f);
        published_fast.set_constant(1, f);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        values.clear(f);
        f.instruction(&Instruction::Else);
        s.array_storage_mode_is(&self.array, ArrayIndexedStorageMode::PackedNumbers, f);
        element.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        builder.open_frame(ControlFrameKind::If, f);
        let numbers = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::NUMBERS)
                .read(&self.array, s, f)
                .reference(),
            f,
        );
        next.load(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LeU);
        builder.open_frame(ControlFrameKind::If, f);
        let presence = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::NUMBER_PRESENT)
                .read(&self.array, s, f)
                .reference(),
            f,
        );
        let slot = s.reserve_i32_local(f);
        let word = s.reserve_i32_local(f);
        let mask = s.reserve_i64_local(f);
        let bits = s.reserve_i64_local(f);
        self.ordinal.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<NumberBitsI64Array>().write(
            &numbers,
            slot,
            GcOperand::i64_local(element.scalar()),
            s,
            f,
        );
        slot.load(f);
        f.instruction(&Instruction::I32Const(6));
        f.instruction(&Instruction::I32ShrU);
        word.store(f);
        f.instruction(&Instruction::I64Const(1));
        slot.load(f);
        f.instruction(&Instruction::I32Const(63));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Shl);
        mask.store(f);
        s.array_type::<NumberPresenceU64Array>()
            .read(&presence, word, s, f)
            .store_i64(bits, f);
        bits.load(f);
        mask.load(f);
        f.instruction(&Instruction::I64Or);
        bits.store(f);
        s.array_type::<NumberPresenceU64Array>().write(
            &presence,
            word,
            GcOperand::i64_local(bits),
            s,
            f,
        );
        s.release_i64_local(bits, f);
        s.release_i64_local(mask, f);
        s.release_i32_local(word, f);
        s.release_i32_local(slot, f);
        presence.clear(f);
        s.field(ArrayObjectSchema::COUNT)
            .write(&self.array, GcOperand::i64_local(next), s, f);
        s.field(ArrayObjectSchema::LENGTH)
            .write(&self.array, GcOperand::i64_local(next), s, f);
        published_fast.set_constant(1, f);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        numbers.clear(f);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        published_fast.load(f);
        f.instruction(&Instruction::I32Eqz);
        builder.open_frame(ControlFrameKind::If, f);
        // The private frame supplies an in-range unique Array ordinal. Storage
        // publication never substitutes for parser state or length validation.
        builder.emit_array_indexed_publish_default_data(&self.array, self.ordinal, element, f)?;
        s.field(ArrayObjectSchema::LENGTH)
            .write(&self.array, GcOperand::i64_local(next), s, f);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(published_fast, f);
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
