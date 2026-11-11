//! Actual statement phase reservations extend the shared async expression tape.

use super::*;
use crate::async_pattern_source::{append_async_expression_states, AsyncPatternSource};

pub(crate) fn variables(
    source: &[Variable],
    cursor: &mut u32,
    awaits: &mut Vec<(u32, u32)>,
) -> Option<()> {
    let mut points = Vec::new();
    variables_protocol(source, cursor, &mut points)?;
    awaits.extend(
        points
            .into_iter()
            .map(|point| (point.suspend_state, point.resume_state)),
    );
    Some(())
}

pub(crate) fn expression_states(
    source: &Expression,
    cursor: &mut u32,
    points: &mut Vec<ResumableSuspensionPointIr>,
) -> Option<()> {
    let mut awaits = Vec::new();
    append_async_expression_states(source, cursor, &mut awaits)?;
    points.extend(awaits.into_iter().map(|(suspend_state, resume_state)| {
        ResumableSuspensionPointIr {
            kind: ResumableSuspensionKindIr::Await,
            suspend_state,
            resume_state,
            resume_environment: ResumableResumeEnvironmentIr::InvocationOuter,
        }
    }));
    Some(())
}
fn pattern_states(
    source: &Pattern,
    cursor: &mut u32,
    points: &mut Vec<ResumableSuspensionPointIr>,
) -> Option<()> {
    let mut awaits = Vec::new();
    AsyncPatternSource::new(source)?.append(cursor, &mut awaits)?;
    points.extend(awaits.into_iter().map(|(suspend_state, resume_state)| {
        ResumableSuspensionPointIr {
            kind: ResumableSuspensionKindIr::Await,
            suspend_state,
            resume_state,
            resume_environment: ResumableResumeEnvironmentIr::InvocationOuter,
        }
    }));
    Some(())
}

