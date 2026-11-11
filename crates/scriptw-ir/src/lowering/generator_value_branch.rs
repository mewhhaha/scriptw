use super::*;
use crate::generator_loop_control::{GeneratorLoopRegionIr, GeneratorLoopSourceRange};

mod optional_chain;

/// An actual GetValue saved before either arm is entered. Its original value,
/// rather than the source Reference, supplies both selection and skipped result.
struct RetainedGeneratorValue {
    name: String,
    info: ValueInfo,
}

impl RetainedGeneratorValue {
    fn new(
        lowerer: &mut ScriptLowerer<'_>,
        prefix: &mut Vec<StatementIr>,
        value: TypedExpr,
    ) -> Self {
        let info = value.value_info();
        let name = lowerer.alloc_suspension_owned_binding("generator.branch.saved.", info.clone());
        prefix.push(StatementIr::Lexical {
            mode: BindingMode::Let,
            name: name.clone(),
            init: value,
        });
        Self { name, info }
    }

    fn read(&self) -> TypedExpr {
        TypedExpr::from_info(self.info.clone(), ExprIr::Identifier(self.name.clone()))
    }

    fn into_value(self) -> TypedExpr {
        TypedExpr::from_info(self.info, ExprIr::Identifier(self.name))
    }
}

/// The declaration, both Normal-only publications and the final read share
/// this consumed owner. No arm can substitute a caller-provided result cell.
struct GeneratorValueResult {
    name: String,
}

impl GeneratorValueResult {
    fn new(lowerer: &mut ScriptLowerer<'_>, prefix: &mut Vec<StatementIr>) -> Self {
        let name = lowerer.alloc_suspension_owned_binding(
            "generator.branch.result.",
            unknown_runtime_value_info(),
        );
        prefix.push(StatementIr::Lexical {
            mode: BindingMode::Let,
            name: name.clone(),
            init: TypedExpr::undefined(),
        });
        Self { name }
    }
}

pub(super) fn nullish_condition(saved: TypedExpr) -> TypedExpr {
    let equals = |rhs| {
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::StrictEquality {
                op: EqualityBinaryOp::StrictEqual,
                lhs: Box::new(saved.clone()),
                rhs: Box::new(rhs),
            },
        )
    };
    TypedExpr::from_info(
        ValueInfo::new(ValueKind::Boolean),
        ExprIr::LogicalShortCircuit {
            op: LogicalBinaryOp::Or,
            lhs: Box::new(equals(TypedExpr::from_info(
                ValueInfo::new(ValueKind::Null),
                ExprIr::Null,
            ))),
            rhs: Box::new(equals(TypedExpr::undefined())),
        },
    )
}

enum CompleteGeneratorValueArmSource<'ast> {
    Expression(&'ast Expression),
    Retained(RetainedGeneratorValue),
}

/// A complete arm is checked against its own source range before publishing a
/// result. Its nested branches and every plain/delegated Yield remain inside
/// the existing ordinary-generator region owner.
struct CompleteGeneratorValueArm {
    region: GeneratorLoopRegionIr,
    info: ValueInfo,
}

impl CompleteGeneratorValueArm {
    fn lower(
        lowerer: &mut ScriptLowerer<'_>,
        source: CompleteGeneratorValueArmSource<'_>,
        range: GeneratorLoopSourceRange,
        result: &GeneratorValueResult,
    ) -> Option<Self> {
        lowerer.push_scope();
        lowerer.current_generator_resume_state = Some(range.entry);
        let completed = (|| {
            let (mut statements, value) = match source {
                CompleteGeneratorValueArmSource::Expression(source) => {
                    lowerer.lower_staged_generator_expression(source)?
                }
                CompleteGeneratorValueArmSource::Retained(value) => {
                    (Vec::new(), value.into_value())
                }
            };
            if lowerer.plain_generator_entry_state()? != range.end {
                return None;
            }
            let info = value.value_info();
            statements.push(StatementIr::Expression(
                lowerer.lower_identifier_assign_value(result.name.clone(), value),
            ));
            let region = GeneratorLoopRegionIr::new(
                BlockIr {
                    statements,
                    result_kind: ValueKind::Undefined,
                    lexical_environment: None,
                },
                range,
            )
            .ok()?;
            Some(Self { region, info })
        })();
        lowerer.pop_scope();
        completed
    }
}

impl ScriptLowerer<'_> {
    pub(super) fn generator_value_branch_admission(&self) -> GeneratorValueBranchAdmission {
        if self.plain_generator_entry_state().is_some()
            && (self.loop_depth == 0 || self.ordinary_generator_region_depth > 0)
        {
            GeneratorValueBranchAdmission::OrdinaryOutsideLoops
        } else {
            GeneratorValueBranchAdmission::LinearOnly
        }
    }

    pub(super) fn lower_generator_value_branch(
        &mut self,
        source: GeneratorValueBranchSource<'_>,
    ) -> Option<(Vec<StatementIr>, TypedExpr)> {
        if !matches!(
            self.generator_value_branch_admission(),
            GeneratorValueBranchAdmission::OrdinaryOutsideLoops
        ) {
            return None;
        }
        // The complete selector prefix runs before allocating arm states. In
        // particular a resumed left operand cannot re-enter its source read.
        let (mut prefix, selector_value) =
            self.lower_staged_generator_expression(source.selector_source())?;
        let states = source.states(self.plain_generator_entry_state()?)?;
        let selector = RetainedGeneratorValue::new(self, &mut prefix, selector_value);
        let (condition, then_source, else_source) = match source.into_kind() {
            GeneratorValueBranchKind::Conditional {
                selector: _,
                then_arm,
                else_arm,
            } => (
                selector.into_value(),
                CompleteGeneratorValueArmSource::Expression(then_arm.source()),
                CompleteGeneratorValueArmSource::Expression(else_arm.source()),
            ),
            GeneratorValueBranchKind::Logical { op, left: _, rhs } => {
                let condition = match op {
                    LogicalOp::And | LogicalOp::Or => selector.read(),
                    LogicalOp::Coalesce => nullish_condition(selector.read()),
                };
                let selected = CompleteGeneratorValueArmSource::Expression(rhs.source());
                let skipped = CompleteGeneratorValueArmSource::Retained(selector);
                let (then_source, else_source) = match op {
                    LogicalOp::And | LogicalOp::Coalesce => (selected, skipped),
                    LogicalOp::Or => (skipped, selected),
                };
                (condition, then_source, else_source)
            }
        };
        let result = GeneratorValueResult::new(self, &mut prefix);
        let before = self.capture_conditional_flow_facts();
        let then_arm =
            CompleteGeneratorValueArm::lower(self, then_source, states.then_arm(), &result)?;
        let then_facts = self.capture_conditional_flow_facts();
        self.install_conditional_flow_facts(before);
        let else_arm =
            CompleteGeneratorValueArm::lower(self, else_source, states.else_arm(), &result)?;
        let else_facts = self.capture_conditional_flow_facts();
        self.merge_conditional_flow_facts(then_facts, else_facts);
        let mut info = self.merge_value_infos(then_arm.info, else_arm.info);
        info.heap_shape = None;
        let branch = OrdinaryGeneratorIfIr::new(
            condition,
            states.entry(),
            then_arm.region,
            else_arm.region,
            states.exit(),
        )
        .ok()?;
        prefix.push(StatementIr::OrdinaryGeneratorIf(Box::new(branch)));
        self.current_generator_resume_state = Some(states.exit());
        self.set_binding_value_info(&result.name, info.clone());
        let value = TypedExpr::from_info(info, ExprIr::Identifier(result.name));
        Some((prefix, value))
    }
}
