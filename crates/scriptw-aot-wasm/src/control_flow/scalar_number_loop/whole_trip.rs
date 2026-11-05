//! Whole-trip kernels share the scalar loop's original guard and publication.
use super::chunked::{bounded_counter_literal, increasing_counter, WholeTripAffine32};
use super::*;

pub(super) enum WholeTrip {
    Affine32(WholeTripAffine32),
    ExactSum(ExactSum),
}

impl WholeTrip {
    pub(super) fn new(
        builder: &FunctionBuilder<'_>,
        init: Option<&ForInitIr>,
        source_test: &TypedExpr,
        plan: &ScalarNumberLoop,
    ) -> Option<Self> {
        if let Some(affine) = WholeTripAffine32::new(builder, init, source_test, plan) {
            return Some(Self::Affine32(affine));
        }
        ExactSum::new(builder, init, source_test, plan).map(Self::ExactSum)
    }

    pub(super) fn emit_guarded(
        &self,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        value: LoopLocal,
        iterated: I32Local,
        done: ControlTarget,
        function: &mut Function,
    ) {
        match self {
            Self::Affine32(affine) => {
                affine.emit_guarded(builder, locals, value, iterated, done, function)
            }
            Self::ExactSum(sum) => {
                sum.emit_guarded(builder, locals, value, iterated, done, function)
            }
        }
    }
}

/// Each original RHS node, not merely its simplified result, must stay an
/// exactly representable integer throughout this literal induction interval.
struct Affine {
    multiplier: i128,
    addend: i128,
}

impl Affine {
    fn at(&self, counter: u32) -> Option<i128> {
        self.multiplier
            .checked_mul(i128::from(counter))?
            .checked_add(self.addend)
    }

    fn new(value: &NumberExpression, counter: usize, first: u32, last: u32) -> Option<Self> {
        let affine = match value {
            NumberExpression::Constant(bits) => {
                let value = f64::from_bits(*bits);
                if *bits == (-0.0_f64).to_bits()
                    || !value.is_finite()
                    || value.fract() != 0.0
                    || value.abs() > MAX_SAFE_INTEGER as f64
                {
                    return None;
                }
                Self {
                    multiplier: 0,
                    addend: value as i128,
                }
            }
            NumberExpression::Binding(index) if *index == counter => Self {
                multiplier: 1,
                addend: 0,
            },
            NumberExpression::Negate(inner) => {
                let inner = Self::new(inner, counter, first, last)?;
                Self {
                    multiplier: inner.multiplier.checked_neg()?,
                    addend: inner.addend.checked_neg()?,
                }
            }
            NumberExpression::Arithmetic(op, left, right) => {
                let left = Self::new(left, counter, first, last)?;
                let right = Self::new(right, counter, first, last)?;
                match op {
                    NumberArithmetic::Add => Self {
                        multiplier: left.multiplier.checked_add(right.multiplier)?,
                        addend: left.addend.checked_add(right.addend)?,
                    },
                    NumberArithmetic::Sub => Self {
                        multiplier: left.multiplier.checked_sub(right.multiplier)?,
                        addend: left.addend.checked_sub(right.addend)?,
                    },
                    NumberArithmetic::Mul => {
                        let (constant, variable) = if left.multiplier == 0 {
                            (left.addend, right)
                        } else if right.multiplier == 0 {
                            (right.addend, left)
                        } else {
                            return None;
                        };
                        Self {
                            multiplier: variable.multiplier.checked_mul(constant)?,
                            addend: variable.addend.checked_mul(constant)?,
                        }
                    }
                    NumberArithmetic::Div => return None,
                }
            }
            _ => return None,
        };
        let safe = i128::from(MAX_SAFE_INTEGER);
        for endpoint in [first, last] {
            if !(-safe..=safe).contains(&affine.at(endpoint)?) {
                return None;
            }
        }
        Some(affine)
    }
}

/// No constructor can skip a node's exactness proof or prefix/seed bounds.
/// RHS zero signs cannot escape Add/Sub/Negate/Mul; rejecting a -0 seed
/// ensures the accumulator never acquires -0, including exact cancellation.
pub(super) struct ExactSum {
    counter: usize,
    accumulator: usize,
    initial_bits: u64,
    limit: u32,
    delta: i64,
    seed_lower: i64,
    seed_upper: i64,
}

