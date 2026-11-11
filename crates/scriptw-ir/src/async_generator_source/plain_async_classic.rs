//! Plain Async phases consume the same expression and statement source owners.
use super::*;
use crate::async_pattern_source::append_async_expression_states;
use crate::async_with_source::append_async_variables;
use crate::lowering_helpers::append_async_statement_protocol_states;

#[derive(Clone, Copy)]
pub(crate) enum PlainAsyncClassicLoopSource<'ast> {
    For(&'ast ForLoop),
    While(&'ast WhileLoop),
    DoWhile(&'ast DoWhileLoop),
    GeneratorResourceFor(&'ast ForLoop),
}

fn phase(
    cursor: &mut u32,
    awaits: &mut Vec<ResumableSuspensionPointIr>,
    append: impl FnOnce(&mut u32, &mut Vec<ResumableSuspensionPointIr>) -> Option<()>,
) -> Option<AsyncGeneratorSourceRange> {
    let entry = *cursor;
    append(cursor, awaits)?;
    let range = AsyncGeneratorSourceRange {
        entry,
        end: *cursor,
    };
    *cursor = cursor.checked_add(1)?;
    Some(range)
}
fn scalar(
    cursor: &mut u32,
    points: &mut Vec<ResumableSuspensionPointIr>,
    append: impl FnOnce(&mut u32, &mut Vec<(u32, u32)>) -> Option<()>,
) -> Option<()> {
    let mut awaits = Vec::new();
    append(cursor, &mut awaits)?;
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
fn expression(
    source: &Expression,
    cursor: &mut u32,
    points: &mut Vec<ResumableSuspensionPointIr>,
) -> Option<()> {
    scalar(cursor, points, |cursor, awaits| {
        append_async_expression_states(source, cursor, awaits)
    })
}
fn optional(
    source: Option<&Expression>,
    cursor: &mut u32,
    awaits: &mut Vec<ResumableSuspensionPointIr>,
) -> Option<()> {
    if let Some(source) = source {
        expression(source, cursor, awaits)?;
    }
    Some(())
}
fn initializer(
    source: Option<&ForLoopInitializer>,
    cursor: &mut u32,
    awaits: &mut Vec<ResumableSuspensionPointIr>,
) -> Option<()> {
    match source {
        None => Some(()),
        Some(ForLoopInitializer::Expression(source)) => expression(source, cursor, awaits),
        Some(ForLoopInitializer::Var(source)) => scalar(cursor, awaits, |cursor, awaits| {
            append_async_variables(source.0.as_ref(), cursor, awaits)
        }),
        Some(ForLoopInitializer::Lexical(source)) => match source.declaration() {
            LexicalDeclaration::Let(source) | LexicalDeclaration::Const(source) => {
                scalar(cursor, awaits, |cursor, awaits| {
                    append_async_variables(source.as_ref(), cursor, awaits)
                })
            }
            // Their complete whole-loop capability is a separate source owner.
            LexicalDeclaration::Using(_) | LexicalDeclaration::AwaitUsing(_) => None,
        },
    }
}
impl PlainAsyncClassicLoopSource<'_> {
    pub(crate) fn for_generator_resource(
        source: &ForLoop,
    ) -> Option<PlainAsyncClassicLoopSource<'_>> {
        let owner = PlainAsyncClassicLoopSource::GeneratorResourceFor(source);
        owner.plan(0)?;
        Some(owner)
    }
    pub(crate) fn execution(self) -> ResumableRegionProtocolIr {
        match self {
            Self::GeneratorResourceFor(_) => ResumableRegionProtocolIr::Generator,
            Self::For(_) | Self::While(_) | Self::DoWhile(_) => ResumableRegionProtocolIr::Async,
        }
    }
    pub(crate) fn plan(self, entry: u32) -> Option<AsyncGeneratorLoopSourceStates> {
        match self {
            Self::GeneratorResourceFor(source) => {
                return resource_for_plan(source, entry, ResumableRegionProtocolIr::Generator)
            }
            Self::For(source) if resource_head(source).is_some() => {
                return resource_for_plan(source, entry, ResumableRegionProtocolIr::Async)
            }
            Self::For(_) | Self::While(_) | Self::DoWhile(_) => {}
        }
        let mut cursor = entry;
        let mut awaits = Vec::new();
        let (kind, initialization, test, body, update) = match self {
            Self::For(source) => {
                if contains(source, ContainsSymbol::YieldExpression) {
                    return None;
                }
                let initialization = phase(&mut cursor, &mut awaits, |cursor, awaits| {
                    initializer(source.init(), cursor, awaits)
                })?;
                let test = phase(&mut cursor, &mut awaits, |cursor, awaits| {
                    optional(source.condition(), cursor, awaits)
                })?;
                let body = phase(&mut cursor, &mut awaits, |cursor, awaits| {
                    append_async_statement_protocol_states(source.body(), cursor, awaits, false)
                })?;
                let update = phase(&mut cursor, &mut awaits, |cursor, awaits| {
                    optional(source.final_expr(), cursor, awaits)
                })?;
                (
                    GeneratorLoopKindIr::For,
                    Some(initialization),
                    test,
                    body,
                    Some(update),
                )
            }
            Self::While(source) => {
                if contains(source, ContainsSymbol::YieldExpression) {
                    return None;
                }
                let test = phase(&mut cursor, &mut awaits, |cursor, awaits| {
                    expression(source.condition(), cursor, awaits)
                })?;
                let body = phase(&mut cursor, &mut awaits, |cursor, awaits| {
                    append_async_statement_protocol_states(source.body(), cursor, awaits, false)
                })?;
                (GeneratorLoopKindIr::While, None, test, body, None)
            }
            Self::DoWhile(source) => {
                if contains(source, ContainsSymbol::YieldExpression) {
                    return None;
                }
                let body = phase(&mut cursor, &mut awaits, |cursor, awaits| {
                    append_async_statement_protocol_states(source.body(), cursor, awaits, false)
                })?;
                let test = phase(&mut cursor, &mut awaits, |cursor, awaits| {
                    expression(source.cond(), cursor, awaits)
                })?;
                (GeneratorLoopKindIr::DoWhile, None, test, body, None)
            }
            Self::GeneratorResourceFor(_) => return None,
        };
        Some(AsyncGeneratorLoopSourceStates {
            execution: ResumableRegionProtocolIr::Async,
            kind,
            initialization,
            test,
            body,
            update,
            resource: None,
            exit: cursor,
            suspensions: awaits,
        })
    }
    pub(crate) fn append_typed(
        self,
        cursor: &mut u32,
        points: &mut Vec<ResumableSuspensionPointIr>,
    ) -> Option<()> {
        let plan = self.plan(*cursor)?;
        points.extend_from_slice(plan.suspensions());
        *cursor = plan.exit();
        Some(())
    }
    pub(crate) fn append_generator(
        self,
        cursor: &mut u32,
        points: &mut Vec<GeneratorSuspensionPointIr>,
    ) -> Option<()> {
        if self.execution() != ResumableRegionProtocolIr::Generator {
            return None;
        }
        let plan = self.plan(*cursor)?;
        points.extend(
            plan.suspensions()
                .iter()
                .map(|point| GeneratorSuspensionPointIr {
                    suspend_state: point.suspend_state,
                    resume_state: point.resume_state,
                }),
        );
        *cursor = plan.exit();
        Some(())
    }
}

fn resource_head(source: &ForLoop) -> Option<(ResourceDisposalHintIr, &[Variable])> {
    let ForLoopInitializer::Lexical(declaration) = source.init()? else {
        return None;
    };
    match declaration.declaration() {
        LexicalDeclaration::Using(variables) => {
            Some((ResourceDisposalHintIr::Sync, variables.as_ref()))
        }
        LexicalDeclaration::AwaitUsing(variables) => {
            Some((ResourceDisposalHintIr::Async, variables.as_ref()))
        }
        LexicalDeclaration::Let(_) | LexicalDeclaration::Const(_) => None,
    }
}

fn resource_phase(
    states: &mut ResumableStateAllocator,
    append: impl FnOnce(&mut ResumableStateAllocator) -> Result<(), AsyncGeneratorSourceError>,
) -> Result<AsyncGeneratorSourceRange, AsyncGeneratorSourceError> {
    let entry = states.current();
    states.with_resume_environment(ResumableResumeEnvironmentIr::InvocationOuter, append)?;
    let range = AsyncGeneratorSourceRange {
        entry,
        end: states.current(),
    };
    states.reserve()?;
    Ok(range)
}

fn resource_for_plan(
    source: &ForLoop,
    entry: u32,
    execution: ResumableRegionProtocolIr,
) -> Option<AsyncGeneratorLoopSourceStates> {
    let (hint, variables) = resource_head(source)?;
    if variables.is_empty()
        || (execution == ResumableRegionProtocolIr::Generator
            && hint == ResourceDisposalHintIr::Async)
        || (execution == ResumableRegionProtocolIr::Generator
            && contains(source, ContainsSymbol::AwaitExpression))
        || (execution == ResumableRegionProtocolIr::Async
            && contains(source, ContainsSymbol::YieldExpression))
    {
        return None;
    }
    let mut states = ResumableStateAllocator::at(entry);
    let mut registrations = Vec::new();
    let initialization = resource_phase(&mut states, |states| {
        resource::append_registration_variables_for_protocol(
            hint,
            variables,
            states,
            &mut registrations,
            execution,
        )
    })
    .ok()?;
    let test = resource_phase(&mut states, |states| {
        if let Some(source) = source.condition() {
            for_of::append_expression(source, execution, states)?;
        }
        Ok(())
    })
    .ok()?;
    let body = resource_phase(&mut states, |states| {
        let mut cursor = states.current();
        match execution {
            ResumableRegionProtocolIr::Generator => {
                let mut points = Vec::new();
                crate::generator_loop_source::append_classic_generator_statement(
                    source.body(),
                    &mut cursor,
                    &mut points,
                )
                .ok_or(AsyncGeneratorSourceError::ForeignContinuation)?;
                states.append_protocol_points(
                    cursor,
                    points.into_iter().map(|point| {
                        (
                            ResumableSuspensionKindIr::Yield,
                            point.suspend_state,
                            point.resume_state,
                        )
                    }),
                );
            }
            ResumableRegionProtocolIr::Async => {
                let mut points = Vec::new();
                append_async_statement_protocol_states(
                    source.body(),
                    &mut cursor,
                    &mut points,
                    false,
                )
                .ok_or(AsyncGeneratorSourceError::ForeignContinuation)?;
                states.append_region(cursor, points, Vec::new());
            }
            ResumableRegionProtocolIr::AsyncGenerator => {
                return Err(AsyncGeneratorSourceError::ForeignContinuation)
            }
        }
        Ok(())
    })
    .ok()?;
    let update = resource_phase(&mut states, |states| {
        if let Some(source) = source.final_expr() {
            for_of::append_expression(source, execution, states)?;
        }
        Ok(())
    })
    .ok()?;
    let resource = resource::finish_scoped_for_protocol(
        entry,
        update.end(),
        registrations,
        &mut states,
        execution,
    )
    .ok()??;
    let exit = states.current();
    let (suspensions, _) = states.into_tape();
    Some(AsyncGeneratorLoopSourceStates {
        execution,
        kind: GeneratorLoopKindIr::For,
        initialization: Some(initialization),
        test,
        body,
        update: Some(update),
        resource: Some(resource),
        exit,
        suspensions,
    })
}
