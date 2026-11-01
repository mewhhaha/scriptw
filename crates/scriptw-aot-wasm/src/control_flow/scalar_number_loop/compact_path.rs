//! Static own-data paths are validated across the complete read-only region.
use super::invariant_imul::CachedRuntimeName;
use super::*;
use crate::data::PooledLiteralDomain;

#[derive(PartialEq, Eq)]
pub(super) enum CachedReadName {
    Runtime(CachedRuntimeName),
    Program(ProgramLiteralIndex),
}

impl CachedReadName {
    fn new(builder: &FunctionBuilder<'_>, name: &str) -> Option<Self> {
        let index = builder.strings.pooled_string_index(name).ok()?;
        Some(match builder.strings.pooled_string_domain(index).ok()? {
            PooledLiteralDomain::Runtime(_) => {
                Self::Runtime(CachedRuntimeName::new(name, builder.strings)?)
            }
            PooledLiteralDomain::Program(index) => Self::Program(index),
        })
    }

    /// P literals already exist. A missing R cache entry declines; this guard
    /// never resolves a name or allocates a String merely to speculate.
    fn cached(
        &self,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) -> GcLocal<StringValue, Nullable> {
        let s = builder.runtime_schema();
        match self {
            Self::Program(index) => s.reserve_gc_local(function).initialize(
                builder
                    .current_program()
                    .literal(s, *index, function)
                    .nullable(),
                function,
            ),
            Self::Runtime(name) => name.cached(builder, function),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct CompactPathId(usize);

impl CompactPathId {
    pub(super) fn new(index: usize) -> Self {
        Self(index)
    }
}

/// These are the complete first-batch paths, not a general property evaluator.
#[derive(PartialEq, Eq)]
pub(super) enum CompactReadPath {
    Number(CachedReadName),
    ArrayNumber(CachedReadName, LocalBindingId),
}

impl CompactReadPath {
    pub(super) fn new<'a>(
        builder: &FunctionBuilder<'_>,
        read: PropertyReadView<'a>,
    ) -> Option<(Self, &'a TypedExpr)> {
        if let dense_array::ReadKey::StaticString(name) = read.key {
            return Some((
                Self::Number(CachedReadName::new(builder, name)?),
                read.target,
            ));
        }
        let dense_array::ReadKey::Computed(index) = read.key else {
            return None;
        };
        let index = dense_array::private_local(builder, index)?;
        let inner = PropertyReadView::of(read.target)?;
        let dense_array::ReadKey::StaticString(name) = inner.key else {
            return None;
        };
        let row = inner.target;
        Some((
            Self::ArrayNumber(CachedReadName::new(builder, name)?, index),
            row,
        ))
    }

    pub(super) fn input(&self) -> Option<LocalBindingId> {
        match self {
            Self::Number(_) => None,
            Self::ArrayNumber(_, index) => Some(*index),
        }
    }

    fn name(&self) -> &CachedReadName {
        match self {
            Self::Number(name) | Self::ArrayNumber(name, _) => name,
        }
    }

    fn tag(&self) -> WasmRuntimeValueTag {
        match self {
            Self::Number(_) => WasmRuntimeValueTag::Number,
            Self::ArrayNumber(_, _) => WasmRuntimeValueTag::Array,
        }
    }
}

/// The canonical table/shape plus checked slot and column metadata suffice.
/// No projected values, retained rows or alternative storage are allocated.
pub(super) struct CompactPathBorrow<'a> {
    paths: &'a [CompactReadPath],
    values: GcLocal<PackedValueTable, Nullable>,
    shape: GcLocal<CompactDataShape, Nullable>,
    slots: Vec<I32Local>,
    columns: Vec<Option<I32Local>>,
    numbers: Vec<F64Local>,
}

impl<'a> CompactPathBorrow<'a> {
    pub(super) fn new(
        paths: &'a [CompactReadPath],
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Self {
        let s = builder.runtime_schema();
        Self {
            paths,
            values: s.reserve_gc_local(function).initialize_null(s, function),
            shape: s.reserve_gc_local(function).initialize_null(s, function),
            slots: paths
                .iter()
                .map(|_| s.reserve_i32_local(function))
                .collect(),
            columns: paths
                .iter()
                .map(|path| path.input().map(|_| s.reserve_i32_local(function)))
                .collect(),
            numbers: paths
                .iter()
                .map(|_| s.reserve_f64_local(function))
                .collect(),
        }
    }

    pub(super) fn capture(
        &self,
        array: &GcLocal<ArrayObject, Nullable>,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let s = builder.runtime_schema();
        self.values.replace(
            s.field(ArrayObjectSchema::VALUES)
                .read(array, s, function)
                .reference(),
            function,
        );
    }

    pub(super) fn emit_capacity(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        s.array_type::<PackedValueTable>()
            .length(&self.values, s, function);
    }

    pub(super) fn missing(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        self.values
            .load(builder.runtime_schema(), function)
            .is_null(function);
    }

    /// The scalar entry guard owns Number/initialization and the sealed plan
    /// excludes writes to each column throughout test/body/update.
    pub(super) fn capture_columns(
        &self,
        builder: &FunctionBuilder<'_>,
        declined: ControlTarget,
        function: &mut Function,
    ) {
        for (path, column) in self.paths.iter().zip(&self.columns) {
            if let (Some(id), Some(column)) = (path.input(), column) {
                let bits = builder.local_binding(id).value().scalar();
                // The enclosing scalar guard establishes Number/initialization.
                for (bound, op) in [
                    (0.0, Instruction::F64Ge),
                    ((1 << 20) as f64, Instruction::F64Le),
                ] {
                    bits.load(function);
                    function.instruction(&Instruction::F64ReinterpretI64);
                    function.instruction(&Instruction::F64Const(Ieee64::from(bound)));
                    function.instruction(&op);
                    function.instruction(&Instruction::I32Eqz);
                    builder.emit_branch_if_to_target(declined, function);
                }
                bits.load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                bits.load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                function.instruction(&Instruction::F64Trunc);
                function.instruction(&Instruction::F64Ne);
                builder.emit_branch_if_to_target(declined, function);
                bits.load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                function.instruction(&Instruction::I32TruncSatF64U);
                column.store(function);
            }
        }
    }

    /// Validate and capture the complete current row before any body effect.
    /// Every miss clears temporary roots, then the caller publishes the completed
    /// prefix and resumes its original loop at the untouched current counter.
    pub(super) fn try_prepare_row(
        &self,
        slot: I32Local,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> I32Local {
        let s = builder.runtime_schema();
        let accepted = s.reserve_i32_local(function);
        accepted.set_constant(0, function);
        let key_length = s.reserve_i32_local(function);
        let scratch = RowScratch::new(builder, function);
        let actual_shape = s
            .reserve_gc_local::<CompactDataShape, Nullable>(function)
            .initialize_null(s, function);
        let keys = s
            .reserve_gc_local::<CompactShapeKeys, Nullable>(function)
            .initialize_null(s, function);
        // Compiler-owned roots are all initialized before any emitted decline;
        // name cache reads occur only while selecting the first actual shape.
        let names = self
            .paths
            .iter()
            .map(|_| {
                s.reserve_gc_local::<StringValue, Nullable>(function)
                    .initialize_null(s, function)
            })
            .collect::<Vec<_>>();
        let declined = builder.open_frame(ControlFrameKind::Block, function);
        scratch.load_row(&self.values, slot, builder, function);
        scratch.value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Object.tag()));
        function.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(declined, function);
        scratch.value.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<OrdinaryObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(declined, function);
        scratch.load_storage(builder, function);
        scratch.base.load(s, function);
        function.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<InlineCompactDataStorage>(
                crate::gc_types::GcNullability::NonNullable,
            )
            .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(declined, function);
        scratch.cast_inline(builder, function);
        actual_shape.replace(
            s.field(InlineCompactDataStorageSchema::SHAPE)
                .read(&scratch.inline, s, function)
                .reference()
                .nullable(),
            function,
        );
        self.shape.load(s, function).is_null(function);
        builder.open_frame(ControlFrameKind::If, function);
        self.shape.replace(actual_shape.load(s, function), function);
        keys.replace(
            s.field(CompactDataShapeSchema::KEYS)
                .read(&actual_shape, s, function)
                .reference()
                .nullable(),
            function,
        );
        s.array_type::<CompactShapeKeys>()
            .length(&keys, s, function);
        key_length.store(function);
        key_length.load(function);
        function.instruction(&Instruction::I32Const(2));
        function.instruction(&Instruction::I32GtU);
        builder.emit_branch_if_to_target(declined, function);
        for ((path, slot), name) in self.paths.iter().zip(&self.slots).zip(&names) {
            let cached = path.name().cached(builder, function);
            name.replace(cached.load(s, function), function);
            cached.clear(function);
            name.load(s, function).is_null(function);
            builder.emit_branch_if_to_target(declined, function);
            slot.set_constant(0, function);
            let found = builder.open_frame(ControlFrameKind::Block, function);
            let probe = builder.open_frame(ControlFrameKind::Loop, function);
            slot.load(function);
            key_length.load(function);
            function.instruction(&Instruction::I32GeU);
            builder.emit_branch_if_to_target(declined, function);
            let actual = s.reserve_gc_local(function).initialize(
                s.array_type::<CompactShapeKeys>()
                    .read(&keys, *slot, s, function)
                    .reference(),
                function,
            );
            let expected = s
                .reserve_gc_local(function)
                .initialize(name.load(s, function).require_non_null(function), function);
            builder.emit_string_payload_equality_i32(&actual, &expected, function);
            expected.clear(function);
            actual.clear(function);
            builder.emit_branch_if_to_target(found, function);
            slot.load(function);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32Add);
            slot.store(function);
            builder.emit_branch_to_target(probe, function);
            builder.pop_control(ControlFrameKind::Loop);
            function.instruction(&Instruction::End);
            builder.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
        }
        function.instruction(&Instruction::Else);
        actual_shape.load(s, function);
        self.shape.load(s, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(declined, function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for (((path, slot), column), number) in self
            .paths
            .iter()
            .zip(&self.slots)
            .zip(&self.columns)
            .zip(&self.numbers)
        {
            scratch.read_slot(*slot, builder, function);
            scratch.value.tag().load(function);
            function.instruction(&Instruction::I32Const(path.tag().tag()));
            function.instruction(&Instruction::I32Ne);
            builder.emit_branch_if_to_target(declined, function);
            if let Some(column) = column {
                scratch.value.reference().load(function);
                function.instruction(&Instruction::RefTestNonNull(
                    s.reference_type::<ArrayObject>(crate::gc_types::GcNullability::NonNullable)
                        .heap_type,
                ));
                function.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(declined, function);
                scratch.cast_array(builder, function);
                s.field(ArrayObjectSchema::MODE)
                    .read(&scratch.array, s, function);
                function.instruction(&Instruction::I32Const(
                    ArrayIndexedStorageMode::PackedNumbers.encode(),
                ));
                function.instruction(&Instruction::I32Ne);
                builder.emit_branch_if_to_target(declined, function);
                s.field(ArrayObjectSchema::LENGTH)
                    .read(&scratch.array, s, function)
                    .store_i64(scratch.length, function);
                scratch.length.load(function);
                function.instruction(&Instruction::I64Const(1 << 20));
                function.instruction(&Instruction::I64GtU);
                builder.emit_branch_if_to_target(declined, function);
                s.field(ArrayObjectSchema::COUNT)
                    .read(&scratch.array, s, function);
                scratch.length.load(function);
                function.instruction(&Instruction::I64Ne);
                builder.emit_branch_if_to_target(declined, function);
                column.load(function);
                function.instruction(&Instruction::I64ExtendI32U);
                scratch.length.load(function);
                function.instruction(&Instruction::I64GeU);
                builder.emit_branch_if_to_target(declined, function);
                scratch.load_numbers(builder, function);
                s.array_type::<NumberBitsI64Array>()
                    .length(&scratch.numbers, s, function);
                function.instruction(&Instruction::I64ExtendI32U);
                scratch.length.load(function);
                function.instruction(&Instruction::I64LtU);
                builder.emit_branch_if_to_target(declined, function);
                s.array_type::<NumberBitsI64Array>()
                    .read(&scratch.numbers, *column, s, function);
            } else {
                scratch.value.scalar().load(function);
            }
            function.instruction(&Instruction::F64ReinterpretI64);
            number.store(function);
        }
        accepted.set_constant(1, function);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        for name in names {
            name.clear(function);
        }
        keys.clear(function);
        actual_shape.clear(function);
        scratch.release(builder, function);
        s.release_i32_local(key_length, function);
        accepted
    }

    pub(super) fn emit_number(&self, path: CompactPathId, function: &mut Function) {
        self.numbers[path.0].load(function);
    }

    pub(super) fn clear_roots(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        self.values.set_null(s, function);
        self.shape.set_null(s, function);
    }

    pub(super) fn release(self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        self.values.clear(function);
        self.shape.clear(function);
        for slot in self.slots {
            s.release_i32_local(slot, function);
        }
        for column in self.columns.into_iter().flatten() {
            s.release_i32_local(column, function);
        }
        for number in self.numbers {
            s.release_f64_local(number, function);
        }
    }
}

/// One temporary projection into the existing canonical GC layouts. Its one
/// release is outside every row decline edge and before each source body.
struct RowScratch {
    stored: GcLocal<StoredValue, Nullable>,
    header: GcLocal<OrdinaryObject, Nullable>,
    base: GcLocal<ObjectPropertyStorage, Nullable>,
    inline: GcLocal<InlineCompactDataStorage, Nullable>,
    array: GcLocal<ArrayObject, Nullable>,
    numbers: GcLocal<NumberBitsI64Array, Nullable>,
    length: I64Local,
    value: ValueLocals,
}

impl RowScratch {
    fn new(builder: &FunctionBuilder<'_>, function: &mut Function) -> Self {
        let s = builder.runtime_schema();
        let value = s.reserve_value_local(function);
        value.set_undefined(function);
        Self {
            stored: s.reserve_gc_local(function).initialize_null(s, function),
            header: s.reserve_gc_local(function).initialize_null(s, function),
            base: s.reserve_gc_local(function).initialize_null(s, function),
            inline: s.reserve_gc_local(function).initialize_null(s, function),
            array: s.reserve_gc_local(function).initialize_null(s, function),
            numbers: s.reserve_gc_local(function).initialize_null(s, function),
            length: s.reserve_i64_local(function),
            value,
        }
    }