impl ExactSum {
    fn new(
        builder: &FunctionBuilder<'_>,
        init: Option<&ForInitIr>,
        source_test: &TypedExpr,
        plan: &ScalarNumberLoop,
    ) -> Option<Self> {
        if plan.invariant.is_some() || !plan.imul.is_empty() || plan.bindings.len() != 2 {
            return None;
        }
        let counter = increasing_counter(plan.update.as_ref()?)?;
        let Some(ForInitIr::Var(declarators)) = init else {
            return None;
        };
        let [declarator] = declarators.as_slice() else {
            return None;
        };
        let ExprIr::Number(initial_bits) = &declarator.init.as_ref()?.expr else {
            return None;
        };
        let initial = bounded_counter_literal(*initial_bits)?;
        let initial_binding = builder.private_local_binding(&declarator.name)?;
        let counter_binding = plan.bindings[counter].id;
        if initial_binding != counter_binding {
            return None;
        }
        let (source_left, source_right) = match &source_test.expr {
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
        if source_counter(builder, source_left)? != counter_binding {
            return None;
        }
        let ExprIr::Number(limit_bits) = &source_right.expr else {
            return None;
        };
        let NumberPredicate::Compare(RelationalBinaryOp::LessThan, left, right) = &plan.test else {
            return None;
        };
        if !numeric_counter(left, counter)
            || !matches!(right, NumberExpression::Constant(bits) if bits == limit_bits)
        {
            return None;
        }
        let limit = bounded_counter_literal(*limit_bits)?;
        let iterations = limit.checked_sub(initial)?;
        if iterations == 0 {
            return None;
        }
        let [NumberStatement::Expression(body)] = plan.body.as_slice() else {
            return None;
        };
        let (accumulator, term) = match body {
            NumberExpression::Compound(binding, NumberArithmetic::Add, term) => {
                (*binding, term.as_ref())
            }
            NumberExpression::Assign(binding, value) => {
                let NumberExpression::Arithmetic(NumberArithmetic::Add, left, term) =
                    value.as_ref()
                else {
                    return None;
                };
                if !matches!(left.as_ref(), NumberExpression::Binding(index) if index == binding) {
                    return None;
                }
                (*binding, term.as_ref())
            }
            _ => return None,
        };
        if accumulator == counter || plan.bindings[accumulator].domain != NumberDomain::Number {
            return None;
        }
        let term = Affine::new(term, counter, initial, limit - 1)?;
        let first = term.at(initial)?;
        let last = term.at(limit - 1)?;
        if !(first >= 0 && last >= 0 || first <= 0 && last <= 0) {
            return None;
        }
        let doubled = first
            .checked_add(last)?
            .checked_mul(i128::from(iterations))?;
        if doubled % 2 != 0 {
            return None;
        }
        let delta = doubled / 2;
        // Same-sign terms make sequential prefixes monotone, including zero.
        let prefix_min = delta.min(0);
        let prefix_max = delta.max(0);
        let safe = i128::from(MAX_SAFE_INTEGER);
        let lower = (-safe).checked_sub(prefix_min)?.max(-safe);
        let upper = safe.checked_sub(prefix_max)?.min(safe);
        if lower > upper {
            return None;
        }
        Some(Self {
            counter,
            accumulator,
            initial_bits: *initial_bits,
            limit,
            delta: i64::try_from(delta).ok()?,
            seed_lower: i64::try_from(lower).ok()?,
            seed_upper: i64::try_from(upper).ok()?,
        })
    }

    fn emit_guarded(
        &self,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        value: LoopLocal,
        iterated: I32Local,
        done: ControlTarget,
        function: &mut Function,
    ) {
        let (LoopLocal::Number(counter), LoopLocal::Number(seed), LoopLocal::Number(value)) = (
            locals.values[self.counter],
            locals.values[self.accumulator],
            value,
        ) else {
            unreachable!("unit counter and Add accumulation retain Number storage");
        };
        counter.load(function);
        function.instruction(&Instruction::I64ReinterpretF64);
        function.instruction(&Instruction::I64Const(self.initial_bits as i64));
        function.instruction(&Instruction::I64Eq);
        for (bound, comparison) in [
            (self.seed_lower, Instruction::F64Ge),
            (self.seed_upper, Instruction::F64Le),
        ] {
            seed.load(function);
            function.instruction(&Instruction::F64Const(Ieee64::from(bound as f64)));
            function.instruction(&comparison);
            function.instruction(&Instruction::I32And);
        }
        seed.load(function);
        seed.load(function);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::I32And);
        seed.load(function);
        function.instruction(&Instruction::I64ReinterpretF64);
        function.instruction(&Instruction::I64Const(i64::MIN));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32And);
        builder.open_frame(ControlFrameKind::If, function);
        seed.load(function);
        function.instruction(&Instruction::I64TruncSatF64S);
        function.instruction(&Instruction::I64Const(self.delta));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::F64ConvertI64S);
        seed.store(function);
        seed.load(function);
        value.store(function);
        function.instruction(&Instruction::F64Const(Ieee64::from(self.limit as f64)));
        counter.store(function);
        iterated.set_constant(1, function);
        builder.emit_branch_to_target(done, function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }
}

/// Admit i or (i + 0) with a positive-zero literal exactly. Unary plus,
/// subtraction, swapped operands and other normalization retain the old loops.
fn source_counter(builder: &FunctionBuilder<'_>, value: &TypedExpr) -> Option<LocalBindingId> {
    let value = match &value.expr {
        ExprIr::BinaryNumber {
            op: ArithmeticBinaryOp::Add,
            lhs,
            rhs,
        }
        | ExprIr::CoerciveBinaryNumber {
            op: ArithmeticBinaryOp::Add,
            lhs,
            rhs,
        }
        | ExprIr::CoerciveAdd { lhs, rhs }
            if matches!(rhs.expr, ExprIr::Number(0)) =>
        {
            lhs.as_ref()
        }
        ExprIr::Identifier(_) => value,
        _ => return None,
    };
    let ExprIr::Identifier(name) = &value.expr else {
        return None;
    };
    builder.private_local_binding(name)
}

fn numeric_counter(value: &NumberExpression, counter: usize) -> bool {
    match value {
        NumberExpression::Binding(index) => *index == counter,
        NumberExpression::Arithmetic(NumberArithmetic::Add, left, right)
            if matches!(right.as_ref(), NumberExpression::Constant(0)) =>
        {
            matches!(left.as_ref(), NumberExpression::Binding(index) if *index == counter)
        }
        _ => false,
    }
}
