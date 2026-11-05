//! Two existing dense Number borrows share one authenticated masked induction.
use super::*;
use dense_array::{private_local, ReadKey, RegionInputSource};

pub(super) struct NumberInputs {
    counter: LocalBindingId,
    index: LocalBindingId,
    phase: RegionInputSource,
    limit: u32,
    mask: u32,
    arrays: Vec<ArrayReadLoop>,
}

impl NumberInputs {
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
        if chunked::bounded_counter_literal(first)? >= limit {
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
        if mask >= 1 << 20 || !(mask + 1).is_power_of_two() {
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
            arrays: Vec::new(),
        })
    }

    pub(super) fn index(&self) -> LocalBindingId {
        self.index
    }

    pub(super) fn is_receiver(&self, id: LocalBindingId) -> bool {
        self.phase.is_local(id) || self.arrays.iter().any(|array| array.is_array(id))
    }

    pub(super) fn read(
        &mut self,
        builder: &FunctionBuilder<'_>,
        read: PropertyReadView<'_>,
    ) -> Option<usize> {
        let ReadKey::Computed(index) = read.key else {
            return None;
        };
        if private_local(builder, index) != Some(self.index) {
            return None;
        }
        let source = RegionInputSource::of(builder, read.target)?;
        if source.is_local(self.index) || source.is_local(self.counter) || source == self.phase {
            return None;
        }
        if let Some(index) = self
            .arrays
            .iter()
            .position(|array| array.has_receiver(&source))
        {
            return Some(index);
        }
        if self.arrays.len() == 2 {
            return None;
        }
        let index = self.arrays.len();
        self.arrays.push(ArrayReadLoop::masked_numbers(
            source,
            self.counter,
            self.limit,
            self.mask,
        ));
        Some(index)
    }

    pub(super) fn complete(&self, bindings: &[NumberBinding], body: &[NumberStatement]) -> bool {
        if self.arrays.is_empty() {
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
        writes[counter].is_none()
            && writes[index].is_none()
            && !bindings.iter().any(|b| self.is_receiver(b.id))
    }

    pub(super) fn guard<'a>(
        &'a self,
        builder: &mut FunctionBuilder<'_>,
        accepted: I32Local,
        function: &mut Function,
    ) -> NumberInputBorrow<'a> {
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
        let arrays: Vec<_> = self
            .arrays
            .iter()
            .map(|array| ArrayReadBorrow::new(array, builder, accepted, function))
            .collect();
        accepted.load(function);
        function.instruction(&Instruction::I32Eqz);
        builder.open_frame(ControlFrameKind::If, function);
        for array in &arrays {
            array.clear_roots(builder, function);
        }
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        NumberInputBorrow {
            arrays,
            phase,
            mask: self.mask,
            index: self.index,
        }
    }
}

pub(super) struct NumberInputBorrow<'a> {
    arrays: Vec<ArrayReadBorrow<'a>>,
    phase: I32Local,
    mask: u32,
    index: LocalBindingId,
}

impl NumberInputBorrow<'_> {
    pub(super) fn emit_index(&self, function: &mut Function) {
        // 0 <= counter <= 2^20 and integral signed-32 phase make the original
        // binary64 sum exact. Wrapping i32 addition preserves precisely the
        // low bits subsequently observed by ToInt32 and the nonnegative mask.
        self.arrays[0].load_slot(function);
        self.phase.load(function);
        function.instruction(&Instruction::I32Add);
        function.instruction(&Instruction::I32Const(self.mask as i32));
        function.instruction(&Instruction::I32And);
    }

    pub(super) fn emit_element(
        &self,
        input: usize,
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
        self.arrays[input].emit_element_at(index, builder, function);
    }

    pub(super) fn advance(&self, function: &mut Function) {
        self.arrays[0].advance(function);
    }

    pub(super) fn release(self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        for array in self.arrays {
            array.release(builder, function);
        }
        builder
            .runtime_schema()
            .release_i32_local(self.phase, function);
    }
}
