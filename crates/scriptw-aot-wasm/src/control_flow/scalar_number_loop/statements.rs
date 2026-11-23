//! Ordered statements share the existing Number kernels and publication tail.
use super::*;

pub(super) enum NumberStatement {
    Expression(NumberExpression),
    Initialize(NumberExpression),
    /// An empty `alternate` is an `if` without `else`: a false test
    /// completes the statement with undefined.
    If {
        test: NumberPredicate,
        consequent: Vec<Self>,
        alternate: Vec<Self>,
    },
}

impl NumberStatement {
    pub(super) fn expression(&self) -> Option<&NumberExpression> {
        match self {
            Self::Expression(value) => Some(value),
            Self::Initialize(_) | Self::If { .. } => None,
        }
    }

    pub(super) fn has_value(&self) -> bool {
        match self {
            Self::Expression(_) | Self::If { .. } => true,
            Self::Initialize(_) => false,
        }
    }

    pub(super) fn domain(&self, locals: &LoopLocals<'_>) -> Option<NumberDomain> {
        match self {
            Self::Expression(value) => Some(value.domain(locals)),
            Self::Initialize(_) => None,
            Self::If {
                consequent,
                alternate,
                ..
            } => {
                let left = consequent.last()?.domain(locals)?;
                // Without `else`, the false test's undefined is carried by
                // `LoopLocals::defined`, not by the Number local.
                let Some(alternate) = alternate.last() else {
                    return Some(left);
                };
                let right = alternate.domain(locals)?;
                if left == right {
                    Some(left)
                } else {
                    Some(NumberDomain::Number)
                }
            }
        }
    }

    /// Whether this statement, as the last of the loop body, may leave the
    /// body completion value undefined rather than a Number.
    pub(super) fn may_complete_undefined(&self) -> bool {
        match self {
            Self::Expression(_) | Self::Initialize(_) => false,
            Self::If {
                consequent,
                alternate,
                ..
            } => {
                alternate.is_empty()
                    || consequent.last().is_some_and(Self::may_complete_undefined)
                    || alternate.last().is_some_and(Self::may_complete_undefined)
            }
        }
    }

    /// Calls `visit` on every expression of the statement, children first.
    pub(super) fn visit_mut(&mut self, visit: &mut dyn FnMut(&mut NumberExpression)) {
        match self {
            Self::Expression(value) | Self::Initialize(value) => value.visit_mut(visit),
            Self::If {
                test,
                consequent,
                alternate,
            } => {
                test.visit_mut(visit);
                for statement in consequent.iter_mut().chain(alternate) {
                    statement.visit_mut(visit);
                }
            }
        }
    }

    pub(super) fn collect_writes(&self, writes: &mut [Option<NumberDomain>]) {
        match self {
            Self::Expression(value) | Self::Initialize(value) => value.collect_writes(writes),
            Self::If {
                test,
                consequent,
                alternate,
            } => {
                test.collect_writes(writes);
                for statement in consequent.iter().chain(alternate) {
                    statement.collect_writes(writes);
                }
            }
        }
    }

    pub(super) fn emit(
        &self,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        value: Option<LoopLocal>,
        function: &mut Function,
    ) {
        match self {
            Self::Expression(expression) => {
                if let Some(value) = value {
                    expression.emit_into(builder, locals, value, function);
                    if let Some(defined) = locals.defined {
                        defined.set_constant(1, function);
                    }
                } else {
                    expression.emit_discard(builder, locals, function);
                }
            }
            Self::Initialize(expression) => expression.emit_discard(builder, locals, function),
            Self::If {
                test,
                consequent,
                alternate,
            } => {
                test.emit(builder, locals, function);
                builder.open_frame(ControlFrameKind::If, function);
                Self::emit_list(consequent, builder, locals, value, function);
                function.instruction(&Instruction::Else);
                Self::emit_list(alternate, builder, locals, value, function);
                if alternate.is_empty() && value.is_some() {
                    if let Some(defined) = locals.defined {
                        defined.set_constant(0, function);
                    }
                }
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
    }

    pub(super) fn emit_list(
        statements: &[Self],
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        value: Option<LoopLocal>,
        function: &mut Function,
    ) {
        for (index, statement) in statements.iter().enumerate() {
            statement.emit(
                builder,
                locals,
                value.filter(|_| index + 1 == statements.len()),
                function,
            );
        }
    }
}
