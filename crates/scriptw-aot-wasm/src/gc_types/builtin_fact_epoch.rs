//! The builtin fact epoch: one counter, shared by the runtime and program
//! modules, that advances whenever state a builtin entry guard proved from
//! may have changed.
//!
//! A guard's whole proof is good for one epoch (`BuiltinEntryProof`), so an
//! entry compares one counter instead of reading every fact again. The
//! writers that must advance it are listed in `objects/builtin_fact_epoch.rs`;
//! only the two operations below touch the scalar.

use super::*;

/// 2^64 advances cannot happen, so an epoch never repeats.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct BuiltinFactEpochGlobal {
    index: u32,
}

impl BuiltinFactEpochGlobal {
    pub(super) fn declare(globals: &mut GlobalLedger) -> Self {
        let index = globals.len();
        globals.global(
            GlobalType {
                val_type: ValType::I64,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i64_const(0),
        );
        Self { index }
    }
}

impl RuntimeSchema {
    /// Leaves the current epoch on the stack.
    pub(crate) fn emit_builtin_fact_epoch(&self, function: &mut Function) {
        function.instruction(&Instruction::GlobalGet(self.builtin_fact_epoch.index));
    }

    /// Ends the epoch: every proof made in it is void from here on.
    pub(crate) fn emit_advance_builtin_fact_epoch(&self, function: &mut Function) {
        function.instruction(&Instruction::GlobalGet(self.builtin_fact_epoch.index));
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::GlobalSet(self.builtin_fact_epoch.index));
    }
}
