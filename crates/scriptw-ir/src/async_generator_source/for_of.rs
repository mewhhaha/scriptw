//! Complete iterator phases use the same source allocator and protocol tape.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AsyncGeneratorForOfSourceIdentity(usize);
impl AsyncGeneratorForOfSourceIdentity {
    pub(crate) fn from_source(source: &ForOfLoop) -> Self {
        Self(source as *const ForOfLoop as usize)
    }
}

pub(crate) struct AsyncGeneratorForOfSourceStates {
    source: AsyncGeneratorForOfSourceIdentity,
    mode: BindingMode,
    execution: ResumableRegionProtocolIr,
    head: AsyncGeneratorSourceRange,
    acquisition: u32,
    advance: u32,
    initialization: AsyncGeneratorSourceRange,
    body: AsyncGeneratorSourceRange,
    exit: u32,
    protocol: AsyncGeneratorIteratorProtocolIr,
    resource: Option<AsyncGeneratorForOfResourceSourceStates>,
    suspensions: Vec<ResumableSuspensionPointIr>,
}
impl AsyncGeneratorForOfSourceStates {
    pub(crate) fn identity(&self) -> AsyncGeneratorForOfSourceIdentity {
        self.source
    }
    pub(crate) fn execution(&self) -> ResumableRegionProtocolIr {
        self.execution
    }
    pub(crate) fn head(&self) -> AsyncGeneratorSourceRange {
        self.head
    }
    pub(crate) fn acquisition_state(&self) -> u32 {
        self.acquisition
    }
    pub(crate) fn advance_state(&self) -> u32 {
        self.advance
    }
    pub(crate) fn initialization(&self) -> AsyncGeneratorSourceRange {
        self.initialization
    }
    pub(crate) fn body(&self) -> AsyncGeneratorSourceRange {
        self.body
    }
    pub(crate) fn exit(&self) -> u32 {
        self.exit
    }
    pub(crate) fn head_mode(&self) -> BindingMode {
        self.mode
    }
    pub(crate) fn protocol(&self) -> AsyncGeneratorIteratorProtocolIr {
        self.protocol
    }
    pub(crate) fn resource(&self) -> Option<&AsyncGeneratorForOfResourceSourceStates> {
        self.resource.as_ref()
    }
    pub(crate) fn suspensions(&self) -> &[ResumableSuspensionPointIr] {
        &self.suspensions
    }
}

pub(crate) struct AsyncGeneratorForOfResourceSourceStates {
    registration: AsyncGeneratorResourceRegistrationSource,
    finalizer: Option<AsyncDisposableFinalizerPlanIr>,
    exit: u32,
}
impl AsyncGeneratorForOfResourceSourceStates {
    pub(crate) fn registration(&self) -> &AsyncGeneratorResourceRegistrationSource {
        &self.registration
    }
    pub(crate) fn hint(&self) -> ResourceDisposalHintIr {
        self.registration.hint()
    }
    pub(crate) fn finalizer(&self) -> Option<&AsyncDisposableFinalizerPlanIr> {
        self.finalizer.as_ref()
    }
    pub(crate) fn exit(&self) -> u32 {
        self.exit
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AsyncGeneratorForOfSource<'ast> {
    source: &'ast ForOfLoop,
    execution: ResumableRegionProtocolIr,
}
impl<'ast> AsyncGeneratorForOfSource<'ast> {
    #[cfg(test)]
    pub(crate) fn new(source: &'ast ForOfLoop) -> Option<Self> {
        Self::for_execution(source, ResumableRegionProtocolIr::AsyncGenerator)
    }
    pub(crate) fn for_execution(
        source: &'ast ForOfLoop,
        execution: ResumableRegionProtocolIr,
    ) -> Option<Self> {
        admits(source).then_some(())?;
        match execution {
            ResumableRegionProtocolIr::Generator
                if source.r#await()
                    || contains(source, ContainsSymbol::AwaitExpression)
                    || matches!(source.initializer(), IterableLoopInitializer::AwaitUsing(_)) =>
            {
                return None
            }
            ResumableRegionProtocolIr::Async
                if contains(source, ContainsSymbol::YieldExpression) =>
            {
                return None
            }
            ResumableRegionProtocolIr::Generator
            | ResumableRegionProtocolIr::Async
            | ResumableRegionProtocolIr::AsyncGenerator => {}
        }
        let owner = Self { source, execution };
        owner.states(0)?;
        Some(owner)
    }
    pub(crate) fn source(self) -> &'ast ForOfLoop {
        self.source
    }
    pub(crate) fn execution(self) -> ResumableRegionProtocolIr {
        self.execution
    }
    pub(crate) fn states(self, entry: u32) -> Option<AsyncGeneratorForOfSourceStates> {
        let mut states = ResumableStateAllocator::at(entry);
        let mode = head_mode(self.source)?;
        let (head, acquisition, advance, initialization, body, protocol, resource) =
            append_phases(self.source, mode, self.execution, &mut states).ok()?;
        let exit = states.current();
        let (suspensions, _) = states.into_tape();
        Some(AsyncGeneratorForOfSourceStates {
            source: AsyncGeneratorForOfSourceIdentity::from_source(self.source),
            mode,
            execution: self.execution,
            head,
            acquisition,
            advance,
            initialization,
            body,
            exit,
            protocol,
            resource,
            suspensions,
        })
    }
}

