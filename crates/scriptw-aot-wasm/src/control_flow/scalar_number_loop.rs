//! Closed Number loops retain their live values in primitive Wasm locals.
//!
//! The checked plan contains the entire test/body/update region. No admitted
//! operation can call source code, observe a deferred store, or leave the loop
//! abruptly. Entry tags and initialization are runtime facts; failure keeps the
//! original loop, including its coercions, references and completion handling.
use super::*;
use crate::emit::LocalBindingId;

mod invariant_imul;
use invariant_imul::InvariantImul;
mod chunked;
use chunked::{ChunkedLoop, WholeTripAffine32};

const MAX_NODES: usize = 256;
const MAX_DEPTH: usize = 64;
const MAX_BINDINGS: usize = 16;
const MAX_SAFE_INTEGER: i64 = (1_i64 << 53) - 1;

#[derive(Clone, Copy)]
enum NumberArithmetic {
    Add,
    Sub,
    Mul,
    Div,
}

impl NumberArithmetic {
    fn new(op: ArithmeticBinaryOp) -> Option<Self> {
        Some(match op {
            ArithmeticBinaryOp::Add => Self::Add,
            ArithmeticBinaryOp::Sub => Self::Sub,
            ArithmeticBinaryOp::Mul => Self::Mul,
            ArithmeticBinaryOp::Div => Self::Div,
            ArithmeticBinaryOp::Mod | ArithmeticBinaryOp::Exp => return None,
        })
    }

    fn emit(self, function: &mut Function) {
        function.instruction(&match self {
            Self::Add => Instruction::F64Add,
            Self::Sub => Instruction::F64Sub,
            Self::Mul => Instruction::F64Mul,
            Self::Div => Instruction::F64Div,
        });
    }
}

enum NumberExpression {
    Constant(u64),
    Binding(usize),
    Negate(Box<Self>),
    Arithmetic(NumberArithmetic, Box<Self>, Box<Self>),
    Bitwise(BitwiseBinaryOp, Box<Self>, Box<Self>),
    Complement(Box<Self>),
    Imul(Box<Self>, Box<Self>),
    Assign(usize, Box<Self>),
    Compound(usize, NumberArithmetic, Box<Self>),
    Update(usize, NumericUpdateOp, UpdateReturnMode),
    Comma(Box<Self>, Box<Self>),
}

/// These expression facts establish exact conversion ranges, independently of
/// mutable local values. Full Number consumers always keep binary64 arithmetic;
/// residue consumers may wrap exact integral addition and subtraction in i32.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NumberDomain {
    Number,
    Signed32,
    Unsigned32,
    SafeInteger,
}

enum NumberPredicate {
    Boolean(bool),
    Truthy(NumberExpression),
    Not(Box<Self>),
    Compare(RelationalBinaryOp, NumberExpression, NumberExpression),
    Equal(bool, NumberExpression, NumberExpression),
}

struct NumberBinding {
    id: LocalBindingId,
    written: bool,
    domain: NumberDomain,
}

#[derive(Clone, Copy)]
enum LoopLocal {
    Number(F64Local),
    Signed32(I32Local),
    Unsigned32(I32Local),
}

impl LoopLocal {
    fn load_number(self, function: &mut Function) {
        match self {
            Self::Number(value) => value.load(function),
            Self::Signed32(value) => {
                value.load(function);
                function.instruction(&Instruction::F64ConvertI32S);
            }
            Self::Unsigned32(value) => {
                value.load(function);
                function.instruction(&Instruction::F64ConvertI32U);
            }
        }
    }

    fn store(self, function: &mut Function) {
        match self {
            Self::Number(value) => value.store(function),
            Self::Signed32(value) | Self::Unsigned32(value) => value.store(function),
        }
    }
}

struct LoopLocals<'a> {
    bindings: &'a [NumberBinding],
    values: Vec<LoopLocal>,
}

/// Construction seals the complete operation and binding set before emission.
pub(super) struct ScalarNumberLoop {
    bindings: Vec<NumberBinding>,
    test: NumberPredicate,
    body: Vec<NumberExpression>,
    update: Option<NumberExpression>,
    imul: Vec<InvariantImul>,
    whole_trip: Option<WholeTripAffine32>,
}

