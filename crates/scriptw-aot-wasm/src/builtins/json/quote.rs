//! QuoteJSONString keeps surrogate pairs and escapes lone UTF-16 surrogates.
use super::*;
impl FunctionBuilder<'_> {
    /// Appends QuoteJSONString(string) to the accumulator. Room for the
    /// quotes and every unit copied verbatim is reserved up front; each
    /// escape reserves its own extra units.
    pub(super) fn emit_json_quote_into(
        &mut self,
        accumulator: &GcLocal<StringAccumulator>,
        string: &GcLocal<StringValue>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_json_quote_into_with_punctuation(accumulator, string, None, false, f)
    }

    /// Saves `mark` as the accumulator length before this member, appends an
    /// optional leading `,` (when `emitted` is nonzero), `QuoteJSONString(key)`,
    /// and a trailing `:`, all under a single `StringAccumulatorCursor` open and
    /// upfront capacity reservation.
    pub(super) fn emit_json_quote_object_key_into(
        &mut self,
        accumulator: &GcLocal<StringAccumulator>,
        string: &GcLocal<StringValue>,
        emitted: I32Local,
        mark: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_json_quote_into_with_punctuation(
            accumulator,
            string,
            Some((emitted, mark)),
            true,
            f,
        )
    }

    fn emit_json_quote_into_with_punctuation(
        &mut self,
        accumulator: &GcLocal<StringAccumulator>,
        string: &GcLocal<StringValue>,
        leading_comma: Option<(I32Local, I32Local)>,
        trailing_colon: bool,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let units = s.reserve_gc_local(f).initialize(
            s.struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(string, s, f)
                .reference(),
            f,
        );
        let length = s.reserve_i32_local(f);
        let index = s.reserve_i32_local(f);
        let unit = s.reserve_i32_local(f);
        let next_unit = s.reserve_i32_local(f);
        let escape = s.reserve_i32_local(f);
        let hex = s.reserve_i32_local(f);
        let extra = s.reserve_i32_local(f);
        let spare = s.reserve_i32_local(f);
        s.array_type::<CodeUnitArray>().length(&units, s, f);
        length.store(f);
        let output = StringAccumulatorCursor::open(accumulator, s, f);
        if let Some((_, mark)) = leading_comma {
            output.length().load(f);
            mark.store(f);
        }
        // Optional leading comma, both quotes, optional trailing colon, and one
        // output unit per remaining input unit.
        let fixed_padding = 2 + i32::from(leading_comma.is_some()) + i32::from(trailing_colon);
        let trailing_padding = 1 + i32::from(trailing_colon);
        length.load(f);
        f.instruction(&Instruction::I32Const(fixed_padding));
        f.instruction(&Instruction::I32Add);
        extra.store(f);
        output.reserve(accumulator, extra, s, f);
        if let Some((emitted, _)) = leading_comma {
            emitted.load(f);
            self.open_frame(ControlFrameKind::If, f);
            json_i32(unit, 44, f);
            output.push(unit, s, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        json_i32(unit, 34, f);
        output.push(unit, s, f);
        json_i32(index, 0, f);
        self.emit_json_quote_plain_prefix(accumulator, &output, &units, length, index, unit, f);
        json_i32(spare, 0, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, f);
        s.array_type::<CodeUnitArray>()
            .read(&units, index, s, f)
            .store(unit, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        // Fast path for common verbatim units (0x20..=0xD7FF excluding '"' and '\\').
        unit.load(f);
        f.instruction(&Instruction::I32Const(0x20));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(0xd800 - 0x20));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(34));
        f.instruction(&Instruction::I32Eq);
        unit.load(f);
        f.instruction(&Instruction::I32Const(92));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        // Fast batched escape for '"' and '\\': reserve 15 extra escape slots
        // once so strings with embedded JSON (many quotes) never re-check
        // accumulator capacity on each quote.
        spare.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        length.load(f);
        index.load(f);
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(trailing_padding + 17));
        f.instruction(&Instruction::I32Add);
        extra.store(f);
        output.reserve(accumulator, extra, s, f);
        json_i32(spare, 15, f);
        f.instruction(&Instruction::Else);
        spare.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        spare.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        json_i32(hex, 92, f);
        output.push(hex, s, f);
        output.push(unit, s, f);
        self.emit_branch_to_target(next, f);
        f.instruction(&Instruction::Else);
        output.push(unit, s, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Plain BMP units at or above U+E000 also copy verbatim.
        unit.load(f);
        f.instruction(&Instruction::I32Const(0xe000));
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        output.push(unit, s, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // A well-formed surrogate pair is copied as two units.
        unit.load(f);
        f.instruction(&Instruction::I32Const(0xd800));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(0x3ff));
        f.instruction(&Instruction::I32LeU);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        s.array_type::<CodeUnitArray>()
            .read(&units, index, s, f)
            .store(next_unit, f);
        next_unit.load(f);
        f.instruction(&Instruction::I32Const(0xdc00));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(0x3ff));
        f.instruction(&Instruction::I32LeU);
        self.open_frame(ControlFrameKind::If, f);
        output.push(unit, s, f);
        output.push(next_unit, s, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Every remaining unit is escaped: a two-unit short escape or a
        // six-unit \uXXXX escape (control units and lone surrogates).
        json_i32(escape, 0, f);
        for (code, escaped) in [
            (8, 98),
            (9, 116),
            (10, 110),
            (12, 102),
            (13, 114),
        ] {
            unit.load(f);
            f.instruction(&Instruction::I32Const(code));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            json_i32(escape, escaped, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        // Room for this escape, one unit for every unread input unit, the
        // closing quote, and any trailing colon, all relative to current length.
        json_i32(extra, 6, f);
        escape.load(f);
        self.open_frame(ControlFrameKind::If, f);
        json_i32(extra, 2, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length.load(f);
        index.load(f);
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(trailing_padding));
        f.instruction(&Instruction::I32Add);
        extra.load(f);
        f.instruction(&Instruction::I32Add);
        extra.store(f);
        output.reserve(accumulator, extra, s, f);
        json_i32(spare, 0, f);
        json_i32(hex, 92, f);
        output.push(hex, s, f);
        escape.load(f);
        self.open_frame(ControlFrameKind::If, f);
        output.push(escape, s, f);
        f.instruction(&Instruction::Else);
        json_i32(hex, 117, f);
        output.push(hex, s, f);
        for shift in [12, 8, 4, 0] {
            unit.load(f);
            f.instruction(&Instruction::I32Const(shift));
            f.instruction(&Instruction::I32ShrU);
            f.instruction(&Instruction::I32Const(15));
            f.instruction(&Instruction::I32And);
            hex.store(f);
            hex.load(f);
            f.instruction(&Instruction::I32Const(10));
            f.instruction(&Instruction::I32LtU);
            self.open_frame(ControlFrameKind::If, f);
            hex.load(f);
            f.instruction(&Instruction::I32Const(48));
            f.instruction(&Instruction::I32Add);
            hex.store(f);
            f.instruction(&Instruction::Else);
            hex.load(f);
            f.instruction(&Instruction::I32Const(87));
            f.instruction(&Instruction::I32Add);
            hex.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            output.push(hex, s, f);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        json_i32(unit, 34, f);
        output.push(unit, s, f);
        if trailing_colon {
            json_i32(unit, 58, f);
            output.push(unit, s, f);
        }
        output.close(accumulator, s, f);
        for local in [spare, extra, hex, escape, next_unit, unit, index, length] {
            s.release_i32_local(local, f);
        }
        units.clear(f);
        Ok(())
    }

    /// Bulk-copy a long ordinary prefix. A prefix shorter than 16 units
    /// leaves the scalar quote loop at its original starting point.
    fn emit_json_quote_plain_prefix(
        &mut self,
        accumulator: &GcLocal<StringAccumulator>,
        output: &StringAccumulatorCursor,
        units: &GcLocal<CodeUnitArray>,
        length: I32Local,
        index: I32Local,
        unit: I32Local,
        f: &mut Function,
    ) {
        const BULK_MIN: i32 = 16;
        let s = self.runtime_schema();
        let done = self.open_frame(ControlFrameKind::Block, f);
        length.load(f);
        f.instruction(&Instruction::I32Const(BULK_MIN));
        f.instruction(&Instruction::I32LtU);
        self.emit_branch_if_to_target(done, f);
        let span = s.reserve_i32_local(f);
        span.set_constant(0, f);
        let scanned = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        s.array_type::<CodeUnitArray>()
            .read(units, span, s, f)
            .store(unit, f);
        for (op, limit) in [
            (Instruction::I32LtU, 0x20),
            (Instruction::I32Eq, 34),
            (Instruction::I32Eq, 92),
        ] {
            unit.load(f);
            f.instruction(&Instruction::I32Const(limit));
            f.instruction(&op);
            self.emit_branch_if_to_target(scanned, f);
        }
        unit.load(f);
        f.instruction(&Instruction::I32Const(0xd800));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(0x800));
        f.instruction(&Instruction::I32LtU);
        self.emit_branch_if_to_target(scanned, f);
        span.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        span.store(f);
        span.load(f);
        length.load(f);
        f.instruction(&Instruction::I32LtU);
        self.emit_branch_if_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        span.load(f);
        f.instruction(&Instruction::I32Const(BULK_MIN));
        f.instruction(&Instruction::I32LtU);
        self.emit_branch_if_to_target(done, f);
        let _ = accumulator;
        output.append_reserved_units_range(units, index, span, s, f);
        span.load(f);
        index.store(f);
        s.release_i32_local(span, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }
}
