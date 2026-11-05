//! One pure strict pair is projected over its complete repeated masked period.
use super::*;
use dense_array::{private_local, ReadKey, RegionInputSource};

pub(super) struct StrictPairInputs {
    counter: LocalBindingId,
    index: LocalBindingId,
    phase: RegionInputSource,
    limit: u32,
    mask: u32,
    arrays: Option<[ArrayReadLoop; 2]>,
}

impl StrictPairInputs {
    pub(super) fn new(
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
        let ExprIr::Number(bits) = rhs.expr else {
            return None;
        };
        let limit = chunked::bounded_counter_literal(bits)?;
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
        let first = chunked::bounded_counter_literal(first)?;
        if first >= limit {
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
        let StatementIr::Block(block) = body else {
            return None;
        };
        if block.lexical_environment.is_some() {
            return None;
        }
        let StatementIr::Var(declarations) = block.statements.first()? else {
            return None;
        };
        let [declaration] = declarations.as_slice() else {
            return None;
        };
        let index = builder.private_local_binding(&declaration.name)?;
        if index == counter
            || builder
                .local_binding(index)
                .initialization_check()
                .is_some()
        {
            return None;
        }
        let ExprIr::BitwiseNumeric {
            op: BitwiseBinaryOp::And,
            lhs,
            rhs,
        } = &declaration.init.as_ref()?.expr
        else {
            return None;
        };
        let ExprIr::Number(bits) = rhs.expr else {
            return None;
        };
        let mask = chunked::bounded_counter_literal(bits)?;
        if mask >= 1 << 20 || !(mask + 1).is_power_of_two() || limit - first <= mask + 1 {
            return None;
        }
        let ExprIr::CoerciveAdd { lhs, rhs } = &lhs.expr else {
            return None;
        };
        if private_local(builder, lhs) != Some(counter) {
            return None;
        }
        let phase = RegionInputSource::of(builder, rhs)?;
        if phase.is_local(counter) || phase.is_local(index) {
            return None;
        }
        Some(Self {
            counter,
            index,
            phase,
            limit,
            mask,
            arrays: None,
        })
    }

    pub(super) fn index(&self) -> LocalBindingId {
        self.index
    }

    pub(super) fn is_receiver(&self, id: LocalBindingId) -> bool {
        self.phase.is_local(id)
            || self
                .arrays
                .as_ref()
                .is_some_and(|arrays| arrays.iter().any(|array| array.is_array(id)))
    }

    pub(super) fn pair(
        &mut self,
        builder: &FunctionBuilder<'_>,
        left: &TypedExpr,
        right: &TypedExpr,
    ) -> Option<()> {
        let source = |operand: &TypedExpr| {
            let read = PropertyReadView::of(operand)?;
            let ReadKey::Computed(index) = read.key else {
                return None;
            };
            if private_local(builder, index) != Some(self.index) {
                return None;
            }
            let source = RegionInputSource::of(builder, read.target)?;
            if source.is_local(self.index) || source.is_local(self.counter) || source == self.phase
            {
                return None;
            }
            Some(source)
        };
        // Validate both operands before mutating the sealed input set.
        let left = source(left)?;
        let right = source(right)?;
        if self.arrays.is_none() {
            self.arrays = Some([left, right].map(|source| {
                ArrayReadLoop::masked_values(source, self.counter, self.limit, self.mask)
            }));
            return Some(());
        }
        let arrays = self.arrays.as_ref().expect("one sealed pair");
        (arrays[0].has_receiver(&left) && arrays[1].has_receiver(&right)).then_some(())
    }

    pub(super) fn complete(
        &self,
        bindings: &mut [NumberBinding],
        body: &[NumberStatement],
    ) -> bool {
        if self.arrays.is_none() {
            return false;
        }
        let Some(counter) = bindings.iter().position(|b| b.id == self.counter) else {
            return false;
        };
        let Some(index) = bindings.iter().position(|b| b.id == self.index) else {
            return false;
        };
        let Some((NumberStatement::Initialize(NumberExpression::Assign(target, value)), rest)) =
            body.split_first()
        else {
            return false;
        };
        if *target != index || !matches!(value.as_ref(), NumberExpression::MaskedInputIndex) {
            return false;
        }
        let mut writes = vec![None; bindings.len()];
        for statement in rest {
            statement.collect_writes(&mut writes);
        }
        if writes[counter].is_some()
            || writes[index].is_some()
            || bindings.iter().any(|b| self.is_receiver(b.id))
        {
            return false;
        }
        // Literal 0 <= first < limit <= 2^20, Increment, and no body counter
        // writes prove every value through loop exit fits Signed32. Existing
        // literal and integer entry checks retain the negative-zero fallback.
        bindings[counter].domain = NumberDomain::Signed32;
        true
    }

    pub(super) fn guard(
        &self,
        builder: &mut FunctionBuilder<'_>,
        accepted: I32Local,
        function: &mut Function,
    ) -> Result<StrictPairProjection, EmitError> {
        let schema = builder.runtime_schema();
        let phase = schema.reserve_i32_local(function);
        phase.set_constant(0, function);
        accepted.load(function);
        builder.open_frame(ControlFrameKind::If, function);
        accepted.set_constant(0, function);
        let source = schema.reserve_value_local(function);
        source.set_undefined(function);
        let decline = builder.open_frame(ControlFrameKind::Block, function);
        // No input/TDZ probe before the original positive-trip test.
        builder
            .local_binding(self.counter)
            .value()
            .scalar()
            .load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::from(self.limit))));
        function.instruction(&Instruction::F64Lt);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        self.phase.load(&source, builder, decline, function);
        source.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
        function.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(decline, function);
        for (bound, comparison) in [
            (i32::MIN as f64, Instruction::F64Ge),
            (i32::MAX as f64, Instruction::F64Le),
        ] {
            source.scalar().load(function);
            function.instruction(&Instruction::F64ReinterpretI64);
            function.instruction(&Instruction::F64Const(Ieee64::from(bound)));
            function.instruction(&comparison);
            function.instruction(&Instruction::I32Eqz);
            builder.emit_branch_if_to_target(decline, function);
        }
        source.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        source.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::F64Ne);
        builder.emit_branch_if_to_target(decline, function);
        source.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::I32TruncSatF64S);
        phase.store(function);
        accepted.set_constant(1, function);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        source.clear(function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let arrays = self
            .arrays
            .as_ref()
            .expect("complete proof seals one pair")
            .each_ref()
            .map(|array| ArrayReadBorrow::new(array, builder, accepted, function));
        let projection = schema
            .reserve_gc_local::<NumberBitsI64Array, Nullable>(function)
            .initialize_null(schema, function);
        accepted.load(function);
        builder.open_frame(ControlFrameKind::If, function);
        let cursor = schema.reserve_i32_local(function);
        cursor.set_constant((self.mask + 1) as i32, function);
        projection.replace(
            schema
                .array_type::<NumberBitsI64Array>()
                .filled(GcOperand::i64(0), cursor, function)
                .nullable(),
            function,
        );
        cursor.set_constant(0, function);
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        left.set_undefined(function);
        right.set_undefined(function);
        let bits = schema.reserve_i64_local(function);
        let done = builder.open_frame(ControlFrameKind::Block, function);
        let repeat = builder.open_frame(ControlFrameKind::Loop, function);
        cursor.load(function);
        function.instruction(&Instruction::I32Const((self.mask + 1) as i32));
        function.instruction(&Instruction::I32GeU);
        builder.emit_branch_if_to_target(done, function);
        // Both canonical dense borrows are rooted before allocation. These
        // reads and the existing exact comparator cannot invoke source code.
        arrays[0].emit_value_at(cursor, &left, builder, function);
        arrays[1].emit_value_at(cursor, &right, builder, function);
        builder.emit_tagged_payload_equality_i32(&left, &right, function)?;
        function.instruction(&Instruction::F64ConvertI32U);
        function.instruction(&Instruction::I64ReinterpretF64);
        bits.store(function);
        schema.array_type::<NumberBitsI64Array>().write(
            &projection,
            cursor,
            GcOperand::i64_local(bits),
            schema,
            function,
        );
        cursor.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        cursor.store(function);
        builder.emit_branch_to_target(repeat, function);
        builder.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i64_local(bits, function);
        right.clear(function);
        left.clear(function);
        schema.release_i32_local(cursor, function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // No input roots survive into the source loop or its original fallback.
        // Only the private Number projection and primitive induction are needed.
        for array in arrays {
            array.release(builder, function);
        }
        Ok(StrictPairProjection {
            projection,
            phase,
            counter: self.counter,
            limit: self.limit,
            mask: self.mask,
            index: self.index,
        })
    }
}

