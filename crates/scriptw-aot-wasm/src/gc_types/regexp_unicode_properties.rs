//! One readiness bit per actual R instance, never a host/process cache.
use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct RegExpUnicodePropertiesGlobal {
    index: u32,
}
impl RegExpUnicodePropertiesGlobal {
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
    pub(crate) fn ensure_regexp_unicode_properties(
        &self,
        region: crate::data::RegExpUnicodePropertyRegion,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::GlobalGet(
            self.regexp_unicode_properties.index,
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
        // No allocation, coercion or JS call can expose partial readiness.
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::GlobalSet(
            self.regexp_unicode_properties.index,
        ));
        function.instruction(&Instruction::End);
    }
}