struct LoopProof<'a, 'b> {
    builder: &'a FunctionBuilder<'b>,
    bindings: Vec<NumberBinding>,
    remaining: usize,
    imul: Vec<InvariantImul>,
}

impl ScalarNumberLoop {
    fn emit_body(
        &self,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        value: Option<LoopLocal>,
        function: &mut Function,
    ) {
        for (index, expression) in self.body.iter().enumerate() {
            if let Some(value) = value.filter(|_| index + 1 == self.body.len()) {
                expression.emit_into(builder, locals, value, function);
            } else {
                expression.emit_discard(builder, locals, function);
            }
        }
    }

    pub(super) fn new(
        builder: &FunctionBuilder<'_>,
        init: Option<&ForInitIr>,
        test: Option<&TypedExpr>,
        update: Option<&TypedExpr>,
        body: &StatementIr,
    ) -> Option<Self> {
        // Source globals, resumable activations and runtime bodies have their
        // own environment/completion lifetimes and never enter this region.
        if !builder.has_source_execution_environment()
            || !builder.current_function_meta().is_some_and(|meta| {
                meta.protocol().execution_kind() == FunctionExecutionKind::Ordinary
            })
            || builder.statement_list_value_context.is_some()
        {
            return None;
        }
        let mut proof = LoopProof {
            builder,
            bindings: Vec::new(),
            remaining: MAX_NODES,
            imul: Vec::new(),
        };
        let source_test = test?;
        let test = proof.predicate(source_test, MAX_DEPTH)?;
        let mut expressions = Vec::new();
        proof.statement(body, &mut expressions, MAX_DEPTH)?;
        let update = match update {
            Some(expression) => Some(proof.expression(expression, MAX_DEPTH)?),
            None => None,
        };
        if proof.imul.iter().any(|guard| {
            guard.local_receiver().is_some_and(|id| {
                proof
                    .bindings
                    .iter()
                    .any(|binding| binding.id == id && binding.written)
            })
        }) {
            return None;
        }
        // Every write establishes its range without a loop-carried range
        // assumption. Runtime entry checks establish the induction's base case.
        let mut writes = vec![None; proof.bindings.len()];
        test.collect_writes(&mut writes);
        for expression in &expressions {
            expression.collect_writes(&mut writes);
        }
        if let Some(update) = &update {
            update.collect_writes(&mut writes);
        }
        for (binding, domain) in proof.bindings.iter_mut().zip(writes) {
            binding.domain = domain.unwrap_or(NumberDomain::Number);
        }
        let mut plan = Self {
            bindings: proof.bindings,
            test,
            body: expressions,
            update,
            imul: proof.imul,
            whole_trip: None,
        };
        // Declining this narrower proof keeps the existing scalar plan intact.
        plan.whole_trip = WholeTripAffine32::new(builder, init, source_test, &plan);
        Some(plan)
    }

    pub(super) fn emit_entry_guard(
        &self,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        function.instruction(&Instruction::I32Const(1));
        for binding in &self.bindings {
            let source = builder.local_binding(binding.id);
            source.value().tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32And);
            if let Some(initialized) = source.initialization_check() {
                initialized.load(function);
                function.instruction(&Instruction::I32And);
            }
            let range = match binding.domain {
                NumberDomain::Number => None,
                NumberDomain::Signed32 => Some((i32::MIN as f64, i32::MAX as f64)),
                NumberDomain::Unsigned32 => Some((0.0, u32::MAX as f64)),
                NumberDomain::SafeInteger => {
                    Some((-(MAX_SAFE_INTEGER as f64), MAX_SAFE_INTEGER as f64))
                }
            };
            if let Some((lower, upper)) = range {
                for (bound, comparison) in
                    [(lower, Instruction::F64Ge), (upper, Instruction::F64Le)]
                {
                    source.value().scalar().load(function);
                    function.instruction(&Instruction::F64ReinterpretI64);
                    function.instruction(&Instruction::F64Const(Ieee64::from(bound)));
                    function.instruction(&comparison);
                    function.instruction(&Instruction::I32And);
                }
                source.value().scalar().load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                source.value().scalar().load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                function.instruction(&Instruction::F64Trunc);
                function.instruction(&Instruction::F64Eq);
                function.instruction(&Instruction::I32And);
                // Integer locals cannot preserve the sign of zero, including
                // a written binding in a loop whose first test is false.
                source.value().scalar().load(function);
                function.instruction(&Instruction::I64Const(i64::MIN));
                function.instruction(&Instruction::I64Ne);
                function.instruction(&Instruction::I32And);
            }
        }
        if !self.imul.is_empty() {
            let schema = builder.runtime_schema();
            let accepted = schema.reserve_i32_local(function);
            accepted.store(function);
            for guard in &self.imul {
                accepted.load(function);
                builder.open_frame(ControlFrameKind::If, function);
                guard.emit_guard(builder, accepted, function)?;
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            accepted.load(function);
            schema.release_i32_local(accepted, function);
        }
        Ok(())
    }
}

