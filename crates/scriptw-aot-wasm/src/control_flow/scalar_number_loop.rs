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
/// mutable local values. Arithmetic keeps binary64 rounding and returns Number.
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

struct LoopLocal {
    value: F64Local,
    domain: NumberDomain,
}

/// Construction seals the complete operation and binding set before emission.
pub(super) struct ScalarNumberLoop {
    bindings: Vec<NumberBinding>,
    test: NumberPredicate,
    body: Vec<NumberExpression>,
    update: Option<NumberExpression>,
    imul: Vec<InvariantImul>,
}

struct LoopProof<'a, 'b> {
    builder: &'a FunctionBuilder<'b>,
    bindings: Vec<NumberBinding>,
    remaining: usize,
    imul: Vec<InvariantImul>,
}

impl ScalarNumberLoop {
    pub(super) fn new(
        builder: &FunctionBuilder<'_>,
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
        let test = proof.predicate(test?, MAX_DEPTH)?;
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
        Some(Self {
            bindings: proof.bindings,
            test,
            body: expressions,
            update,
            imul: proof.imul,
        })
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
                if value.fract() == 0.0 && (i32::MIN as f64..=i32::MAX as f64).contains(&value) {
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

    fn domain(&self, locals: &[LoopLocal]) -> NumberDomain {
        match self {
            Self::Binding(index) => locals[*index].domain,
            Self::Assign(_, value) | Self::Comma(_, value) => value.domain(locals),
            Self::Arithmetic(op, lhs, rhs) => {
                Self::arithmetic_domain(*op, lhs, lhs.domain(locals), rhs, rhs.domain(locals))
            }
            _ => self.intrinsic_domain(),
        }
    }

    fn integer_bounds(&self, domain: NumberDomain) -> Option<(i64, i64)> {
        if let Self::Constant(bits) = self {
            let value = f64::from_bits(*bits);
            if value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER as f64 {
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
        // Keep the f64 Add/Sub and its evaluation order. This fact changes
        // only the subsequent ToUint32: no integral result here can round or
        // overflow a signed i64 conversion. Larger or fractional ranges retain
        // the arbitrary-Number modulo owner.
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
        locals: &[LoopLocal],
        function: &mut Function,
    ) {
        self.emit(builder, locals, function);
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

    /// The Wasm stack snapshots the complete left Number before evaluating
    /// the right expression, including assignments to any left-hand binding.
    fn emit(
        &self,
        builder: &mut FunctionBuilder<'_>,
        locals: &[LoopLocal],
        function: &mut Function,
    ) {
        match self {
            Self::Constant(bits) => {
                function.instruction(&Instruction::F64Const(Ieee64::from(f64::from_bits(*bits))));
            }
            Self::Binding(index) => locals[*index].value.load(function),
            Self::Negate(value) => {
                value.emit(builder, locals, function);
                function.instruction(&Instruction::F64Neg);
            }
            Self::Arithmetic(op, lhs, rhs) => {
                lhs.emit(builder, locals, function);
                rhs.emit(builder, locals, function);
                op.emit(function);
            }
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
                function.instruction(&if *op == BitwiseBinaryOp::UShr {
                    Instruction::F64ConvertI32U
                } else {
                    Instruction::F64ConvertI32S
                });
            }
            Self::Complement(value) => {
                value.emit_uint32(builder, locals, function);
                function.instruction(&Instruction::I32Const(-1));
                function.instruction(&Instruction::I32Xor);
                function.instruction(&Instruction::F64ConvertI32S);
            }
            Self::Imul(lhs, rhs) => {
                lhs.emit_uint32(builder, locals, function);
                rhs.emit_uint32(builder, locals, function);
                function.instruction(&Instruction::I32Mul);
                function.instruction(&Instruction::F64ConvertI32S);
            }
            Self::Assign(index, value) => {
                value.emit(builder, locals, function);
                locals[*index].value.store(function);
                locals[*index].value.load(function);
            }
            Self::Compound(index, op, value) => {
                locals[*index].value.load(function);
                value.emit(builder, locals, function);
                op.emit(function);
                locals[*index].value.store(function);
                locals[*index].value.load(function);
            }
            Self::Update(index, op, mode) => {
                if *mode == UpdateReturnMode::Postfix {
                    locals[*index].value.load(function);
                }
                locals[*index].value.load(function);
                function.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
                match op {
                    NumericUpdateOp::Increment => function.instruction(&Instruction::F64Add),
                    NumericUpdateOp::Decrement => function.instruction(&Instruction::F64Sub),
                };
                locals[*index].value.store(function);
                if *mode == UpdateReturnMode::Prefix {
                    locals[*index].value.load(function);
                }
            }
            Self::Comma(lhs, rhs) => {
                lhs.emit(builder, locals, function);
                function.instruction(&Instruction::Drop);
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
        locals: &[LoopLocal],
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
        let locals: Vec<_> = plan
            .bindings
            .iter()
            .map(|binding| {
                let local = schema.reserve_f64_local(function);
                self.local_binding(binding.id)
                    .value()
                    .scalar()
                    .load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                local.store(function);
                LoopLocal {
                    value: local,
                    domain: binding.domain,
                }
            })
            .collect();
        let value = schema.reserve_f64_local(function);
        let iterated = schema.reserve_i32_local(function);
        iterated.set_constant(0, function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        plan.test.emit(self, &locals, function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, function);
        for expression in &plan.body {
            expression.emit(self, &locals, function);
            value.store(function);
        }
        iterated.set_constant(1, function);
        if let Some(update) = &plan.update {
            update.emit(self, &locals, function);
            function.instruction(&Instruction::Drop);
        }
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        for (binding, local) in plan.bindings.iter().zip(&locals) {
            if binding.written {
                local.value.load(function);
                function.instruction(&Instruction::I64ReinterpretF64);
                self.local_binding(binding.id)
                    .value()
                    .scalar()
                    .store(function);
            }
        }
        if !plan.body.is_empty() {
            iterated.load(function);
            self.open_frame(ControlFrameKind::If, function);
            value.load(function);
            function.instruction(&Instruction::I64ReinterpretF64);
            self.completion().value().scalar().store(function);
            self.completion()
                .value()
                .set_number(self.completion().value().scalar(), function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        schema.release_i32_local(iterated, function);
        schema.release_f64_local(value, function);
        for local in locals.into_iter().rev() {
            schema.release_f64_local(local.value, function);
        }
    }
}
