//! Body-only facts for private counters. The ordinary loop still owns every
//! read, call, write, completion and update; no source operation is deferred.
use super::*;
use crate::emit::LocalBindingId;

#[derive(Clone, Copy)]
pub(crate) struct BoundedInduction {
    binding: LocalBindingId,
}

/// Ties an unsigned Number proof to the actual read being compiled.
pub(crate) struct BoundedInductionRead<'a> {
    expression: &'a TypedExpr,
}

impl<'a> BoundedInductionRead<'a> {
    pub(crate) fn expression(&self) -> &'a TypedExpr {
        self.expression
    }
}

impl BoundedInduction {
    pub(super) fn new(
        builder: &FunctionBuilder<'_>,
        init: Option<&ForInitIr>,
        test: Option<&TypedExpr>,
        update: Option<&TypedExpr>,
        body: &StatementIr,
        lexical_environment: Option<&ForLexicalEnvironmentIr>,
    ) -> Option<Self> {
        if lexical_environment.is_some()
            || !builder.has_source_execution_environment()
            || !builder.current_function_meta().is_some_and(|meta| {
                meta.protocol().execution_kind() == FunctionExecutionKind::Ordinary
            })
        {
            return None;
        }
        let (name, initial) = match init? {
            ForInitIr::Var(declarations) if declarations.len() == 1 => {
                let declaration = &declarations[0];
                (declaration.name.as_str(), declaration.init.as_ref()?)
            }
            ForInitIr::Lexical {
                mode: BindingMode::Let,
                name,
                init,
            } => (name.as_str(), init),
            _ => return None,
        };
        // Sloppy parameters can alias a mapped arguments object, including one
        // exposed to a callback through legacy function reflection.
        if name == "arguments"
            || builder
                .params
                .iter()
                .any(|parameter| parameter.name == name)
        {
            return None;
        }
        unsigned_literal(initial)?;
        let binding = builder.private_local_binding(name)?;
        let (lhs, rhs) = match &test?.expr {
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
        if !matches!(&lhs.expr, ExprIr::Identifier(read) if read == name) {
            return None;
        }
        unsigned_literal(rhs)?;
        if !matches!(&update?.expr, ExprIr::UpdateIdentifier {
            name: target, op: NumericUpdateOp::Increment, value_kind, ..
        } if target == name && !matches!(value_kind, NumericUpdateValueKind::BigInt))
        {
            return None;
        }
        if !(PreservesCounter {
            name,
            remaining: 1024,
        })
        .statement(body, 64)
        {
            return None;
        }
        Some(Self { binding })
    }
}

fn unsigned_literal(expression: &TypedExpr) -> Option<u32> {
    let ExprIr::Number(bits) = expression.expr else {
        return None;
    };
    let number = f64::from_bits(bits);
    // In particular, an initial -0 cannot justify unsigned remainder's +0.
    if !number.is_finite()
        || number.is_sign_negative()
        || number.fract() != 0.0
        || number > f64::from(u32::MAX)
    {
        return None;
    }
    Some(number as u32)
}

impl FunctionBuilder<'_> {
    pub(crate) fn bounded_induction_read<'a>(
        &self,
        expression: &'a TypedExpr,
    ) -> Option<BoundedInductionRead<'a>> {
        let ExprIr::Identifier(name) = &expression.expr else {
            return None;
        };
        let binding = self.private_local_binding(name)?;
        if self
            .bounded_inductions
            .iter()
            .any(|fact| fact.binding == binding)
        {
            return Some(BoundedInductionRead { expression });
        }
        None
    }
}

/// Unsupported trees decline the fact. Ordinary calls are allowed because a
/// Local binding has no closure/global/environment alias; operands still need
/// inspection for explicit writes. Shadowing conservatively declines too.
struct PreservesCounter<'a> {
    name: &'a str,
    remaining: usize,
}

