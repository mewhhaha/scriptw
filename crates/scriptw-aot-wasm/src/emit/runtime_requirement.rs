use crate::data::StringPool;
use scriptw_ir::{
    ArithmeticBinaryOp, BlockIr, ExprIr, ScriptIr, StatementIr, TypedExpr, ValueKind,
};
use std::collections::BTreeSet;

/// Elide Realm bootstrap only when the lowered script cannot observe it.
/// This is a conservative IR proof: every unrecognized operation retains the
/// ordinary runtime, including declarations, observable coercions, calls and
/// abrupt exits.
/// Admitted expressions still go through the same Wasm expression emitter.
#[derive(Debug)]
pub(crate) struct StandaloneAdmission<'script> {
    script: &'script ScriptIr,
    pool: StringPool,
}

impl<'script> StandaloneAdmission<'script> {
    pub(crate) fn new(script: &'script ScriptIr) -> Option<Self> {
        if script.eval_environment.is_some()
            || !script.foreign_modules.is_empty()
            || script.module_prelude.is_some()
            || !script.prepared_scripts.is_empty()
            || script.runtime_declarations != Default::default()
            || !script.prepared_dynamic_functions.is_empty()
            || !script.functions.is_empty()
            || !script.owned_env_bindings.is_empty()
            || !script.global_bindings.lexical_bindings().is_empty()
            || !script.host_builtins.is_empty()
            || script.top_level_this_uses != 0
        {
            return None;
        }
        let mut literals = BTreeSet::new();
        if !block_is_runtime_free(&script.body, &mut literals) {
            return None;
        }
        Some(Self {
            script,
            pool: StringPool::from_standalone_literals(literals),
        })
    }

    pub(crate) fn script(&self) -> &'script ScriptIr {
        self.script
    }

    pub(crate) fn pool(&self) -> &StringPool {
        &self.pool
    }
}

fn block_is_runtime_free<'s>(block: &'s BlockIr, literals: &mut BTreeSet<&'s str>) -> bool {
    block.lexical_environment.is_none()
        && block
            .statements
            .iter()
            .all(|statement| statement_is_runtime_free(statement, literals))
}

fn statement_is_runtime_free<'s>(
    statement: &'s StatementIr,
    literals: &mut BTreeSet<&'s str>,
) -> bool {
    match statement {
        StatementIr::Empty => true,
        StatementIr::Expression(expression) => expression_is_runtime_free(expression, literals),
        StatementIr::Block(block) => block_is_runtime_free(block, literals),
        StatementIr::LexicalBlock(statements) => statements
            .iter()
            .all(|statement| statement_is_runtime_free(statement, literals)),
        _ => false,
    }
}

fn expression_is_runtime_free<'s>(
    expression: &'s TypedExpr,
    literals: &mut BTreeSet<&'s str>,
) -> bool {
    match &expression.expr {
        ExprIr::Undefined | ExprIr::Null | ExprIr::Boolean(_) | ExprIr::Number(_) => true,
        ExprIr::String(value) => {
            literals.insert(value.as_str());
            true
        }
        ExprIr::Void { expr } | ExprIr::DeleteValue { expr } | ExprIr::LogicalNot { expr } => {
            expression_is_runtime_free(expr, literals)
        }
        ExprIr::UnaryPlus { expr }
        | ExprIr::UnaryMinusNumeric { expr }
        | ExprIr::UnaryBitwiseNumeric { expr, .. } => number_is_runtime_free(expr, literals),
        ExprIr::BinaryNumber { op, lhs, rhs } | ExprIr::CoerciveBinaryNumber { op, lhs, rhs } => {
            // Exponentiation calls a separately planned host function. All
            // remaining Number operations emit scalar Wasm instructions.
            let scalar_operation = match op {
                ArithmeticBinaryOp::Add
                | ArithmeticBinaryOp::Sub
                | ArithmeticBinaryOp::Mul
                | ArithmeticBinaryOp::Div
                | ArithmeticBinaryOp::Mod => true,
                ArithmeticBinaryOp::Exp => false,
            };
            scalar_operation
                && expression.kind == ValueKind::Number
                && number_is_runtime_free(lhs, literals)
                && number_is_runtime_free(rhs, literals)
        }
        ExprIr::CompareNumber { lhs, rhs, .. } => {
            number_is_runtime_free(lhs, literals) && number_is_runtime_free(rhs, literals)
        }
        ExprIr::Comma { lhs, rhs } | ExprIr::LogicalShortCircuit { lhs, rhs, .. } => {
            expression_is_runtime_free(lhs, literals) && expression_is_runtime_free(rhs, literals)
        }
        ExprIr::Conditional {
            condition,
            then_expr,
            else_expr,
        } => {
            expression_is_runtime_free(condition, literals)
                && expression_is_runtime_free(then_expr, literals)
                && expression_is_runtime_free(else_expr, literals)
        }
        _ => false,
    }
}

fn number_is_runtime_free<'s>(expression: &'s TypedExpr, literals: &mut BTreeSet<&'s str>) -> bool {
    expression.kind == ValueKind::Number && expression_is_runtime_free(expression, literals)
}
