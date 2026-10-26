//! Exact integer decimal publication shared by numeric safe integers and
//! private unsigned accumulators. No binary64 rounding enters this helper.
use super::*;
use crate::runtime_helpers::HelperParameters;

impl FunctionBuilder<'_> {
    pub(crate) fn compile_integer_decimal_string_helper(&mut self) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::IntegerDecimalString);
        let p = self
            .helper_parameters::<crate::runtime_helpers::IntegerDecimalStringParameters>(&mut body);
        let f = &mut body;
        let s = self.runtime_schema();
        p.negative.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        let remaining = s.reserve_i64_local(f);
        let count = s.reserve_i32_local(f);
        let cursor = s.reserve_i32_local(f);
        let unit = s.reserve_i32_local(f);
        p.magnitude.load(f);
        remaining.store(f);
        p.negative.load(f);
        count.store(f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        count.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        count.store(f);
        remaining.load(f);
        f.instruction(&Instruction::I64Const(10));
        f.instruction(&Instruction::I64DivU);
        remaining.store(f);
        remaining.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        let text =
            crate::gc_types::StringConstruction::allocate(s, s.reserve_gc_local(f), count, f);
        p.magnitude.load(f);
        remaining.store(f);
        count.load(f);
        cursor.store(f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        cursor.store(f);
        remaining.load(f);
        f.instruction(&Instruction::I64Const(10));
        f.instruction(&Instruction::I64RemU);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::I32Const(48));
        f.instruction(&Instruction::I32Add);
        unit.store(f);
        text.write(cursor, unit, s, f);
        remaining.load(f);
        f.instruction(&Instruction::I64Const(10));
        f.instruction(&Instruction::I64DivU);
        remaining.store(f);
        cursor.load(f);
        p.negative.load(f);
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        p.negative.load(f);
        self.open_frame(ControlFrameKind::If, f);
        cursor.set_constant(0, f);
        unit.set_constant(45, f);
        text.write(cursor, unit, s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let _ = text.publish(s, f);
        s.release_i32_local(unit, f);
        s.release_i32_local(cursor, f);
        s.release_i32_local(count, f);
        s.release_i64_local(remaining, f);
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }
}
