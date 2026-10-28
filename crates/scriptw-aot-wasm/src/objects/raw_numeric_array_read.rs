//! A raw Number key can read a concrete Array's present packed own data without
//! allocating its decimal property name. Every other case keeps the original
//! lookup, receiver and raw key for the caller's complete property operation.
use super::*;
use crate::gc_types::{ArrayObject, GcNullability};

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
        lookup.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32And);
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
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Trunc);
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
        found
    }
}
