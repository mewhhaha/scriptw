//! One readiness bit per actual R instance, never a host/process cache.
use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct NormalizationTablesGlobal {
    index: u32,
}
impl NormalizationTablesGlobal {
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
}
impl RuntimeSchema {
    pub(crate) fn ensure_normalization_tables(
        &self,
        region: crate::data::NormalizationRegion,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::GlobalGet(self.normalization_tables.index));
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(region.start() as i32));
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32Const(region.length() as i32));
        function.instruction(&Instruction::MemoryInit {
            mem: 0,
            data_index: region.segment().index(),
        });
        // No allocation, coercion or JS call can expose partial readiness.
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::GlobalSet(self.normalization_tables.index));
        function.instruction(&Instruction::End);
    }
}
