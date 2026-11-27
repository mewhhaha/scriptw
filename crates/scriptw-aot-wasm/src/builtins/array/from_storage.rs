//! Reserve storage for the private, bounded array-like result of Array.from.
use super::*;
use crate::gc_types::{
    ArrayIndexedStorageMode, NumberBitsI64Array, NumberPresenceU64Array, PackedValueTable,
};

pub(super) const PACKED_LENGTH_LIMIT: i64 = 65_536;

impl FunctionBuilder<'_> {
    /// Called before the first definition on a fresh intrinsic Array. The
    /// first mapped value selects Number or general value storage, so an
    /// all-Number result keeps unboxed elements. Common publication handles
    /// promotion if a later value needs the general representation.
    pub(super) fn emit_array_from_reserve_packed(
        &mut self,
        target: &ValueLocals,
        length: I64Local,
        first: &ValueLocals,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let array = s
            .reserve_gc_local(f)
            .initialize(target.cast_reference::<ArrayObject>(s, f), f);
        let capacity = s.reserve_i32_local(f);
        // Round the nonzero length up to a power of two for the existing
        // packed storage invariant: 1 << (32 - clz(length - 1)).
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Const(32));
        length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Clz);
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Shl);
        capacity.store(f);
        first.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let words = s.reserve_i32_local(f);
        capacity.load(f);
        f.instruction(&Instruction::I32Const(63));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Const(6));
        f.instruction(&Instruction::I32ShrU);
        words.store(f);
        let numbers = s.reserve_gc_local(f).initialize(
            s.array_type::<NumberBitsI64Array>()
                .filled(GcOperand::i64(0), capacity, f),
            f,
        );
        let presence = s.reserve_gc_local(f).initialize(
            s.array_type::<NumberPresenceU64Array>()
                .filled(GcOperand::i64(0), words, f),
            f,
        );
        s.field(ArrayObjectSchema::NUMBERS)
            .write(&array, GcOperand::reference(&numbers, s), s, f);
        s.field(ArrayObjectSchema::NUMBER_PRESENT).write(
            &array,
            GcOperand::reference(&presence, s),
            s,
            f,
        );
        presence.clear(f);
        numbers.clear(f);
        s.release_i32_local(words, f);
        f.instruction(&Instruction::Else);
        let values = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>()
                .filled(GcOperand::null(s), capacity, f),
            f,
        );
        s.field(ArrayObjectSchema::VALUES).write(
            &array,
            GcOperand::nullable_reference(&values, s),
            s,
            f,
        );
        s.set_array_storage_mode(&array, ArrayIndexedStorageMode::PackedValues, f);
        values.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(capacity, f);
        array.clear(f);
    }
}
