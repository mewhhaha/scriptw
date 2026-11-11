//! One linked choice arena owns every snapshot and reverse search.

use super::*;
mod required_run;
pub(in crate::builtins::regexp) use required_run::CheckedTemplateRunNode;

pub(super) const SNAPSHOT_HEADER_BYTES: u64 = 64;
const EMPTY_LINK: i64 = -1;

#[derive(Clone, Copy)]
pub(in crate::builtins::regexp) enum ChoiceEntryKind {
    Ordinary,
    GreedyProgress,
    LazyProgressChoice,
    LazyProgressAttempt,
    RequiredRun,
}

impl ChoiceEntryKind {
    pub(in crate::builtins::regexp) const fn word(self) -> i64 {
        match self {
            Self::Ordinary => 0,
            Self::GreedyProgress => 1,
            Self::LazyProgressChoice => 2,
            Self::LazyProgressAttempt => 3,
            Self::RequiredRun => 4,
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::builtins::regexp) enum SnapshotChoice {
    Ordinary {
        fallback: I64Local,
        origin: I64Local,
    },
    Progress {
        fallback: I64Local,
        origin: I64Local,
        lazy: I64Local,
    },
}

#[derive(Clone, Copy)]
pub(in crate::builtins::regexp) struct ChoiceStack<'workspace> {
    workspace: &'workspace MatcherWorkspace,
}

/// Only the stack's checked linked walk can produce an entry. Its restoration
/// methods retain their original workspace and accept no foreign live slab.
pub(in crate::builtins::regexp) struct ChoiceEntry<'entry> {
    stack: ChoiceStack<'entry>,
    offset: I64Local,
    address: I64Local,
}

#[derive(Clone, Copy)]
pub(in crate::builtins::regexp) enum ChoiceSnapshotWord {
    Kind,
    Fallback,
    Origin,
    Byte,
    Utf16,
    LowSurrogate,
}

impl ChoiceSnapshotWord {
    fn offset(self) -> u64 {
        match self {
            Self::Kind => 16,
            Self::Fallback => 24,
            Self::Origin => 32,
            Self::Byte => 40,
            Self::Utf16 => 48,
            Self::LowSurrogate => 56,
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::builtins::regexp) struct ChoiceCursor<'workspace> {
    stack: ChoiceStack<'workspace>,
    local: I64Local,
}

pub(in crate::builtins::regexp) struct ChoiceSnapshot<'entry> {
    entry: ChoiceEntry<'entry>,
}

/// A readonly leaf retains the checked original arena. Unlike a physical
/// entry it cannot restore, truncate, or manufacture a linked choice cursor.
pub(in crate::builtins::regexp) struct ChoiceSnapshotView<'leaf> {
    workspace: &'leaf MatcherWorkspace,
    address: I64Local,
}

