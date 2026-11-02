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
        let read = |word: u64, f: &mut Function| {
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

/// A row admitted from one exact complete R layout. Its private fields retain
/// builtin, descriptor and key authority across the runtime representation.
pub(crate) struct RuntimeHelperPropertyLayoutRow {
    key: RuntimeHelperIntrinsicKeyParameter,
    flags: I32Local,
    group: I32Local,
    kind: I32Local,
    payload: [I32Local; 3],
    raw_key_hash: I64Local,
}
impl RuntimeHelperPropertyLayoutRow {
    pub(crate) fn key(&self) -> &RuntimeHelperIntrinsicKeyParameter {
        &self.key
    }
    pub(crate) fn flags(&self) -> I32Local {
        self.flags
    }
    pub(crate) fn group(&self) -> I32Local {
        self.group
    }
    pub(crate) fn kind(&self) -> I32Local {
        self.kind
    }
    pub(crate) fn payload0(&self) -> I32Local {
        self.payload[0]
    }
    pub(crate) fn payload1(&self) -> I32Local {
        self.payload[1]
    }
    pub(crate) fn payload2(&self) -> I32Local {
        self.payload[2]
    }
    pub(crate) fn raw_key_hash(&self) -> I64Local {
        self.raw_key_hash
    }

    /// Side tags belong to the sealed Accessor row. Data methods use their
    /// separate row kind; no absent or canonical side can become a builtin ID.
    pub(crate) fn emit_callable_kind(&self, setter: bool, f: &mut Function) {
        self.payload[2].load(f);
        if setter {
            f.instruction(&Instruction::I32Const(2));
            f.instruction(&Instruction::I32ShrU);
        }
        f.instruction(&Instruction::I32Const(3));
        f.instruction(&Instruction::I32And);
    }

    pub(crate) fn emit_well_known_symbol_ordinal(&self, f: &mut Function) {
        self.kind.load(f);
        f.instruction(&Instruction::I32Const(
            crate::intrinsics::property_layouts::IntrinsicPropertyRecipeKind::WellKnownSymbol
                as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        self.payload[0].load(f);
    }

    /// Kind checking prevents a numeric payload from becoming builtin authority.
    /// The returned view borrows the row's local; only row.clear retires it.
    pub(crate) fn method_builtin(
        &self,
        setter: bool,
        f: &mut Function,
    ) -> RuntimeHelperStandardBuiltinParameter {
        use crate::intrinsics::property_layouts::IntrinsicPropertyRecipeKind as Kind;
        self.kind.load(f);
        f.instruction(&Instruction::I32Const(Kind::Accessor as i32));
        f.instruction(&Instruction::I32Eq);
        self.emit_callable_kind(setter, f);
        f.instruction(&Instruction::I32Const(
            crate::intrinsics::property_layouts::IntrinsicCallableRecipeKind::Method as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        if !setter {
            self.kind.load(f);
            f.instruction(&Instruction::I32Const(Kind::Method as i32));
            f.instruction(&Instruction::I32Eq);
            f.instruction(&Instruction::I32Or);
        }
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        let mut index = 0;
        if setter {
            index = 1;
        }
        RuntimeHelperStandardBuiltinParameter {
            local: self.payload[index],
        }
    }

    pub(crate) fn clear(self, s: &RuntimeSchema, f: &mut Function) {
        s.release_i64_local(self.raw_key_hash, f);
        for local in self.payload.into_iter().rev() {
            s.release_i32_local(local, f);
        }
        s.release_i32_local(self.kind, f);
        s.release_i32_local(self.group, f);
        s.release_i32_local(self.flags, f);
        s.release_i32_local(self.key.ordinal, f);
        s.release_i32_local(self.key.is_symbol, f);
    }
}

impl RuntimeHelperPropertyLayoutRow {
    /// Only the sealed span constructor creates this storage type. Its immutable
    /// address/count retain the R-table proof; only a dynamic slot needs checking.
    pub(crate) fn from_storage(
        storage: &GcLocal<IntrinsicPropertyStorage>,
        index: I32Local,
        s: &RuntimeSchema,
        f: &mut Function,
    ) -> Self {
        let count = s.reserve_i32_local(f);
        s.field(IntrinsicPropertyStorageSchema::COUNT)
            .read(storage, s, f)
            .store(count, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        s.release_i32_local(count, f);
        let address = s.reserve_i32_local(f);
        s.field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(storage, s, f)
            .store(address, f);
        let cursor = s.reserve_i32_local(f);
        address.load(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(
            crate::data::RPropertyLayoutRows::ROW_BYTES as i32,
        ));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Add);
        cursor.store(f);
        let read = |word: u64, f: &mut Function| {
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
        let key = RuntimeHelperIntrinsicKeyParameter {
            is_symbol: read(0, f),
            ordinal: read(1, f),
        };
        let flags = read(2, f);
        let group = read(3, f);
        let kind = read(4, f);
        let payload = [read(5, f), read(6, f), read(7, f)];
        let raw_key_hash = s.reserve_i64_local(f);
        cursor.load(f);
        f.instruction(&Instruction::I64Load(wasm_encoder::MemArg {
            offset: 32,
            align: 0,
            memory_index: 0,
        }));
        raw_key_hash.store(f);
        s.release_i32_local(cursor, f);
        s.release_i32_local(address, f);
        RuntimeHelperPropertyLayoutRow {
            key,
            flags,
            group,
            kind,
            payload,
            raw_key_hash,
        }
    }
}
