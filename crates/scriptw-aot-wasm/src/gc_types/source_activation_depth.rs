//! Checked ownership of the implementation's finite source execution stack.
//!
//! Runtime and program modules share the declaration-owned scalar. A rejected
//! entry returns a whole JavaScript Throw before an owner can be constructed:
//! a program body tail-calls one program-owned helper that builds the
//! RangeError; an R-owned body builds it inline.
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
        emit_overflow: impl FnOnce(&mut Function) -> Result<SourceDepthOverflow, E>,
    ) -> Result<SourceActivationDepth, E> {
        let previous = function.reserve_typed_local(ValType::I32);
        function.instruction(&Instruction::GlobalGet(self.index));
        function.instruction(&Instruction::LocalSet(previous));
        function.instruction(&Instruction::LocalGet(previous));
        function.instruction(&Instruction::I32Const(MAX_ACTIVE_SOURCE_BODIES));
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        // The guard owns the terminal transfer, not merely a predicate which
        // a caller could forget to route or turn into a normal entry.
        match emit_overflow(function)? {
            SourceDepthOverflow::ProgramHelper(SourceDepthOverflowTail { .. }) => {}
            SourceDepthOverflow::Inline(overflow) => {
                overflow.set_kind(CompletionKind::Throw, function);
                overflow.emit(function);
                overflow.clear(function);
                function.instruction(&Instruction::Return);
            }
        }
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

/// How a rejected source entry ends. Either way its Throw is the body's
/// completion and no parameter, activation or reflection effect happened.
#[must_use = "the depth guard must end the rejected entry"]
pub(crate) enum SourceDepthOverflow {
    /// A program body tail-calls the one program-owned helper that builds
    /// the RangeError, instead of a copy of it in every body.
    ProgramHelper(SourceDepthOverflowTail),
    /// An R-owned source body cannot reach a P helper: it builds the Throw
    /// inline and the guard returns it.
    Inline(CompletionLocals),
}

/// Proof that a rejected entry ended in a tail call of the program's
/// `source_depth_overflow` helper. Only
/// [`RuntimeSchema::return_call_source_depth_overflow`] mints it.
pub(crate) struct SourceDepthOverflowTail {
    _tail: (),
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

    /// Returns the saved-depth local to the pool without a store: nothing
    /// reads it again, and the next holder writes it before reading it.
    pub(crate) fn clear(self, function: &mut Function) {
        function.release_typed_local(self.previous);
    }
}

impl RuntimeSchema {
    /// A closed region may represent several source entries without publishing
    /// intermediate activations. Decline before effects unless all would fit.
    pub(crate) fn emit_source_activation_admission_test_for_depth(
        &self,
        additional: u32,
        function: &mut Function,
    ) {
        assert!(additional > 0 && additional <= MAX_ACTIVE_SOURCE_BODIES as u32);
        function.instruction(&Instruction::GlobalGet(self.source_activation_depth.index));
        function.instruction(&Instruction::I32Const(
            MAX_ACTIVE_SOURCE_BODIES - additional as i32,
        ));
        function.instruction(&Instruction::I32LeU);
    }
    pub(crate) fn emit_source_activation_admission_test(&self, function: &mut Function) {
        self.source_activation_depth.emit_admission_test(function);
    }

    pub(crate) fn enter_source_activation_depth<E>(
        &self,
        function: &mut Function,
        emit_overflow: impl FnOnce(&mut Function) -> Result<SourceDepthOverflow, E>,
    ) -> Result<SourceActivationDepth, E> {
        self.source_activation_depth.enter(function, emit_overflow)
    }

    /// Ends a rejected source entry: the program-owned helper builds the
    /// RangeError of `realm` (the rejected body's execution Realm) and its
    /// Throw completion becomes this body's result. Source bodies return the
    /// Completion ABI the helper returns, so the tail call is exact.
    pub(crate) fn return_call_source_depth_overflow(
        &self,
        realm: &GcLocal<RealmRecord>,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> SourceDepthOverflowTail {
        self.return_call_helper(
            crate::runtime_helpers::SourceDepthOverflowArguments::new(realm),
            base,
            function,
        );
        SourceDepthOverflowTail { _tail: () }
    }
}