pub(super) fn head_mode(source: &ForOfLoop) -> Option<BindingMode> {
    match source.initializer() {
        IterableLoopInitializer::Var(variable) if variable.init().is_none() => {
            Some(BindingMode::Var)
        }
        IterableLoopInitializer::Let(_) => Some(BindingMode::Let),
        IterableLoopInitializer::Const(_) => Some(BindingMode::Const),
        IterableLoopInitializer::Using(Binding::Identifier(_))
        | IterableLoopInitializer::AwaitUsing(Binding::Identifier(_)) => Some(BindingMode::Const),
        IterableLoopInitializer::Identifier(_)
        | IterableLoopInitializer::Pattern(_)
        | IterableLoopInitializer::Access(_) => Some(BindingMode::Var),
        IterableLoopInitializer::Var(_)
        | IterableLoopInitializer::Using(Binding::Pattern(_))
        | IterableLoopInitializer::AwaitUsing(Binding::Pattern(_))
        | IterableLoopInitializer::WebCompatCall(_) => None,
    }
}
pub(super) fn admits(source: &ForOfLoop) -> bool {
    head_mode(source).is_some() && region::admits(source)
}
pub(super) fn append(
    source: &ForOfLoop,
    states: &mut ResumableStateAllocator,
) -> Result<(), AsyncGeneratorSourceError> {
    let mode = head_mode(source).ok_or(AsyncGeneratorSourceError::ForeignContinuation)?;
    if !region::admits(source) {
        return Err(AsyncGeneratorSourceError::ForeignContinuation);
    }
    append_phases(
        source,
        mode,
        ResumableRegionProtocolIr::AsyncGenerator,
        states,
    )
    .map(|_| ())
}

pub(crate) fn append_for_execution(
    source: &ForOfLoop,
    execution: ResumableRegionProtocolIr,
    cursor: &mut u32,
    points: &mut Vec<ResumableSuspensionPointIr>,
) -> Option<()> {
    let mut states = ResumableStateAllocator::at(*cursor);
    let mode = head_mode(source)?;
    append_phases(source, mode, execution, &mut states).ok()?;
    *cursor = states.current();
    points.extend(states.into_tape().0);
    Some(())
}

