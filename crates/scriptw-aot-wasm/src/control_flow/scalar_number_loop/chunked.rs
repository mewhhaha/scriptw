//! Bounded pure iterations share a backedge; exact residues may compose steps.
//! The original scalar loop remains the tail and owns every rejected boundary.
use super::*;

const UNROLL: u32 = 5;
const AFFINE_STEPS: u32 = 8;
const MAX_EXPANDED_NODES: usize = 256;
const EXACT_COUNTER_LIMIT: f64 = (MAX_SAFE_INTEGER - AFFINE_STEPS as i64) as f64;

#[derive(Clone, Copy)]
enum Direction {
    Up,
    Down,
}

enum Limit {
    Constant(u64),
    Binding(usize),
}

impl Limit {
    fn new(expression: &NumberExpression, bindings: &[NumberBinding]) -> Option<Self> {
        match expression {
            NumberExpression::Constant(bits) => Some(Self::Constant(*bits)),
            NumberExpression::Binding(index) if !bindings[*index].written => {
                Some(Self::Binding(*index))
            }
            _ => None,
        }
    }

    fn emit(&self, locals: &LoopLocals<'_>, function: &mut Function) {
        match self {
            Self::Constant(bits) => {
                function.instruction(&Instruction::F64Const(Ieee64::from(f64::from_bits(*bits))));
            }
            Self::Binding(index) => locals.values[*index].load_number(function),
        }
    }
}

/// Only this constructor may remove intermediate tests or recurrence steps.
pub(super) struct ChunkedLoop {
    counter: usize,
    direction: Direction,
    comparison: RelationalBinaryOp,
    limit: Limit,
    affine: Option<Affine32>,
}

impl ChunkedLoop {
    pub(super) fn new(plan: &ScalarNumberLoop) -> Option<Self> {
        let (counter, direction) = counter_update(plan.update.as_ref()?)?;
        let NumberPredicate::Compare(op, left, right) = &plan.test else {
            return None;
        };
        let (comparison, limit) = if counter_test(left, counter) {
            (*op, Limit::new(right, &plan.bindings)?)
        } else if counter_test(right, counter) {
            let reversed = match op {
                RelationalBinaryOp::LessThan => RelationalBinaryOp::GreaterThan,
                RelationalBinaryOp::LessThanOrEqual => RelationalBinaryOp::GreaterThanOrEqual,
                RelationalBinaryOp::GreaterThan => RelationalBinaryOp::LessThan,
                RelationalBinaryOp::GreaterThanOrEqual => RelationalBinaryOp::LessThanOrEqual,
            };
            (reversed, Limit::new(left, &plan.bindings)?)
        } else {
            return None;
        };
        if !matches!(
            (direction, comparison),
            (
                Direction::Up,
                RelationalBinaryOp::LessThan | RelationalBinaryOp::LessThanOrEqual
            ) | (
                Direction::Down,
                RelationalBinaryOp::GreaterThan | RelationalBinaryOp::GreaterThanOrEqual
            )
        ) {
            return None;
        }
        let mut writes = vec![None; plan.bindings.len()];
        for expression in &plan.body {
            expression.collect_writes(&mut writes);
        }
        if writes[counter].is_some()
            || plan.body.iter().map(node_count).sum::<usize>() * UNROLL as usize
                > MAX_EXPANDED_NODES
        {
            return None;
        }
        Some(Self {
            counter,
            direction,
            comparison,
            limit,
            affine: Affine32::new(plan, counter),
        })
    }

    fn counter(&self, locals: &LoopLocals<'_>) -> F64Local {
        let LoopLocal::Number(counter) = locals.values[self.counter] else {
            unreachable!("unit counter updates establish Number storage")
        };
        counter
    }

    fn advance(&self, count: u32, locals: &LoopLocals<'_>, function: &mut Function) {
        self.counter(locals).load(function);
        function.instruction(&Instruction::F64Const(Ieee64::from(count as f64)));
        function.instruction(&match self.direction {
            Direction::Up => Instruction::F64Add,
            Direction::Down => Instruction::F64Sub,
        });
        self.counter(locals).store(function);
    }

