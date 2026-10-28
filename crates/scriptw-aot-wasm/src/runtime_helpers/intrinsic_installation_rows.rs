//! Checked immutable-row reads retain builtin/key helper-argument authority.
use super::*;
use crate::data::RInstallationRows;
use wasm_encoder::BlockType;

pub(crate) enum RuntimeHelperStandardBuiltinArgument<'a> {
    Literal(scriptw_ir::StandardBuiltinId),
    Parameter(&'a RuntimeHelperStandardBuiltinParameter),
}
impl From<scriptw_ir::StandardBuiltinId> for RuntimeHelperStandardBuiltinArgument<'_> {
    fn from(value: scriptw_ir::StandardBuiltinId) -> Self {
        Self::Literal(value)
    }
}
impl RuntimeHelperStandardBuiltinArgument<'_> {
    pub(super) fn emit(&self, f: &mut Function) {
        match self {
            Self::Literal(id) => {
                f.instruction(&Instruction::I32Const(*id as i32));
            }
            Self::Parameter(parameter) => parameter.local.load(f),
        }
    }
}

pub(crate) enum RuntimeHelperIntrinsicKeyArgument<'a> {
    Literal(&'a crate::operations::IntrinsicInstallationKey),
    Parameter(&'a RuntimeHelperIntrinsicKeyParameter),
}
impl<'a> From<&'a crate::operations::IntrinsicInstallationKey>
    for RuntimeHelperIntrinsicKeyArgument<'a>
{
    fn from(value: &'a crate::operations::IntrinsicInstallationKey) -> Self {
        Self::Literal(value)
    }
}
impl RuntimeHelperIntrinsicKeyArgument<'_> {
    pub(super) fn emit(&self, f: &mut Function) {
        match self {
            Self::Literal(key) => key.emit(f),
            Self::Parameter(key) => {
                key.is_symbol.load(f);
                key.ordinal.load(f);
            }
        }
    }
}

/// Acquired only by the helper ABI whose callers require RInstallationSpan.
pub(crate) struct RuntimeHelperInstallationSpanParameter {
    address: I32Local,
    count: I32Local,
}
impl RuntimeHelperInstallationSpanParameter {
    pub(super) fn acquire(s: &RuntimeSchema, index: u32, f: &mut Function) -> Self {
        Self {
            address: s.parameter_i32(index, f),
            count: s.parameter_i32(index + 1, f),
        }
    }

    pub(crate) fn emit_for_each(
        &self,
        rows: RInstallationRows,
        s: &RuntimeSchema,
        f: &mut Function,
        emit: impl FnOnce(&RuntimeHelperInstallationRow, &mut Function),
    ) {
        let offset = s.reserve_i32_local(f);
        self.address.load(f);
        f.instruction(&Instruction::I32Const(rows.address() as i32));
        f.instruction(&Instruction::I32Sub);
        offset.store(f);
        // Subtraction rejects addresses below the table through unsigned range
        // checking. Count is bounded before multiplication or any memory read.
        offset.load(f);
        f.instruction(&Instruction::I32Const(rows.byte_length() as i32));
        f.instruction(&Instruction::I32GtU);
        offset.load(f);
        f.instruction(&Instruction::I32Const(RInstallationRows::ROW_BYTES as i32));
        f.instruction(&Instruction::I32RemU);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        self.count.load(f);
        f.instruction(&Instruction::I32Const(rows.byte_length() as i32));
        offset.load(f);
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(RInstallationRows::ROW_BYTES as i32));
        f.instruction(&Instruction::I32DivU);
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        s.release_i32_local(offset, f);

        let cursor = s.reserve_i32_local(f);
        self.address.load(f);
        cursor.store(f);
        let remaining = s.reserve_i32_local(f);
        self.count.load(f);
        remaining.store(f);
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        remaining.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::BrIf(1));

        // This is the sole dynamic constructor of the builtin/key parameter
        // proofs. The checked cursor addresses compiler-validated immutable
        // rows; neither a Program ordinal nor an arbitrary i32 can enter it.
        let read = |word, f: &mut Function| {
            let local = s.reserve_i32_local(f);
            cursor.load(f);
            f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                offset: word * 4,
                align: 0,
                memory_index: 0,
            }));
            local.store(f);
            local
        };
        let builtin = RuntimeHelperStandardBuiltinParameter { local: read(0, f) };
        let key = RuntimeHelperIntrinsicKeyParameter {
            is_symbol: read(1, f),
            ordinal: read(2, f),
        };
        let attributes = read(3, f);
        let writable = s.reserve_i32_local(f);
        attributes.load(f);
        f.instruction(&Instruction::I32Const(RInstallationRows::WRITABLE as i32));
        f.instruction(&Instruction::I32And);
        writable.store(f);
        let configurable = s.reserve_i32_local(f);
        attributes.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32And);
        configurable.store(f);
        let row = RuntimeHelperInstallationRow {
            builtin,
            key,
            writable,
            configurable,
        };
        emit(&row, f);
        s.release_i32_local(configurable, f);
        s.release_i32_local(writable, f);
        s.release_i32_local(attributes, f);
        s.release_i32_local(row.key.ordinal, f);
        s.release_i32_local(row.key.is_symbol, f);
        s.release_i32_local(row.builtin.local, f);

        cursor.load(f);
        f.instruction(&Instruction::I32Const(RInstallationRows::ROW_BYTES as i32));
        f.instruction(&Instruction::I32Add);
        cursor.store(f);
        remaining.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        remaining.store(f);
        f.instruction(&Instruction::Br(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        s.release_i32_local(remaining, f);
        s.release_i32_local(cursor, f);
    }
}

pub(crate) struct RuntimeHelperInstallationRow {
    builtin: RuntimeHelperStandardBuiltinParameter,
    key: RuntimeHelperIntrinsicKeyParameter,
    pub(crate) writable: I32Local,
    pub(crate) configurable: I32Local,
}
impl RuntimeHelperInstallationRow {
    pub(crate) fn builtin(&self) -> RuntimeHelperStandardBuiltinArgument<'_> {
        RuntimeHelperStandardBuiltinArgument::Parameter(&self.builtin)
    }
    pub(crate) fn key(&self) -> RuntimeHelperIntrinsicKeyArgument<'_> {
        RuntimeHelperIntrinsicKeyArgument::Parameter(&self.key)
    }
}
