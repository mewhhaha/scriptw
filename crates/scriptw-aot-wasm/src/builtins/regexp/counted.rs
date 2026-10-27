//! Counted repetition over one admitted body. Every choice restores counters
//! together with captures, so a fallback resumes its original repeat state.

use super::*;
mod completed_exact_child;
mod input_only_assertion;
mod natural_counter;
use natural_counter::{MaximumCounter, MinimumCounter, NaturalCounterLocals};
mod required_empty;
use required_empty::RequiredEmptyReplayLocals;
mod independent_iterations;
use independent_iterations::IndependentIterationLocals;
pub(super) mod required_choice_run;
use required_choice_run::RequiredChoiceRunLocals;

#[derive(Clone, Copy)]
enum CountedOperation {
    Begin,
    Guard,
    End,
    Exit,
}

impl CountedOperation {
    const fn opcode(self) -> u64 {
        match self {
            Self::Begin => REGEXP_OPCODE_REPEAT_BEGIN,
            Self::Guard => REGEXP_OPCODE_REPEAT_GUARD,
            Self::End => REGEXP_OPCODE_REPEAT_END,
            Self::Exit => REGEXP_OPCODE_REPEAT_EXIT,
        }
    }
}

#[derive(Clone, Copy)]
enum RepeatIterationStage {
    Required,
    Optional,
}

impl RepeatIterationStage {
    const fn word(self) -> u64 {
        match self {
            Self::Required => 1,
            Self::Optional => 2,
        }
    }
}

pub(super) struct CountedMatcherLocals {
    pub(super) program: I64Local,
    pub(super) instructions: I64Local,
    pub(super) slots: I64Local,
    pub(super) bounds: I64Local,
    pub(super) opcode: I64Local,
    pub(super) operand0: I64Local,
    pub(super) pc: I64Local,
    pub(super) cursor: RegExpInputCursor,
    pub(super) reverse_mode: I64Local,
    pub(super) choice_header: I64Local,
    pub(super) input_utf16_len: I64Local,
    pub(super) named_group_table_ptr: I64Local,
    pub(super) named_records_ptr: I64Local,
    pub(super) named_group_count: I64Local,
}

/// Only `admit` produces these addresses. Every read stays inside a checked
/// instruction record or a dense slot admitted by the program descriptor.
struct CountedLoopLocals {
    begin: I64Local,
    guard: I64Local,
    end: I64Local,
    lazy: I64Local,
    state: I64Local,
    temporary: I64Local,
    address: I64Local,
    slot: I64Local,
    bound_row: I64Local,
    minimum_digits: I64Local,
    minimum_length: I64Local,
    maximum_digits: I64Local,
    maximum_length: I64Local,
    maximum_kind: I64Local,
    minimum: NaturalCounterLocals<MinimumCounter>,
    maximum: NaturalCounterLocals<MaximumCounter>,
}

impl CountedLoopLocals {
    fn reject(workspace: &MatcherWorkspace, builder: &FunctionBuilder<'_>, f: &mut Function) {
        f.instruction(&Instruction::If(BlockType::Empty));
        workspace.fail(builder, RegExpMatcherFailure::CorruptProgram, f);
        f.instruction(&Instruction::End);
    }