pub(super) struct StrictPairProjection {
    projection: GcLocal<NumberBitsI64Array, Nullable>,
    phase: I32Local,
    counter: LocalBindingId,
    limit: u32,
    mask: u32,
    index: LocalBindingId,
}

impl StrictPairProjection {
    fn counter(&self, locals: &LoopLocals<'_>) -> I32Local {
        let index = locals
            .bindings
            .iter()
            .position(|b| b.id == self.counter)
            .expect("sealed induction binding");
        let LoopLocal::Signed32(counter) = locals.values[index] else {
            unreachable!("the complete induction proof seals Signed32")
        };
        counter
    }

    pub(super) fn emit_test(&self, locals: &LoopLocals<'_>, function: &mut Function) {
        self.counter(locals).load(function);
        function.instruction(&Instruction::I32Const(self.limit as i32));
        function.instruction(&Instruction::I32LtU);
    }

    pub(super) fn emit_index(&self, locals: &LoopLocals<'_>, function: &mut Function) {
        // The bounded counter plus integral signed-i32 phase is exact in f64.
        // Wrapping addition retains precisely the masked ToInt32 low bits.
        self.counter(locals).load(function);
        self.phase.load(function);
        function.instruction(&Instruction::I32Add);
        function.instruction(&Instruction::I32Const(self.mask as i32));
        function.instruction(&Instruction::I32And);
    }

    pub(super) fn emit_predicate(
        &self,
        locals: &LoopLocals<'_>,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let index = locals
            .bindings
            .iter()
            .position(|b| b.id == self.index)
            .expect("sealed index binding");
        let LoopLocal::Signed32(index) = locals.values[index] else {
            unreachable!("masked index is nonnegative signed32")
        };
        let s = builder.runtime_schema();
        s.array_type::<NumberBitsI64Array>()
            .read(&self.projection, index, s, function);
        // The sealed projection stores exactly the canonical f64 bits of 0/1.
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
    }

    pub(super) fn advance(&self, locals: &LoopLocals<'_>, function: &mut Function) {
        self.counter(locals).load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        self.counter(locals).store(function);
    }

    pub(super) fn release(self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        self.projection.clear(function);
        builder
            .runtime_schema()
            .release_i32_local(self.phase, function);
    }
}
