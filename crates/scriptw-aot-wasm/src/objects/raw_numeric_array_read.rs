//! A raw Number key can read a concrete Array's present packed own data without
//! allocating its decimal property name. Every other case keeps the original
//! lookup, receiver and raw key for the caller's complete property operation.
//! The same key inside a primitive String reads its one code unit directly.
use super::*;
use crate::gc_types::{
    ArrayObject, CodeUnitArray, GcNullability, StringConstruction, StringValue, StringValueSchema,
};

impl FunctionBuilder<'_> {
    pub(crate) fn emit_raw_numeric_array_own_data_read(
        &mut self,
        lookup: &ValueLocals,
        raw_key: &ValueLocals,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let found = s.reserve_i32_local(f);
        found.set_constant(0, f);
        raw_key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Eq);
        // Most named reads (notably TypedArray length) cannot use this probe.
        // Avoid their otherwise redundant GC subtype test entirely.
        self.open_frame(ControlFrameKind::If, f);
        lookup.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        // Ordered bounds reject NaN and infinities before the integer cast.
        // Numeric -0 is index 0; the distinct String "-0" never enters this arm.
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Ge);
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(u32::MAX as f64)));
        f.instruction(&Instruction::F64Lt);
        f.instruction(&Instruction::I32And);
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        // An integral value equals its saturating i64 truncation converted
        // back; the bounds above make that exact, and no rounding operator
        // (a host call on the baseline x86-64 target) is needed.
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncSatF64S);
        f.instruction(&Instruction::F64ConvertI64S);
        f.instruction(&Instruction::F64Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let index = s.reserve_i64_local(f);
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncF64U);
        index.store(f);
        let array = s
            .reserve_gc_local(f)
            .initialize(lookup.cast_reference::<ArrayObject>(s, f), f);
        let storage = self.emit_array_index_storage(&array, f);
        s.array_storage_is_packed(&storage, f);
        self.open_frame(ControlFrameKind::If, f);
        let value = s.reserve_value_local(f);
        // The packed probe never allocates, invokes user code or materializes
        // descriptors. A present undefined is distinct from an absent slot.
        self.emit_packed_array_value(&storage, index, &value, found, f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        result.set_normal(&value, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        storage.clear(f);
        array.clear(f);
        s.release_i64_local(index, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        found
    }

    /// A raw Number key inside a primitive String reads that String's own
    /// index property, a String of the one code unit, without allocating the
    /// wrapper object ToObject would create or the decimal property name.
    /// The property is own data on every String object, so no other object
    /// or user code is consulted. Every other case is left to the caller.
    pub(crate) fn emit_raw_numeric_string_index_read(
        &mut self,
        lookup: &ValueLocals,
        raw_key: &ValueLocals,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let found = s.reserve_i32_local(f);
        found.set_constant(0, f);
        raw_key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Eq);
        lookup.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        // Ordered bounds reject NaN and infinities before the integer
        // conversion. Numeric -0 is index 0, as its property name is "0".
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Ge);
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(u32::MAX as f64)));
        f.instruction(&Instruction::F64Lt);
        f.instruction(&Instruction::I32And);
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        // An integral value equals its saturating i64 truncation converted
        // back; the bounds above make that exact, and no rounding operator
        // (a host call on the baseline x86-64 target) is needed.
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncSatF64S);
        f.instruction(&Instruction::F64ConvertI64S);
        f.instruction(&Instruction::F64Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let index = s.reserve_i32_local(f);
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I32TruncF64U);
        index.store(f);
        let string = s
            .reserve_gc_local(f)
            .initialize(lookup.cast_reference::<StringValue>(s, f), f);
        let units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(&string, s, f)
                .reference(),
            f,
        );
        string.clear(f);
        index.load(f);
        s.array_type::<CodeUnitArray>().length(&units, s, f);
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        let length = s.reserve_i32_local(f);
        let zero = s.reserve_i32_local(f);
        let unit = s.reserve_i32_local(f);
        length.set_constant(1, f);
        zero.set_constant(0, f);
        s.array_type::<CodeUnitArray>()
            .read(&units, index, s, f)
            .store(unit, f);
        let construction = StringConstruction::allocate(s, s.reserve_gc_local(f), length, f);
        construction.write(zero, unit, s, f);
        let character = s
            .reserve_gc_local(f)
            .initialize(construction.publish(s, f), f);
        let value = s.reserve_value_local(f);
        value.set_reference(&character, s, f);
        result.set_normal(&value, f);
        found.set_constant(1, f);
        value.clear(f);
        character.clear(f);
        s.release_i32_local(unit, f);
        s.release_i32_local(zero, f);
        s.release_i32_local(length, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        units.clear(f);
        s.release_i32_local(index, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        found
    }
}
