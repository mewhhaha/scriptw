//! One canonical indexed Array borrow belongs to the complete scalar region.
use super::*;

pub(super) enum ArrayRead {
    Length,
    Element,
    CompactPath(CompactPathId),
}

/// Construction fixes the sole receiver, induction binding and loop shape.
pub(super) struct ArrayReadLoop {
    array: RegionInputSource,
    counter: LocalBindingId,
    iteration: ArrayIteration,
    row_alias: Option<LocalBindingId>,
    paths: ReadPaths,
}

/// A source-local or authenticated captured cell, without callbacks.
/// Consumers separately validate its current value kind and representation.
#[derive(PartialEq, Eq)]
pub(super) enum RegionInputSource {
    Local(LocalBindingId),
    Cell(scriptw_ir::ProvenBindingCellIr),
}

impl RegionInputSource {
    pub(super) fn of(builder: &FunctionBuilder<'_>, value: &TypedExpr) -> Option<Self> {
        if let Some(local) = private_local(builder, value) {
            return Some(Self::Local(local));
        }
        let ExprIr::EnvironmentIdentifier(identifier) = &value.expr else {
            return None;
        };
        if !matches!(
            identifier.operation,
            scriptw_ir::EnvironmentIdentifierOperationIr::Read
        ) {
            return None;
        }
        let scriptw_ir::EnvironmentIdentifierResolutionStart::Cell(proof) =
            identifier.resolution_start()
        else {
            return None;
        };
        if !matches!(
            proof.origin(),
            scriptw_ir::ProvenBindingCellOrigin::Captured { .. }
        ) {
            return None;
        }
        builder.module_cell_storage(&proof, &identifier.name).ok()?;
        Some(Self::Cell(proof))
    }

    pub(super) fn load(
        &self,
        source: &ValueLocals,
        builder: &mut FunctionBuilder<'_>,
        decline: ControlTarget,
        function: &mut Function,
    ) {
        let s = builder.runtime_schema();
        match self {
            RegionInputSource::Local(id) => {
                let local = builder.local_binding(*id);
                if let Some(initialized) = local.initialization_check() {
                    initialized.load(function);
                    function.instruction(&Instruction::I32Eqz);
                    builder.emit_branch_if_to_target(decline, function);
                }
                source.copy_from(local.value(), function);
            }
            RegionInputSource::Cell(proof) => {
                let BindingStorage::EnvSlot { slot, hops } = builder
                    .module_cell_storage(proof, proof.source_name())
                    .expect("the sealed region input authenticates this captured cell")
                else {
                    unreachable!("proven captured region input uses a cell");
                };
                let environment = builder.resolve_env_handle_local(hops, function);
                let cell = builder.emit_environment_cell_local(&environment, slot, function);
                let initialized = builder.emit_read_environment_cell(&cell, source, function);
                initialized.load(function);
                s.release_i32_local(initialized, function);
                cell.clear(function);
                environment.clear(function);
                function.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(decline, function);
            }
        }
    }

    pub(super) fn is_local(&self, id: LocalBindingId) -> bool {
        matches!(self, Self::Local(local) if *local == id)
    }
}

#[derive(Clone, Copy)]
enum ArrayIteration {
    Length,
    Masked { limit: u32, mask: u32 },
}

impl ArrayIteration {
    fn projection_period(self) -> Option<u32> {
        let Self::Masked { limit, mask } = self else {
            return None;
        };
        let period = mask.checked_add(1)?;
        if period.is_power_of_two() && limit > period {
            Some(period)
        } else {
            None
        }
    }

