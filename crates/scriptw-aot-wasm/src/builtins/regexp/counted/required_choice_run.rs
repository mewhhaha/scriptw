//! Compact mandatory iterations retain every ordered continuation. Two real
//! successful iterations must first publish identical complete templates.
use super::natural_counter::SnapshotCounterRelation;
use super::*;
use crate::builtins::regexp::matcher_workspace::{
    ChoiceEntryKind, ChoiceSnapshotView, ChoiceSnapshotWord,
};
use scriptw_ir::RegExpOpcode;
mod nested;

struct ProofScope;

/// The arena writer accepts only this proof from its actual runtime If. The
/// numeric spans cannot be manufactured by a caller or moved to another pair.
pub(in crate::builtins::regexp) struct ProvenRequiredChoiceRun<'body, 'scope> {
    pair: &'body CountedLoopLocals,
    workspace: &'body MatcherWorkspace,
    template_newest: I64Local,
    template_oldest: I64Local,
    replaced_oldest: I64Local,
    entry_count: I64Local,
    template_bytes: I64Local,
    row_offset: I64Local,
    _scope: &'scope ProofScope,
}

impl ProvenRequiredChoiceRun<'_, '_> {
    pub(in crate::builtins::regexp) fn workspace(&self) -> &MatcherWorkspace {
        self.workspace
    }
    pub(in crate::builtins::regexp) fn template_oldest(&self) -> I64Local {
        self.template_oldest
    }
    pub(in crate::builtins::regexp) fn replaced_oldest(&self) -> I64Local {
        self.replaced_oldest
    }
    pub(in crate::builtins::regexp) fn entry_count(&self) -> I64Local {
        self.entry_count
    }
    pub(in crate::builtins::regexp) fn template_bytes(&self) -> I64Local {
        self.template_bytes
    }
    pub(in crate::builtins::regexp) fn row_offset(&self) -> I64Local {
        self.row_offset
    }
    pub(in crate::builtins::regexp) fn minimum_capacity(&self) -> I64Local {
        self.pair.minimum.capacity()
    }
    pub(in crate::builtins::regexp) fn maximum_capacity(&self) -> I64Local {
        self.pair.maximum.capacity()
    }
    pub(in crate::builtins::regexp) fn minimum_limbs(&self) -> I64Local {
        self.pair.minimum.limbs()
    }
    pub(in crate::builtins::regexp) fn maximum_limbs(&self) -> I64Local {
        self.pair.maximum.limbs()
    }
    pub(in crate::builtins::regexp) fn maximum_kind(&self) -> I64Local {
        self.pair.maximum_kind
    }
    pub(in crate::builtins::regexp) fn load_minimum_used(
        &self,
        builder: &FunctionBuilder<'_>,
        output: I64Local,
        f: &mut Function,
    ) {
        self.pair
            .minimum
            .load_used(builder, self.pair.state, output, f);
    }
    pub(in crate::builtins::regexp) fn load_maximum_used(
        &self,
        builder: &FunctionBuilder<'_>,
        output: I64Local,
        f: &mut Function,
    ) {
        self.pair
            .maximum
            .load_used(builder, self.pair.state, output, f);
    }

    pub(super) fn finish_required_batch(
        self,
        builder: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.pair.maximum_kind.load(f);
        f.instruction(&Instruction::I64Const(
            RegExpRepeatMaximumKind::Finite.word() as i64,
        ));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.pair.maximum.subtract_minimum(
            builder,
            self.workspace,
            self.pair.state,
            &self.pair.minimum,
            f,
        );
        f.instruction(&Instruction::End);
        // Minimum is still intact: the Run owns M+1 exact iterations (the two
        // observed iterations and M-1 future iterations), and the diminished
        // finite maximum is its immutable affine base.
        self.workspace
            .choices()
            .append_required_run(builder, &self, f)?;
        self.pair.minimum.clear(builder, self.pair.state, f);
        Ok(())
    }
}

pub(super) struct RequiredChoiceRunLocals {
    instruction: I64Local,
    opcode: I64Local,
    temporary: I64Local,
    other: I64Local,
    left_slab: I64Local,
    right_slab: I64Local,
    saved_state: I64Local,
    row_offset: I64Local,
    row_end: I64Local,
    count: I64Local,
    compared: I64Local,
    index: I64Local,
    template_newest: I64Local,
    template_oldest: I64Local,
    replaced_oldest: I64Local,
    eligible: I32Local,
    template_bytes: I64Local,
    template_end: I64Local,
    expected_end: I64Local,
    entry_length: I64Local,
    again: I32Local,
}

