//! JSON token scanners read one loaded code-unit backing through a 32-bit cursor.
use super::*;

/// The immutable source backing of a JSON text and its current UTF-16 position.
/// Code-unit arrays have u32 lengths, so every comparison is unsigned. Hoisting
/// one cursor across the entire parse avoids reloading `StringValue::CODE_UNITS`
/// and converting indices between `i64` and `i32` on every token.
pub(super) struct JsonCursor<'a> {
    pub(super) source: &'a GcLocal<StringValue>,
    pub(super) units: GcLocal<CodeUnitArray>,
    pub(super) at: I32Local,
    pub(super) end: I32Local,
}

impl<'a> JsonCursor<'a> {
    pub(super) fn open(
        text: &'a GcLocal<StringValue>,
        s: &RuntimeSchema,
        f: &mut Function,
    ) -> Self {
        let units = s.reserve_gc_local(f).initialize(
            s.struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(text, s, f)
                .reference(),
            f,
        );
        let at = s.reserve_i32_local(f);
        let end = s.reserve_i32_local(f);
        at.set_constant(0, f);
        s.array_type::<CodeUnitArray>().length(&units, s, f);
        end.store(f);
        Self {
            source: text,
            units,
            at,
            end,
        }
    }

    /// `units[at]`, or -1 at the end of the text.
    pub(super) fn peek(&self, out: I32Local, s: &RuntimeSchema, f: &mut Function) {
        self.at.load(f);
        self.end.load(f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        s.array_type::<CodeUnitArray>()
            .read(&self.units, self.at, s, f)
            .store(out, f);
        f.instruction(&Instruction::Else);
        out.set_constant(-1, f);
        f.instruction(&Instruction::End);
    }

    pub(super) fn advance(&self, f: &mut Function) {
        self.advance_by(1, f);
    }

    pub(super) fn advance_by(&self, delta: i32, f: &mut Function) {
        self.at.load(f);
        f.instruction(&Instruction::I32Const(delta));
        f.instruction(&Instruction::I32Add);
        self.at.store(f);
    }

    /// Advances over JSONWhiteSpace (U+0020, U+0009, U+000A, U+000D).
    /// Every JSON token starts above U+0020, so compact JSON exits on one check.
    pub(super) fn skip_whitespace(&self, s: &RuntimeSchema, f: &mut Function) {
        let unit = s.reserve_i32_local(f);
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        self.at.load(f);
        self.end.load(f);
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::BrIf(1));
        s.array_type::<CodeUnitArray>()
            .read(&self.units, self.at, s, f)
            .store(unit, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(0x20));
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::BrIf(1));
        unit.load(f);
        f.instruction(&Instruction::I32Const(0x20));
        f.instruction(&Instruction::I32Eq);
        unit.load(f);
        f.instruction(&Instruction::I32Const(0x09));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        unit.load(f);
        f.instruction(&Instruction::I32Const(0x0a));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        unit.load(f);
        f.instruction(&Instruction::I32Const(0x0d));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::BrIf(1));
        self.advance(f);
        f.instruction(&Instruction::Br(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        s.release_i32_local(unit, f);
    }

    pub(super) fn expect_unit(
        &self,
        expected: i32,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        builder: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = builder.runtime_schema();
        let unit = s.reserve_i32_local(f);
        self.peek(unit, s, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(expected));
        f.instruction(&Instruction::I32Ne);
        builder.open_frame(ControlFrameKind::If, f);
        builder.emit_json_bad_text(realm, message, f)?;
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.advance(f);
        s.release_i32_local(unit, f);
        Ok(())
    }

    /// Copies the validated UTF-16 slice `start..end` directly from `units` into
    /// a `StringValue`, reusing the interned empty string when `start == end`
    /// and `self.source` directly when the slice spans the entire source text.
    pub(super) fn slice_string(
        &self,
        start: I32Local,
        end: I32Local,
        builder: &FunctionBuilder<'_>,
        f: &mut Function,
    ) -> Result<GcStackReference<StringValue>, EmitError> {
        let s = builder.runtime_schema();
        let len = s.reserve_i32_local(f);
        end.load(f);
        start.load(f);
        f.instruction(&Instruction::I32Sub);
        len.store(f);
        let result = s
            .reserve_gc_local::<StringValue, Nullable>(f)
            .initialize_null(s, f);
        len.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        result.replace(builder.emit_interned_string_reference("", f)?.nullable(), f);
        f.instruction(&Instruction::Else);
        // `0 <= start <= end <= self.end` implies `len == self.end` iff `start == 0 && end == self.end`.
        len.load(f);
        self.end.load(f);
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Empty));
        result.replace(self.source.load(s, f).nullable(), f);
        f.instruction(&Instruction::Else);
        let construction = StringConstruction::allocate(s, s.reserve_gc_local(f), len, f);
        let zero = s.reserve_i32_local(f);
        zero.set_constant(0, f);
        construction.copy_units_range_from(zero, &self.units, start, len, s, f);
        s.release_i32_local(zero, f);
        result.replace(construction.publish(s, f).nullable(), f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        let selected = result.load(s, f).require_non_null(f);
        result.clear(f);
        s.release_i32_local(len, f);
        Ok(selected)
    }

    /// Releases the cursor's locals and backing root.
    pub(super) fn close(self, s: &RuntimeSchema, f: &mut Function) {
        s.release_i32_local(self.end, f);
        s.release_i32_local(self.at, f);
        self.units.clear(f);
    }
}

/// `unit - '0'` is at most 9 exactly for an ASCII decimal digit; -1 is none.
pub(super) fn json_cursor_digit(unit: I32Local, f: &mut Function) {
    unit.load(f);
    f.instruction(&Instruction::I32Const(48));
    f.instruction(&Instruction::I32Sub);
    f.instruction(&Instruction::I32Const(9));
    f.instruction(&Instruction::I32LeU);
}
