//! Generator default ranges adapt the shared original pattern targets.

use super::*;
use crate::generator_loop_control::{GeneratorLoopRegionIr, GeneratorLoopSourceRange};

impl ScriptLowerer<'_> {
    pub(super) fn apply_generator_pattern_default(
        &mut self,
        value: &TypedExpr,
        default: &Expression,
        statements: &mut Vec<StatementIr>,
    ) -> Option<()> {
        let ExprIr::Identifier(name) = &value.expr else {
            unreachable!("a retained acquired pattern value names its owned cell")
        };
        let states = GeneratorObjectPatternSource::default_states(
            default,
            self.plain_generator_entry_state()?,
        )?;
        let before_vars = self.var_bindings.clone();
        let before_globals = self.global_properties.clone();
        let then_branch =
            self.lower_generator_pattern_default_region(Some((default, name)), states.then_arm())?;
        let then_vars = self.var_bindings.clone();
        let then_globals = self.global_properties.clone();
        self.var_bindings = before_vars.clone();
        self.global_properties = before_globals.clone();
        let else_branch = self.lower_generator_pattern_default_region(None, states.else_arm())?;
        self.var_bindings = self.merge_var_bindings(&then_vars, &self.var_bindings);
        self.global_properties =
            self.merge_global_properties(&then_globals, &self.global_properties);
        let condition = TypedExpr::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::StrictEquality {
                op: EqualityBinaryOp::StrictEqual,
                lhs: Box::new(value.clone()),
                rhs: Box::new(TypedExpr::undefined()),
            },
        );
        let plan = OrdinaryGeneratorIfIr::new(
            condition,
            states.entry(),
            then_branch,
            else_branch,
            states.exit(),
        )
        .ok()?;
        statements.push(StatementIr::OrdinaryGeneratorIf(Box::new(plan)));
        self.current_generator_resume_state = Some(states.exit());
        Some(())
    }
    fn lower_generator_pattern_default_region(
        &mut self,
        source: Option<(&Expression, &str)>,
        range: GeneratorLoopSourceRange,
    ) -> Option<GeneratorLoopRegionIr> {
        self.push_scope();
        self.current_generator_resume_state = Some(range.entry);
        let result = (|| {
            let statements = if let Some((source, result_name)) = source {
                let (mut prefix, value) = self.lower_staged_generator_expression(source)?;
                prefix.push(StatementIr::Expression(
                    self.lower_identifier_assign_value(result_name.to_owned(), value),
                ));
                prefix
            } else {
                Vec::new()
            };
            if self.plain_generator_entry_state()? != range.end {
                return None;
            }
            GeneratorLoopRegionIr::new(
                BlockIr {
                    statements,
                    result_kind: ValueKind::Undefined,
                    lexical_environment: None,
                },
                range,
            )
            .ok()
        })();
        self.pop_scope();
        result
    }
}
