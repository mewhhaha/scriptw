//! Exclusive demand-grown numeric matcher state, after immutable transients.

use super::*;

mod choice_entries;
pub(super) use choice_entries::{
    CheckedTemplateRunNode, ChoiceEntryKind, ChoiceSnapshotView,
    ChoiceSnapshotWord, ChoiceStack, SnapshotChoice,
};

/// Owning the tail is required to publish any choice frame. The actual cursor
/// is checked before growth; another transient allocation cannot silently
/// become part of this workspace. Capture output belongs to the caller.
#[must_use]
pub(super) struct MatcherWorkspace {
    pub(super) base: I64Local,
    pub(super) capture_bytes: I64Local,
    pub(super) repeat_bytes: I64Local,
    pub(super) live_bytes: I64Local,
    snapshot_bytes: I64Local,
    choice_top: I64Local,
    choice_used: I64Local,
    active_run_offset: I64Local,
    active_run_row: I64Local,
    active_run_node: I64Local,
    capacity: I64Local,
    maximum_capacity: I64Local,
    allocated_bytes: I64Local,
    checkpoint: I64Local,
    start: I64Local,
}

impl MatcherWorkspace {
    pub(super) fn allocate(
        builder: &mut FunctionBuilder<'_>,
        captures: I64Local,
        repeat_state_bytes: I64Local,
        checkpoint: I64Local,
        start: I64Local,
        function: &mut Function,
    ) -> Result<Self, EmitError> {
        let schema = builder.runtime_schema();
        let workspace = Self {
            base: schema.reserve_i64_local(function),
            capture_bytes: schema.reserve_i64_local(function),
            repeat_bytes: schema.reserve_i64_local(function),
            live_bytes: schema.reserve_i64_local(function),
            snapshot_bytes: schema.reserve_i64_local(function),
            choice_top: schema.reserve_i64_local(function),
            choice_used: schema.reserve_i64_local(function),
            active_run_offset: schema.reserve_i64_local(function),
            active_run_row: schema.reserve_i64_local(function),
            active_run_node: schema.reserve_i64_local(function),
            capacity: schema.reserve_i64_local(function),
            maximum_capacity: schema.reserve_i64_local(function),
            allocated_bytes: schema.reserve_i64_local(function),
            checkpoint,
            start,
        };
        captures.load(function);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64Mul);
        workspace.capture_bytes.store(function);
        repeat_state_bytes.load(function);
        workspace.repeat_bytes.store(function);
        workspace.capture_bytes.load(function);
        workspace.repeat_bytes.load(function);
        function.instruction(&Instruction::I64Add);
        workspace.live_bytes.store(function);
        workspace.live_bytes.load(function);
        function.instruction(&Instruction::I64Const(
            choice_entries::SNAPSHOT_HEADER_BYTES as i64,
        ));
        function.instruction(&Instruction::I64Add);
        workspace.snapshot_bytes.store(function);

        // Account for the caller's output prefix too. All counts have already
        // passed descriptor admission; this bound precedes any allocation.
        workspace.live_bytes.load(function);
        workspace.snapshot_bytes.load(function);
        function.instruction(&Instruction::I64Add);
        workspace.capture_bytes.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(REGEXP_MATCHER_SCRATCH_MAX_BYTES));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        workspace.fail(builder, RegExpMatcherFailure::ResourceExhausted, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(REGEXP_MATCHER_SCRATCH_MAX_BYTES));
        workspace.capture_bytes.load(function);
        function.instruction(&Instruction::I64Sub);
        workspace.live_bytes.load(function);
        function.instruction(&Instruction::I64Sub);
        workspace.maximum_capacity.store(function);
        workspace.snapshot_bytes.load(function);
        workspace.capacity.store(function);
        workspace.live_bytes.load(function);
        workspace.snapshot_bytes.load(function);
        function.instruction(&Instruction::I64Add);
        workspace.allocated_bytes.store(function);
        builder.emit_regexp_transient_allocation(
            workspace.allocated_bytes,
            workspace.base,
            &ScratchFailure::Matcher { checkpoint, start },
            function,
        )?;
        workspace.choices().reset(function);
        Ok(workspace)
    }

    pub(super) fn fail(
        &self,
        builder: &FunctionBuilder<'_>,
        failure: RegExpMatcherFailure,
        function: &mut Function,
    ) {
        builder.emit_regexp_match_result(
            self.checkpoint,
            self.start,
            self.start,
            RegExpMatcherResult::Failed(failure),
            function,
        );
        function.instruction(&Instruction::Return);
    }

    fn ensure_choice_bytes(
        &self,
        builder: &mut FunctionBuilder<'_>,
        required: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        function.instruction(&Instruction::GlobalGet(PRIVATE_BYTE_CURSOR_GLOBAL_INDEX));
        self.base.load(function);
        self.allocated_bytes.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.fail(builder, RegExpMatcherFailure::CorruptProgram, function);
        function.instruction(&Instruction::End);
        required.load(function);
        self.maximum_capacity.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.fail(builder, RegExpMatcherFailure::ResourceExhausted, function);
        function.instruction(&Instruction::End);
        required.load(function);
        self.capacity.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        let schema = builder.runtime_schema();
        let next = schema.reserve_i64_local(function);
        let extra = schema.reserve_i64_local(function);
        let extension = schema.reserve_i64_local(function);
        self.capacity.load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Mul);
        next.store(function);
        required.load(function);
        next.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        required.load(function);
        next.store(function);
        function.instruction(&Instruction::End);
        next.load(function);
        self.maximum_capacity.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.maximum_capacity.load(function);
        next.store(function);
        function.instruction(&Instruction::End);
        next.load(function);
        self.capacity.load(function);
        function.instruction(&Instruction::I64Sub);
        extra.store(function);
        builder.emit_regexp_transient_allocation(
            extra,
            extension,
            &ScratchFailure::Matcher {
                checkpoint: self.checkpoint,
                start: self.start,
            },
            function,
        )?;
        extension.load(function);
        self.base.load(function);
        self.allocated_bytes.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.fail(builder, RegExpMatcherFailure::CorruptProgram, function);
        function.instruction(&Instruction::End);
        self.allocated_bytes.load(function);
        extra.load(function);
        function.instruction(&Instruction::I64Add);
        self.allocated_bytes.store(function);
        next.load(function);
        self.capacity.store(function);
        schema.release_i64_local(extension, function);
        schema.release_i64_local(extra, function);
        schema.release_i64_local(next, function);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(super) fn choices(&self) -> ChoiceStack<'_> {
        ChoiceStack::new(self)
    }

    pub(super) fn reset_repeats(&self, f: &mut Function) {
        self.base.load(f);
        self.capture_bytes.load(f);
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::I32Const(0));
        self.repeat_bytes.load(f);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::MemoryFill(0));
    }

    pub(super) fn publish_captures(&self, output: I64Local, f: &mut Function) {
        output.load(f);
        f.instruction(&Instruction::I32WrapI64);
        self.base.load(f);
        f.instruction(&Instruction::I32WrapI64);
        self.capture_bytes.load(f);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        });
    }
}
