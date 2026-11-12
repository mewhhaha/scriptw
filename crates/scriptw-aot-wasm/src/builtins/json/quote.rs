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
        s.array_type::<CodeUnitArray>().length(&units, s, f);
        length.store(f);
        let output = StringAccumulatorCursor::open(accumulator, s, f);
        // Both quotes and one output unit per remaining input unit.
        length.load(f);
        f.instruction(&Instruction::I32Const(2));
        f.instruction(&Instruction::I32Add);
        extra.store(f);
        output.reserve(accumulator, extra, s, f);
        json_i32(unit, 34, f);
        output.push(unit, s, f);
        json_i32(index, 0, f);
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
        // Common units: at least U+0020, not '"' or '\\', not a surrogate.
        unit.load(f);
        f.instruction(&Instruction::I32Const(32));
        f.instruction(&Instruction::I32GeU);
        unit.load(f);
        f.instruction(&Instruction::I32Const(34));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        unit.load(f);
        f.instruction(&Instruction::I32Const(92));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        unit.load(f);
        f.instruction(&Instruction::I32Const(0xd800));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(0x7ff));
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::I32And);
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
            (34, 34),
            (92, 92),
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
        // Room for this escape, one unit for every unread input unit and
        // the closing quote, all relative to the current used length.
        json_i32(extra, 6, f);
        escape.load(f);
        self.open_frame(ControlFrameKind::If, f);
        json_i32(extra, 2, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length.load(f);
        index.load(f);
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        extra.load(f);
        f.instruction(&Instruction::I32Add);
        extra.store(f);
        output.reserve(accumulator, extra, s, f);
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
        output.close(accumulator, s, f);
        for local in [extra, hex, escape, next_unit, unit, index, length] {
            s.release_i32_local(local, f);
        }
        units.clear(f);
        Ok(())
    }
}