pub(super) fn append_expression(
    source: &Expression,
    execution: ResumableRegionProtocolIr,
    states: &mut ResumableStateAllocator,
) -> Result<(), AsyncGeneratorSourceError> {
    match execution {
        ResumableRegionProtocolIr::AsyncGenerator => expression::append(source, states),
        ResumableRegionProtocolIr::Generator => {
            let mut cursor = states.current();
            let mut points = Vec::new();
            crate::lowering_helpers::GeneratorExpressionSourcePlan::new(
                source,
                crate::lowering_helpers::GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
            )
            .and_then(|source| source.append(&mut cursor, &mut points))
            .ok_or(AsyncGeneratorSourceError::UnsupportedExpression)?;
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
            Ok(())
        }
        ResumableRegionProtocolIr::Async => {
            let mut cursor = states.current();
            let mut points = Vec::new();
            crate::lowering_helpers::append_async_expression_states(
                source,
                &mut cursor,
                &mut points,
            )
            .ok_or(AsyncGeneratorSourceError::UnsupportedExpression)?;
            states.append_protocol_points(
                cursor,
                points
                    .into_iter()
                    .map(|(start, end)| (ResumableSuspensionKindIr::Await, start, end)),
            );
            Ok(())
        }
    }
}

pub(super) fn append_body(
    source: &Statement,
    execution: ResumableRegionProtocolIr,
    states: &mut ResumableStateAllocator,
) -> Result<(), AsyncGeneratorSourceError> {
    match execution {
        ResumableRegionProtocolIr::AsyncGenerator => {
            statement::append(source, states, AsyncGeneratorSourceDomain::FunctionBody)
        }
        ResumableRegionProtocolIr::Generator => {
            let mut cursor = states.current();
            let mut points = Vec::new();
            crate::generator_loop_source::append_classic_generator_statement(
                source,
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
            Ok(())
        }
        ResumableRegionProtocolIr::Async => {
            let mut cursor = states.current();
            let mut points = Vec::new();
            crate::lowering_helpers::append_async_statement_protocol_states(
                source,
                &mut cursor,
                &mut points,
                false,
            )
            .ok_or(AsyncGeneratorSourceError::ForeignContinuation)?;
            states.append_region(cursor, points, Vec::new());
            Ok(())
        }
    }
}

fn append_owned_pattern(
    source: &Pattern,
    execution: ResumableRegionProtocolIr,
    states: &mut ResumableStateAllocator,
) -> Result<(), AsyncGeneratorSourceError> {
    if !has_suspension(source) {
        return Ok(());
    }
    match execution {
        ResumableRegionProtocolIr::AsyncGenerator => pattern::append_if_suspended(source, states),
        ResumableRegionProtocolIr::Generator => {
            let mut cursor = states.current();
            let mut points = Vec::new();
            let result = match source {
                Pattern::Array(_) => {
                    crate::lowering_helpers::GeneratorArrayPatternSource::new(source)
                        .and_then(|source| source.append(&mut cursor, &mut points))
                }
                Pattern::Object(_) => {
                    crate::lowering_helpers::GeneratorObjectPatternSource::new(source)
                        .and_then(|source| source.append(&mut cursor, &mut points))
                }
            };
            result.ok_or(AsyncGeneratorSourceError::ForeignContinuation)?;
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
            Ok(())
        }
        ResumableRegionProtocolIr::Async => {
            let mut cursor = states.current();
            let mut points = Vec::new();
            crate::lowering_helpers::AsyncPatternSource::new(source)
                .and_then(|source| source.append(&mut cursor, &mut points))
                .ok_or(AsyncGeneratorSourceError::ForeignContinuation)?;
            states.append_protocol_points(
                cursor,
                points
                    .into_iter()
                    .map(|(start, end)| (ResumableSuspensionKindIr::Await, start, end)),
            );
            Ok(())
        }
    }
}

pub(super) fn append_initializer(
    source: &IterableLoopInitializer,
    execution: ResumableRegionProtocolIr,
    states: &mut ResumableStateAllocator,
) -> Result<(), AsyncGeneratorSourceError> {
    match source {
        IterableLoopInitializer::Identifier(_)
        | IterableLoopInitializer::Let(Binding::Identifier(_))
        | IterableLoopInitializer::Const(Binding::Identifier(_)) => Ok(()),
        IterableLoopInitializer::Using(Binding::Identifier(_))
        | IterableLoopInitializer::AwaitUsing(Binding::Identifier(_)) => Ok(()),
        IterableLoopInitializer::Var(variable) => match variable.binding() {
            Binding::Identifier(_) => Ok(()),
            Binding::Pattern(pattern) => append_owned_pattern(pattern, execution, states),
        },
        IterableLoopInitializer::Let(Binding::Pattern(pattern))
        | IterableLoopInitializer::Const(Binding::Pattern(pattern))
        | IterableLoopInitializer::Pattern(pattern) => {
            append_owned_pattern(pattern, execution, states)
        }
        IterableLoopInitializer::Access(access) => match access {
            PropertyAccess::Simple(source) => {
                append_expression(source.target(), execution, states)?;
                if let PropertyAccessField::Expr(key) = source.field() {
                    append_expression(key, execution, states)?;
                }
                Ok(())
            }
            PropertyAccess::Private(source) => {
                append_expression(source.target(), execution, states)
            }
            PropertyAccess::Super(source) => match source.field() {
                PropertyAccessField::Const(_) => Ok(()),
                PropertyAccessField::Expr(key) => append_expression(key, execution, states),
            },
        },
        IterableLoopInitializer::Using(Binding::Pattern(_))
        | IterableLoopInitializer::AwaitUsing(Binding::Pattern(_))
        | IterableLoopInitializer::WebCompatCall(_) => {
            Err(AsyncGeneratorSourceError::ForeignContinuation)
        }
    }
}

fn append_phases(
    source: &ForOfLoop,
    mode: BindingMode,
    execution: ResumableRegionProtocolIr,
    states: &mut ResumableStateAllocator,
) -> Result<
    (
        AsyncGeneratorSourceRange,
        u32,
        u32,
        AsyncGeneratorSourceRange,
        AsyncGeneratorSourceRange,
        AsyncGeneratorIteratorProtocolIr,
        Option<AsyncGeneratorForOfResourceSourceStates>,
    ),
    AsyncGeneratorSourceError,
> {
    let head = phase(states, |states| {
        if matches!(mode, BindingMode::Let | BindingMode::Const) {
            states.with_enclosing_scope(|states| {
                append_expression(source.iterable(), execution, states)
            })
        } else {
            append_expression(source.iterable(), execution, states)
        }
    })?;
    let acquisition = states.current();
    states.reserve()?;
    let advance = states.current();
    let next = if source.r#await() {
        states
            .with_resume_environment(ResumableResumeEnvironmentIr::SavedLexicalChain, |states| {
                states.suspend(ResumableSuspensionKindIr::ForAwaitNext)
            })?;
        Some((advance, states.current()))
    } else {
        None
    };
    states.reserve()?;
    let initialization = phase(states, |states| {
        states.with_enclosing_scope(|states| {
            append_initializer(source.initializer(), execution, states)
        })
    })?;
    let body = phase(states, |states| {
        states.with_enclosing_scope(|states| append_body(source.body(), execution, states))
    })?;
    let resource = if let Some(registration) =
        AsyncGeneratorResourceRegistrationSource::for_of(source, initialization.end())
    {
        let finalizer = match registration.hint() {
            ResourceDisposalHintIr::Sync => None,
            ResourceDisposalHintIr::Async => Some(states.with_enclosing_scope(|states| {
                states.finish_resource_finalizer(initialization.entry(), body.end())
            })?),
        };
        Some(AsyncGeneratorForOfResourceSourceStates {
            registration,
            finalizer,
            exit: states.current(),
        })
    } else {
        None
    };
    let protocol = if let Some((next_suspend_state, next_resume_state)) = next {
        let close_suspend_state = states.current();
        states
            .with_resume_environment(ResumableResumeEnvironmentIr::SavedLexicalChain, |states| {
                states.suspend(ResumableSuspensionKindIr::ForAwaitClose)
            })?;
        let close_resume_state = states.current();
        states.reserve()?;
        AsyncGeneratorIteratorProtocolIr::Awaited {
            next_suspend_state,
            next_resume_state,
            close_suspend_state,
            close_resume_state,
        }
    } else {
        AsyncGeneratorIteratorProtocolIr::Sync
    };
    Ok((
        head,
        acquisition,
        advance,
        initialization,
        body,
        protocol,
        resource,
    ))
}