    pub(super) fn emit(
        &self,
        builder: &mut FunctionBuilder<'_>,
        plan: &ScalarNumberLoop,
        locals: &LoopLocals<'_>,
        value: Option<LoopLocal>,
        iterated: I32Local,
        function: &mut Function,
    ) {
        // Check once. Within these bounds every unit counter step and the
        // last-position lookahead are exact, including after an inclusive test.
        // Fractions, NaN, infinities and the 2^53 boundary keep the original tail.
        function.instruction(&Instruction::I32Const(1));
        for is_counter in [true, false] {
            for (bound, op) in [
                (-EXACT_COUNTER_LIMIT, Instruction::F64Ge),
                (EXACT_COUNTER_LIMIT, Instruction::F64Le),
            ] {
                if is_counter {
                    self.counter(locals).load(function);
                } else {
                    self.limit.emit(locals, function);
                }
                function.instruction(&Instruction::F64Const(Ieee64::from(bound)));
                function.instruction(&op);
                function.instruction(&Instruction::I32And);
            }
        }
        self.counter(locals).load(function);
        self.counter(locals).load(function);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::I32And);
        builder.open_frame(ControlFrameKind::If, function);
        let done = builder.open_frame(ControlFrameKind::Block, function);
        let next = builder.open_frame(ControlFrameKind::Loop, function);
        let steps = if self.affine.is_some() {
            AFFINE_STEPS
        } else {
            UNROLL
        };
        self.counter(locals).load(function);
        function.instruction(&Instruction::F64Const(Ieee64::from((steps - 1) as f64)));
        function.instruction(&match self.direction {
            Direction::Up => Instruction::F64Add,
            Direction::Down => Instruction::F64Sub,
        });
        self.limit.emit(locals, function);
        function.instruction(&match self.comparison {
            RelationalBinaryOp::LessThan => Instruction::F64Lt,
            RelationalBinaryOp::LessThanOrEqual => Instruction::F64Le,
            RelationalBinaryOp::GreaterThan => Instruction::F64Gt,
            RelationalBinaryOp::GreaterThanOrEqual => Instruction::F64Ge,
        });
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(done, function);
        if let Some(affine) = &self.affine {
            affine.emit(
                locals,
                value.expect("affine assignment has a body value"),
                function,
            );
            self.advance(steps, locals, function);
        } else {
            // Floating-point expressions remain in their original order and
            // round after each source operation. Only dead intermediate body
            // completions and the known unit update's discarded result vanish.
            for step in 0..steps {
                plan.emit_body(
                    builder,
                    locals,
                    (step + 1 == steps).then_some(value).flatten(),
                    function,
                );
                self.advance(1, locals, function);
            }
        }
        iterated.set_constant(1, function);
        builder.emit_branch_to_target(next, function);
        builder.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }
}

