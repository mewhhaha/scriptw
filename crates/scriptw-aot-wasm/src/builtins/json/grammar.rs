//! JSON grammar reads UTF-16 units; it never invokes a JavaScript parser.
use super::parse_cursor::{json_cursor_digit, JsonCursor};
use super::*;
impl FunctionBuilder<'_> {
    pub(super) fn emit_json_bad_text(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_json_error(realm, NativeErrorKind::SyntaxError, message, f)
    }

    /// Verifies `cursor.units[at + 1 .. at + 1 + tail.len()] == tail` after
    /// `units[at]` already matched the literal's first byte, then advances past
    /// the entire literal.
    fn emit_json_expect_literal_tail(
        &mut self,
        cursor: &JsonCursor<'_>,
        tail: &[u8],
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let total_len = (1 + tail.len()) as i32;
        cursor.end.load(f);
        cursor.at.load(f);
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(total_len));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_bad_text(realm, message, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let pos = s.reserve_i32_local(f);
        let mismatch = s.reserve_i32_local(f);
        mismatch.set_constant(0, f);
        for (offset, &byte) in tail.iter().enumerate() {
            cursor.at.load(f);
            f.instruction(&Instruction::I32Const((offset + 1) as i32));
            f.instruction(&Instruction::I32Add);
            pos.store(f);
            s.array_type::<CodeUnitArray>()
                .read(&cursor.units, pos, s, f);
            f.instruction(&Instruction::I32Const(i32::from(byte)));
            f.instruction(&Instruction::I32Ne);
            mismatch.load(f);
            f.instruction(&Instruction::I32Or);
            mismatch.store(f);
        }
        s.release_i32_local(pos, f);
        mismatch.load(f);
        s.release_i32_local(mismatch, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_bad_text(realm, message, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.advance_by(total_len, f);
        Ok(())
    }

    pub(super) fn emit_json_parse_primitive(
        &mut self,
        cursor: &JsonCursor<'_>,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let unit = s.reserve_i32_local(f);
        cursor.peek(unit, s, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(34));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let string = self.emit_json_parse_string(cursor, realm, message, f)?;
        out.set_reference(&string, s, f);
        string.clear(f);
        f.instruction(&Instruction::Else);
        json_cursor_digit(unit, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(45));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_parse_number(cursor, realm, message, out, f)?;
        f.instruction(&Instruction::Else);
        unit.load(f);
        f.instruction(&Instruction::I32Const(i32::from(b't')));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_expect_literal_tail(cursor, b"rue", realm, message, f)?;
        out.set_scalar(ScalarValue::Boolean(true), f);
        f.instruction(&Instruction::Else);
        unit.load(f);
        f.instruction(&Instruction::I32Const(i32::from(b'f')));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_expect_literal_tail(cursor, b"alse", realm, message, f)?;
        out.set_scalar(ScalarValue::Boolean(false), f);
        f.instruction(&Instruction::Else);
        unit.load(f);
        f.instruction(&Instruction::I32Const(i32::from(b'n')));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_expect_literal_tail(cursor, b"ull", realm, message, f)?;
        out.set_scalar(ScalarValue::Null, f);
        f.instruction(&Instruction::Else);
        self.emit_json_bad_text(realm, message, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(unit, f);
        Ok(())
    }
}