impl LoopProof<'_, '_> {
    fn visit(&mut self, depth: usize) -> Option<usize> {
        self.remaining = self.remaining.checked_sub(1)?;
        depth.checked_sub(1)
    }

    fn binding(&mut self, name: &str, written: bool) -> Option<usize> {
        if name == LEXICAL_THIS_NAME || self.builder.is_script_global_binding(name) {
            return None;
        }
        let BindingStorage::Local(id) = self.builder.lookup_binding(name)? else {
            // A captured binding's actual cell is never a scalar snapshot.
            return None;
        };
        if let Some(index) = self.bindings.iter().position(|binding| binding.id == id) {
            self.bindings[index].written |= written;
            return Some(index);
        }
        if self.bindings.len() == MAX_BINDINGS {
            return None;
        }
        let index = self.bindings.len();
        self.bindings.push(NumberBinding {
            id,
            written,
            domain: NumberDomain::Number,
        });
        Some(index)
    }

    fn expression(&mut self, value: &TypedExpr, depth: usize) -> Option<NumberExpression> {
        let depth = self.visit(depth)?;
        Some(match &value.expr {
            ExprIr::Number(bits) => NumberExpression::Constant(*bits),
            ExprIr::Identifier(name) => NumberExpression::Binding(self.binding(name, false)?),
            ExprIr::UnaryPlus { expr } => return self.expression(expr, depth),
            ExprIr::UnaryMinusNumeric { expr } => {
                NumberExpression::Negate(Box::new(self.expression(expr, depth)?))
            }
            ExprIr::BinaryNumber { op, lhs, rhs }
            | ExprIr::CoerciveBinaryNumber { op, lhs, rhs } => NumberExpression::Arithmetic(
                NumberArithmetic::new(*op)?,
                Box::new(self.expression(lhs, depth)?),
                Box::new(self.expression(rhs, depth)?),
            ),
            ExprIr::CoerciveAdd { lhs, rhs } => NumberExpression::Arithmetic(
                NumberArithmetic::Add,
                Box::new(self.expression(lhs, depth)?),
                Box::new(self.expression(rhs, depth)?),
            ),
            ExprIr::BitwiseNumeric { op, lhs, rhs } => NumberExpression::Bitwise(
                *op,
                Box::new(self.expression(lhs, depth)?),
                Box::new(self.expression(rhs, depth)?),
            ),
            ExprIr::UnaryBitwiseNumeric { op, expr } => match op {
                UnaryBitwiseOp::Complement => {
                    NumberExpression::Complement(Box::new(self.expression(expr, depth)?))
                }
            },
            ExprIr::AssignIdentifier { name, value } => NumberExpression::Assign(
                self.binding(name, true)?,
                Box::new(self.expression(value, depth)?),
            ),
            ExprIr::CompoundAssignIdentifier { name, op, value } => NumberExpression::Compound(
                self.binding(name, true)?,
                NumberArithmetic::new(*op)?,
                Box::new(self.expression(value, depth)?),
            ),
            ExprIr::UpdateIdentifier {
                name,
                op,
                return_mode,
                value_kind,
            } if !matches!(value_kind, NumericUpdateValueKind::BigInt) => {
                NumberExpression::Update(self.binding(name, true)?, *op, *return_mode)
            }
            ExprIr::Comma { lhs, rhs } => NumberExpression::Comma(
                Box::new(self.expression(lhs, depth)?),
                Box::new(self.expression(rhs, depth)?),
            ),
            ExprIr::MaterializeBinding { .. } => {
                let (guard, left, right) = InvariantImul::of_call(value, self.builder)?;
                let left = self.expression(left, depth)?;
                let right = self.expression(right, depth)?;
                if !self.imul.contains(&guard) {
                    if self.imul.len() == 4 {
                        return None;
                    }
                    self.imul.push(guard);
                }
                NumberExpression::Imul(Box::new(left), Box::new(right))
            }
            // No call/property/environment/coercion observer can be hidden in
            // a Number kind hint: the complete concrete tree must be admitted.
            _ => return None,
        })
    }