fn counter_update(expression: &NumberExpression) -> Option<(usize, Direction)> {
    match expression {
        NumberExpression::Update(index, op, _) => Some((
            *index,
            match op {
                NumericUpdateOp::Increment => Direction::Up,
                NumericUpdateOp::Decrement => Direction::Down,
            },
        )),
        NumberExpression::Compound(index, op, value) if one(value) => match op {
            NumberArithmetic::Add => Some((*index, Direction::Up)),
            NumberArithmetic::Sub => Some((*index, Direction::Down)),
            _ => None,
        },
        NumberExpression::Assign(index, value) => {
            let NumberExpression::Arithmetic(op, left, right) = value.as_ref() else {
                return None;
            };
            if !matches!(left.as_ref(), NumberExpression::Binding(id) if id == index) || !one(right)
            {
                return None;
            }
            match op {
                NumberArithmetic::Add => Some((*index, Direction::Up)),
                NumberArithmetic::Sub => Some((*index, Direction::Down)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn one(value: &NumberExpression) -> bool {
    matches!(value, NumberExpression::Constant(bits) if f64::from_bits(*bits) == 1.0)
}

fn counter_test(value: &NumberExpression, counter: usize) -> bool {
    match value {
        NumberExpression::Binding(index) => *index == counter,
        NumberExpression::Arithmetic(
            NumberArithmetic::Add | NumberArithmetic::Sub,
            left,
            right,
        ) if matches!(right.as_ref(), NumberExpression::Constant(bits) if f64::from_bits(*bits) == 0.0) =>
        {
            matches!(left.as_ref(), NumberExpression::Binding(index) if *index == counter)
        }
        _ => false,
    }
}

fn node_count(value: &NumberExpression) -> usize {
    1 + match value {
        NumberExpression::Constant(_)
        | NumberExpression::Binding(_)
        | NumberExpression::Update(_, _, _) => 0,
        NumberExpression::Negate(value)
        | NumberExpression::Complement(value)
        | NumberExpression::Assign(_, value)
        | NumberExpression::Compound(_, _, value) => node_count(value),
        NumberExpression::Arithmetic(_, left, right)
        | NumberExpression::Bitwise(_, left, right)
        | NumberExpression::Imul(left, right)
        | NumberExpression::Comma(left, right) => node_count(left) + node_count(right),
    }
}

struct Affine32 {
    binding: usize,
    multiplier: u32,
    addend: u32,
}

impl Affine32 {
    fn new(plan: &ScalarNumberLoop, counter: usize) -> Option<Self> {
        let [NumberExpression::Assign(binding, value)] = plan.body.as_slice() else {
            return None;
        };
        if *binding == counter
            || !matches!(
                plan.bindings[*binding].domain,
                NumberDomain::Signed32 | NumberDomain::Unsigned32
            )
        {
            return None;
        }
        let (multiplier, addend) = affine(value, *binding, &plan.bindings)?;
        let (mut composed_multiplier, mut composed_addend) = (1_u32, 0_u32);
        for _ in 0..AFFINE_STEPS {
            composed_multiplier = multiplier.wrapping_mul(composed_multiplier);
            composed_addend = multiplier
                .wrapping_mul(composed_addend)
                .wrapping_add(addend);
        }
        Some(Self {
            binding: *binding,
            multiplier: composed_multiplier,
            addend: composed_addend,
        })
    }

    fn emit(&self, locals: &LoopLocals<'_>, value: LoopLocal, function: &mut Function) {
        let (LoopLocal::Signed32(state) | LoopLocal::Unsigned32(state)) =
            locals.values[self.binding]
        else {
            unreachable!("affine recurrence requires an exact32 local")
        };
        state.load(function);
        function.instruction(&Instruction::I32Const(self.multiplier as i32));
        function.instruction(&Instruction::I32Mul);
        function.instruction(&Instruction::I32Const(self.addend as i32));
        function.instruction(&Instruction::I32Add);
        state.store(function);
        state.load(function);
        value.store(function);
    }
}

/// Return coefficients only where the existing residue emitter proves the
/// source Number operation exact. In particular ordinary floating multiply,
/// nested assignments and nonzero bit shifts cannot enter this algebra.
fn affine(
    value: &NumberExpression,
    binding: usize,
    bindings: &[NumberBinding],
) -> Option<(u32, u32)> {
    match value {
        NumberExpression::Binding(index) if *index == binding => Some((1, 0)),
        NumberExpression::Constant(bits) if value.intrinsic_domain() != NumberDomain::Number => {
            Some((0, f64::from_bits(*bits) as i64 as u32))
        }
        NumberExpression::Imul(left, right) => {
            let (a, b) = affine(left, binding, bindings)?;
            let (c, d) = affine(right, binding, bindings)?;
            if a == 0 {
                Some((c.wrapping_mul(b), d.wrapping_mul(b)))
            } else if c == 0 {
                Some((a.wrapping_mul(d), b.wrapping_mul(d)))
            } else {
                None
            }
        }
        NumberExpression::Arithmetic(op, left, right)
            if value.binding_domain(bindings) == NumberDomain::SafeInteger =>
        {
            let (a, b) = affine(left, binding, bindings)?;
            let (c, d) = affine(right, binding, bindings)?;
            match op {
                NumberArithmetic::Add => Some((a.wrapping_add(c), b.wrapping_add(d))),
                NumberArithmetic::Sub => Some((a.wrapping_sub(c), b.wrapping_sub(d))),
                _ => None,
            }
        }
        NumberExpression::Bitwise(op, left, right)
            if matches!(right.as_ref(), NumberExpression::Constant(bits) if f64::from_bits(*bits) == 0.0)
                && matches!(
                    op,
                    BitwiseBinaryOp::Or
                        | BitwiseBinaryOp::Xor
                        | BitwiseBinaryOp::Shl
                        | BitwiseBinaryOp::Shr
                        | BitwiseBinaryOp::UShr
                ) =>
        {
            affine(left, binding, bindings)
        }
        NumberExpression::Complement(inner) => {
            let (a, b) = affine(inner, binding, bindings)?;
            Some((0_u32.wrapping_sub(a), !b))
        }
        _ => None,
    }
}
