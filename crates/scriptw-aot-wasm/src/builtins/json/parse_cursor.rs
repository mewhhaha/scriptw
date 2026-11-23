//! JSON token scanners read one loaded code-unit backing through a 32-bit cursor.
use super::*;

/// The immutable source backing of one String or Number token and its
/// position. Code-unit arrays have u32 lengths, so every comparison is
/// unsigned. The parser's I64 index is read once by `open` and written once
/// by `close`; a throw leaves it unread.
pub(super) struct JsonCursor {
    pub(super) units: GcLocal<CodeUnitArray>,
    pub(super) at: I32Local,
    pub(super) end: I32Local,
}

impl JsonCursor {
    pub(super) fn open(
        text: &GcLocal<StringValue>,
        index: I64Local,
        length: I64Local,
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
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        at.store(f);
        length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        end.store(f);
        Self { units, at, end }
    }

    /// `units[at]`, or -1 at the end of the text.
    pub(super) fn peek(&self, out: I32Local, s: &RuntimeSchema, f: &mut Function) {
        self.peek_at(self.at, out, s, f);
    }

    pub(super) fn peek_at(
        &self,
        position: I32Local,
        out: I32Local,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        position.load(f);
        self.end.load(f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        s.array_type::<CodeUnitArray>()
            .read(&self.units, position, s, f)
            .store(out, f);
        f.instruction(&Instruction::Else);
        out.set_constant(-1, f);
        f.instruction(&Instruction::End);
    }

    pub(super) fn advance(&self, f: &mut Function) {
        self.at.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        self.at.store(f);
    }

    /// Publishes the advanced position to the parser.
    pub(super) fn close(self, index: I64Local, s: &RuntimeSchema, f: &mut Function) {
        self.at.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        index.store(f);
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