    fn predicate(&mut self, value: &TypedExpr, depth: usize) -> Option<NumberPredicate> {
        let depth = self.visit(depth)?;
        Some(match &value.expr {
            ExprIr::Boolean(value) => NumberPredicate::Boolean(*value),
            ExprIr::LogicalNot { expr } => {
                NumberPredicate::Not(Box::new(self.predicate(expr, depth)?))
            }
            ExprIr::CompareNumber { op, lhs, rhs } | ExprIr::CompareValue { op, lhs, rhs } => {
                NumberPredicate::Compare(
                    *op,
                    self.expression(lhs, depth)?,
                    self.expression(rhs, depth)?,
                )
            }
            ExprIr::StrictEquality { op, lhs, rhs } | ExprIr::LooseEquality { op, lhs, rhs } => {
                NumberPredicate::Equal(
                    matches!(
                        op,
                        EqualityBinaryOp::StrictEqual | EqualityBinaryOp::LooseEqual
                    ),
                    self.expression(lhs, depth)?,
                    self.expression(rhs, depth)?,
                )
            }
            _ => NumberPredicate::Truthy(self.expression(value, depth)?),
        })
    }

    fn statement(
        &mut self,
        statement: &StatementIr,
        expressions: &mut Vec<NumberExpression>,
        depth: usize,
    ) -> Option<()> {
        let depth = self.visit(depth)?;
        match statement {
            StatementIr::Empty => {}
            StatementIr::Expression(value) => expressions.push(self.expression(value, depth)?),
            StatementIr::Block(block) if block.lexical_environment.is_none() => {
                for statement in &block.statements {
                    self.statement(statement, expressions, depth)?;
                }
            }
            // Declining declarations also keeps const/TDZ prefixes, shadowing,
            // fresh per-iteration cells, and disposable scopes on their owner.
            _ => return None,
        }
        Some(())
    }
}

impl NumberExpression {
    fn intrinsic_domain(&self) -> NumberDomain {
        match self {
            Self::Constant(bits) => {
                let value = f64::from_bits(*bits);
                if *bits == (-0.0_f64).to_bits() {
                    NumberDomain::Number
                } else if value.fract() == 0.0
                    && (i32::MIN as f64..=i32::MAX as f64).contains(&value)
                {
                    NumberDomain::Signed32
                } else if value.fract() == 0.0 && (0.0..=u32::MAX as f64).contains(&value) {
                    NumberDomain::Unsigned32
                } else if value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER as f64 {
                    NumberDomain::SafeInteger
                } else {
                    NumberDomain::Number
                }
            }
            Self::Bitwise(BitwiseBinaryOp::UShr, _, _) => NumberDomain::Unsigned32,
            Self::Bitwise(_, _, _) | Self::Complement(_) | Self::Imul(_, _) => {
                NumberDomain::Signed32
            }
            Self::Assign(_, value) | Self::Comma(_, value) => value.intrinsic_domain(),
            Self::Arithmetic(op, lhs, rhs) => Self::arithmetic_domain(
                *op,
                lhs,
                lhs.intrinsic_domain(),
                rhs,
                rhs.intrinsic_domain(),
            ),
            Self::Binding(_)
            | Self::Negate(_)
            | Self::Compound(_, _, _)
            | Self::Update(_, _, _) => NumberDomain::Number,
        }
    }

