//! A declaration-owned immutable undefined snapshot, private to this runtime.
//! StoredValue's fields are immutable. Descriptor replacement changes its own
//! reference fields; no user-visible descriptor or object is shared here.
use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct StoredUndefinedGlobal {
    index: u32,
}

impl StoredUndefinedGlobal {
    pub(super) fn declare(layouts: &GcLayoutRegistry, globals: &mut GlobalLedger) -> Self {
        let index = globals.len();
        globals.global(
            GlobalType {
                val_type: ValType::Ref(
                    layouts.reference(StoredValue::LAYOUT, GcNullability::NonNullable),
                ),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([
                Instruction::I64Const(0),
                Instruction::I32Const(crate::WasmRuntimeValueTag::Undefined as i32),
                Instruction::RefNull(RefType::EQREF.heap_type),
                Instruction::StructNew(layouts.struct_type::<StoredValue>().type_index().raw()),
            ]),
        );
        Self { index }
    }

    pub(super) fn index(&self) -> u32 {
        self.index
    }
}