/// Searches consume either a real snapshot or a selected continuation inside
/// its checked Run. This view cannot be used as a physical template cursor.
pub(in crate::builtins::regexp) enum LogicalChoiceEntry<'entry> {
    Snapshot(ChoiceSnapshot<'entry>),
    Run(required_run::LogicalRunEntry<'entry>),
}

#[derive(Clone, Copy)]
enum ChoiceSearchPredicate {
    Assertion(I64Local),
    Progress(I64Local),
}
impl ChoiceSearchPredicate {
    fn emit(self, address: I64Local, kind: Option<I64Local>, function: &mut Function) {
        let load_kind = |function: &mut Function| match kind {
            Some(kind) => kind.load(function),
            None => ChoiceStack::load(address, 16, function),
        };
        match self {
            Self::Assertion(pc) => {
                load_kind(function);
                function.instruction(&Instruction::I64Const(ChoiceEntryKind::Ordinary.word()));
                function.instruction(&Instruction::I64Eq);
                ChoiceStack::load(address, 24, function);
                pc.load(function);
                function.instruction(&Instruction::I64Eq);
                function.instruction(&Instruction::I32And);
            }
            Self::Progress(origin) => {
                load_kind(function);
                function.instruction(&Instruction::I64Const(
                    ChoiceEntryKind::GreedyProgress.word(),
                ));
                function.instruction(&Instruction::I64Eq);
                load_kind(function);
                function.instruction(&Instruction::I64Const(
                    ChoiceEntryKind::LazyProgressAttempt.word(),
                ));
                function.instruction(&Instruction::I64Eq);
                function.instruction(&Instruction::I32Or);
                ChoiceStack::load(address, 32, function);
                origin.load(function);
                function.instruction(&Instruction::I64Eq);
                function.instruction(&Instruction::I32And);
            }
        }
    }
}

impl<'workspace> ChoiceStack<'workspace> {
    pub(super) fn new(workspace: &'workspace MatcherWorkspace) -> Self {
        Self { workspace }
    }

    pub(in crate::builtins::regexp) fn reset(self, function: &mut Function) {
        self.clear_active_run(function);
        function.instruction(&Instruction::I64Const(EMPTY_LINK));
        self.workspace.choice_top.store(function);
        function.instruction(&Instruction::I64Const(0));
        self.workspace.choice_used.store(function);
    }

    pub(in crate::builtins::regexp) fn is_empty(self, function: &mut Function) {
        self.workspace.choice_top.load(function);
        function.instruction(&Instruction::I64Const(EMPTY_LINK));
        function.instruction(&Instruction::I64Eq);
    }

    pub(in crate::builtins::regexp) fn push_snapshot(
        self,
        builder: &mut FunctionBuilder<'_>,
        choice: SnapshotChoice,
        cursor: RegExpInputCursor,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = builder.runtime_schema();
        let required = schema.reserve_i64_local(function);
        let address = schema.reserve_i64_local(function);
        self.workspace.choice_used.load(function);
        self.workspace.snapshot_bytes.load(function);
        function.instruction(&Instruction::I64Add);
        required.store(function);
        self.workspace
            .ensure_choice_bytes(builder, required, function)?;
        self.address(self.workspace.choice_used, address, function);
        Self::store(address, 0, self.workspace.snapshot_bytes, function);
        Self::store(address, 8, self.workspace.choice_top, function);
        address.load(function);
        function.instruction(&Instruction::I32WrapI64);
        match choice {
            SnapshotChoice::Ordinary { .. } => {
                function.instruction(&Instruction::I64Const(ChoiceEntryKind::Ordinary.word()));
            }
            SnapshotChoice::Progress { lazy, .. } => {
                lazy.load(function);
                function.instruction(&Instruction::I32WrapI64);
                function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
                function.instruction(&Instruction::I64Const(
                    ChoiceEntryKind::LazyProgressChoice.word(),
                ));
                function.instruction(&Instruction::Else);
                function.instruction(&Instruction::I64Const(
                    ChoiceEntryKind::GreedyProgress.word(),
                ));
                function.instruction(&Instruction::End);
            }
        }
        function.instruction(&Instruction::I64Store(FunctionBuilder::memarg8(16)));
        let fallback = match choice {
            SnapshotChoice::Ordinary { fallback, .. }
            | SnapshotChoice::Progress { fallback, .. } => fallback,
        };
        Self::store(address, 24, fallback, function);
        address.load(function);
        function.instruction(&Instruction::I32WrapI64);
        match choice {
            SnapshotChoice::Ordinary { origin, .. } | SnapshotChoice::Progress { origin, .. } => {
                origin.load(function)
            }
        }
        function.instruction(&Instruction::I64Store(FunctionBuilder::memarg8(32)));
        for (offset, local) in [
            (40, cursor.byte),
            (48, cursor.utf16),
            (56, cursor.on_low_surrogate),
        ] {
            Self::store(address, offset, local, function);
        }
        address.load(function);
        function.instruction(&Instruction::I64Const(SNAPSHOT_HEADER_BYTES as i64));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        self.workspace.base.load(function);
        function.instruction(&Instruction::I32WrapI64);
        self.workspace.live_bytes.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        });
        self.workspace.choice_used.load(function);
        self.workspace.choice_top.store(function);
        required.load(function);
        self.workspace.choice_used.store(function);
        schema.release_i64_local(address, function);
        schema.release_i64_local(required, function);
        Ok(())
    }

    pub(in crate::builtins::regexp) fn top_cursor(
        self,
        output: I64Local,
        function: &mut Function,
    ) -> ChoiceCursor<'workspace> {
        self.workspace.choice_top.load(function);
        output.store(function);
        ChoiceCursor {
            stack: self,
            local: output,
        }
    }

    pub(in crate::builtins::regexp) fn reserve_top_cursor(
        self,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) -> ChoiceCursor<'workspace> {
        self.top_cursor(
            builder.runtime_schema().reserve_i64_local(function),
            function,
        )
    }

    pub(in crate::builtins::regexp) fn with_top<R>(
        self,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
        finish: impl for<'entry> FnOnce(
            ChoiceEntry<'entry>,
            &mut FunctionBuilder<'_>,
            &mut Function,
        ) -> R,
    ) -> R {
        let schema = builder.runtime_schema();
        let offset = schema.reserve_i64_local(function);
        let address = schema.reserve_i64_local(function);
        self.workspace.choice_top.load(function);
        offset.store(function);
        self.check_entry(builder, offset, address, function);
        let result = finish(
            ChoiceEntry {
                stack: self,
                offset,
                address,
            },
            builder,
            function,
        );
        schema.release_i64_local(address, function);
        schema.release_i64_local(offset, function);
        result
    }

    pub(in crate::builtins::regexp) fn find_assertion(
        self,
        builder: &mut FunctionBuilder<'_>,
        failure_pc: I64Local,
        function: &mut Function,
        finish: impl for<'entry> FnMut(
            LogicalChoiceEntry<'entry>,
            &mut FunctionBuilder<'_>,
            &mut Function,
        ) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        self.find(
            builder,
            function,
            ChoiceSearchPredicate::Assertion(failure_pc),
            finish,
        )
    }

    pub(in crate::builtins::regexp) fn find_progress(
        self,
        builder: &mut FunctionBuilder<'_>,
        origin: I64Local,
        function: &mut Function,
        finish: impl for<'entry> FnMut(
            LogicalChoiceEntry<'entry>,
            &mut FunctionBuilder<'_>,
            &mut Function,
        ) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        self.find(
            builder,
            function,
            ChoiceSearchPredicate::Progress(origin),
            finish,
        )
    }

    fn find(
        self,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
        predicate: ChoiceSearchPredicate,
        mut finish: impl for<'entry> FnMut(
            LogicalChoiceEntry<'entry>,
            &mut FunctionBuilder<'_>,
            &mut Function,
        ) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let schema = builder.runtime_schema();
        let offset = schema.reserve_i64_local(function);
        let address = schema.reserve_i64_local(function);
        let found = schema.reserve_i32_local(function);
        self.workspace.choice_top.load(function);
        offset.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        self.check_entry(builder, offset, address, function);
        let entry = ChoiceEntry {
            stack: self,
            offset,
            address,
        };
        entry.is_kind(ChoiceEntryKind::RequiredRun, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        entry.find_required_run(builder, predicate, found, function, &mut finish)?;
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        found.store(function);
        predicate.emit(address, None, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        entry.with_snapshot(builder, function, |snapshot, builder, function| {
            finish(LogicalChoiceEntry::Snapshot(snapshot), builder, function)
        })?;
        function.instruction(&Instruction::I32Const(1));
        found.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        found.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);
        Self::load(address, 8, function);
        offset.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i64_local(address, function);
        schema.release_i64_local(offset, function);
        schema.release_i32_local(found, function);
        Ok(())
    }

    fn address(self, offset: I64Local, output: I64Local, function: &mut Function) {
        self.workspace.base.load(function);
        self.workspace.live_bytes.load(function);
        function.instruction(&Instruction::I64Add);
        offset.load(function);
        function.instruction(&Instruction::I64Add);
        output.store(function);
    }

    fn check_entry(
        self,
        builder: &FunctionBuilder<'_>,
        offset: I64Local,
        address: I64Local,
        function: &mut Function,
    ) {
        offset.load(function);
        self.workspace.choice_used.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.workspace
            .fail(builder, RegExpMatcherFailure::CorruptProgram, function);
        function.instruction(&Instruction::End);
        self.workspace.choice_used.load(function);
        offset.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(24));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.workspace
            .fail(builder, RegExpMatcherFailure::CorruptProgram, function);
        function.instruction(&Instruction::End);
        self.address(offset, address, function);
        Self::load(address, 0, function);
        function.instruction(&Instruction::I64Const(24));
        function.instruction(&Instruction::I64LtU);
        Self::load(address, 0, function);
        self.workspace.choice_used.load(function);
        offset.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        Self::load(address, 16, function);
        function.instruction(&Instruction::I64Const(ChoiceEntryKind::RequiredRun.word()));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.workspace
            .fail(builder, RegExpMatcherFailure::CorruptProgram, function);
        function.instruction(&Instruction::End);
        Self::load(address, 16, function);
        function.instruction(&Instruction::I64Const(ChoiceEntryKind::RequiredRun.word()));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        Self::load(address, 0, function);
        function.instruction(&Instruction::I64Const(required_run::HEADER_BYTES as i64));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.workspace
            .fail(builder, RegExpMatcherFailure::CorruptProgram, function);
        function.instruction(&Instruction::End);
        required_run::validate_header(address, self.workspace, builder, function);
        function.instruction(&Instruction::Else);
        Self::load(address, 0, function);
        self.workspace.snapshot_bytes.load(function);
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.workspace
            .fail(builder, RegExpMatcherFailure::CorruptProgram, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        Self::load(address, 8, function);
        function.instruction(&Instruction::I64Const(EMPTY_LINK));
        function.instruction(&Instruction::I64Ne);
        Self::load(address, 8, function);
        offset.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.workspace
            .fail(builder, RegExpMatcherFailure::CorruptProgram, function);
        function.instruction(&Instruction::End);
    }

    fn load(address: I64Local, offset: u64, function: &mut Function) {
        address.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load(FunctionBuilder::memarg8(offset)));
    }

    fn store(address: I64Local, offset: u64, value: I64Local, function: &mut Function) {
        address.load(function);
        function.instruction(&Instruction::I32WrapI64);
        value.load(function);
        function.instruction(&Instruction::I64Store(FunctionBuilder::memarg8(offset)));
    }
}