    fn domain(&self, locals: &LoopLocals<'_>) -> NumberDomain {
        self.binding_domain(locals.bindings)
    }

    fn binding_domain(&self, bindings: &[NumberBinding]) -> NumberDomain {
        match self {
            Self::Binding(index) => bindings[*index].domain,
            Self::Assign(_, value) | Self::Comma(_, value) => value.binding_domain(bindings),
            Self::Arithmetic(op, lhs, rhs) => Self::arithmetic_domain(
                *op,
                lhs,
                lhs.binding_domain(bindings),
                rhs,
                rhs.binding_domain(bindings),
            ),
            _ => self.intrinsic_domain(),
        }
    }

    fn integer_bounds(&self, domain: NumberDomain) -> Option<(i64, i64)> {
        if let Self::Constant(bits) = self {
            let value = f64::from_bits(*bits);
            if *bits != (-0.0_f64).to_bits()
                && value.fract() == 0.0
                && value.abs() <= MAX_SAFE_INTEGER as f64
            {
                let integer = value as i64;
                return Some((integer, integer));
            }
        }
        Some(match domain {
            NumberDomain::Number => return None,
            NumberDomain::Signed32 => (i32::MIN as i64, i32::MAX as i64),
            NumberDomain::Unsigned32 => (0, u32::MAX as i64),
            NumberDomain::SafeInteger => (-MAX_SAFE_INTEGER, MAX_SAFE_INTEGER),
        })
    }

    fn arithmetic_domain(
        op: NumberArithmetic,
        lhs: &Self,
        left: NumberDomain,
        rhs: &Self,
        right: NumberDomain,
    ) -> NumberDomain {
        let bounds = || {
            let (left_min, left_max) = lhs.integer_bounds(left)?;
            let (right_min, right_max) = rhs.integer_bounds(right)?;
            let (lower, upper) = match op {
                NumberArithmetic::Add => (
                    left_min.checked_add(right_min)?,
                    left_max.checked_add(right_max)?,
                ),
                NumberArithmetic::Sub => (
                    left_min.checked_sub(right_max)?,
                    left_max.checked_sub(right_min)?,
                ),
                NumberArithmetic::Mul | NumberArithmetic::Div => return None,
            };
            (lower >= -MAX_SAFE_INTEGER && upper <= MAX_SAFE_INTEGER).then_some(())
        };
        // Every result in these bounds is an exact integral Number. Its low
        // bits may be computed with wrapping i32 Add/Sub when only ToUint32
        // observes the result. Full Number consumers still emit f64 Add/Sub.
        // Larger or fractional ranges retain the arbitrary-Number modulo owner.
        if bounds().is_some() {
            NumberDomain::SafeInteger
        } else {
            NumberDomain::Number
        }
    }

    fn collect_writes(&self, writes: &mut [Option<NumberDomain>]) {
        fn record(writes: &mut [Option<NumberDomain>], index: usize, domain: NumberDomain) {
            writes[index] = Some(match writes[index] {
                None => domain,
                Some(old) if old == domain => domain,
                Some(_) => NumberDomain::Number,
            });
        }
        match self {
            Self::Assign(index, value) => {
                record(writes, *index, value.intrinsic_domain());
                value.collect_writes(writes);
            }
            Self::Compound(index, _, value) => {
                record(writes, *index, NumberDomain::Number);
                value.collect_writes(writes);
            }
            Self::Update(index, _, _) => record(writes, *index, NumberDomain::Number),
            Self::Negate(value) | Self::Complement(value) => value.collect_writes(writes),
            Self::Arithmetic(_, lhs, rhs)
            | Self::Bitwise(_, lhs, rhs)
            | Self::Imul(lhs, rhs)
            | Self::Comma(lhs, rhs) => {
                lhs.collect_writes(writes);
                rhs.collect_writes(writes);
            }
            Self::Constant(_) | Self::Binding(_) => {}
        }
    }

