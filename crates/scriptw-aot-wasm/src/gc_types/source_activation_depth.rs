//! Checked ownership of the implementation's finite source execution stack.
//!
//! Runtime and program modules share the declaration-owned scalar. A rejected
//! entry returns a whole JavaScript Throw before an owner can be constructed.
//! The admitted owner restores its saved depth on each source-body exit.

use super::*;
use crate::emit::CompletionKind;

/// This is an implementation resource bound on simultaneously active source
/// bodies, including resumable bodies while executing. It is deliberately
/// conservative relative to the product's 32 MiB native Wasm stack. Native
/// stack space is not exposed by a portable Wasm instruction; this bound does
/// not purport to measure arbitrary native helper or generated frame sizes.
const MAX_ACTIVE_SOURCE_BODIES: i32 = 128;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SourceActivationDepthGlobal {
    index: u32,
}

impl SourceActivationDepthGlobal {
    fn emit_admission_test(&self, function: &mut Function) {
        function.instruction(&Instruction::GlobalGet(self.index));
        function.instruction(&Instruction::I32Const(MAX_ACTIVE_SOURCE_BODIES));
        function.instruction(&Instruction::I32LtU);
    }
    pub(super) fn declare(globals: &mut GlobalLedger) -> Self {
        let index = globals.len();
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(0),
        );
        Self { index }
    }

    fn enter<E>(
        &self,
        function: &mut Function,
        emit_overflow: impl FnOnce(&mut Function) -> Result<CompletionLocals, E>,
    ) -> Result<SourceActivationDepth, E> {
        let previous = function.reserve_typed_local(ValType::I32);
        function.instruction(&Instruction::GlobalGet(self.index));
        function.instruction(&Instruction::LocalSet(previous));
        function.instruction(&Instruction::LocalGet(previous));
        function.instruction(&Instruction::I32Const(MAX_ACTIVE_SOURCE_BODIES));
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        let overflow = emit_overflow(function)?;
        // The guard owns the terminal Throw, not merely a predicate which a
        // caller could forget to route or accidentally turn into a normal entry.
        overflow.set_kind(CompletionKind::Throw, function);
        overflow.emit(function);
        overflow.clear(function);
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::LocalGet(previous));
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        function.instruction(&Instruction::GlobalSet(self.index));
        Ok(SourceActivationDepth {
            global: self.index,
            previous,
        })
    }
}

/// Only a successful checked entry can produce this non-copyable owner.
/// Raw indices stay private and restore names the same declared scalar which
/// admitted the entry. Restoring one emitted branch does not release the local
/// needed by a sibling branch; final body cleanup consumes it separately.
#[must_use = "admitted source depth must be restored on every body exit"]
pub(crate) struct SourceActivationDepth {
    global: u32,
    previous: u32,
}

impl SourceActivationDepth {
    pub(crate) fn restore(&self, function: &mut Function) {
        function.instruction(&Instruction::LocalGet(self.previous));
        function.instruction(&Instruction::GlobalSet(self.global));
    }

    pub(crate) fn clear(self, function: &mut Function) {
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::LocalSet(self.previous));
        function.release_typed_local(self.previous);
    }
}

impl RuntimeSchema {
    pub(crate) fn emit_source_activation_admission_test(&self, function: &mut Function) {
        self.source_activation_depth.emit_admission_test(function);
    }

    pub(crate) fn enter_source_activation_depth<E>(
        &self,
        function: &mut Function,
        emit_overflow: impl FnOnce(&mut Function) -> Result<CompletionLocals, E>,
    ) -> Result<SourceActivationDepth, E> {
        self.source_activation_depth.enter(function, emit_overflow)
    }
}