pub(crate) fn append_protocol(
    source: &Statement,
    cursor: &mut u32,
    awaits: &mut Vec<ResumableSuspensionPointIr>,
    foreign_loop: bool,
) -> Option<()> {
    match source {
        Statement::Expression(source) => expression_states(source, cursor, awaits),
        Statement::Var(source) => variables_protocol(source.0.as_ref(), cursor, awaits),
        Statement::Return(source) => optional_expression(source.target(), cursor, awaits),
        Statement::Throw(source) => expression_states(source.target(), cursor, awaits),
        Statement::Block(source) => items(
            source.statement_list().statements(),
            cursor,
            awaits,
            foreign_loop,
        ),
        Statement::With(source) if !foreign_loop => {
            AsyncWithSource::new(source)?.append_protocol(cursor, awaits)
        }
        Statement::With(source) => {
            // The analyzed foreign-loop domain retains the original eager or
            // linear With compiler. It receives no complete With phase rows.
            if contains(source, ContainsSymbol::AwaitExpression)
                || contains(source, ContainsSymbol::YieldExpression)
            {
                return None;
            }
            let entry = *cursor;
            expression_states(source.expression(), cursor, awaits)?;
            append_protocol(source.statement(), cursor, awaits, true)?;
            (*cursor == entry).then_some(())
        }
        Statement::If(source) => {
            expression_states(source.cond(), cursor, awaits)?;
            let entry = *cursor;
            let point_start = awaits.len();
            let then_entry = entry.checked_add(1)?;
            let mut then_end = then_entry;
            append_protocol(source.body(), &mut then_end, awaits, foreign_loop)?;
            let else_entry = then_end.checked_add(1)?;
            let mut else_end = else_entry;
            if let Some(source) = source.else_node() {
                append_protocol(source, &mut else_end, awaits, foreign_loop)?;
            }
            if then_end == then_entry && else_end == else_entry {
                // AsyncFunctionIfPlanIr::new returns None for two eager arms.
                if awaits.len() != point_start {
                    return None;
                }
                *cursor = entry;
            } else {
                *cursor = else_end.checked_add(1)?;
            }
            Some(())
        }
        Statement::Try(source) => {
            items(
                source.block().statement_list().statements(),
                cursor,
                awaits,
                foreign_loop,
            )?;
            next(cursor)?;
            if let Some(source) = source.catch() {
                if let Some(Binding::Pattern(pattern)) = source.parameter() {
                    if contains(pattern, ContainsSymbol::YieldExpression) {
                        return None;
                    }
                    if contains(pattern, ContainsSymbol::AwaitExpression) {
                        pattern_states(pattern, cursor, awaits)?;
                    }
                }
                items(
                    source.block().statement_list().statements(),
                    cursor,
                    awaits,
                    foreign_loop,
                )?;
                next(cursor)?;
            }
            if let Some(source) = source.finally() {
                items(
                    source.block().statement_list().statements(),
                    cursor,
                    awaits,
                    foreign_loop,
                )?;
                next(cursor)?;
            }
            Some(())
        }
        Statement::Switch(source) => {
            if crate::async_generator_source::switch_has_async_operands(source)
                || (!foreign_loop
                    && crate::async_generator_source::switch_has_direct_resources(source))
            {
                return crate::async_generator_source::append_resource_switch_for_protocol(
                    source,
                    ResumableRegionProtocolIr::Async,
                    cursor,
                    awaits,
                );
            }
            expression_states(source.val(), cursor, awaits)?;
            let entry = *cursor;
            let selectors_suspend = source
                .cases()
                .iter()
                .filter_map(|case| case.condition())
                .any(|condition| contains(condition, ContainsSymbol::AwaitExpression));
            let mut candidate = entry.checked_add(1)?;
            // Actual lowering evaluates every selector prefix before lowering
            // any case body. Default/no-match owns one separate fallback state.
            if selectors_suspend {
                for condition in source.cases().iter().filter_map(|case| case.condition()) {
                    expression_states(condition, &mut candidate, awaits)?;
                    next(&mut candidate)?;
                }
                next(&mut candidate)?;
            }
            let mut owns_case = selectors_suspend;
            for case in source.cases() {
                let start = candidate;
                items(
                    case.body().statements(),
                    &mut candidate,
                    awaits,
                    foreign_loop,
                )?;
                owns_case |= candidate != start;
                next(&mut candidate)?;
            }
            *cursor = if owns_case { candidate } else { entry };
            Some(())
        }
        Statement::Labelled(source) => {
            let mut source = source.item();
            loop {
                match source {
                    LabelledItem::FunctionDeclaration(_) => return Some(()),
                    LabelledItem::Statement(Statement::Labelled(inner)) => source = inner.item(),
                    LabelledItem::Statement(statement) => {
                        let entry = *cursor;
                        append_protocol(statement, cursor, awaits, foreign_loop)?;
                        if *cursor != entry
                            && !matches!(
                                statement,
                                Statement::WhileLoop(_)
                                    | Statement::DoWhileLoop(_)
                                    | Statement::ForLoop(_)
                                    | Statement::ForInLoop(_)
                                    | Statement::ForOfLoop(_)
                            )
                        {
                            next(cursor)?;
                        }
                        return Some(());
                    }
                }
            }
        }
        Statement::WhileLoop(source) => {
            if contains(source.condition(), ContainsSymbol::YieldExpression) {
                return None;
            }
            if plain_async_while_uses_eager_body(source) {
                // The existing checked while-condition compiler deliberately
                // lowers its suspension-free body without async clause states.
                expression_states(source.condition(), cursor, awaits)?;
                next(cursor)
            } else {
                crate::async_generator_source::PlainAsyncClassicLoopSource::While(source)
                    .append_typed(cursor, awaits)
            }
        }
        Statement::ForLoop(source) => {
            crate::async_generator_source::PlainAsyncClassicLoopSource::For(source)
                .append_typed(cursor, awaits)
        }
        Statement::DoWhileLoop(source) => {
            crate::async_generator_source::PlainAsyncClassicLoopSource::DoWhile(source)
                .append_typed(cursor, awaits)
        }
        Statement::ForInLoop(source)
            if !foreign_loop
                && !matches!(
                    source.initializer(),
                    IterableLoopInitializer::WebCompatCall(_)
                ) =>
        {
            crate::async_generator_source::AsyncGeneratorForInSource::for_execution(
                source,
                ResumableRegionProtocolIr::Async,
            )?;
            crate::async_generator_source::append_complete_for_in_source(
                source,
                ResumableRegionProtocolIr::Async,
                cursor,
                awaits,
            )
        }
        Statement::ForInLoop(source) => {
            if contains(source.target(), ContainsSymbol::AwaitExpression)
                || contains(source.target(), ContainsSymbol::YieldExpression)
                || contains(source.initializer(), ContainsSymbol::AwaitExpression)
                || contains(source.initializer(), ContainsSymbol::YieldExpression)
            {
                return None;
            }
            phase_free_loop(source.body(), cursor, awaits)
        }
        Statement::ForOfLoop(source) if !foreign_loop => {
            crate::async_generator_source::AsyncGeneratorForOfSource::for_execution(
                source,
                ResumableRegionProtocolIr::Async,
            )?;
            crate::async_generator_source::append_complete_for_of_source(
                source,
                ResumableRegionProtocolIr::Async,
                cursor,
                awaits,
            )
        }
        Statement::ForOfLoop(source) => {
            if source.r#await()
                || matches!(source.initializer(), IterableLoopInitializer::AwaitUsing(_))
                || contains(source.iterable(), ContainsSymbol::AwaitExpression)
                || contains(source.initializer(), ContainsSymbol::AwaitExpression)
                || contains(source, ContainsSymbol::YieldExpression)
            {
                return None;
            }
            phase_free_loop(source.body(), cursor, awaits)
        }
        Statement::Empty | Statement::Debugger | Statement::Break(_) | Statement::Continue(_) => {
            Some(())
        }
    }
}