    fn emit_uint32(
        &self,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        function: &mut Function,
    ) {
        match self {
            Self::Constant(bits) if self.intrinsic_domain() != NumberDomain::Number => {
                function.instruction(&Instruction::I32Const(f64::from_bits(*bits) as i64 as i32));
            }
            Self::Binding(index) => match locals.values[*index] {
                LoopLocal::Signed32(value) | LoopLocal::Unsigned32(value) => {
                    value.load(function);
                }
                LoopLocal::Number(_) => {
                    self.emit(builder, locals, function);
                    self.emit_number_to_uint32(builder, locals, function);
                }
            },
            Self::Bitwise(op, lhs, rhs) => {
                lhs.emit_uint32(builder, locals, function);
                rhs.emit_uint32(builder, locals, function);
                function.instruction(&match op {
                    BitwiseBinaryOp::And => Instruction::I32And,
                    BitwiseBinaryOp::Or => Instruction::I32Or,
                    BitwiseBinaryOp::Xor => Instruction::I32Xor,
                    BitwiseBinaryOp::Shl => Instruction::I32Shl,
                    BitwiseBinaryOp::Shr => Instruction::I32ShrS,
                    BitwiseBinaryOp::UShr => Instruction::I32ShrU,
                });
            }
            Self::Complement(value) => {
                value.emit_uint32(builder, locals, function);
                function.instruction(&Instruction::I32Const(-1));
                function.instruction(&Instruction::I32Xor);
            }
            Self::Imul(lhs, rhs) => {
                lhs.emit_uint32(builder, locals, function);
                rhs.emit_uint32(builder, locals, function);
                function.instruction(&Instruction::I32Mul);
            }
            Self::Arithmetic(op, lhs, rhs) if self.domain(locals) == NumberDomain::SafeInteger => {
                lhs.emit_uint32(builder, locals, function);
                rhs.emit_uint32(builder, locals, function);
                function.instruction(&match op {
                    NumberArithmetic::Add => Instruction::I32Add,
                    NumberArithmetic::Sub => Instruction::I32Sub,
                    // Only exact integral Add/Sub establish SafeInteger.
                    NumberArithmetic::Mul | NumberArithmetic::Div => unreachable!(),
                });
            }
            Self::Assign(index, value) => match locals.values[*index] {
                LoopLocal::Signed32(target) | LoopLocal::Unsigned32(target) => {
                    value.emit_uint32(builder, locals, function);
                    target.store(function);
                    target.load(function);
                }
                LoopLocal::Number(target) => {
                    // This assignment is observable after the enclosing residue
                    // is consumed. Store its full Number exactly once first.
                    value.emit(builder, locals, function);
                    target.store(function);
                    target.load(function);
                    self.emit_number_to_uint32(builder, locals, function);
                }
            },
            Self::Comma(lhs, rhs) => {
                lhs.emit_discard(builder, locals, function);
                rhs.emit_uint32(builder, locals, function);
            }
            _ => {
                self.emit(builder, locals, function);
                self.emit_number_to_uint32(builder, locals, function);
            }
        }
    }

    fn emit_number_to_uint32(
        &self,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        function: &mut Function,
    ) {
        match self.domain(locals) {
            NumberDomain::Signed32 => {
                function.instruction(&Instruction::I32TruncSatF64S);
            }
            NumberDomain::Unsigned32 => {
                function.instruction(&Instruction::I32TruncSatF64U);
            }
            NumberDomain::SafeInteger => {
                function.instruction(&Instruction::I64TruncSatF64S);
                function.instruction(&Instruction::I32WrapI64);
            }
            NumberDomain::Number => {
                let schema = builder.runtime_schema();
                let bits = schema.reserve_i64_local(function);
                function.instruction(&Instruction::I64ReinterpretF64);
                bits.store(function);
                builder.emit_to_uint32_i64_from_number_payload(bits, bits, function);
                bits.load(function);
                function.instruction(&Instruction::I32WrapI64);
                schema.release_i64_local(bits, function);
            }
        }
    }

    fn emit_into(
        &self,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        target: LoopLocal,
        function: &mut Function,
    ) {
        match target {
            LoopLocal::Number(_) => self.emit(builder, locals, function),
            LoopLocal::Signed32(_) | LoopLocal::Unsigned32(_) => {
                self.emit_uint32(builder, locals, function);
            }
        }
        target.store(function);
    }