    fn load_row(
        &self,
        values: &GcLocal<PackedValueTable, Nullable>,
        slot: I32Local,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let s = builder.runtime_schema();
        self.stored.replace(
            s.array_type::<PackedValueTable>()
                .read(values, slot, s, function)
                .reference(),
            function,
        );
        // Dense canonical PackedValues proves this own slot is nonnull.
        s.field(StoredValueSchema::TAG)
            .read(&self.stored, s, function)
            .store(self.value.tag(), function);
        s.field(StoredValueSchema::REFERENCE)
            .read(&self.stored, s, function)
            .store_eq(self.value.reference(), function);
    }

    fn load_storage(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        self.header.replace(
            self.value
                .cast_reference::<OrdinaryObject>(s, function)
                .nullable(),
            function,
        );
        self.base.replace(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&self.header, s, function)
                .reference()
                .nullable(),
            function,
        );
    }

    fn cast_inline(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        self.inline.replace(
            self.base
                .load(s, function)
                .cast_subtype::<CompactDataStorage>(s, function)
                .cast_subtype::<InlineCompactDataStorage>(s, function)
                .nullable(),
            function,
        );
    }

    fn read_slot(
        &self,
        slot: I32Local,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let s = builder.runtime_schema();
        s.field(InlineCompactDataStorageSchema::PACKED_TAGS)
            .read(&self.inline, s, function);
        slot.load(function);
        function.instruction(&Instruction::I32Const(8));
        function.instruction(&Instruction::I32Mul);
        function.instruction(&Instruction::I32ShrU);
        function.instruction(&Instruction::I32Const(255));
        function.instruction(&Instruction::I32And);
        self.value.tag().store(function);
        slot.load(function);
        function.instruction(&Instruction::I32Eqz);
        builder.open_frame(ControlFrameKind::If, function);
        s.field(InlineCompactDataStorageSchema::SCALAR0)
            .read(&self.inline, s, function)
            .store_i64(self.value.scalar(), function);
        s.field(InlineCompactDataStorageSchema::REFERENCE0)
            .read(&self.inline, s, function)
            .store_eq(self.value.reference(), function);
        function.instruction(&Instruction::Else);
        s.field(InlineCompactDataStorageSchema::SCALAR1)
            .read(&self.inline, s, function)
            .store_i64(self.value.scalar(), function);
        s.field(InlineCompactDataStorageSchema::REFERENCE1)
            .read(&self.inline, s, function)
            .store_eq(self.value.reference(), function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    fn cast_array(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        self.array.replace(
            self.value
                .cast_reference::<ArrayObject>(s, function)
                .nullable(),
            function,
        );
    }

    fn load_numbers(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        self.numbers.replace(
            s.field(ArrayObjectSchema::NUMBERS)
                .read(&self.array, s, function)
                .reference()
                .nullable(),
            function,
        );
    }

    fn release(self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        self.value.clear(function);
        self.numbers.clear(function);
        self.array.clear(function);
        self.inline.clear(function);
        self.base.clear(function);
        self.header.clear(function);
        self.stored.clear(function);
        s.release_i64_local(self.length, function);
    }
}
