//! One canonical indexed Array borrow belongs to the complete scalar region.
use super::*;

pub(super) enum ArrayRead {
    Length,
    Element,
    CompactPath(CompactPathId),
}

/// Construction fixes the sole receiver, induction binding and loop shape.
pub(super) struct ArrayReadLoop {
    array: LocalBindingId,
    counter: LocalBindingId,
    paths: ReadPaths,
}

enum ReadPaths {
    LengthOnly,
    Numbers,
    Compact(Vec<CompactReadPath>),
}

pub(super) fn private_local(
    builder: &FunctionBuilder<'_>,
    expression: &TypedExpr,
) -> Option<LocalBindingId> {
    let ExprIr::Identifier(name) = &expression.expr else {
        return None;
    };
    builder.private_local_binding(name)
}

/// A borrowed projection of the two canonical property-read carriers. It
/// recognizes syntax only; the sealed Array plan still admits the exact key,
/// receiver and induction binding before any read can enter a Number region.
#[derive(Clone, Copy)]
pub(super) struct PropertyReadView<'a> {
    pub(super) target: &'a TypedExpr,
    pub(super) key: ReadKey<'a>,
}

#[derive(Clone, Copy)]
pub(super) enum ReadKey<'a> {
    StaticString(&'a str),
    Computed(&'a TypedExpr),
    Length,
}

impl ReadKey<'_> {
    fn is_length(self) -> bool {
        match self {
            Self::Length => true,
            Self::StaticString(key) => key == "length",
            Self::Computed(_) => false,
        }
    }
}

impl<'a> PropertyReadView<'a> {
    pub(super) fn of(value: &'a TypedExpr) -> Option<Self> {
        let (target, key) = match &value.expr {
            ExprIr::PropertyRead { target, key } => {
                let key = match key {
                    PropertyKeyIr::StaticString(key) => ReadKey::StaticString(key),
                    PropertyKeyIr::StringExpr(key) | PropertyKeyIr::ArrayIndex(key) => {
                        ReadKey::Computed(key)
                    }
                    PropertyKeyIr::ArrayLength => ReadKey::Length,
                };
                (target.as_ref(), key)
            }
            ExprIr::SpecOperation {
                operation: SpecOperationIr::GetV,
                operands,
            } => {
                let [target, key] = operands.as_slice() else {
                    return None;
                };
                let key = match &key.expr {
                    ExprIr::String(key) => ReadKey::StaticString(key),
                    ExprIr::Identifier(_) => ReadKey::Computed(key),
                    _ => return None,
                };
                (target, key)
            }
            _ => return None,
        };
        Some(Self { target, key })
    }
}

impl ArrayReadLoop {
    pub(super) fn new(
        builder: &FunctionBuilder<'_>,
        test: &TypedExpr,
        update: Option<&TypedExpr>,
    ) -> Option<Self> {
        let (lhs, rhs) = match &test.expr {
            ExprIr::CompareNumber {
                op: RelationalBinaryOp::LessThan,
                lhs,
                rhs,
            }
            | ExprIr::CompareValue {
                op: RelationalBinaryOp::LessThan,
                lhs,
                rhs,
            } => (lhs, rhs),
            _ => return None,
        };
        let counter = private_local(builder, lhs)?;
        let read = PropertyReadView::of(rhs)?;
        if !read.key.is_length() {
            return None;
        }
        let array = private_local(builder, read.target)?;
        let ExprIr::UpdateIdentifier {
            name,
            op: NumericUpdateOp::Increment,
            value_kind,
            ..
        } = &update?.expr
        else {
            return None;
        };
        if matches!(value_kind, NumericUpdateValueKind::BigInt)
            || builder.private_local_binding(name) != Some(counter)
            || array == counter
        {
            return None;
        }
        Some(Self {
            array,
            counter,
            paths: ReadPaths::LengthOnly,
        })
    }

    pub(super) fn is_array(&self, id: LocalBindingId) -> bool {
        self.array == id
    }

    pub(super) fn counter(&self, bindings: &[NumberBinding]) -> Option<usize> {
        bindings
            .iter()
            .position(|binding| binding.id == self.counter)
    }

    pub(super) fn is_compact(&self) -> bool {
        matches!(self.paths, ReadPaths::Compact(_))
    }

    fn is_counter_key(&self, builder: &FunctionBuilder<'_>, key: ReadKey<'_>) -> bool {
        let ReadKey::Computed(index) = key else {
            return false;
        };
        private_local(builder, index) == Some(self.counter)
    }