impl<'workspace> ChoiceCursor<'workspace> {
    pub(in crate::builtins::regexp) fn local(self) -> I64Local {
        self.local
    }

    pub(in crate::builtins::regexp) fn move_to_previous(
        self,
        entry: &ChoiceEntry<'_>,
        function: &mut Function,
    ) {
        assert!(
            core::ptr::eq(self.stack.workspace, entry.stack.workspace),
            "choice cursors retain one workspace"
        );
        entry.load(8, function);
        self.local.store(function);
    }

    pub(in crate::builtins::regexp) fn release(
        self,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        builder
            .runtime_schema()
            .release_i64_local(self.local, function);
    }

    pub(in crate::builtins::regexp) fn is_empty(self, function: &mut Function) {
        self.local.load(function);
        function.instruction(&Instruction::I64Const(EMPTY_LINK));
        function.instruction(&Instruction::I64Eq);
    }

    pub(in crate::builtins::regexp) fn with_entry_at<R>(
        self,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
        finish: impl for<'entry> FnOnce(
            ChoiceEntry<'entry>,
            &mut FunctionBuilder<'_>,
            &mut Function,
        ) -> R,
    ) -> R {
        let schema = builder.runtime_schema();
        let address = schema.reserve_i64_local(function);
        self.stack
            .check_entry(builder, self.local, address, function);
        let result = finish(
            ChoiceEntry {
                stack: self.stack,
                offset: self.local,
                address,
            },
            builder,
            function,
        );
        schema.release_i64_local(address, function);
        result
    }
}