    fn instruction_address(&self, c: &CountedMatcherLocals, pc: I64Local, f: &mut Function) {
        c.program.load(f);
        pc.load(f);
        f.instruction(&Instruction::I64Const(REGEXP_INSTRUCTION_WIDTH as i64));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64Add);
        self.address.store(f);
    }

    fn admit(
        builder: &FunctionBuilder<'_>,
        workspace: &MatcherWorkspace,
        c: &CountedMatcherLocals,
        f: &mut Function,
    ) -> Self {
        Self::admit_selected_begin(builder, workspace, c, None, f)
    }

    fn admit_at_begin(
        builder: &FunctionBuilder<'_>,
        workspace: &MatcherWorkspace,
        c: &CountedMatcherLocals,
        begin: I64Local,
        f: &mut Function,
    ) -> Self {
        Self::admit_selected_begin(builder, workspace, c, Some(begin), f)
    }

    /// Both actual dispatch and nested replay read the same checked v3 pair.
    /// A nested read never substitutes the matcher's current PC or opcode.
    fn admit_selected_begin(
        builder: &FunctionBuilder<'_>,
        workspace: &MatcherWorkspace,
        c: &CountedMatcherLocals,
        selected_begin: Option<I64Local>,
        f: &mut Function,
    ) -> Self {
        let s = builder.runtime_schema();
        let p = Self {
            begin: s.reserve_i64_local(f),
            guard: s.reserve_i64_local(f),
            end: s.reserve_i64_local(f),
            lazy: s.reserve_i64_local(f),
            state: s.reserve_i64_local(f),
            temporary: s.reserve_i64_local(f),
            address: s.reserve_i64_local(f),
            slot: s.reserve_i64_local(f),
            bound_row: s.reserve_i64_local(f),
            minimum_digits: s.reserve_i64_local(f),
            minimum_length: s.reserve_i64_local(f),
            maximum_digits: s.reserve_i64_local(f),
            maximum_length: s.reserve_i64_local(f),
            maximum_kind: s.reserve_i64_local(f),
            minimum: NaturalCounterLocals::reserve(builder, f),
            maximum: NaturalCounterLocals::reserve(builder, f),
        };
        if let Some(begin) = selected_begin {
            begin.load(f);
            c.instructions.load(f);
            f.instruction(&Instruction::I64GeU);
            Self::reject(workspace, builder, f);
            begin.load(f);
            p.begin.store(f);
        } else {
            c.opcode.load(f);
            f.instruction(&Instruction::I64Const(REGEXP_OPCODE_REPEAT_BEGIN as i64));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::If(BlockType::Empty));
            c.pc.load(f);
            p.begin.store(f);
            f.instruction(&Instruction::Else);
            c.opcode.load(f);
            f.instruction(&Instruction::I64Const(REGEXP_OPCODE_REPEAT_GUARD as i64));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::If(BlockType::Empty));
            c.pc.load(f);
            f.instruction(&Instruction::I64Eqz);
            Self::reject(workspace, builder, f);
            c.pc.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Sub);
            p.begin.store(f);
            f.instruction(&Instruction::Else);
            c.operand0.load(f);
            c.pc.load(f);
            f.instruction(&Instruction::I64GeU);
            Self::reject(workspace, builder, f);
            c.operand0.load(f);
            p.begin.store(f);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::End);
        }

        p.instruction_address(c, p.begin, f);
        builder.emit_regexp_instruction_load(p.address, 0, p.temporary, f);
        p.temporary.load(f);
        f.instruction(&Instruction::I64Const(REGEXP_OPCODE_REPEAT_BEGIN as i64));
        f.instruction(&Instruction::I64Ne);
        Self::reject(workspace, builder, f);
        builder.emit_regexp_instruction_load(p.address, 8, p.slot, f);
        builder.emit_regexp_instruction_load(p.address, 16, p.temporary, f);
        p.temporary.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        Self::reject(workspace, builder, f);
        p.begin.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        p.guard.store(f);
        p.guard.load(f);
        c.instructions.load(f);
        f.instruction(&Instruction::I64GeU);
        Self::reject(workspace, builder, f);
        p.instruction_address(c, p.guard, f);
        builder.emit_regexp_instruction_load(p.address, 0, p.temporary, f);
        p.temporary.load(f);
        f.instruction(&Instruction::I64Const(REGEXP_OPCODE_REPEAT_GUARD as i64));
        f.instruction(&Instruction::I64Ne);
        Self::reject(workspace, builder, f);
        builder.emit_regexp_instruction_load(p.address, 8, p.end, f);
        builder.emit_regexp_instruction_load(p.address, 16, p.temporary, f);
        p.temporary.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64And);
        p.lazy.store(f);
        p.temporary.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64ShrU);
        p.temporary.store(f);
        p.temporary.load(f);
        c.slots.load(f);
        f.instruction(&Instruction::I64GeU);
        p.temporary.load(f);
        p.slot.load(f);
        f.instruction(&Instruction::I64Ne);
        f.instruction(&Instruction::I32Or);
        p.end.load(f);
        p.guard.load(f);
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32Or);
        p.end.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        c.instructions.load(f);
        f.instruction(&Instruction::I64GeU);
        f.instruction(&Instruction::I32Or);
        // Reject before adding one to an untrusted End, including u64::MAX.
        p.end.load(f);
        c.instructions.load(f);
        f.instruction(&Instruction::I64GeU);
        f.instruction(&Instruction::I32Or);
        Self::reject(workspace, builder, f);
        c.bounds.load(f);
        p.slot.load(f);
        f.instruction(&Instruction::I64Const(
            REGEXP_REPEAT_BOUND_RECORD_SIZE as i64,
        ));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64Add);
        p.bound_row.store(f);
        for (word, output) in [
            (RegExpRepeatBoundWord::MinimumDigitsOffset, p.minimum_digits),
            (RegExpRepeatBoundWord::MinimumDigitsLength, p.minimum_length),
            (RegExpRepeatBoundWord::MaximumKind, p.maximum_kind),
            (RegExpRepeatBoundWord::MaximumDigitsOffset, p.maximum_digits),
            (RegExpRepeatBoundWord::MaximumDigitsLength, p.maximum_length),
            (RegExpRepeatBoundWord::StateOffset, p.temporary),
        ] {
            builder.emit_regexp_instruction_load(p.bound_row, word.offset(), output, f);
        }
        workspace.base.load(f);
        workspace.capture_bytes.load(f);
        f.instruction(&Instruction::I64Add);
        p.temporary.load(f);
        f.instruction(&Instruction::I64Add);
        p.state.store(f);
        for digits in [p.minimum_digits, p.maximum_digits] {
            c.program.load(f);
            f.instruction(&Instruction::I64Const(REGEXP_PROGRAM_HEADER_SIZE as i64));
            f.instruction(&Instruction::I64Sub);
            digits.load(f);
            f.instruction(&Instruction::I64Add);
            digits.store(f);
        }
        p.minimum.configure(p.state, p.minimum_length, f);
        p.maximum
            .configure(p.state, p.maximum_length, &p.minimum, f);

        for (delta, opcode) in [
            (0, REGEXP_OPCODE_REPEAT_END),
            (1, REGEXP_OPCODE_REPEAT_EXIT),
        ] {
            p.end.load(f);
            f.instruction(&Instruction::I64Const(delta));
            f.instruction(&Instruction::I64Add);
            p.temporary.store(f);
            p.instruction_address(c, p.temporary, f);
            builder.emit_regexp_instruction_load(p.address, 0, p.temporary, f);
            p.temporary.load(f);
            f.instruction(&Instruction::I64Const(opcode as i64));
            f.instruction(&Instruction::I64Ne);
            builder.emit_regexp_instruction_load(p.address, 8, p.temporary, f);
            p.temporary.load(f);
            p.begin.load(f);
            f.instruction(&Instruction::I64Ne);
            f.instruction(&Instruction::I32Or);
            builder.emit_regexp_instruction_load(p.address, 16, p.temporary, f);
            p.temporary.load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::I32Eqz);
            f.instruction(&Instruction::I32Or);
            Self::reject(workspace, builder, f);
        }
        if selected_begin.is_none() {
            for operation in [CountedOperation::End, CountedOperation::Exit] {
                c.opcode.load(f);
                f.instruction(&Instruction::I64Const(operation.opcode() as i64));
                f.instruction(&Instruction::I64Eq);
                c.pc.load(f);
                p.end.load(f);
                if matches!(operation, CountedOperation::Exit) {
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Add);
                }
                f.instruction(&Instruction::I64Ne);
                f.instruction(&Instruction::I32And);
                Self::reject(workspace, builder, f);
            }
        }
        p
    }

    fn release(self, builder: &FunctionBuilder<'_>, f: &mut Function) {
        let s = builder.runtime_schema();
        self.maximum.release(builder, f);
        self.minimum.release(builder, f);
        for local in [
            self.maximum_kind,
            self.maximum_length,
            self.maximum_digits,
            self.minimum_length,
            self.minimum_digits,
            self.bound_row,
            self.slot,
            self.address,
            self.temporary,
            self.state,
            self.lazy,
            self.end,
            self.guard,
            self.begin,
        ] {
            s.release_i64_local(local, f);
        }
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_regexp_counted_dispatch(
        &mut self,
        workspace: &MatcherWorkspace,
        backtrack: &BacktrackRequest,
        c: &CountedMatcherLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        c.opcode.load(f);
        f.instruction(&Instruction::I64Const(REGEXP_OPCODE_REPEAT_BEGIN as i64));
        f.instruction(&Instruction::I64GeU);
        c.opcode.load(f);
        f.instruction(&Instruction::I64Const(REGEXP_OPCODE_REPEAT_EXIT as i64));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::If(BlockType::Empty));
        let p = CountedLoopLocals::admit(self, workspace, c, f);
        for operation in [
            CountedOperation::Begin,
            CountedOperation::Guard,
            CountedOperation::End,
            CountedOperation::Exit,
        ] {
            c.opcode.load(f);
            f.instruction(&Instruction::I64Const(operation.opcode() as i64));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::If(BlockType::Empty));
            match operation {
                CountedOperation::Begin => {
                    self.emit_regexp_scratch_store_word_const(
                        p.state,
                        RegExpRepeatStateWord::Active.offset(),
                        1,
                        f,
                    );
                    p.minimum
                        .initialize(self, p.state, p.minimum_digits, p.minimum_length, f);
                    p.maximum_kind.load(f);
                    f.instruction(&Instruction::I64Const(
                        RegExpRepeatMaximumKind::Finite.word() as i64,
                    ));
                    f.instruction(&Instruction::I64Eq);
                    f.instruction(&Instruction::If(BlockType::Empty));
                    p.maximum
                        .initialize(self, p.state, p.maximum_digits, p.maximum_length, f);
                    f.instruction(&Instruction::Else);
                    self.emit_regexp_scratch_store_word_const(
                        p.state,
                        RegExpRepeatStateWord::MaximumUsed.offset(),
                        0,
                        f,
                    );
                    f.instruction(&Instruction::End);
                    IndependentIterationLocals::emit_if_proven(
                        self,
                        workspace,
                        &p,
                        c,
                        f,
                        |proof, builder, f| proof.translate_required_count(builder, f),
                    );
                    p.guard.load(f);
                    c.pc.store(f);
                }
                CountedOperation::Guard => {
                    self.emit_regexp_scratch_load_word(
                        p.state,
                        RegExpRepeatStateWord::Active.offset(),
                        p.temporary,
                        f,
                    );
                    p.temporary.load(f);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Ne);
                    CountedLoopLocals::reject(workspace, self, f);
                    self.emit_regexp_scratch_store_word(
                        p.state,
                        RegExpRepeatStateWord::PreUtf16.offset(),
                        c.cursor.utf16,
                        f,
                    );
                    p.minimum.load_used(self, p.state, p.temporary, f);
                    p.temporary.load(f);
                    f.instruction(&Instruction::I64Eqz);
                    f.instruction(&Instruction::If(BlockType::Empty));
                    p.maximum.load_used(self, p.state, p.temporary, f);
                    p.temporary.load(f);
                    f.instruction(&Instruction::I64Eqz);
                    p.maximum_kind.load(f);
                    f.instruction(&Instruction::I64Const(
                        RegExpRepeatMaximumKind::Finite.word() as i64,
                    ));
                    f.instruction(&Instruction::I64Eq);
                    f.instruction(&Instruction::I32And);
                    f.instruction(&Instruction::If(BlockType::Empty));
                    p.end.load(f);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Add);
                    c.pc.store(f);
                    f.instruction(&Instruction::Else);
                    self.emit_regexp_scratch_store_word_const(
                        p.state,
                        RegExpRepeatStateWord::Stage.offset(),
                        RepeatIterationStage::Optional.word(),
                        f,
                    );
                    p.lazy.load(f);
                    f.instruction(&Instruction::I32WrapI64);
                    f.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
                    p.guard.load(f);
                    f.instruction(&Instruction::Else);
                    p.end.load(f);
                    f.instruction(&Instruction::End);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Add);
                    c.choice_header.store(f);
                    workspace.choices().push_snapshot(
                        self,
                        SnapshotChoice::Ordinary {
                            fallback: c.choice_header,
                            origin: c.pc,
                        },
                        c.cursor,
                        f,
                    )?;
                    p.lazy.load(f);
                    f.instruction(&Instruction::I32WrapI64);
                    f.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
                    p.end.load(f);
                    f.instruction(&Instruction::Else);
                    p.guard.load(f);
                    f.instruction(&Instruction::End);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Add);
                    c.pc.store(f);
                    f.instruction(&Instruction::End);
                    f.instruction(&Instruction::Else);
                    self.emit_regexp_scratch_store_word_const(
                        p.state,
                        RegExpRepeatStateWord::Stage.offset(),
                        RepeatIterationStage::Required.word(),
                        f,
                    );
                    p.guard.load(f);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Add);
                    c.pc.store(f);
                    f.instruction(&Instruction::End);
                }
                CountedOperation::End => {
                    // A failed virtual group can represent older groups only
                    // if none of its fallbacks observed this affine row.
                    workspace.choices().observe_repeat_end(self, p.state, f);
                    self.emit_regexp_scratch_load_word(
                        p.state,
                        RegExpRepeatStateWord::Active.offset(),
                        p.temporary,
                        f,
                    );
                    p.temporary.load(f);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Ne);
                    CountedLoopLocals::reject(workspace, self, f);
                    self.emit_regexp_scratch_load_word(
                        p.state,
                        RegExpRepeatStateWord::Stage.offset(),
                        p.temporary,
                        f,
                    );
                    p.temporary.load(f);
                    f.instruction(&Instruction::I64Const(
                        RepeatIterationStage::Required.word() as i64,
                    ));
                    f.instruction(&Instruction::I64LtU);
                    p.temporary.load(f);
                    f.instruction(&Instruction::I64Const(
                        RepeatIterationStage::Optional.word() as i64,
                    ));
                    f.instruction(&Instruction::I64GtU);
                    f.instruction(&Instruction::I32Or);
                    CountedLoopLocals::reject(workspace, self, f);
                    p.temporary.load(f);
                    f.instruction(&Instruction::I64Const(
                        RepeatIterationStage::Optional.word() as i64,
                    ));
                    f.instruction(&Instruction::I64Eq);
                    self.emit_regexp_scratch_load_word(
                        p.state,
                        RegExpRepeatStateWord::PreUtf16.offset(),
                        p.temporary,
                        f,
                    );
                    p.temporary.load(f);
                    c.cursor.utf16.load(f);
                    f.instruction(&Instruction::I64Eq);
                    f.instruction(&Instruction::I32And);
                    self.emit_regexp_backtrack_request_if(backtrack, f);
                    let accelerated = self.runtime_schema().reserve_i32_local(f);
                    f.instruction(&Instruction::I32Const(0));
                    accelerated.store(f);
                    RequiredEmptyReplayLocals::emit_if_proven(
                        self,
                        workspace,
                        &p,
                        &c,
                        f,
                        |proof, builder, f| {
                            proof.finish_required_batch(builder, f);
                            f.instruction(&Instruction::I32Const(1));
                            accelerated.store(f);
                        },
                    );
                    accelerated.load(f);
                    f.instruction(&Instruction::I32Eqz);
                    f.instruction(&Instruction::If(BlockType::Empty));
                    RequiredChoiceRunLocals::emit_if_proven(
                        self,
                        workspace,
                        &p,
                        c,
                        f,
                        |proof, builder, f| {
                            proof.finish_required_batch(builder, f)?;
                            f.instruction(&Instruction::I32Const(1));
                            accelerated.store(f);
                            Ok(())
                        },
                    )?;
                    f.instruction(&Instruction::End);
                    accelerated.load(f);
                    f.instruction(&Instruction::I32Eqz);
                    f.instruction(&Instruction::If(BlockType::Empty));
                    p.minimum.load_used(self, p.state, p.temporary, f);
                    p.temporary.load(f);
                    f.instruction(&Instruction::I64Eqz);
                    f.instruction(&Instruction::I32Eqz);
                    f.instruction(&Instruction::If(BlockType::Empty));
                    p.minimum.decrement(self, workspace, p.state, f);
                    f.instruction(&Instruction::End);
                    p.maximum_kind.load(f);
                    f.instruction(&Instruction::I64Const(
                        RegExpRepeatMaximumKind::Finite.word() as i64,
                    ));
                    f.instruction(&Instruction::I64Eq);
                    f.instruction(&Instruction::If(BlockType::Empty));
                    p.maximum.decrement(self, workspace, p.state, f);
                    f.instruction(&Instruction::End);
                    f.instruction(&Instruction::End);
                    self.runtime_schema().release_i32_local(accelerated, f);
                    p.guard.load(f);
                    c.pc.store(f);
                }
                CountedOperation::Exit => {
                    self.emit_regexp_scratch_load_word(
                        p.state,
                        RegExpRepeatStateWord::Active.offset(),
                        p.temporary,
                        f,
                    );
                    p.temporary.load(f);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Ne);
                    p.minimum.load_used(self, p.state, p.temporary, f);
                    p.temporary.load(f);
                    f.instruction(&Instruction::I64Eqz);
                    f.instruction(&Instruction::I32Eqz);
                    f.instruction(&Instruction::I32Or);
                    CountedLoopLocals::reject(workspace, self, f);
                    self.emit_regexp_scratch_store_word_const(
                        p.state,
                        RegExpRepeatStateWord::Active.offset(),
                        0,
                        f,
                    );
                    c.pc.load(f);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Add);
                    c.pc.store(f);
                }
            }
            // Continue the instruction loop outside both dispatch If frames.
            f.instruction(&Instruction::Br(2));
            f.instruction(&Instruction::End);
        }
        p.release(self, f);
        f.instruction(&Instruction::End);
        Ok(())
    }
}