impl PreservesCounter<'_> {
    fn enter(&mut self, depth: usize) -> Option<usize> {
        self.remaining = self.remaining.checked_sub(1)?;
        depth.checked_sub(1)
    }

    fn block(&mut self, block: &BlockIr, depth: usize) -> bool {
        block
            .statements
            .iter()
            .all(|statement| self.statement(statement, depth))
    }

    fn init(&mut self, init: &ForInitIr, depth: usize) -> bool {
        match init {
            ForInitIr::Var(declarations) => declarations.iter().all(|declaration| {
                declaration.name != self.name
                    && declaration
                        .init
                        .as_ref()
                        .is_none_or(|value| self.expression(value, depth))
            }),
            ForInitIr::Lexical { name, init, .. } => {
                name != self.name && self.expression(init, depth)
            }
            ForInitIr::Expression(expression) => self.expression(expression, depth),
            _ => false,
        }
    }

    fn statement(&mut self, statement: &StatementIr, depth: usize) -> bool {
        let Some(depth) = self.enter(depth) else {
            return false;
        };
        match statement {
            StatementIr::Empty
            | StatementIr::Debugger
            | StatementIr::Break { .. }
            | StatementIr::Continue { .. } => true,
            StatementIr::Expression(value)
            | StatementIr::DeclarationEvaluation(value)
            | StatementIr::Return(value)
            | StatementIr::Throw(value) => self.expression(value, depth),
            StatementIr::Var(declarations) => declarations.iter().all(|declaration| {
                declaration.name != self.name
                    && declaration
                        .init
                        .as_ref()
                        .is_none_or(|value| self.expression(value, depth))
            }),
            StatementIr::Lexical { name, init, .. } => {
                name != self.name && self.expression(init, depth)
            }
            StatementIr::LexicalBlock(statements) => {
                statements.iter().all(|value| self.statement(value, depth))
            }
            StatementIr::EmptyStatementCompletion(value) => {
                self.statement(value.statement(), depth)
            }
            StatementIr::Block(block) => self.block(block, depth),
            StatementIr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expression(condition, depth)
                    && self.statement(then_branch, depth)
                    && else_branch
                        .as_ref()
                        .is_none_or(|branch| self.statement(branch, depth))
            }
            StatementIr::While { condition, body } | StatementIr::DoWhile { condition, body } => {
                self.expression(condition, depth) && self.statement(body, depth)
            }
            StatementIr::For {
                init,
                test,
                update,
                body,
                ..
            } => {
                init.as_ref().is_none_or(|init| self.init(init, depth))
                    && test
                        .as_ref()
                        .is_none_or(|test| self.expression(test, depth))
                    && update
                        .as_ref()
                        .is_none_or(|update| self.expression(update, depth))
                    && self.statement(body, depth)
            }
            StatementIr::Labelled {
                statement,
                async_plan: None,
                ..
            } => self.statement(statement, depth),
            StatementIr::TryCatch {
                try_block,
                catch_name,
                catch_block,
                generator_plan: None,
                async_plan: None,
                ..
            } => {
                catch_name != self.name
                    && self.block(try_block, depth)
                    && self.block(catch_block, depth)
            }
            StatementIr::TryFinally {
                try_block,
                finally_block,
                generator_plan: None,
                async_plan: None,
            } => self.block(try_block, depth) && self.block(finally_block, depth),
            StatementIr::TryCatchFinally {
                try_block,
                catch_name,
                catch_block,
                finally_block,
                generator_plan: None,
                async_plan: None,
                ..
            } => {
                catch_name != self.name
                    && self.block(try_block, depth)
                    && self.block(catch_block, depth)
                    && self.block(finally_block, depth)
            }
            StatementIr::Switch {
                discriminant,
                lexical_declarations,
                cases,
                ..
            } => {
                self.expression(discriminant, depth)
                    && lexical_declarations
                        .iter()
                        .all(|value| self.statement(value, depth))
                    && cases.iter().all(|case| {
                        case.condition
                            .as_ref()
                            .is_none_or(|value| self.expression(value, depth))
                            && self.block(&case.body, depth)
                    })
            }
            _ => false,
        }
    }

    fn key(&mut self, key: &PropertyKeyIr, depth: usize) -> bool {
        match key {
            PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => true,
            PropertyKeyIr::ArrayIndex(expression) | PropertyKeyIr::StringExpr(expression) => {
                self.expression(expression, depth)
            }
        }
    }

    fn expression(&mut self, expression: &TypedExpr, depth: usize) -> bool {
        let Some(depth) = self.enter(depth) else {
            return false;
        };
        match &expression.expr {
            ExprIr::Undefined
            | ExprIr::ArrayHole
            | ExprIr::Null
            | ExprIr::Boolean(_)
            | ExprIr::Number(_)
            | ExprIr::BigInt(_)
            | ExprIr::WellKnownSymbol(_)
            | ExprIr::String(_)
            | ExprIr::RegExpLiteral { .. }
            | ExprIr::FunctionValue(_)
            | ExprIr::Identifier(_)
            | ExprIr::This
            | ExprIr::Arguments
            | ExprIr::NewTarget
            | ExprIr::ExecutionGlobalObject
            | ExprIr::GlobalPropertyRead { .. }
            | ExprIr::GlobalIdentifierRead { .. }
            | ExprIr::TypeOfUnresolvedIdentifier { .. }
            | ExprIr::RuntimeThrow { .. }
            | ExprIr::TemplateObject(_) => true,
            ExprIr::AssignIdentifier { name, value }
            | ExprIr::CompoundAssignIdentifier { name, value, .. } => {
                name != self.name && self.expression(value, depth)
            }
            ExprIr::UpdateIdentifier { name, .. } | ExprIr::DeleteIdentifier { name } => {
                name != self.name
            }
            ExprIr::MaterializeBinding { name, value, body } => {
                name != self.name && self.expression(value, depth) && self.expression(body, depth)
            }
            ExprIr::UnaryPlus { expr }
            | ExprIr::UnaryMinusNumeric { expr }
            | ExprIr::UnaryBitwiseNumeric { expr, .. }
            | ExprIr::Void { expr }
            | ExprIr::DeleteValue { expr }
            | ExprIr::TypeOf { expr }
            | ExprIr::LogicalNot { expr } => self.expression(expr, depth),
            ExprIr::BinaryNumber { lhs, rhs, .. }
            | ExprIr::CoerciveAdd { lhs, rhs }
            | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
            | ExprIr::BitwiseNumeric { lhs, rhs, .. }
            | ExprIr::CompareNumber { lhs, rhs, .. }
            | ExprIr::CompareValue { lhs, rhs, .. }
            | ExprIr::StrictEquality { lhs, rhs, .. }
            | ExprIr::LooseEquality { lhs, rhs, .. }
            | ExprIr::LogicalShortCircuit { lhs, rhs, .. }
            | ExprIr::Comma { lhs, rhs }
            | ExprIr::StringConcat { lhs, rhs }
            | ExprIr::InstanceOf { lhs, rhs }
            | ExprIr::In { lhs, rhs } => self.expression(lhs, depth) && self.expression(rhs, depth),
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                self.expression(condition, depth)
                    && self.expression(then_expr, depth)
                    && self.expression(else_expr, depth)
            }
            ExprIr::CallNamed { name, args } => {
                name != "eval" && args.iter().all(|arg| self.expression(arg, depth))
            }
            ExprIr::CallIndirect {
                direct_eval: None,
                callee,
                this_arg,
                args,
                ..
            } => {
                self.expression(callee, depth)
                    && this_arg
                        .as_ref()
                        .is_none_or(|arg| self.expression(arg, depth))
                    && args.iter().all(|arg| self.expression(arg, depth))
            }
            ExprIr::Construct { callee, args, .. } => {
                self.expression(callee, depth) && args.iter().all(|arg| self.expression(arg, depth))
            }
            ExprIr::CallMethod {
                receiver,
                key,
                args,
            } => {
                self.expression(receiver, depth)
                    && self.key(key, depth)
                    && args.iter().all(|arg| self.expression(arg, depth))
            }
            ExprIr::PropertyRead { target, key } | ExprIr::DeleteProperty { target, key, .. } => {
                self.expression(target, depth) && self.key(key, depth)
            }
            ExprIr::PropertyWrite {
                target, key, value, ..
            } => {
                self.expression(target, depth)
                    && self.key(key, depth)
                    && self.expression(value, depth)
            }
            ExprIr::GlobalPropertyWrite { value, .. } => self.expression(value, depth),
            ExprIr::OrdinaryPropertyAssignment(value) => {
                self.expression(value.base_and_receiver(), depth)
                    && self.key(value.referenced_name(), depth)
                    && self.expression(value.rhs(), depth)
            }
            ExprIr::OrdinaryPropertyLogicalAssignment(value) => {
                self.expression(value.base_and_receiver(), depth)
                    && self.key(value.referenced_name(), depth)
                    && self.expression(value.rhs(), depth)
            }
            ExprIr::OrdinaryPropertyEagerCompoundAssignment(value) => {
                self.expression(value.base_and_receiver(), depth)
                    && self.key(value.referenced_name(), depth)
                    && self.expression(value.result(), depth)
            }
            ExprIr::OrdinaryPropertyNumericUpdate(value) => {
                self.expression(value.base_and_receiver(), depth)
                    && self.key(value.referenced_name(), depth)
            }
            ExprIr::ArrayLiteral(values) => {
                values.iter().all(|value| self.expression(value, depth))
            }
            ExprIr::SpecOperation { operands, .. } => {
                operands.iter().all(|value| self.expression(value, depth))
            }
            ExprIr::AssertSameValue {
                actual, expected, ..
            } => self.expression(actual, depth) && self.expression(expected, depth),
            ExprIr::StringFromCharCode { code } => self.expression(code, depth),
            ExprIr::Symbol { description } => description
                .as_ref()
                .is_none_or(|value| self.expression(value, depth)),
            _ => false,
        }
    }
}
