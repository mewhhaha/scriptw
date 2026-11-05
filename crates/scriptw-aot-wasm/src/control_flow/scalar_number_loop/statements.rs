//! Ordered statements share the existing Number kernels and publication tail.
use super::*;

pub(super) enum NumberStatement {
    Expression(NumberExpression),
    Initialize(NumberExpression),
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
                let right = alternate.last()?.domain(locals)?;
                if left == right {
                    Some(left)
                } else {
                    Some(NumberDomain::Number)
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