    fn mask(self) -> Option<u32> {
        match self {
            Self::Length => None,
            Self::Masked { mask, .. } => Some(mask),
        }
    }
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
                    ExprIr::Identifier(_)
                    | ExprIr::BitwiseNumeric {
                        op: BitwiseBinaryOp::And,
                        ..
                    }
                    | ExprIr::PropertyRead { .. }
                    | ExprIr::SpecOperation {
                        operation: SpecOperationIr::GetV,
                        ..
                    } => {
                        // A structural view only. Each region consumer must
                        // authenticate its actual receiver/key and induction.
                        ReadKey::Computed(key)
                    }
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
    pub(super) fn masked_numbers(
        array: RegionInputSource,
        counter: LocalBindingId,
        limit: u32,
        mask: u32,
    ) -> Self {
        Self {
            array,
            counter,
            iteration: ArrayIteration::Masked { limit, mask },
            row_alias: None,
            paths: ReadPaths::Numbers,
        }
    }

    pub(super) fn has_receiver(&self, receiver: &RegionInputSource) -> bool {
        &self.array == receiver
    }

    pub(super) fn new(
        builder: &FunctionBuilder<'_>,
        init: Option<&ForInitIr>,
        test: &TypedExpr,
        update: Option<&TypedExpr>,
        body: &StatementIr,
    ) -> Option<Self> {
        Self::length_bound(builder, test, update)
            .or_else(|| Self::masked_rows(builder, init, test, update, body))
    }

