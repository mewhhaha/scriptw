use super::*;
use crate::resumable_for_of_control::resumable_sync_for_of_body_has_local_control_owners;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GeneratorSuspensionRegion {
    FunctionBody,
    IteratorBody,
}

pub(crate) fn generator_for_of_source_shape_is_supported(for_of: &ForOfLoop) -> bool {
    let simple_binding = match for_of.initializer() {
        IterableLoopInitializer::Var(variable) => {
            matches!(variable.binding(), Binding::Identifier(_))
        }
        IterableLoopInitializer::Let(_)
        | IterableLoopInitializer::Const(_)
        | IterableLoopInitializer::Identifier(_)
        | IterableLoopInitializer::Access(PropertyAccess::Simple(_)) => true,
        IterableLoopInitializer::Access(PropertyAccess::Private(_) | PropertyAccess::Super(_))
        | IterableLoopInitializer::Pattern(_)
        | IterableLoopInitializer::Using(_)
        | IterableLoopInitializer::AwaitUsing(_)
        | IterableLoopInitializer::WebCompatCall(_) => false,
    };
    simple_binding
        && !for_of.r#await()
        && !contains(for_of.initializer(), ContainsSymbol::YieldExpression)
        && !contains(for_of.initializer(), ContainsSymbol::AwaitExpression)
        && !contains(for_of.iterable(), ContainsSymbol::YieldExpression)
        && resumable_sync_for_of_body_has_local_control_owners(for_of.body())
}

