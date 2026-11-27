use super::*;

enum StringWellFormedOperation {
    IsWellFormed,
    ToWellFormed,
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_string_is_well_formed_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_well_formed(StringWellFormedOperation::IsWellFormed, f)
    }
    pub(crate) fn emit_string_to_well_formed_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_well_formed(StringWellFormedOperation::ToWellFormed, f)
    }

    fn emit_native_string_well_formed(
        &mut self,
        operation: StringWellFormedOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f, |b, string, output, _, f| {
            let s = b.runtime_schema();
            let length = s.reserve_i64_local(f);
            let allocation_length = s.reserve_i32_local(f);
            let index = s.reserve_i64_local(f);
            let ordinal = s.reserve_i32_local(f);
            let next_index = s.reserve_i64_local(f);
            let unit = s.reserve_i32_local(f);
            let following = s.reserve_i32_local(f);
            let pair = s.reserve_i32_local(f);
            let valid = s.reserve_i32_local(f);
            let result = s.reserve_value_local(f);
            b.emit_native_gc_string_length(string, length, f);
            length.load(f);
            f.instruction(&Instruction::I32WrapI64);
            allocation_length.store(f);
            f.instruction(&Instruction::I32Const(1));
            valid.store(f);
            f.instruction(&Instruction::I64Const(0));
            index.store(f);
            let done = b.open_frame(ControlFrameKind::Block, f);
            let next = b.open_frame(ControlFrameKind::Loop, f);
            index.load(f);
            length.load(f);
            f.instruction(&Instruction::I64GeU);
            b.emit_branch_if_to_target(done, f);
            b.emit_gc_string_code_unit_i32(string, index, f);
            unit.store(f);
            f.instruction(&Instruction::I32Const(0));
            pair.store(f);
            unit.load(f);
            f.instruction(&Instruction::I32Const(0xd800));
            f.instruction(&Instruction::I32GeU);
            unit.load(f);
            f.instruction(&Instruction::I32Const(0xdbff));
            f.instruction(&Instruction::I32LeU);
            f.instruction(&Instruction::I32And);
            b.open_frame(ControlFrameKind::If, f);
            index.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Add);
            next_index.store(f);
            next_index.load(f);
            length.load(f);
            f.instruction(&Instruction::I64LtU);
            b.open_frame(ControlFrameKind::If, f);
            b.emit_gc_string_code_unit_i32(string, next_index, f);
            following.store(f);
            following.load(f);
            f.instruction(&Instruction::I32Const(0xdc00));
            f.instruction(&Instruction::I32GeU);
            following.load(f);
            f.instruction(&Instruction::I32Const(0xdfff));
            f.instruction(&Instruction::I32LeU);
            f.instruction(&Instruction::I32And);
            pair.store(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            pair.load(f);
            f.instruction(&Instruction::I32Eqz);
            b.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I32Const(0));
            valid.store(f);
            b.emit_branch_to_target(done, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::Else);
            unit.load(f);
            f.instruction(&Instruction::I32Const(0xdc00));
            f.instruction(&Instruction::I32GeU);
            unit.load(f);
            f.instruction(&Instruction::I32Const(0xdfff));
            f.instruction(&Instruction::I32LeU);
            f.instruction(&Instruction::I32And);
            b.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I32Const(0));
            valid.store(f);
            b.emit_branch_to_target(done, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            pair.load(f);
            b.open_frame(ControlFrameKind::If, f);
            next_index.load(f);
            index.store(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            index.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Add);
            index.store(f);
            b.emit_branch_to_target(next, f);
            b.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            b.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            match operation {
                StringWellFormedOperation::IsWellFormed => result.set_boolean(valid, f),
                StringWellFormedOperation::ToWellFormed => {
                    valid.load(f);
                    b.open_frame(ControlFrameKind::If, f);
                    result.set_reference(string, s, f);
                    f.instruction(&Instruction::Else);
                    let construction = StringConstruction::allocate(
                        s,
                        s.reserve_gc_local(f),
                        allocation_length,
                        f,
                    );
                    f.instruction(&Instruction::I64Const(0));
                    index.store(f);
                    let repair_done = b.open_frame(ControlFrameKind::Block, f);
                    let repair_next = b.open_frame(ControlFrameKind::Loop, f);
                    index.load(f);
                    length.load(f);
                    f.instruction(&Instruction::I64GeU);
                    b.emit_branch_if_to_target(repair_done, f);
                    b.emit_gc_string_code_unit_i32(string, index, f);
                    unit.store(f);
                    f.instruction(&Instruction::I32Const(0));
                    pair.store(f);
                    unit.load(f);
                    f.instruction(&Instruction::I32Const(0xd800));
                    f.instruction(&Instruction::I32GeU);
                    unit.load(f);
                    f.instruction(&Instruction::I32Const(0xdbff));
                    f.instruction(&Instruction::I32LeU);
                    f.instruction(&Instruction::I32And);
                    b.open_frame(ControlFrameKind::If, f);
                    index.load(f);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Add);
                    next_index.store(f);
                    next_index.load(f);
                    length.load(f);
                    f.instruction(&Instruction::I64LtU);
                    b.open_frame(ControlFrameKind::If, f);
                    b.emit_gc_string_code_unit_i32(string, next_index, f);
                    following.store(f);
                    following.load(f);
                    f.instruction(&Instruction::I32Const(0xdc00));
                    f.instruction(&Instruction::I32GeU);
                    following.load(f);
                    f.instruction(&Instruction::I32Const(0xdfff));
                    f.instruction(&Instruction::I32LeU);
                    f.instruction(&Instruction::I32And);
                    pair.store(f);
                    b.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    pair.load(f);
                    f.instruction(&Instruction::I32Eqz);
                    b.open_frame(ControlFrameKind::If, f);
                    f.instruction(&Instruction::I32Const(0xfffd));
                    unit.store(f);
                    b.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    f.instruction(&Instruction::Else);
                    unit.load(f);
                    f.instruction(&Instruction::I32Const(0xdc00));
                    f.instruction(&Instruction::I32GeU);
                    unit.load(f);
                    f.instruction(&Instruction::I32Const(0xdfff));
                    f.instruction(&Instruction::I32LeU);
                    f.instruction(&Instruction::I32And);
                    b.open_frame(ControlFrameKind::If, f);
                    f.instruction(&Instruction::I32Const(0xfffd));
                    unit.store(f);
                    b.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    b.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    index.load(f);
                    f.instruction(&Instruction::I32WrapI64);
                    ordinal.store(f);
                    construction.write(ordinal, unit, s, f);
                    pair.load(f);
                    b.open_frame(ControlFrameKind::If, f);
                    next_index.load(f);
                    f.instruction(&Instruction::I32WrapI64);
                    ordinal.store(f);
                    construction.write(ordinal, following, s, f);
                    next_index.load(f);
                    index.store(f);
                    b.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    index.load(f);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Add);
                    index.store(f);
                    b.emit_branch_to_target(repair_next, f);
                    b.pop_control(ControlFrameKind::Loop);
                    f.instruction(&Instruction::End);
                    b.pop_control(ControlFrameKind::Block);
                    f.instruction(&Instruction::End);
                    let repaired = s
                        .reserve_gc_local(f)
                        .initialize(construction.publish(s, f), f);
                    result.set_reference(&repaired, s, f);
                    repaired.clear(f);
                    b.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                }
            }
            output.set_normal(&result, f);
            result.clear(f);
            s.release_i32_local(valid, f);
            s.release_i32_local(pair, f);
            s.release_i32_local(following, f);
            s.release_i32_local(unit, f);
            s.release_i64_local(next_index, f);
            s.release_i32_local(ordinal, f);
            s.release_i64_local(index, f);
            s.release_i32_local(allocation_length, f);
            s.release_i64_local(length, f);
            Ok(())
        })
    }
}
