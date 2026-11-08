//! Bounded source candidates never assert a live callable's identity.
use super::*;

#[derive(Debug)]
pub(crate) struct CapturedCallSource {
    pub(super) function: FunctionIr,
    pub(super) forwarding: bool,
}

impl CapturedCallSource {
    pub(crate) fn new(function: &FunctionIr) -> Option<Self> {
        crate::function_entry::LocalArgumentsPlan::for_function(function)?;
        if function.params.is_empty()
            || function.params.len() > 2
            || function.eval_environment.is_some()
            || !function.owned_env_bindings.is_empty()
            || function.body.lexical_environment.is_some()
            || function.is_named_expression
            || function.captures_private_environment
            || function.captures_lexical_this
            || function.uses_super
            || !function.private_name_ids.is_empty()
        {
            return None;
        }
        let (last, body) = function.body.statements.split_last()?;
        let StatementIr::Return(result) = last else {
            return None;
        };
        let mut budget = 96;
        for statement in body {
            statement_budget(statement, &mut budget)?;
        }
        expression_budget(result, &mut budget)?;
        let forwarding = match &result.expr {
            ExprIr::CallIndirect { .. } => true,
            ExprIr::MaterializeBinding { body, .. } => {
                matches!(body.expr, ExprIr::CallIndirect { .. })
            }
            _ => false,
        };
        if forwarding
            && body.iter().any(|statement| {
                !matches!(
                    statement,
                    StatementIr::Empty
                        | StatementIr::Expression(TypedExpr {
                            expr: ExprIr::String(_),
                            ..
                        })
                )
            })
        {
            return None;
        }
        Some(Self {
            function: function.clone(),
            forwarding,
        })
    }
}

fn statement_budget(statement: &StatementIr, budget: &mut usize) -> Option<()> {
    *budget = budget.checked_sub(1)?;
    match statement {
        StatementIr::Empty => {}
        StatementIr::Expression(value) => expression_budget(value, budget)?,
        StatementIr::Var(values) => {
            for value in values {
                expression_budget(value.init.as_ref()?, budget)?;
            }
        }
        StatementIr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expression_budget(condition, budget)?;
            statement_budget(then_branch, budget)?;
            if let Some(branch) = else_branch {
                statement_budget(branch, budget)?;
            }
        }
        StatementIr::Block(block) if block.lexical_environment.is_none() => {
            for statement in &block.statements {
                statement_budget(statement, budget)?;
            }
        }
        _ => return None,
    }
    Some(())
}

fn expression_budget(value: &TypedExpr, budget: &mut usize) -> Option<()> {
    *budget = budget.checked_sub(1)?;
    match &value.expr {
        ExprIr::Number(_) | ExprIr::String(_) | ExprIr::Boolean(_) | ExprIr::Identifier(_) => {}
        ExprIr::EnvironmentIdentifier(reference) => match &reference.operation {
            EnvOp::Read => {}
            EnvOp::Assign { value } => expression_budget(value, budget)?,
            _ => return None,
        },
        ExprIr::AssignIdentifier { value, .. }
        | ExprIr::UnaryPlus { expr: value }
        | ExprIr::UnaryMinusNumeric { expr: value }
        | ExprIr::LogicalNot { expr: value } => expression_budget(value, budget)?,
        ExprIr::CoerciveAdd { lhs, rhs }
        | ExprIr::BinaryNumber { lhs, rhs, .. }
        | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
        | ExprIr::BitwiseNumeric { lhs, rhs, .. }
        | ExprIr::CompareNumber { lhs, rhs, .. }
        | ExprIr::CompareValue { lhs, rhs, .. } => {
            expression_budget(lhs, budget)?;
            expression_budget(rhs, budget)?;
        }
        ExprIr::PropertyRead { target, key } => {
            expression_budget(target, budget)?;
            match key {
                PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => {}
                PropertyKeyIr::ArrayIndex(key) | PropertyKeyIr::StringExpr(key) => {
                    expression_budget(key, budget)?
                }
            }
        }
        ExprIr::SpecOperation {
            operation: SpecOperationIr::GetV,
            operands,
        } => {
            for operand in operands {
                expression_budget(operand, budget)?;
            }
        }
        ExprIr::MaterializeBinding { value, body, .. } => {
            expression_budget(value, budget)?;
            expression_budget(body, budget)?;
        }
        ExprIr::CallIndirect {
            callee,
            args,
            this_arg,
            direct_eval: None,
            ..
        } => {
            expression_budget(callee, budget)?;
            if let Some(receiver) = this_arg {
                expression_budget(receiver, budget)?;
            }
            for argument in args {
                expression_budget(argument, budget)?;
            }
        }
        _ => return None,
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};

    #[test]
    fn unchanged_dynamic_sources_retain_bounded_numeric_and_forwarding_candidates() {
        for (source, count, forwarding) in [
            (
                include_str!("../../../../tests/emission/fixtures/numeric_body_args_unused.mjs"),
                1,
                0,
            ),
            (
                include_str!("../../../../tests/emission/fixtures/numeric_body_frame_compact.mjs"),
                3,
                1,
            ),
        ] {
            let ir = scriptw_ir::lower(&parse(source, ParseOptions::module()).unwrap());
            let plans: Vec<_> = ir
                .script
                .as_ref()
                .unwrap()
                .functions
                .iter()
                .filter_map(CapturedCallSource::new)
                .collect();
            assert_eq!(plans.len(), count, "{source}");
            assert_eq!(
                plans.iter().filter(|plan| plan.forwarding).count(),
                forwarding
            );
        }
    }

    #[test]
    fn source_candidates_refuse_arguments_observers_and_mutable_object_writes() {
        for source in [
            "export function step(row,i){return arguments[0].x+i;}",
            "export function step(row,i){row.x=i;return row.x;}",
            "export function step(row,i){return ()=>row.x+i;}",
            "export function step(row,i){return eval('row.x+i');}",
            "export async function step(row,i){return row.x+i;}",
        ] {
            let ir = scriptw_ir::lower(&parse(source, ParseOptions::module()).unwrap());
            assert!(
                ir.script
                    .as_ref()
                    .unwrap()
                    .functions
                    .iter()
                    .all(|source| CapturedCallSource::new(source).is_none()),
                "{source}"
            );
        }
    }
}