pub(crate) fn append_generator_for_of_suspensions(
    for_of: &ForOfLoop,
    current_state: &mut u32,
    suspension_points: &mut Vec<GeneratorSuspensionPointIr>,
) -> Option<()> {
    if crate::async_generator_source::AsyncGeneratorForOfSource::for_execution(
        for_of,
        ResumableRegionProtocolIr::Generator,
    )
    .is_none()
    {
        return (!contains(for_of, ContainsSymbol::YieldExpression) && !for_of.r#await())
            .then_some(());
    }
    let mut points = Vec::new();
    crate::async_generator_source::append_complete_for_of_source(
        for_of,
        ResumableRegionProtocolIr::Generator,
        current_state,
        &mut points,
    )?;
    suspension_points.extend(points.into_iter().map(|point| GeneratorSuspensionPointIr {
        suspend_state: point.suspend_state,
        resume_state: point.resume_state,
    }));
    Some(())
}

pub(crate) fn append_generator_region_suspensions(
    statements: &[StatementListItem],
    current_state: &mut u32,
    suspension_points: &mut Vec<GeneratorSuspensionPointIr>,
    region: GeneratorSuspensionRegion,
) -> Option<()> {
    if matches!(region, GeneratorSuspensionRegion::FunctionBody)
        && crate::async_generator_source::AsyncGeneratorResourceScopeSource::for_protocol(
            statements,
            ResumableRegionProtocolIr::Generator,
        )
        .is_some()
    {
        return crate::generator_loop_source::append_classic_generator_items(
            statements,
            current_state,
            suspension_points,
        );
    }
    let admission = match region {
        GeneratorSuspensionRegion::FunctionBody => {
            GeneratorValueBranchAdmission::OrdinaryOutsideLoops
        }
        GeneratorSuspensionRegion::IteratorBody => GeneratorValueBranchAdmission::LinearOnly,
    };
    for item in statements {
        let StatementListItem::Statement(statement) = item else {
            if contains(item, ContainsSymbol::YieldExpression) {
                GeneratorExpressionSourcePlan::declaration(item, admission)?
                    .append(current_state, suspension_points)?;
            }
            continue;
        };
        let yield_expression = match statement.as_ref() {
            Statement::Expression(Expression::Yield(expression)) => Some((expression, true)),
            Statement::Expression(Expression::Assign(assignment))
                if assignment.op() == AssignOp::Assign
                    && matches!(
                        assignment.lhs(),
                        AssignTarget::Identifier(_) | AssignTarget::Access(_)
                    )
                    && !contains(assignment.lhs(), ContainsSymbol::YieldExpression) =>
            {
                match assignment.rhs() {
                    Expression::Yield(expression) => Some((expression, false)),
                    _ => None,
                }
            }
            Statement::Return(statement) => match statement.target() {
                Some(Expression::Yield(expression)) => Some((expression, true)),
                _ => None,
            },
            _ => None,
        };
        if let Some((yield_expression, nested_yield_allowed)) = yield_expression {
            if !nested_yield_allowed
                && yield_expression
                    .target()
                    .is_some_and(|target| contains(target, ContainsSymbol::YieldExpression))
            {
                return None;
            }
            let expression = Expression::Yield(yield_expression.clone());
            GeneratorExpressionSourcePlan::new(&expression, admission)?
                .append(current_state, suspension_points)?;
            continue;
        }
        if let Statement::Return(statement) = statement.as_ref() {
            if let Some(target) = statement
                .target()
                .filter(|target| contains(*target, ContainsSymbol::YieldExpression))
            {
                GeneratorExpressionSourcePlan::new(target, admission)?
                    .append(current_state, suspension_points)?;
                continue;
            }
        }
        if let Statement::Throw(source) = statement.as_ref() {
            if contains(source.target(), ContainsSymbol::YieldExpression) {
                GeneratorExpressionSourcePlan::new(source.target(), admission)?
                    .append(current_state, suspension_points)?;
            }
            continue;
        }
        if let Statement::Var(source) = statement.as_ref() {
            if contains(source, ContainsSymbol::YieldExpression) {
                if !matches!(region, GeneratorSuspensionRegion::FunctionBody) {
                    return None;
                }
                GeneratorExpressionSourcePlan::var_declaration(source)?
                    .append(current_state, suspension_points)?;
            }
            continue;
        }
        if let Statement::Expression(expression) = statement.as_ref() {
            if contains(expression, ContainsSymbol::YieldExpression) {
                append_discarded_generator_expression_suspensions(
                    expression,
                    current_state,
                    suspension_points,
                    admission,
                )?;
                continue;
            }
        }
        if let Statement::Block(block) = statement.as_ref() {
            let inspect_block = match region {
                GeneratorSuspensionRegion::FunctionBody => {
                    contains(block, ContainsSymbol::YieldExpression)
                        || contains_ordinary_generator_phase_owner(block)
                }
                GeneratorSuspensionRegion::IteratorBody => true,
            };
            if inspect_block {
                append_generator_region_suspensions(
                    block.statement_list().statements(),
                    current_state,
                    suspension_points,
                    region,
                )?;
                continue;
            }
        }
        if matches!(region, GeneratorSuspensionRegion::FunctionBody) {
            if let Statement::With(source) = statement.as_ref() {
                GeneratorWithSource::new(source)?.append(current_state, suspension_points)?;
                continue;
            }
            if let Statement::ForInLoop(source) = statement.as_ref() {
                append_generator_for_in_suspensions(source, current_state, suspension_points)?;
                continue;
            }
            if let Statement::If(source) = statement.as_ref() {
                if crate::lowering_helpers::ordinary_generator_if_requires_complete_owner(source) {
                    crate::generator_loop_source::append_classic_generator_statement(
                        statement,
                        current_state,
                        suspension_points,
                    )?;
                    continue;
                }
            }
            if let Statement::Switch(source) = statement.as_ref() {
                if contains(source, ContainsSymbol::YieldExpression)
                    || contains_ordinary_generator_phase_owner(source)
                {
                    append_generator_switch_suspensions(source, current_state, suspension_points)?;
                    continue;
                }
            }
            let classic = match statement.as_ref() {
                Statement::ForLoop(source) => {
                    Some(crate::generator_loop_source::ClassicGeneratorLoopSource::For(source))
                }
                Statement::WhileLoop(source) => {
                    Some(crate::generator_loop_source::ClassicGeneratorLoopSource::While(source))
                }
                Statement::DoWhileLoop(source) => {
                    Some(crate::generator_loop_source::ClassicGeneratorLoopSource::DoWhile(source))
                }
                _ => None,
            };
            if let Some(source) = classic {
                source.append(current_state, suspension_points)?;
                continue;
            }
            if let Statement::Labelled(source) = statement.as_ref() {
                if matches!(
                    source.item(),
                    LabelledItem::Statement(
                        Statement::ForLoop(_)
                            | Statement::WhileLoop(_)
                            | Statement::DoWhileLoop(_)
                            | Statement::Switch(_)
                            | Statement::Labelled(_)
                            | Statement::With(_)
                    )
                ) || contains_ordinary_generator_phase_owner(source)
                {
                    crate::generator_loop_source::append_classic_generator_statement(
                        statement,
                        current_state,
                        suspension_points,
                    )?;
                    continue;
                }
            }
        }
        if let Statement::ForOfLoop(for_of) = statement.as_ref() {
            match region {
                GeneratorSuspensionRegion::FunctionBody => {
                    append_generator_for_of_suspensions(for_of, current_state, suspension_points)?
                }
                GeneratorSuspensionRegion::IteratorBody
                    if contains(&**for_of, ContainsSymbol::YieldExpression) =>
                {
                    return None
                }
                GeneratorSuspensionRegion::IteratorBody => {}
            }
            continue;
        }
        if let Statement::Try(try_statement) = statement.as_ref() {
            append_generator_region_suspensions(
                try_statement.block().statement_list().statements(),
                current_state,
                suspension_points,
                region,
            )?;
            *current_state = current_state.checked_add(1)?;
            if let Some(catch) = try_statement.catch() {
                crate::generator_loop_source::append_generator_catch_parameter(
                    catch.parameter(),
                    current_state,
                    suspension_points,
                )?;
                append_generator_region_suspensions(
                    catch.block().statement_list().statements(),
                    current_state,
                    suspension_points,
                    region,
                )?;
                *current_state = current_state.checked_add(1)?;
            }
            if let Some(finally) = try_statement.finally() {
                append_generator_region_suspensions(
                    finally.block().statement_list().statements(),
                    current_state,
                    suspension_points,
                    region,
                )?;
                *current_state = current_state.checked_add(1)?;
            }
            continue;
        }
        if contains(statement.as_ref(), ContainsSymbol::YieldExpression) {
            return None;
        }
    }
    Some(())
}
