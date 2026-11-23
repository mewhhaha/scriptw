//! JSON grammar reads UTF-16 units; it never invokes a JavaScript parser.
use super::parse_cursor::JsonCursor;
use super::*;
impl FunctionBuilder<'_> {
    pub(super) fn emit_json_peek(
        &self,
        text: &GcLocal<StringValue>,
        index: I64Local,
        length: I64Local,
        out: I32Local,
        f: &mut Function,
    ) {
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.emit_gc_string_code_unit_i32(text, index, f);
        out.store(f);
        f.instruction(&Instruction::Else);
        json_i32(out, -1, f);
        f.instruction(&Instruction::End);
    }
    pub(super) fn emit_json_bad_text(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_json_error(realm, NativeErrorKind::SyntaxError, message, f)
    }
    pub(super) fn emit_json_expect_unit(
        &mut self,
        text: &GcLocal<StringValue>,
        index: I64Local,
        length: I64Local,
        expected: i32,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let unit = s.reserve_i32_local(f);
        self.emit_json_peek(text, index, length, unit, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(expected));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_bad_text(realm, message, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        json_increment(index, f);
        s.release_i32_local(unit, f);
        Ok(())
    }
    pub(super) fn emit_json_skip_whitespace(
        &mut self,
        text: &GcLocal<StringValue>,
        index: I64Local,
        length: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let cursor = JsonCursor::open(text, index, length, s, f);
        let unit = s.reserve_i32_local(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.peek(unit, s, f);
        f.instruction(&Instruction::I32Const(0));
        for code in [0x20, 0x09, 0x0a, 0x0d] {
            unit.load(f);
            f.instruction(&Instruction::I32Const(code));
            f.instruction(&Instruction::I32Eq);
            f.instruction(&Instruction::I32Or);
        }
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, f);
        cursor.advance(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        cursor.close(index, s, f);
        s.release_i32_local(unit, f);
    }
    fn emit_json_digit_i32(&self, unit: I32Local, f: &mut Function) {
        unit.load(f);
        f.instruction(&Instruction::I32Const(48));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(9));
        f.instruction(&Instruction::I32LeU);
    }
    pub(super) fn emit_json_parse_primitive(
        &mut self,
        text: &GcLocal<StringValue>,
        index: I64Local,
        length: I64Local,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let unit = s.reserve_i32_local(f);
        let matched = s.reserve_i32_local(f);
        json_i32(matched, 0, f);
        self.emit_json_peek(text, index, length, unit, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(34));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let string = self.emit_json_parse_string(text, index, length, realm, message, f)?;
        out.set_reference(&string, s, f);
        string.clear(f);
        json_i32(matched, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        for (literal, value) in [
            ("true", ScalarValue::Boolean(true)),
            ("false", ScalarValue::Boolean(false)),
            ("null", ScalarValue::Null),
        ] {
            unit.load(f);
            f.instruction(&Instruction::I32Const(i32::from(literal.as_bytes()[0])));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            for code in literal.bytes() {
                self.emit_json_expect_unit(
                    text,
                    index,
                    length,
                    i32::from(code),
                    realm,
                    message,
                    f,
                )?;
            }
            out.set_scalar(value, f);
            json_i32(matched, 1, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        self.emit_json_digit_i32(unit, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(45));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_parse_number(text, index, length, realm, message, out, f)?;
        json_i32(matched, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        matched.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_bad_text(realm, message, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(matched, f);
        s.release_i32_local(unit, f);
        Ok(())
    }
}
