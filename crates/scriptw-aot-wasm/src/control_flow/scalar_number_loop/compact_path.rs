//! Static own-data paths are validated across the complete read-only region.
mod dictionary;
mod projection;
use super::invariant_imul::CachedRuntimeName;
use super::*;
use crate::data::PooledLiteralDomain;
use dictionary::DictionaryScratch;
use projection::RowProjection;

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
    StringLength(CachedReadName),
    ArrayNumber(CachedReadName, LocalBindingId),
}

impl CompactReadPath {
    pub(super) fn new<'a>(
        builder: &FunctionBuilder<'_>,
        read: PropertyReadView<'a>,
    ) -> Option<(Self, &'a TypedExpr)> {
        if matches!(
            read.key,
            dense_array::ReadKey::Length | dense_array::ReadKey::StaticString("length")
        ) {
            if let Some(inner) = PropertyReadView::of(read.target) {
                if let dense_array::ReadKey::StaticString(name) = inner.key {
                    return Some((
                        Self::StringLength(CachedReadName::new(builder, name)?),
                        inner.target,
                    ));
                }
            }
        }
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
            Self::Number(_) | Self::StringLength(_) => None,
            Self::ArrayNumber(_, index) => Some(*index),
        }
    }

    fn name(&self) -> &CachedReadName {
        match self {
            Self::Number(name) | Self::StringLength(name) | Self::ArrayNumber(name, _) => name,
        }
    }

    fn tag(&self) -> WasmRuntimeValueTag {
        match self {
            Self::Number(_) => WasmRuntimeValueTag::Number,
            Self::StringLength(_) => WasmRuntimeValueTag::String,
            Self::ArrayNumber(_, _) => WasmRuntimeValueTag::Array,
        }
    }
}

/// Borrow the canonical table/shape and checked slot/column metadata. The
/// optional source alias is published from this same table at the completed
/// prefix exit; no projected objects or extra retained-row roots are needed.
pub(super) struct CompactPathBorrow<'a> {
    paths: &'a [CompactReadPath],
    values: GcLocal<PackedValueTable, Nullable>,
    shape: GcLocal<CompactDataShape, Nullable>,
    storage_kind: I32Local,
    names: Vec<GcLocal<StringValue, Nullable>>,
    slots: Vec<I32Local>,
    columns: Vec<Option<I32Local>>,
    numbers: Vec<F64Local>,
    row_alias: Option<LocalBindingId>,
    projection: Option<RowProjection>,
}