    pub(super) fn read(
        &mut self,
        builder: &FunctionBuilder<'_>,
        read: PropertyReadView<'_>,
    ) -> Option<(ArrayRead, Option<LocalBindingId>)> {
        if private_local(builder, read.target) == Some(self.array) {
            if read.key.is_length() {
                return Some((ArrayRead::Length, None));
            }
            if !self.is_counter_key(builder, read.key) || self.is_compact() {
                return None;
            }
            self.paths = ReadPaths::Numbers;
            return Some((ArrayRead::Element, None));
        }
        let (path, row) = CompactReadPath::new(builder, read)?;
        let row = PropertyReadView::of(row)?;
        if private_local(builder, row.target) != Some(self.array)
            || !self.is_counter_key(builder, row.key)
            || path
                .input()
                .is_some_and(|id| id == self.array || id == self.counter)
        {
            return None;
        }
        let input = path.input();
        if matches!(self.paths, ReadPaths::LengthOnly) {
            self.paths = ReadPaths::Compact(Vec::new());
        }
        let ReadPaths::Compact(paths) = &mut self.paths else {
            return None;
        };
        if let Some(index) = paths.iter().position(|existing| existing == &path) {
            return Some((ArrayRead::CompactPath(CompactPathId::new(index)), input));
        }
        if paths.len() == 2 {
            return None;
        }
        let index = paths.len();
        paths.push(path);
        Some((ArrayRead::CompactPath(CompactPathId::new(index)), input))
    }

    pub(super) fn body_preserves_counter(
        &self,
        bindings: &[NumberBinding],
        body: &[NumberExpression],
    ) -> bool {
        let Some(counter) = self.counter(bindings) else {
            return false;
        };
        let mut writes = vec![None; bindings.len()];
        for expression in body {
            expression.collect_writes(&mut writes);
        }
        if writes[counter].is_some() {
            return false;
        }
        if let ReadPaths::Compact(paths) = &self.paths {
            for input in paths.iter().filter_map(CompactReadPath::input) {
                if !bindings
                    .iter()
                    .any(|binding| binding.id == input && !binding.written)
                {
                    return false;
                }
            }
        }
        true
    }
}

/// Only this guarded constructor can root the private region's receiver and
/// backing. Nullable roots stay empty whenever the original loop must run.
pub(super) struct ArrayReadBorrow<'a> {
    array: GcLocal<ArrayObject, Nullable>,
    backing: ReadBacking<'a>,
    length: I64Local,
    length_number: F64Local,
    slot: I32Local,
}

enum ReadBacking<'a> {
    Numbers(GcLocal<NumberBitsI64Array, Nullable>),
    Compact(CompactPathBorrow<'a>),
}

impl ReadBacking<'_> {
    fn mode(&self) -> ArrayIndexedStorageMode {
        match self {
            Self::Numbers(_) => ArrayIndexedStorageMode::PackedNumbers,
            Self::Compact(_) => ArrayIndexedStorageMode::PackedValues,
        }
    }

    fn clear_roots(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        match self {
            Self::Numbers(numbers) => numbers.set_null(builder.runtime_schema(), function),
            Self::Compact(rows) => rows.clear_roots(builder, function),
        }
    }

    fn release(self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        match self {
            Self::Numbers(numbers) => numbers.clear(function),
            Self::Compact(rows) => rows.release(builder, function),
        }
    }
}

