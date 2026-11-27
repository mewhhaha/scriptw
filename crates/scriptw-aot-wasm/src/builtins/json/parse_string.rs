//! JSON Strings: an unescaped token is one `array.copy` slice of the text;
//! escapes decode unit by unit between copied unescaped runs.
use super::parse_cursor::JsonCursor;
use super::*;

impl FunctionBuilder<'_> {
    pub(super) fn emit_json_parse_string(
        &mut self,
        cursor: &JsonCursor<'_>,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let s = self.runtime_schema();
        cursor.expect_unit(34, realm, message, self, f)?;
        let unit = s.reserve_i32_local(f);
        let start = s.reserve_i32_local(f);
        cursor.at.load(f);
        start.store(f);
        self.emit_json_string_run(cursor, unit, f);
        let result = s
            .reserve_gc_local::<StringValue, Nullable>(f)
            .initialize_null(s, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(34));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        result.replace(cursor.slice_string(start, cursor.at, self, f)?.nullable(), f);
        cursor.advance(f);
        f.instruction(&Instruction::Else);
        let decoded = self.emit_json_escaped_string(cursor, start, unit, realm, message, f)?;
        result.replace(decoded.load(s, f).nullable(), f);
        decoded.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let string = s
            .reserve_gc_local(f)
            .initialize(result.load(s, f).require_non_null(f), f);
        result.clear(f);
        s.release_i32_local(start, f);
        s.release_i32_local(unit, f);
        Ok(string)
    }

    /// Advances over units that stand for themselves and leaves the first
    /// other one (`"`, `\`, a control unit, or -1 at the end) in `unit`.
    fn emit_json_string_run(&mut self, cursor: &JsonCursor<'_>, unit: I32Local, f: &mut Function) {
        let s = self.runtime_schema();
        let done = self.open_frame(ControlFrameKind::Block, f);
        if let Some(base) = self.runtime_helper_base {
            cursor.end.load(f);
            cursor.at.load(f);
            f.instruction(&Instruction::I32Sub);
            f.instruction(&Instruction::I32Const(8));
            f.instruction(&Instruction::I32GeU);
            self.open_frame(ControlFrameKind::If, f);
            s.call_helper(
                crate::runtime_helpers::JsonParsePlainRunArguments::new(
                    &cursor.units,
                    cursor.at,
                    cursor.end,
                ),
                base,
                f,
            )
            .store(cursor.at, f);
            cursor.peek(unit, s, f);
            self.emit_branch_to_target(done, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.at.load(f);
        cursor.end.load(f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        unit.set_constant(-1, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.array_type::<CodeUnitArray>()
            .read(&cursor.units, cursor.at, s, f)
            .store(unit, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(32));
        f.instruction(&Instruction::I32LtU);
        unit.load(f);
        f.instruction(&Instruction::I32Const(34));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        unit.load(f);
        f.instruction(&Instruction::I32Const(92));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        self.emit_branch_if_to_target(done, f);
        cursor.advance(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }

    /// Decodes a token whose first run, `start..at`, stopped at `unit`, which
    /// is not `"`. Every escape spans at least two raw units and decodes to
    /// one, so the raw span up to the first `"` outside a backslash pair
    /// bounds the units written before the closing quote or the first error.
    fn emit_json_escaped_string(
        &mut self,
        cursor: &JsonCursor<'_>,
        start: I32Local,
        unit: I32Local,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let s = self.runtime_schema();
        let scan = s.reserve_i32_local(f);
        let probe = s.reserve_i32_local(f);
        cursor.at.load(f);
        scan.store(f);
        let scanned = self.open_frame(ControlFrameKind::Block, f);
        let step = self.open_frame(ControlFrameKind::Loop, f);
        scan.load(f);
        cursor.end.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(scanned, f);
        s.array_type::<CodeUnitArray>()
            .read(&cursor.units, scan, s, f)
            .store(probe, f);
        probe.load(f);
        f.instruction(&Instruction::I32Const(34));
        f.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(scanned, f);
        scan.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        probe.load(f);
        f.instruction(&Instruction::I32Const(92));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Add);
        scan.store(f);
        self.emit_branch_to_target(step, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // A trailing backslash pair steps one unit past the end.
        let capacity = s.reserve_i32_local(f);
        scan.load(f);
        cursor.end.load(f);
        scan.load(f);
        cursor.end.load(f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::Select);
        start.load(f);
        f.instruction(&Instruction::I32Sub);
        capacity.store(f);
        s.release_i32_local(probe, f);
        s.release_i32_local(scan, f);
        let construction = StringConstruction::allocate(s, s.reserve_gc_local(f), capacity, f);
        let used = s.reserve_i32_local(f);
        let run = s.reserve_i32_local(f);
        let zero = s.reserve_i32_local(f);
        zero.set_constant(0, f);
        cursor.at.load(f);
        start.load(f);
        f.instruction(&Instruction::I32Sub);
        used.store(f);
        used.load(f);
        self.open_frame(ControlFrameKind::If, f);
        construction.copy_units_range_from(zero, &cursor.units, start, used, s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(34));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        cursor.advance(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // A control unit or the end of the text; `"` returned above.
        unit.load(f);
        f.instruction(&Instruction::I32Const(92));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_bad_text(realm, message, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.advance(f);
        let decoded = s.reserve_i32_local(f);
        self.emit_json_escape(cursor, unit, decoded, realm, message, f)?;
        construction.write(used, decoded, s, f);
        s.release_i32_local(decoded, f);
        used.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        used.store(f);
        let from = s.reserve_i32_local(f);
        cursor.at.load(f);
        from.store(f);
        self.emit_json_string_run(cursor, unit, f);
        cursor.at.load(f);
        from.load(f);
        f.instruction(&Instruction::I32Sub);
        run.store(f);
        run.load(f);
        self.open_frame(ControlFrameKind::If, f);
        construction.copy_units_range_from(used, &cursor.units, from, run, s, f);
        used.load(f);
        run.load(f);
        f.instruction(&Instruction::I32Add);
        used.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(from, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let string = s
            .reserve_gc_local(f)
            .initialize(construction.publish_prefix(used, s, f), f);
        s.release_i32_local(zero, f);
        s.release_i32_local(run, f);
        s.release_i32_local(used, f);
        s.release_i32_local(capacity, f);
        Ok(string)
    }

    /// Decodes the escape after a consumed backslash into `decoded`.
    fn emit_json_escape(
        &mut self,
        cursor: &JsonCursor<'_>,
        unit: I32Local,
        decoded: I32Local,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let digit = s.reserve_i32_local(f);
        let valid = s.reserve_i32_local(f);
        cursor.peek(unit, s, f);
        cursor.advance(f);
        valid.set_constant(0, f);
        for (escape, value) in [
            (34, 34),
            (92, 92),
            (47, 47),
            (98, 8),
            (102, 12),
            (110, 10),
            (114, 13),
            (116, 9),
        ] {
            unit.load(f);
            f.instruction(&Instruction::I32Const(escape));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            decoded.set_constant(value, f);
            valid.set_constant(1, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        unit.load(f);
        f.instruction(&Instruction::I32Const(117));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        decoded.set_constant(0, f);
        for _ in 0..4 {
            cursor.peek(unit, s, f);
            cursor.advance(f);
            digit.set_constant(-1, f);
            for (low, high, offset) in [(48, 57, 48), (65, 70, 55), (97, 102, 87)] {
                unit.load(f);
                f.instruction(&Instruction::I32Const(low));
                f.instruction(&Instruction::I32GeU);
                unit.load(f);
                f.instruction(&Instruction::I32Const(high));
                f.instruction(&Instruction::I32LeU);
                f.instruction(&Instruction::I32And);
                self.open_frame(ControlFrameKind::If, f);
                unit.load(f);
                f.instruction(&Instruction::I32Const(offset));
                f.instruction(&Instruction::I32Sub);
                digit.store(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            digit.load(f);
            f.instruction(&Instruction::I32Const(-1));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_json_bad_text(realm, message, f)?;
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            decoded.load(f);
            f.instruction(&Instruction::I32Const(4));
            f.instruction(&Instruction::I32Shl);
            digit.load(f);
            f.instruction(&Instruction::I32Or);
            decoded.store(f);
        }
        valid.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_bad_text(realm, message, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(valid, f);
        s.release_i32_local(digit, f);
        Ok(())
    }
}