impl<'a> CompactPathBorrow<'a> {
    pub(super) fn new(
        paths: &'a [CompactReadPath],
        row_alias: Option<LocalBindingId>,
        projection_period: Option<u32>,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Self {
        let s = builder.runtime_schema();
        let storage_kind = s.reserve_i32_local(function);
        storage_kind.set_constant(RowStorageKind::Inline as i32, function);
        Self {
            paths,
            row_alias,
            projection: RowProjection::new(projection_period, paths, builder, function),
            values: s.reserve_gc_local(function).initialize_null(s, function),
            shape: s.reserve_gc_local(function).initialize_null(s, function),
            storage_kind,
            names: paths
                .iter()
                .map(|_| s.reserve_gc_local(function).initialize_null(s, function))
                .collect(),
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

    /// Positive-trip admission prepares the first complete row exactly once.
    /// Only the returned owner can emit established-shape next-row guards.
    pub(super) fn prepare_loop(
        &self,
        slot: I32Local,
        length: I64Local,
        mask: Option<u32>,
        done: ControlTarget,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<PreparedCompactRows<'_, 'a>, EmitError> {
        let prepared = PreparedCompactRows {
            rows: self,
            slot,
            length,
            mask,
            done,
        };
        prepared.guard_bound(builder, function);
        if let Some(projection) = &self.projection {
            projection.guard_reuse(slot, length, done, builder, function);
            projection.prepare(self, slot, done, builder, function)?;
            projection.read_row(self, slot, builder, function);
        } else {
            prepared.guard_row(RowSetup::First, builder, function);
        }
        Ok(prepared)
    }

    /// Every row guard captures the complete footprint before a source body and
    /// clears temporary roots before a miss reaches shared prefix publication.
    fn try_prepare_row(
        &self,
        setup: RowSetup,
        slot: I32Local,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> I32Local {
        let s = builder.runtime_schema();
        let accepted = s.reserve_i32_local(function);
        accepted.set_constant(0, function);
        let scratch = RowScratch::new(builder, function);
        let dictionary = match setup {
            RowSetup::Established(RowStorageKind::Inline) => None,
            RowSetup::First | RowSetup::Established(RowStorageKind::Dictionary) => {
                Some(DictionaryScratch::new(builder, function))
            }
        };
        let first = match setup {
            RowSetup::First => Some(FirstRowKeys::new(self, builder, function)),
            RowSetup::Established(_) => None,
        };
        let declined = builder.open_frame(ControlFrameKind::Block, function);
        scratch.load_row(&self.values, slot, builder, function);
        match setup {
            RowSetup::First => {
                scratch.load_function_or_ordinary_storage(builder, declined, function);
                scratch.base.load(s, function);
                function.instruction(&Instruction::RefTestNonNull(
                    s.reference_type::<InlineCompactDataStorage>(
                        crate::gc_types::GcNullability::NonNullable,
                    )
                    .heap_type,
                ));
                builder.open_frame(ControlFrameKind::If, function);
                scratch.cast_inline(builder, function);
                if let Some(first) = &first {
                    self.storage_kind
                        .set_constant(RowStorageKind::Inline as i32, function);
                    self.shape.replace(
                        s.field(InlineCompactDataStorageSchema::SHAPE)
                            .read(&scratch.inline, s, function)
                            .reference()
                            .nullable(),
                        function,
                    );
                    first.resolve(builder, declined, function);
                }
                function.instruction(&Instruction::Else);
                dictionary
                    .as_ref()
                    .expect("first-row dictionary scratch")
                    .capture(&scratch, builder, declined, function);
                if let Some(first) = &first {
                    self.storage_kind
                        .set_constant(RowStorageKind::Dictionary as i32, function);
                    first.resolve_dictionary(
                        &scratch,
                        dictionary.as_ref().expect("first-row dictionary scratch"),
                        builder,
                        declined,
                        function,
                    );
                }
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            RowSetup::Established(RowStorageKind::Inline) => {
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
                s.field(InlineCompactDataStorageSchema::SHAPE)
                    .read(&scratch.inline, s, function);
                self.shape.load(s, function);
                function.instruction(&Instruction::RefEq);
                function.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(declined, function);
            }
            RowSetup::Established(RowStorageKind::Dictionary) => {
                scratch.load_function_or_ordinary_storage(builder, declined, function);
                dictionary
                    .as_ref()
                    .expect("dictionary-mode scratch")
                    .capture(&scratch, builder, declined, function);
            }
        }
        for ((((path, slot), column), number), name) in self
            .paths
            .iter()
            .zip(&self.slots)
            .zip(&self.columns)
            .zip(&self.numbers)
            .zip(&self.names)
        {
            match setup {
                RowSetup::First => {
                    self.storage_kind.load(function);
                    builder.open_frame(ControlFrameKind::If, function);
                    dictionary
                        .as_ref()
                        .expect("first-row dictionary scratch")
                        .read_slot(&scratch, *slot, name, builder, declined, function);
                    function.instruction(&Instruction::Else);
                    scratch.read_slot(*slot, builder, function);
                    builder.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
                RowSetup::Established(RowStorageKind::Inline) => {
                    scratch.read_slot(*slot, builder, function)
                }
                RowSetup::Established(RowStorageKind::Dictionary) => dictionary
                    .as_ref()
                    .expect("dictionary-mode scratch")
                    .read_slot(&scratch, *slot, name, builder, declined, function),
            }
            scratch.value.tag().load(function);
            function.instruction(&Instruction::I32Const(path.tag().tag()));
            function.instruction(&Instruction::I32Ne);
            builder.emit_branch_if_to_target(declined, function);
            if matches!(path, CompactReadPath::StringLength(_)) {
                scratch.value.reference().load(function);
                function.instruction(&Instruction::RefTestNonNull(
                    s.reference_type::<StringValue>(crate::gc_types::GcNullability::NonNullable)
                        .heap_type,
                ));
                function.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(declined, function);
                let string = s.reserve_gc_local(function).initialize(
                    scratch.value.cast_reference::<StringValue>(s, function),
                    function,
                );
                let units = s.reserve_gc_local(function).initialize(
                    s.field(StringValueSchema::CODE_UNITS)
                        .read(&string, s, function)
                        .reference(),
                    function,
                );
                s.array_type::<CodeUnitArray>().length(&units, s, function);
                function.instruction(&Instruction::F64ConvertI32U);
                units.clear(function);
                string.clear(function);
            } else {
                if let Some(column) = column {
                    scratch.value.reference().load(function);
                    function.instruction(&Instruction::RefTestNonNull(
                        s.reference_type::<ArrayObject>(
                            crate::gc_types::GcNullability::NonNullable,
                        )
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
                    s.array_type::<NumberBitsI64Array>().read(
                        &scratch.numbers,
                        *column,
                        s,
                        function,
                    );
                } else {
                    scratch.value.scalar().load(function);
                }
                function.instruction(&Instruction::F64ReinterpretI64);
            }
            number.store(function);
        }
        accepted.set_constant(1, function);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if let Some(first) = first {
            first.release(builder, function);
        }
        if let Some(dictionary) = dictionary {
            dictionary.release(function);
        }
        scratch.release(builder, function);
        accepted
    }

    pub(super) fn publish_row_alias(
        &self,
        iterated: I32Local,
        next_slot: I32Local,
        mask: u32,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let Some(id) = self.row_alias else {
            return;
        };
        let s = builder.runtime_schema();
        iterated.load(function);
        builder.open_frame(ControlFrameKind::If, function);
        // The mirror advances only after a complete source body/update. No
        // admitted operation or guard writes this table or reenters source.
        // Even on a failed next-row guard, the previous slot is unchanged.
        let last = s.reserve_i32_local(function);
        next_slot.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        function.instruction(&Instruction::I32Const(mask as i32));
        function.instruction(&Instruction::I32And);
        last.store(function);
        let stored = s.reserve_gc_local(function).initialize(
            s.array_type::<PackedValueTable>()
                .read(&self.values, last, s, function)
                .reference()
                .require_non_null(function),
            function,
        );
        s.release_i32_local(last, function);
        let binding = builder.local_binding(id);
        s.struct_type::<StoredValue>()
            .read_into(&stored, binding.value(), s, function);
        binding.mark_initialized(function);
        stored.clear(function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    pub(super) fn emit_number(&self, path: CompactPathId, function: &mut Function) {
        self.numbers[path.0].load(function);
    }

    pub(super) fn clear_roots(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        if let Some(projection) = &self.projection {
            projection.clear_roots(builder, function);
        }
        self.values.set_null(s, function);
        self.shape.set_null(s, function);
        for name in &self.names {
            name.set_null(s, function);
        }
    }

    pub(super) fn release(self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        if let Some(projection) = self.projection {
            projection.release(function);
        }
        self.values.clear(function);
        self.shape.clear(function);
        s.release_i32_local(self.storage_kind, function);
        for name in self.names {
            name.clear(function);
        }
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

#[derive(Clone, Copy)]
#[repr(i32)]
pub(super) enum RowStorageKind {
    Inline = 0,
    Dictionary = 1,
}

#[derive(Clone, Copy)]
pub(super) enum RowLoopKind {
    Guarded(RowStorageKind),
    Projected,
}

#[derive(Clone, Copy)]
enum RowSetup {
    First,
    Established(RowStorageKind),
}

/// Construction emits first-row admission before returning this capability.
/// Its bound and next-row guard retain the same borrow and publication target.
pub(super) struct PreparedCompactRows<'a, 'p> {
    rows: &'a CompactPathBorrow<'p>,
    slot: I32Local,
    length: I64Local,
    mask: Option<u32>,
    done: ControlTarget,
}

impl PreparedCompactRows<'_, '_> {
    fn guard_bound(&self, builder: &mut FunctionBuilder<'_>, function: &mut Function) {
        self.slot.load(function);
        self.length.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(self.done, function);
    }

    fn guard_row(
        &self,
        setup: RowSetup,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let masked = self.mask.map(|mask| {
            let slot = builder.runtime_schema().reserve_i32_local(function);
            self.slot.load(function);
            function.instruction(&Instruction::I32Const(mask as i32));
            function.instruction(&Instruction::I32And);
            slot.store(function);
            slot
        });
        let ready =
            self.rows
                .try_prepare_row(setup, masked.unwrap_or(self.slot), builder, function);
        if let Some(slot) = masked {
            builder.runtime_schema().release_i32_local(slot, function);
        }
        ready.load(function);
        builder.runtime_schema().release_i32_local(ready, function);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(self.done, function);
    }

    pub(super) fn is_projected(&self) -> bool {
        self.rows.projection.is_some()
    }

    pub(super) fn emit_dictionary_mode(&self, function: &mut Function) {
        self.rows.storage_kind.load(function);
    }

    pub(super) fn prepare_next(
        &self,
        kind: RowLoopKind,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        self.guard_bound(builder, function);
        match kind {
            RowLoopKind::Guarded(kind) => {
                self.guard_row(RowSetup::Established(kind), builder, function);
            }
            RowLoopKind::Projected => {
                self.rows
                    .projection
                    .as_ref()
                    .expect("prepared projection owner")
                    .read_row(self.rows, self.slot, builder, function);
            }
        }
    }
}

struct FirstRowKeys<'a, 'p> {
    rows: &'a CompactPathBorrow<'p>,
    keys: GcLocal<CompactShapeKeys, Nullable>,
    length: I32Local,
}

impl<'a, 'p> FirstRowKeys<'a, 'p> {
    fn new(
        rows: &'a CompactPathBorrow<'p>,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Self {
        let s = builder.runtime_schema();
        Self {
            rows,
            keys: s.reserve_gc_local(function).initialize_null(s, function),
            length: s.reserve_i32_local(function),
        }
    }

    fn resolve(
        &self,
        builder: &mut FunctionBuilder<'_>,
        declined: ControlTarget,
        function: &mut Function,
    ) {
        let s = builder.runtime_schema();
        self.keys.replace(
            s.field(CompactDataShapeSchema::KEYS)
                .read(&self.rows.shape, s, function)
                .reference()
                .nullable(),
            function,
        );
        s.array_type::<CompactShapeKeys>()
            .length(&self.keys, s, function);
        self.length.store(function);
        self.length.load(function);
        function.instruction(&Instruction::I32Const(2));
        function.instruction(&Instruction::I32GtU);
        builder.emit_branch_if_to_target(declined, function);
        for ((path, slot), name) in self
            .rows
            .paths
            .iter()
            .zip(&self.rows.slots)
            .zip(&self.rows.names)
        {
            let cached = path.name().cached(builder, function);
            name.replace(cached.load(s, function), function);
            cached.clear(function);
            name.load(s, function).is_null(function);
            builder.emit_branch_if_to_target(declined, function);
            slot.set_constant(0, function);
            let found = builder.open_frame(ControlFrameKind::Block, function);
            let probe = builder.open_frame(ControlFrameKind::Loop, function);
            slot.load(function);
            self.length.load(function);
            function.instruction(&Instruction::I32GeU);
            builder.emit_branch_if_to_target(declined, function);
            let actual = s.reserve_gc_local(function).initialize(
                s.array_type::<CompactShapeKeys>()
                    .read(&self.keys, *slot, s, function)
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
    }

    fn release(self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        self.keys.clear(function);
        builder
            .runtime_schema()
            .release_i32_local(self.length, function);
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