    fn emit_discard(
        &self,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        function: &mut Function,
    ) {
        match self.domain(locals) {
            NumberDomain::Signed32 | NumberDomain::Unsigned32 => {
                self.emit_uint32(builder, locals, function);
            }
            NumberDomain::Number | NumberDomain::SafeInteger => {
                self.emit(builder, locals, function)
            }
        }
        function.instruction(&Instruction::Drop);
    }

    /// The Wasm stack snapshots the complete left Number before evaluating
    /// the right expression, including assignments to any left-hand binding.
    fn emit(
        &self,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        function: &mut Function,
    ) {
        match self {
            Self::Constant(bits) => {
                function.instruction(&Instruction::F64Const(Ieee64::from(f64::from_bits(*bits))));
            }
            Self::Binding(index) => locals.values[*index].load_number(function),
            Self::Negate(value) => {
                value.emit(builder, locals, function);
                function.instruction(&Instruction::F64Neg);
            }
            Self::Arithmetic(op, lhs, rhs) => {
                lhs.emit(builder, locals, function);
                rhs.emit(builder, locals, function);
                op.emit(function);
            }
            Self::Bitwise(_, _, _) | Self::Complement(_) | Self::Imul(_, _) => {
                self.emit_uint32(builder, locals, function);
                function.instruction(&if self.domain(locals) == NumberDomain::Unsigned32 {
                    Instruction::F64ConvertI32U
                } else {
                    Instruction::F64ConvertI32S
                });
            }
            Self::Assign(index, value) => {
                value.emit_into(builder, locals, locals.values[*index], function);
                locals.values[*index].load_number(function);
            }
            Self::Compound(index, op, value) => {
                locals.values[*index].load_number(function);
                value.emit(builder, locals, function);
                op.emit(function);
                let LoopLocal::Number(target) = locals.values[*index] else {
                    unreachable!("compound/update writes require Number locals")
                };
                target.store(function);
                locals.values[*index].load_number(function);
            }
            Self::Update(index, op, mode) => {
                if *mode == UpdateReturnMode::Postfix {
                    locals.values[*index].load_number(function);
                }
                locals.values[*index].load_number(function);
                function.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
                match op {
                    NumericUpdateOp::Increment => function.instruction(&Instruction::F64Add),
                    NumericUpdateOp::Decrement => function.instruction(&Instruction::F64Sub),
                };
                let LoopLocal::Number(target) = locals.values[*index] else {
                    unreachable!("compound/update writes require Number locals")
                };
                target.store(function);
                if *mode == UpdateReturnMode::Prefix {
                    locals.values[*index].load_number(function);
                }
            }
            Self::Comma(lhs, rhs) => {
                lhs.emit_discard(builder, locals, function);
                rhs.emit(builder, locals, function);
            }
        }
    }
}

impl NumberPredicate {
    fn collect_writes(&self, writes: &mut [Option<NumberDomain>]) {
        match self {
            Self::Boolean(_) => {}
            Self::Truthy(value) => value.collect_writes(writes),
            Self::Not(value) => value.collect_writes(writes),
            Self::Compare(_, lhs, rhs) | Self::Equal(_, lhs, rhs) => {
                lhs.collect_writes(writes);
                rhs.collect_writes(writes);
            }
        }
    }