    fn length_bound(
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
            array: RegionInputSource::Local(array),
            counter,
            iteration: ArrayIteration::Length,
            row_alias: None,
            paths: ReadPaths::LengthOnly,
        })
    }

    /// A positive literal trip keeps every induction value in the exact
    /// Number/ToInt32 overlap. The mask need not be a power of two: every
    /// resulting slot is bounded by the mask, checked against real length.
    /// A row declaration or direct accumulation must be first so speculative
    /// preparation never moves a source write ahead of a throwing row read.
    fn masked_rows(
        builder: &FunctionBuilder<'_>,
        init: Option<&ForInitIr>,
        test: &TypedExpr,
        update: Option<&TypedExpr>,
        body: &StatementIr,
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
        let ExprIr::Number(limit) = rhs.expr else {
            return None;
        };
        let limit = super::chunked::bounded_counter_literal(limit)?;
        if limit == 0 || limit > 1 << 20 {
            return None;
        }
        let ForInitIr::Var(initial) = init? else {
            return None;
        };
        let [initial] = initial.as_slice() else {
            return None;
        };
        if builder.private_local_binding(&initial.name) != Some(counter) {
            return None;
        }
        let ExprIr::Number(first) = initial.init.as_ref()?.expr else {
            return None;
        };
        if super::chunked::bounded_counter_literal(first)? >= limit {
            return None;
        }
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
        {
            return None;
        }
        let first_statement = match body {
            StatementIr::Block(block) if block.lexical_environment.is_none() => {
                block.statements.first()?
            }
            StatementIr::Expression(_) => body,
            _ => return None,
        };
        let (row_alias, read) = match first_statement {
            StatementIr::Var(declarations) => {
                let [row] = declarations.as_slice() else {
                    return None;
                };
                let alias = builder.private_local_binding(&row.name)?;
                if alias == counter
                    || builder
                        .local_binding(alias)
                        .initialization_check()
                        .is_some()
                {
                    return None;
                }
                (Some(alias), PropertyReadView::of(row.init.as_ref()?)?)
            }
            StatementIr::Expression(TypedExpr {
                expr: ExprIr::CompoundAssignIdentifier { value, .. },
                ..
            }) => {
                // Find only the direct row read of this accumulation. The
                // complete body/test/update still pass the scalar proof.
                let property = PropertyReadView::of(value)?;
                (None, PropertyReadView::of(property.target)?)
            }
            StatementIr::Expression(TypedExpr {
                expr: ExprIr::AssignIdentifier { name, value },
                ..
            }) => {
                let accumulator = builder.private_local_binding(name)?;
                let ExprIr::CoerciveAdd { lhs, rhs } = &value.expr else {
                    return None;
                };
                if private_local(builder, lhs) != Some(accumulator) {
                    return None;
                }
                // Lowered += can retain its left read as an explicit Add.
                // Authenticate that read before discovering the RHS receiver;
                // the complete scalar proof still owns arithmetic and effects.
                let property = PropertyReadView::of(rhs)?;
                (None, PropertyReadView::of(property.target)?)
            }
            _ => return None,
        };
        let array = RegionInputSource::of(builder, read.target)?;
        if array.is_local(counter) || row_alias.is_some_and(|alias| array.is_local(alias)) {
            return None;
        }
        let ReadKey::Computed(index) = read.key else {
            return None;
        };
        let ExprIr::BitwiseNumeric {
            op: BitwiseBinaryOp::And,
            lhs,
            rhs,
        } = &index.expr
        else {
            return None;
        };
        if private_local(builder, lhs) != Some(counter) {
            return None;
        }
        let ExprIr::Number(mask) = rhs.expr else {
            return None;
        };
        let mask = super::chunked::bounded_counter_literal(mask)?;
        if mask >= 1 << 20 {
            return None;
        }
        Some(Self {
            array,
            counter,
            iteration: ArrayIteration::Masked { limit, mask },
            row_alias,
            paths: ReadPaths::LengthOnly,
        })
    }

    pub(super) fn is_array(&self, id: LocalBindingId) -> bool {
        self.array.is_local(id) || self.row_alias == Some(id)
    }

    pub(super) fn row_alias(&self) -> Option<LocalBindingId> {
        self.row_alias
    }

    pub(super) fn accept_row_declaration(
        &self,
        builder: &FunctionBuilder<'_>,
        declarations: &[scriptw_ir::VarDeclaratorIr],
    ) -> Option<()> {
        let alias = self.row_alias?;
        let [row] = declarations else {
            return None;
        };
        if builder.private_local_binding(&row.name) != Some(alias) {
            return None;
        }
        let read = PropertyReadView::of(row.init.as_ref()?)?;
        if RegionInputSource::of(builder, read.target).as_ref() != Some(&self.array)
            || !self.is_counter_key(builder, read.key)
        {
            return None;
        }
        Some(())
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
        match self.iteration {
            ArrayIteration::Length => private_local(builder, index) == Some(self.counter),
            ArrayIteration::Masked { mask, .. } => {
                let ExprIr::BitwiseNumeric {
                    op: BitwiseBinaryOp::And,
                    lhs,
                    rhs,
                } = &index.expr
                else {
                    return false;
                };
                private_local(builder, lhs) == Some(self.counter)
                    && matches!(rhs.expr, ExprIr::Number(bits) if f64::from_bits(bits) == f64::from(mask))
            }
        }
    }

    pub(super) fn read(
        &mut self,
        builder: &FunctionBuilder<'_>,
        read: PropertyReadView<'_>,
    ) -> Option<(ArrayRead, Option<LocalBindingId>)> {
        if RegionInputSource::of(builder, read.target).as_ref() == Some(&self.array) {
            if self.row_alias.is_some() {
                return None;
            }
            if read.key.is_length() {
                return Some((ArrayRead::Length, None));
            }
            if !self.is_counter_key(builder, read.key) || self.is_compact() {
                return None;
            }
            self.paths = ReadPaths::Numbers;
            return Some((ArrayRead::Element, None));
        }
        let (path, row) = CompactReadPath::new(builder, read, self.counter)?;
        if let Some(alias) = self.row_alias {
            if private_local(builder, row) != Some(alias) {
                return None;
            }
        } else {
            let row = PropertyReadView::of(row)?;
            if RegionInputSource::of(builder, row.target).as_ref() != Some(&self.array)
                || !self.is_counter_key(builder, row.key)
            {
                return None;
            }
        }
        if let Some(key) = path.cyclic_key() {
            // The declaration rewrites this alias for every source iteration;
            // it cannot also supply an invariant key table.
            if self
                .row_alias
                .is_some_and(|alias| key.is_local_receiver(alias))
            {
                return None;
            }
            let ArrayIteration::Masked { limit, mask } = self.iteration else {
                return None;
            };
            let period = (mask + 1).max(key.period());
            if !(mask + 1).is_power_of_two() || limit <= period {
                return None;
            }
        }
        if path
            .input()
            .is_some_and(|id| self.is_array(id) || id == self.counter)
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
        body: &[NumberStatement],
    ) -> bool {
        if self.row_alias.is_some() && !self.is_compact() {
            return false;
        }
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
            if paths
                .iter()
                .filter_map(CompactReadPath::cyclic_key)
                .any(|key| {
                    bindings
                        .iter()
                        .any(|binding| key.is_local_receiver(binding.id))
                })
            {
                return false;
            }
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
    mask: Option<u32>,
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
                ReadPaths::Compact(paths) => ReadBacking::Compact(CompactPathBorrow::new(
                    paths,
                    plan.row_alias,
                    plan.iteration.projection_period(),
                    builder,
                    function,
                )),
            },
            length: s.reserve_i64_local(function),
            length_number: s.reserve_f64_local(function),
            slot: s.reserve_i32_local(function),
            mask: plan.iteration.mask(),
        };
        borrow.length.set_constant(0, function);
        accepted.load(function);
        builder.open_frame(ControlFrameKind::If, function);
        accepted.set_constant(0, function);
        let decline = builder.open_frame(ControlFrameKind::Block, function);
        // A literal-trip loop does not evaluate its array expression when the
        // test is false. Decline before even inspecting a captured TDZ cell.
        if let ArrayIteration::Masked { limit, .. } = plan.iteration {
            builder
                .local_binding(plan.counter)
                .value()
                .scalar()
                .load(function);
            function.instruction(&Instruction::F64ReinterpretI64);
            function.instruction(&Instruction::F64Const(Ieee64::from(f64::from(limit))));
            function.instruction(&Instruction::F64Lt);
            function.instruction(&Instruction::I32Eqz);
            builder.emit_branch_if_to_target(decline, function);
        }
        let source = s.reserve_value_local(function);
        source.set_undefined(function);
        plan.array.load(&source, builder, decline, function);
        source.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Array.tag()));
        function.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(decline, function);
        source.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        borrow.array.replace(
            source.cast_reference::<ArrayObject>(s, function).nullable(),
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
        if let ArrayIteration::Masked { limit, mask } = plan.iteration {
            borrow.length.load(function);
            function.instruction(&Instruction::I64Const(i64::from(mask)));
            function.instruction(&Instruction::I64LeU);
            builder.emit_branch_if_to_target(decline, function);
            // The original trip limit and the masked backing extent are
            // distinct. Capacity/density above use the actual array length.
            borrow.length.set_constant(i64::from(limit), function);
        }
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
        source.clear(function);
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
    ) -> Result<Option<PreparedCompactRows<'_, 'a>>, EmitError> {
        let ReadBacking::Compact(rows) = &self.backing else {
            return Ok(None);
        };
        Ok(Some(rows.prepare_loop(
            self.slot,
            self.length,
            self.mask,
            done,
            builder,
            function,
        )?))
    }

    pub(super) fn publish_row_alias(
        &self,
        iterated: I32Local,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        if let (ReadBacking::Compact(rows), Some(mask)) = (&self.backing, self.mask) {
            rows.publish_row_alias(iterated, self.slot, mask, builder, function);
        }
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

    pub(super) fn load_slot(&self, function: &mut Function) {
        self.slot.load(function);
    }

    pub(super) fn emit_element(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        self.emit_element_at(self.slot, builder, function);
    }

    pub(super) fn emit_element_at(
        &self,
        index: I32Local,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let s = builder.runtime_schema();
        let ReadBacking::Numbers(numbers) = &self.backing else {
            unreachable!("direct Number reads require their matching guarded backing");
        };
        s.array_type::<NumberBitsI64Array>()
            .read(numbers, index, s, function);
        function.instruction(&Instruction::F64ReinterpretI64);
    }

    pub(super) fn clear_roots(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        self.array.set_null(builder.runtime_schema(), function);
        self.backing.clear_roots(builder, function);
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