impl RequiredChoiceRunLocals {
    fn intersect(&self, f: &mut Function) {
        self.eligible.load(f);
        f.instruction(&Instruction::I32And);
        self.eligible.store(f);
    }

    fn in_body(&self, pair: &CountedLoopLocals, target: I64Local, f: &mut Function) {
        target.load(f);
        pair.guard.load(f);
        f.instruction(&Instruction::I64GtU);
        target.load(f);
        pair.end.load(f);
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
    }

    fn instruction_at(
        &self,
        current: &CountedMatcherLocals,
        pc: I64Local,
        builder: &FunctionBuilder<'_>,
        f: &mut Function,
    ) {
        current.program.load(f);
        pc.load(f);
        f.instruction(&Instruction::I64Const(REGEXP_INSTRUCTION_WIDTH as i64));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64Add);
        self.instruction.store(f);
        builder.emit_regexp_instruction_load(self.instruction, 0, self.opcode, f);
    }

    /// One physical region proof checks assertion ancestry, counted children,
    /// directions and all ordinary/progress edges before template comparison.
    fn check_body(
        &self,
        builder: &FunctionBuilder<'_>,
        workspace: &MatcherWorkspace,
        pair: &CountedLoopLocals,
        current: &CountedMatcherLocals,
        f: &mut Function,
    ) {
        input_only_assertion::emit_choice_template_body(
            builder,
            workspace,
            pair,
            current,
            self.eligible,
            f,
        );
    }

    fn row_matches(
        &self,
        builder: &FunctionBuilder<'_>,
        workspace: &MatcherWorkspace,
        pair: &CountedLoopLocals,
        current: &CountedMatcherLocals,
        slab: I64Local,
        relation: SnapshotCounterRelation,
        f: &mut Function,
    ) {
        slab.load(f);
        self.row_offset.load(f);
        f.instruction(&Instruction::I64Add);
        self.saved_state.store(f);
        for (word, expected) in [
            (RegExpRepeatStateWord::Active, 1),
            (
                RegExpRepeatStateWord::Stage,
                RepeatIterationStage::Required.word() as i64,
            ),
        ] {
            builder.emit_regexp_scratch_load_word(
                self.saved_state,
                word.offset(),
                self.temporary,
                f,
            );
            self.temporary.load(f);
            f.instruction(&Instruction::I64Const(expected));
            f.instruction(&Instruction::I64Eq);
            if matches!(word, RegExpRepeatStateWord::Stage) {
                f.instruction(&Instruction::I32And);
            }
        }
        builder.emit_regexp_scratch_load_word(
            self.saved_state,
            RegExpRepeatStateWord::PreUtf16.offset(),
            self.temporary,
            f,
        );
        self.temporary.load(f);
        current.cursor.utf16.load(f);
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::I32And);
        pair.minimum.snapshot_equals(
            builder,
            workspace,
            self.saved_state,
            pair.state,
            relation,
            f,
        );
        f.instruction(&Instruction::I32And);
        pair.maximum_kind.load(f);
        f.instruction(&Instruction::I64Const(
            RegExpRepeatMaximumKind::Finite.word() as i64,
        ));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        pair.maximum.snapshot_equals(
            builder,
            workspace,
            self.saved_state,
            pair.state,
            relation,
            f,
        );
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32And);
    }

    fn compare_snapshots(
        &self,
        builder: &FunctionBuilder<'_>,
        workspace: &MatcherWorkspace,
        pair: &CountedLoopLocals,
        current: &CountedMatcherLocals,
        left: &ChoiceSnapshotView<'_>,
        right: &ChoiceSnapshotView<'_>,
        f: &mut Function,
    ) {
        left.snapshot_slab_address(self.left_slab, f);
        right.snapshot_slab_address(self.right_slab, f);
        self.row_matches(
            builder,
            workspace,
            pair,
            current,
            self.left_slab,
            SnapshotCounterRelation::Current,
            f,
        );
        self.intersect(f);
        self.row_matches(
            builder,
            workspace,
            pair,
            current,
            self.right_slab,
            SnapshotCounterRelation::PreviousIteration,
            f,
        );
        self.intersect(f);
        for word in [
            ChoiceSnapshotWord::Kind,
            ChoiceSnapshotWord::Fallback,
            ChoiceSnapshotWord::Origin,
            ChoiceSnapshotWord::Byte,
            ChoiceSnapshotWord::Utf16,
            ChoiceSnapshotWord::LowSurrogate,
        ] {
            left.load_header(word, self.temporary, f);
            right.load_header(word, self.other, f);
            self.temporary.load(f);
            self.other.load(f);
            f.instruction(&Instruction::I64Eq);
            self.intersect(f);
        }
        for (word, actual) in [
            (ChoiceSnapshotWord::Byte, current.cursor.byte),
            (ChoiceSnapshotWord::Utf16, current.cursor.utf16),
            (
                ChoiceSnapshotWord::LowSurrogate,
                current.cursor.on_low_surrogate,
            ),
        ] {
            left.load_header(word, self.temporary, f);
            self.temporary.load(f);
            actual.load(f);
            f.instruction(&Instruction::I64Eq);
            self.intersect(f);
        }
        left.load_header(ChoiceSnapshotWord::Origin, self.temporary, f);
        self.in_body(pair, self.temporary, f);
        self.intersect(f);
        // Kind and origin retain the original ordered continuation role.
        // Progress choices use their real Split and whole saved state; no
        // equal-cursor inference or ordinary-entry reinterpretation is used.
        self.instruction_at(current, self.temporary, builder, f);
        left.load_header(ChoiceSnapshotWord::Kind, self.other, f);
        self.other.load(f);
        f.instruction(&Instruction::I64Const(ChoiceEntryKind::Ordinary.word()));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        self.opcode.load(f);
        f.instruction(&Instruction::I64Const(REGEXP_OPCODE_SPLIT as i64));
        f.instruction(&Instruction::I64Eq);
        self.opcode.load(f);
        f.instruction(&Instruction::I64Const(REGEXP_OPCODE_REPEAT_GUARD as i64));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::Else);
        self.opcode.load(f);
        f.instruction(&Instruction::I64Const(RegExpOpcode::ProgressSplit as i64));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::End);
        self.intersect(f);
        f.instruction(&Instruction::I64Const(0));
        self.index.store(f);
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        self.index.load(f);
        workspace.live_bytes.load(f);
        f.instruction(&Instruction::I64GeU);
        f.instruction(&Instruction::BrIf(1));
        self.index.load(f);
        self.row_offset.load(f);
        f.instruction(&Instruction::I64LtU);
        self.index.load(f);
        self.row_end.load(f);
        f.instruction(&Instruction::I64GeU);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::If(BlockType::Empty));
        for slab in [self.left_slab, self.right_slab] {
            slab.load(f);
            self.index.load(f);
            f.instruction(&Instruction::I64Add);
            f.instruction(&Instruction::I32WrapI64);
            f.instruction(&Instruction::I64Load(FunctionBuilder::memarg8(0)));
        }
        f.instruction(&Instruction::I64Eq);
        self.intersect(f);
        f.instruction(&Instruction::End);
        self.index.load(f);
        f.instruction(&Instruction::I64Const(8));
        f.instruction(&Instruction::I64Add);
        self.index.store(f);
        f.instruction(&Instruction::Br(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
    }

    pub(super) fn emit_if_proven<'body>(
        builder: &mut FunctionBuilder<'_>,
        workspace: &'body MatcherWorkspace,
        pair: &'body CountedLoopLocals,
        current: &CountedMatcherLocals,
        f: &mut Function,
        callback: impl for<'scope> FnOnce(
            ProvenRequiredChoiceRun<'body, 'scope>,
            &mut FunctionBuilder<'_>,
            &mut Function,
        ) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let schema = builder.runtime_schema();
        let locals = Self {
            instruction: schema.reserve_i64_local(f),
            opcode: schema.reserve_i64_local(f),
            temporary: schema.reserve_i64_local(f),
            other: schema.reserve_i64_local(f),
            left_slab: schema.reserve_i64_local(f),
            right_slab: schema.reserve_i64_local(f),
            saved_state: schema.reserve_i64_local(f),
            row_offset: schema.reserve_i64_local(f),
            row_end: schema.reserve_i64_local(f),
            count: schema.reserve_i64_local(f),
            compared: schema.reserve_i64_local(f),
            index: schema.reserve_i64_local(f),
            template_newest: schema.reserve_i64_local(f),
            template_oldest: schema.reserve_i64_local(f),
            replaced_oldest: schema.reserve_i64_local(f),
            template_bytes: schema.reserve_i64_local(f),
            template_end: schema.reserve_i64_local(f),
            expected_end: schema.reserve_i64_local(f),
            entry_length: schema.reserve_i64_local(f),
            eligible: schema.reserve_i32_local(f),
            again: schema.reserve_i32_local(f),
        };
        let left = workspace.choices().reserve_top_cursor(builder, f);
        let right = workspace.choices().reserve_top_cursor(builder, f);
        left.local().load(f);
        locals.template_newest.store(f);
        pair.state.load(f);
        workspace.base.load(f);
        f.instruction(&Instruction::I64Sub);
        locals.row_offset.store(f);
        locals.row_offset.load(f);
        f.instruction(&Instruction::I64Const(
            REGEXP_REPEAT_STATE_HEADER_SIZE as i64,
        ));
        f.instruction(&Instruction::I64Add);
        for capacity in [pair.minimum.capacity(), pair.maximum.capacity()] {
            capacity.load(f);
            f.instruction(&Instruction::I64Const(
                REGEXP_REPEAT_COUNTER_LIMB_WIDTH as i64,
            ));
            f.instruction(&Instruction::I64Mul);
            f.instruction(&Instruction::I64Add);
        }
        f.instruction(&Instruction::I64Const(7));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64Const(-8));
        f.instruction(&Instruction::I64And);
        locals.row_end.store(f);
        current.pc.load(f);
        pair.end.load(f);
        f.instruction(&Instruction::I64Eq);
        current.opcode.load(f);
        f.instruction(&Instruction::I64Const(REGEXP_OPCODE_REPEAT_END as i64));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::I32And);
        for (word, expected) in [
            (RegExpRepeatStateWord::Active, 1),
            (
                RegExpRepeatStateWord::Stage,
                RepeatIterationStage::Required.word() as i64,
            ),
        ] {
            builder.emit_regexp_scratch_load_word(pair.state, word.offset(), locals.temporary, f);
            locals.temporary.load(f);
            f.instruction(&Instruction::I64Const(expected));
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::I32And);
        }
        builder.emit_regexp_scratch_load_word(
            pair.state,
            RegExpRepeatStateWord::PreUtf16.offset(),
            locals.temporary,
            f,
        );
        locals.temporary.load(f);
        current.cursor.utf16.load(f);
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::If(BlockType::Empty));
        pair.minimum
            .greater_than_one(builder, workspace, pair.state, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::I32Const(1));
        locals.eligible.store(f);
        locals.check_body(builder, workspace, pair, current, f);
        f.instruction(&Instruction::I64Const(0));
        locals.count.store(f);
        // Every physical node in this group retains leaves from this exact
        // outer iteration. Nested multiplicities are never unfolded.
        left.is_empty(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        left.with_entry_at(builder, f, |entry, _builder, f| {
            entry.length(locals.entry_length, f);
            entry.offset(locals.template_end, f);
            locals.template_end.load(f);
            locals.entry_length.load(f);
            f.instruction(&Instruction::I64Add);
            locals.template_end.store(f);
            locals.template_end.load(f);
            locals.expected_end.store(f);
        });
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I32Const(0));
        locals.eligible.store(f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        right.is_empty(f);
        locals.eligible.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::BrIf(1));
        f.instruction(&Instruction::I32Const(1));
        locals.again.store(f);
        right.with_entry_at(builder, f, |entry, builder, f| {
            entry.with_template_snapshots(
                builder,
                f,
                |snapshot, builder, f| {
                    snapshot.snapshot_slab_address(locals.left_slab, f);
                    locals.row_matches(
                        builder,
                        workspace,
                        pair,
                        current,
                        locals.left_slab,
                        SnapshotCounterRelation::Current,
                        f,
                    );
                    locals.again.load(f);
                    f.instruction(&Instruction::I32And);
                    locals.again.store(f);
                    Ok(())
                },
                |node, builder, f| {
                    nested::emit_source_node(
                        &node,
                        builder,
                        workspace,
                        pair,
                        current,
                        locals.again,
                        f,
                    );
                    Ok(())
                },
            )?;
            locals.again.load(f);
            f.instruction(&Instruction::If(BlockType::Empty));
            entry.offset(locals.template_oldest, f);
            entry.length(locals.entry_length, f);
            locals.template_oldest.load(f);
            locals.entry_length.load(f);
            f.instruction(&Instruction::I64Add);
            locals.expected_end.load(f);
            f.instruction(&Instruction::I64Eq);
            locals.intersect(f);
            locals.template_oldest.load(f);
            locals.expected_end.store(f);
            locals.count.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Add);
            locals.count.store(f);
            right.move_to_previous(&entry, f);
            f.instruction(&Instruction::End);
            Ok(())
        })?;
        locals.again.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::BrIf(1));
        f.instruction(&Instruction::Br(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        locals.count.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        locals.intersect(f);
        locals.template_end.load(f);
        locals.template_oldest.load(f);
        f.instruction(&Instruction::I64Sub);
        locals.template_bytes.store(f);
        f.instruction(&Instruction::I64Const(0));
        locals.compared.store(f);
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        locals.compared.load(f);
        locals.count.load(f);
        f.instruction(&Instruction::I64GeU);
        locals.eligible.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::BrIf(1));
        left.is_empty(f);
        right.is_empty(f);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::I32Const(0));
        locals.eligible.store(f);
        f.instruction(&Instruction::Else);
        left.with_entry_at(builder, f, |left_entry, builder, f| {
            right.with_entry_at(builder, f, |right_entry, builder, f| {
                left_entry.compare_template_tree(
                    &right_entry,
                    builder,
                    locals.eligible,
                    f,
                    |a, b, builder, f| {
                        locals.compare_snapshots(builder, workspace, pair, current, &a, &b, f);
                        Ok(())
                    },
                )?;
                right_entry.offset(locals.replaced_oldest, f);
                right_entry.length(locals.entry_length, f);
                locals.replaced_oldest.load(f);
                locals.entry_length.load(f);
                f.instruction(&Instruction::I64Add);
                locals.expected_end.load(f);
                f.instruction(&Instruction::I64Eq);
                locals.intersect(f);
                locals.replaced_oldest.load(f);
                locals.expected_end.store(f);
                left.move_to_previous(&left_entry, f);
                right.move_to_previous(&right_entry, f);
                Ok(())
            })
        })?;
        f.instruction(&Instruction::End);
        locals.compared.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        locals.compared.store(f);
        f.instruction(&Instruction::Br(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        // No retained leaf below this boundary may still belong to the older
        // iteration. A matching prefix is insufficient even inside a child Run.
        right.is_empty(f);
        f.instruction(&Instruction::I32Eqz);
        locals.eligible.load(f);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::I32Const(0));
        locals.again.store(f);
        right.with_entry_at(builder, f, |entry, builder, f| {
            entry.with_template_snapshots(
                builder,
                f,
                |snapshot, builder, f| {
                    snapshot.snapshot_slab_address(locals.left_slab, f);
                    locals.row_matches(
                        builder,
                        workspace,
                        pair,
                        current,
                        locals.left_slab,
                        SnapshotCounterRelation::PreviousIteration,
                        f,
                    );
                    locals.again.load(f);
                    f.instruction(&Instruction::I32Or);
                    locals.again.store(f);
                    Ok(())
                },
                |_node, _builder, _f| Ok(()),
            )
        })?;
        locals.again.load(f);
        f.instruction(&Instruction::I32Eqz);
        locals.intersect(f);
        f.instruction(&Instruction::End);
        locals.eligible.load(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        let scope = ProofScope;
        let result = callback(
            ProvenRequiredChoiceRun {
                pair,
                workspace,
                template_newest: locals.template_newest,
                template_oldest: locals.template_oldest,
                replaced_oldest: locals.replaced_oldest,
                entry_count: locals.count,
                row_offset: locals.row_offset,
                _scope: &scope,
                template_bytes: locals.template_bytes,
            },
            builder,
            f,
        );
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        left.release(builder, f);
        right.release(builder, f);
        schema.release_i32_local(locals.again, f);
        schema.release_i32_local(locals.eligible, f);
        for local in [
            locals.entry_length,
            locals.expected_end,
            locals.template_end,
            locals.template_bytes,
            locals.replaced_oldest,
            locals.template_oldest,
            locals.template_newest,
            locals.index,
            locals.compared,
            locals.count,
            locals.row_end,
            locals.row_offset,
            locals.saved_state,
            locals.right_slab,
            locals.left_slab,
            locals.other,
            locals.temporary,
            locals.opcode,
            locals.instruction,
        ] {
            schema.release_i64_local(local, f);
        }
        result
    }
}