impl<'a> ArrayReadBorrow<'a> {
    pub(super) fn new(
        plan: &'a ArrayReadLoop,
        builder: &mut FunctionBuilder<'_>,
        accepted: I32Local,
        function: &mut Function,
    ) -> Self {
        let s = builder.runtime_schema();
        let borrow = Self {
            array: s.reserve_gc_local(function).initialize_null(s, function),
            backing: match &plan.paths {
                ReadPaths::LengthOnly | ReadPaths::Numbers => {
                    ReadBacking::Numbers(s.reserve_gc_local(function).initialize_null(s, function))
                }
                ReadPaths::Compact(paths) => {
                    ReadBacking::Compact(CompactPathBorrow::new(paths, builder, function))
                }
            },
            length: s.reserve_i64_local(function),
            length_number: s.reserve_f64_local(function),
            slot: s.reserve_i32_local(function),
        };
        borrow.length.set_constant(0, function);
        accepted.load(function);
        builder.open_frame(ControlFrameKind::If, function);
        accepted.set_constant(0, function);
        let decline = builder.open_frame(ControlFrameKind::Block, function);
        let source = builder.local_binding(plan.array);
        if let Some(initialized) = source.initialization_check() {
            initialized.load(function);
            function.instruction(&Instruction::I32Eqz);
            builder.emit_branch_if_to_target(decline, function);
        }
        builder
            .local_binding(plan.array)
            .value()
            .tag()
            .load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Array.tag()));
        function.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(decline, function);
        builder
            .local_binding(plan.array)
            .value()
            .reference()
            .load(function);
        function.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        borrow.array.replace(
            builder
                .local_binding(plan.array)
                .value()
                .cast_reference::<ArrayObject>(s, function)
                .nullable(),
            function,
        );
        s.field(ArrayObjectSchema::MODE)
            .read(&borrow.array, s, function);
        function.instruction(&Instruction::I32Const(borrow.backing.mode().encode()));
        function.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(decline, function);
        s.field(ArrayObjectSchema::LENGTH)
            .read(&borrow.array, s, function)
            .store_i64(borrow.length, function);
        borrow.length.load(function);
        function.instruction(&Instruction::I64Const(1 << 20));
        function.instruction(&Instruction::I64GtU);
        builder.emit_branch_if_to_target(decline, function);
        s.field(ArrayObjectSchema::COUNT)
            .read(&borrow.array, s, function);
        borrow.length.load(function);
        function.instruction(&Instruction::I64Ne);
        builder.emit_branch_if_to_target(decline, function);
        match &borrow.backing {
            ReadBacking::Numbers(numbers) => {
                numbers.replace(
                    s.field(ArrayObjectSchema::NUMBERS)
                        .read(&borrow.array, s, function)
                        .reference()
                        .nullable(),
                    function,
                );
                s.array_type::<NumberBitsI64Array>()
                    .length(numbers, s, function);
            }
            ReadBacking::Compact(rows) => {
                rows.capture(&borrow.array, builder, function);
                rows.missing(builder, function);
                builder.emit_branch_if_to_target(decline, function);
                rows.emit_capacity(builder, function);
            }
        }
        function.instruction(&Instruction::I64ExtendI32U);
        borrow.length.load(function);
        function.instruction(&Instruction::I64LtU);
        builder.emit_branch_if_to_target(decline, function);
        let counter = builder.local_binding(plan.counter).value().scalar();
        // The enclosing scalar guard established this initialized Number.
        // Keep binary64 storage so an untouched negative zero stays negative.
        counter.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Ge);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        counter.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        borrow.length.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::F64Le);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        counter.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        counter.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::F64Ne);
        builder.emit_branch_if_to_target(decline, function);
        // This mirror follows each original f64 update. Its proof never
        // changes the source counter's representation, including negative zero.
        counter.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::I32TruncSatF64U);
        borrow.slot.store(function);
        borrow.length.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        borrow.length_number.store(function);
        if let ReadBacking::Compact(rows) = &borrow.backing {
            rows.capture_columns(builder, decline, function);
        }
        accepted.set_constant(1, function);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        accepted.load(function);
        function.instruction(&Instruction::I32Eqz);
        builder.open_frame(ControlFrameKind::If, function);
        borrow.array.set_null(s, function);
        borrow.backing.clear_roots(builder, function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        borrow
    }

    pub(super) fn emit_length(&self, function: &mut Function) {
        self.length_number.load(function);
    }

    pub(super) fn is_compact(&self) -> bool {
        matches!(self.backing, ReadBacking::Compact(_))
    }

    pub(super) fn prepare_compact_loop(
        &self,
        done: ControlTarget,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Option<PreparedCompactRows<'_, 'a>> {
        let ReadBacking::Compact(rows) = &self.backing else {
            return None;
        };
        Some(rows.prepare_loop(self.slot, self.length, done, builder, function))
    }

    pub(super) fn emit_compact_path(&self, path: CompactPathId, function: &mut Function) {
        let ReadBacking::Compact(rows) = &self.backing else {
            unreachable!("compact leaves require their matching guarded backing");
        };
        rows.emit_number(path, function);
    }

    pub(super) fn emit_test(&self, function: &mut Function) {
        self.slot.load(function);
        self.length.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32LtU);
    }

    pub(super) fn emit_four_step_test(&self, function: &mut Function) {
        // The admitted extent is at most 2^20, so this lookahead cannot wrap.
        self.slot.load(function);
        function.instruction(&Instruction::I32Const(3));
        function.instruction(&Instruction::I32Add);
        self.length.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32LtU);
    }

    pub(super) fn advance(&self, function: &mut Function) {
        self.slot.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        self.slot.store(function);
    }

    pub(super) fn emit_element(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        let ReadBacking::Numbers(numbers) = &self.backing else {
            unreachable!("direct Number reads require their matching guarded backing");
        };
        s.array_type::<NumberBitsI64Array>()
            .read(numbers, self.slot, s, function);
        function.instruction(&Instruction::F64ReinterpretI64);
    }

    pub(super) fn release(self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        let s = builder.runtime_schema();
        self.backing.release(builder, function);
        self.array.clear(function);
        s.release_f64_local(self.length_number, function);
        s.release_i32_local(self.slot, function);
        s.release_i64_local(self.length, function);
    }
}