    fn emit(
        &self,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        function: &mut Function,
    ) {
        match self {
            Self::Boolean(value) => {
                function.instruction(&Instruction::I32Const(i32::from(*value)));
            }
            Self::Truthy(value) => {
                let schema = builder.runtime_schema();
                let evaluated = schema.reserve_f64_local(function);
                value.emit(builder, locals, function);
                evaluated.store(function);
                evaluated.load(function);
                function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
                function.instruction(&Instruction::F64Ne);
                evaluated.load(function);
                evaluated.load(function);
                function.instruction(&Instruction::F64Eq);
                function.instruction(&Instruction::I32And);
                schema.release_f64_local(evaluated, function);
            }
            Self::Not(value) => {
                value.emit(builder, locals, function);
                function.instruction(&Instruction::I32Eqz);
            }
            Self::Compare(op, lhs, rhs) => {
                lhs.emit(builder, locals, function);
                rhs.emit(builder, locals, function);
                function.instruction(&match op {
                    RelationalBinaryOp::LessThan => Instruction::F64Lt,
                    RelationalBinaryOp::LessThanOrEqual => Instruction::F64Le,
                    RelationalBinaryOp::GreaterThan => Instruction::F64Gt,
                    RelationalBinaryOp::GreaterThanOrEqual => Instruction::F64Ge,
                });
            }
            Self::Equal(equal, lhs, rhs) => {
                lhs.emit(builder, locals, function);
                rhs.emit(builder, locals, function);
                function.instruction(&if *equal {
                    Instruction::F64Eq
                } else {
                    Instruction::F64Ne
                });
            }
        }
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn compile_scalar_number_loop(
        &mut self,
        plan: &ScalarNumberLoop,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let values = plan
            .bindings
            .iter()
            .map(|binding| {
                self.local_binding(binding.id)
                    .value()
                    .scalar()
                    .load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                let local = match binding.domain {
                    NumberDomain::Number | NumberDomain::SafeInteger => {
                        LoopLocal::Number(schema.reserve_f64_local(function))
                    }
                    NumberDomain::Signed32 => {
                        function.instruction(&Instruction::I32TruncSatF64S);
                        LoopLocal::Signed32(schema.reserve_i32_local(function))
                    }
                    NumberDomain::Unsigned32 => {
                        function.instruction(&Instruction::I32TruncSatF64U);
                        LoopLocal::Unsigned32(schema.reserve_i32_local(function))
                    }
                };
                local.store(function);
                local
            })
            .collect();
        let locals = LoopLocals {
            bindings: &plan.bindings,
            values,
        };
        // The body's last value is independent of later update/final-test
        // writes. Keep its proven representation until loop exit, too.
        let value = plan
            .body
            .last()
            .map(|expression| match expression.domain(&locals) {
                NumberDomain::Number | NumberDomain::SafeInteger => {
                    LoopLocal::Number(schema.reserve_f64_local(function))
                }
                NumberDomain::Signed32 => LoopLocal::Signed32(schema.reserve_i32_local(function)),
                NumberDomain::Unsigned32 => {
                    LoopLocal::Unsigned32(schema.reserve_i32_local(function))
                }
            });
        let iterated = schema.reserve_i32_local(function);
        iterated.set_constant(0, function);
        let iterations_done = plan.whole_trip.as_ref().map(|whole_trip| {
            let done = self.open_frame(ControlFrameKind::Block, function);
            whole_trip.emit_guarded(
                self,
                &locals,
                value.expect("whole-trip assignment has a body value"),
                iterated,
                done,
                function,
            );
            done
        });
        if let Some(chunked) = ChunkedLoop::new(plan) {
            chunked.emit(self, plan, &locals, value, iterated, function);
        }
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        plan.test.emit(self, &locals, function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, function);
        plan.emit_body(self, &locals, value, function);
        iterated.set_constant(1, function);
        if let Some(update) = &plan.update {
            update.emit_discard(self, &locals, function);
        }
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if iterations_done.is_some() {
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
        }
        for (binding, local) in plan.bindings.iter().zip(&locals.values) {
            if binding.written {
                local.load_number(function);
                function.instruction(&Instruction::I64ReinterpretF64);
                self.local_binding(binding.id)
                    .value()
                    .scalar()
                    .store(function);
            }
        }
        if let Some(value) = value {
            iterated.load(function);
            self.open_frame(ControlFrameKind::If, function);
            value.load_number(function);
            function.instruction(&Instruction::I64ReinterpretF64);
            self.completion().value().scalar().store(function);
            self.completion()
                .value()
                .set_number(self.completion().value().scalar(), function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        schema.release_i32_local(iterated, function);
        for local in value.into_iter().chain(locals.values.into_iter().rev()) {
            match local {
                LoopLocal::Number(value) => schema.release_f64_local(value, function),
                LoopLocal::Signed32(value) | LoopLocal::Unsigned32(value) => {
                    schema.release_i32_local(value, function);
                }
            }
        }
    }
}
