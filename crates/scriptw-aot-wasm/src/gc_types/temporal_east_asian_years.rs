//! One readiness bit per actual R instance, shared by its Chinese/Dangi readers.
use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct TemporalEastAsianYearsGlobal {
    index: u32,
}

impl TemporalEastAsianYearsGlobal {
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
    pub(crate) fn ensure_temporal_east_asian_years(
        &self,
        region: crate::data::TemporalEastAsianYearRegion,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::GlobalGet(
            self.temporal_east_asian_years.index,
        ));
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(region.start() as i32));
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32Const(region.length() as i32));
        function.instruction(&Instruction::MemoryInit {
            mem: 0,
            data_index: region.segment().index(),
        });
        // memory.init cannot call JavaScript or allocate a GC object. Publish
        // readiness only after the entire immutable image has been installed.
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::GlobalSet(
            self.temporal_east_asian_years.index,
        ));
        function.instruction(&Instruction::End);
    }
}