fn next(cursor: &mut u32) -> Option<()> {
    *cursor = cursor.checked_add(1)?;
    Some(())
}

fn optional_expression(
    source: Option<&Expression>,
    cursor: &mut u32,
    awaits: &mut Vec<ResumableSuspensionPointIr>,
) -> Option<()> {
    if let Some(source) = source {
        expression_states(source, cursor, awaits)?;
    }
    Some(())
}

pub(crate) fn variables_protocol(
    source: &[Variable],
    cursor: &mut u32,
    awaits: &mut Vec<ResumableSuspensionPointIr>,
) -> Option<()> {
    for variable in source {
        optional_expression(variable.init(), cursor, awaits)?;
        if contains(variable.binding(), ContainsSymbol::YieldExpression) {
            return None;
        }
        if let Binding::Pattern(pattern) = variable.binding() {
            if contains(pattern, ContainsSymbol::AwaitExpression) {
                pattern_states(pattern, cursor, awaits)?;
            }
        }
    }
    Some(())
}

pub(crate) fn items(
    source: &[StatementListItem],
    cursor: &mut u32,
    awaits: &mut Vec<ResumableSuspensionPointIr>,
    foreign_loop: bool,
) -> Option<()> {
    if !foreign_loop
        && crate::async_generator_source::AsyncGeneratorResourceScopeSource::for_protocol(
            source,
            ResumableRegionProtocolIr::Async,
        )
        .is_some()
    {
        return crate::async_generator_source::append_resource_scope_for_protocol(
            source,
            ResumableRegionProtocolIr::Async,
            cursor,
            awaits,
        );
    }
    for source in source {
        match source {
            StatementListItem::Statement(source) => {
                append_protocol(source, cursor, awaits, foreign_loop)?
            }
            StatementListItem::Declaration(source) => match source.as_ref() {
                Declaration::Lexical(source) => match source {
                    LexicalDeclaration::Let(list)
                    | LexicalDeclaration::Const(list)
                    | LexicalDeclaration::Using(list) => {
                        variables_protocol(list.as_ref(), cursor, awaits)?
                    }
                    // Implicit awaited disposal needs its own source phase owner.
                    LexicalDeclaration::AwaitUsing(_) => return None,
                },
                Declaration::ClassDeclaration(source) => {
                    for operand in
                        class_evaluation_expressions(source.super_ref(), source.elements())
                    {
                        expression_states(operand, cursor, awaits)?;
                    }
                }
                Declaration::FunctionDeclaration(_)
                | Declaration::GeneratorDeclaration(_)
                | Declaration::AsyncFunctionDeclaration(_)
                | Declaration::AsyncGeneratorDeclaration(_) => {}
            },
        }
    }
    Some(())
}

fn phase_free_loop(
    source: &Statement,
    cursor: &mut u32,
    awaits: &mut Vec<ResumableSuspensionPointIr>,
) -> Option<()> {
    let entry = *cursor;
    let count = awaits.len();
    append_protocol(source, cursor, awaits, true)?;
    (*cursor == entry && awaits.len() == count).then_some(())
}
