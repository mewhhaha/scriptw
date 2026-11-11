//! A small initial OwnKeys list is unique by construction. Preserve exactly
//! those String keys whose live descriptor existed, including nonenumerables.
use super::*;
use crate::gc_types::*;

impl FunctionBuilder<'_> {
    /// Called only after the first complete snapshot and before prototype
    /// traversal. Missing keys do not shadow inherited properties; this bitmap
    /// records the descriptor observations, never current property presence.
    pub(super) fn emit_for_in_materialize_initial_keys(
        &mut self,
        record: &GcLocal<ForInEnumerationRecord>,
        keys: &GcLocal<PropertyKeyTable>,
        nullable_visited: &GcLocal<SetObject, Nullable>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        nullable_visited.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let visited = self.emit_for_in_visited_keys(record, f)?;
        nullable_visited.replace(visited.set.load(s, f).nullable(), f);
        let bits = s.reserve_i64_local(f);
        s.field(ForInEnumerationRecordSchema::PREFIX_FOUND_BITS)
            .read(record, s, f)
            .store_i64(bits, f);
        let index = s.reserve_i32_local(f);
        index.set_constant(0, f);
        let length = s.reserve_i32_local(f);
        s.array_type::<PropertyKeyTable>().length(keys, s, f);
        length.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let seed = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, f);
        bits.load(f);
        f.instruction(&Instruction::I64Const(1));
        index.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Shl);
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        // Only the String descriptor-found branch sets a bit. Symbols never
        // enter the private canonical Set, even when sharing this snapshot.
        let key = self.emit_property_key_table_entry(keys, index, f)?;
        let string = s
            .reserve_gc_local(f)
            .initialize(key.value().cast_reference::<StringValue>(s, f), f);
        visited.insert(&string, self, f)?;
        string.clear(f);
        key.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(seed, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.field(ForInEnumerationRecordSchema::PREFIX_FOUND_BITS)
            .write(record, GcOperand::i64(0), s, f);
        s.release_i32_local(length, f);
        s.release_i32_local(index, f);
        s.release_i64_local(bits, f);
        visited.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
}