impl ChoiceEntry<'_> {
    fn load(&self, offset: u64, function: &mut Function) {
        ChoiceStack::load(self.address, offset, function);
    }

    pub(in crate::builtins::regexp) fn offset(&self, output: I64Local, function: &mut Function) {
        self.offset.load(function);
        output.store(function);
    }

    pub(in crate::builtins::regexp) fn length(&self, output: I64Local, function: &mut Function) {
        self.load(0, function);
        output.store(function);
    }

    pub(in crate::builtins::regexp) fn is_snapshot(&self, function: &mut Function) {
        self.load(16, function);
        function.instruction(&Instruction::I64Const(
            ChoiceEntryKind::LazyProgressAttempt.word(),
        ));
        function.instruction(&Instruction::I64LeU);
    }

    pub(in crate::builtins::regexp) fn with_snapshot<R>(
        self,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
        finish: impl for<'entry> FnOnce(
            ChoiceSnapshot<'entry>,
            &mut FunctionBuilder<'_>,
            &mut Function,
        ) -> R,
    ) -> R {
        self.is_snapshot(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.stack
            .workspace
            .fail(builder, RegExpMatcherFailure::CorruptProgram, function);
        function.instruction(&Instruction::End);
        finish(ChoiceSnapshot { entry: self }, builder, function)
    }

    pub(in crate::builtins::regexp) fn is_kind(
        &self,
        kind: ChoiceEntryKind,
        function: &mut Function,
    ) {
        self.load(16, function);
        function.instruction(&Instruction::I64Const(kind.word()));
        function.instruction(&Instruction::I64Eq);
    }

    pub(in crate::builtins::regexp) fn activate_lazy_attempt(&self, function: &mut Function) {
        self.address.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Const(
            ChoiceEntryKind::LazyProgressAttempt.word(),
        ));
        function.instruction(&Instruction::I64Store(FunctionBuilder::memarg8(16)));
    }

    pub(in crate::builtins::regexp) fn discard_through(&self, function: &mut Function) {
        self.stack.clear_removed_active_run(self.offset, function);
        self.load(8, function);
        self.stack.workspace.choice_top.store(function);
        self.offset.load(function);
        self.stack.workspace.choice_used.store(function);
    }

    pub(in crate::builtins::regexp) fn restore_fallback(
        &self,
        pc: I64Local,
        function: &mut Function,
    ) {
        self.load(24, function);
        pc.store(function);
    }

    pub(in crate::builtins::regexp) fn restore_cursor(
        &self,
        cursor: RegExpInputCursor,
        function: &mut Function,
    ) {
        for (offset, local) in [
            (40, cursor.byte),
            (48, cursor.utf16),
            (56, cursor.on_low_surrogate),
        ] {
            self.load(offset, function);
            local.store(function);
        }
    }

    pub(in crate::builtins::regexp) fn load_utf16(&self, function: &mut Function) {
        self.load(48, function);
    }

    fn restore_slice(&self, start: Option<I64Local>, bytes: I64Local, function: &mut Function) {
        self.stack.workspace.base.load(function);
        if let Some(start) = start {
            start.load(function);
            function.instruction(&Instruction::I64Add);
        }
        function.instruction(&Instruction::I32WrapI64);
        self.address.load(function);
        function.instruction(&Instruction::I64Const(SNAPSHOT_HEADER_BYTES as i64));
        function.instruction(&Instruction::I64Add);
        if let Some(start) = start {
            start.load(function);
            function.instruction(&Instruction::I64Add);
        }
        function.instruction(&Instruction::I32WrapI64);
        bytes.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        });
    }

    pub(in crate::builtins::regexp) fn restore_state(&self, function: &mut Function) {
        self.restore_slice(None, self.stack.workspace.live_bytes, function);
    }
    pub(in crate::builtins::regexp) fn restore_captures(&self, function: &mut Function) {
        self.restore_slice(None, self.stack.workspace.capture_bytes, function);
    }
    pub(in crate::builtins::regexp) fn restore_repeats(&self, function: &mut Function) {
        self.restore_slice(
            Some(self.stack.workspace.capture_bytes),
            self.stack.workspace.repeat_bytes,
            function,
        );
    }
}

impl ChoiceSnapshotView<'_> {
    pub(in crate::builtins::regexp) fn load_header(
        &self,
        word: ChoiceSnapshotWord,
        output: I64Local,
        f: &mut Function,
    ) {
        ChoiceStack::load(self.address, word.offset(), f);
        output.store(f);
    }

    pub(in crate::builtins::regexp) fn snapshot_slab_address(
        &self,
        output: I64Local,
        f: &mut Function,
    ) {
        self.address.load(f);
        f.instruction(&Instruction::I64Const(SNAPSHOT_HEADER_BYTES as i64));
        f.instruction(&Instruction::I64Add);
        output.store(f);
    }
}

impl LogicalChoiceEntry<'_> {
    pub(in crate::builtins::regexp) fn restore_cursor(
        &self,
        cursor: RegExpInputCursor,
        function: &mut Function,
    ) {
        match self {
            Self::Snapshot(snapshot) => snapshot.entry.restore_cursor(cursor, function),
            Self::Run(entry) => entry.restore_cursor(cursor, function),
        }
    }
    pub(in crate::builtins::regexp) fn restore_captures(&self, function: &mut Function) {
        match self {
            Self::Snapshot(snapshot) => snapshot.entry.restore_captures(function),
            Self::Run(entry) => entry.restore_captures(function),
        }
    }
    pub(in crate::builtins::regexp) fn restore_repeats(
        &self,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        match self {
            Self::Snapshot(snapshot) => snapshot.entry.restore_repeats(function),
            Self::Run(entry) => entry.restore_repeats(builder, function),
        }
    }
    pub(in crate::builtins::regexp) fn discard_through(
        &self,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        match self {
            Self::Snapshot(snapshot) => {
                snapshot
                    .entry
                    .stack
                    .invalidate_active_run(builder, function);
                snapshot.entry.discard_through(function);
            }
            Self::Run(entry) => entry.discard_through(builder, function),
        }
    }
    pub(in crate::builtins::regexp) fn load_utf16(&self, function: &mut Function) {
        match self {
            Self::Snapshot(snapshot) => snapshot.entry.load_utf16(function),
            Self::Run(entry) => entry.load_utf16(function),
        }
    }
}
