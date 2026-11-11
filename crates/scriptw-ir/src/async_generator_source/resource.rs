//! One lexical list owns staged registrations and one original finalizer.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceSourceIdentity {
    Variable(usize),
    ForOf(usize),
}
impl ResourceSourceIdentity {
    fn from_variable(source: &Variable) -> Self {
        Self::Variable(source as *const Variable as usize)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AsyncGeneratorResourceRegistrationSource {
    source: ResourceSourceIdentity,
    hint: ResourceDisposalHintIr,
    register_state: u32,
}
impl AsyncGeneratorResourceRegistrationSource {
    pub(crate) fn hint(&self) -> ResourceDisposalHintIr {
        self.hint
    }
    pub(crate) fn register_state(&self) -> u32 {
        self.register_state
    }
    pub(crate) fn matches_variable(&self, source: &Variable) -> bool {
        self.source == ResourceSourceIdentity::from_variable(source)
    }
    pub(crate) fn matches_for_of(&self, source: &ForOfLoop) -> bool {
        self.source == ResourceSourceIdentity::ForOf(source as *const ForOfLoop as usize)
    }
    pub(super) fn for_of(source: &ForOfLoop, register_state: u32) -> Option<Self> {
        let hint = match source.initializer() {
            IterableLoopInitializer::Using(Binding::Identifier(_)) => ResourceDisposalHintIr::Sync,
            IterableLoopInitializer::AwaitUsing(Binding::Identifier(_)) => {
                ResourceDisposalHintIr::Async
            }
            _ => return None,
        };
        Some(Self {
            source: ResourceSourceIdentity::ForOf(source as *const ForOfLoop as usize),
            hint,
            register_state,
        })
    }
}
/// The allocator's actual finalizer travels with the source scope. An async
/// scope cannot retain only a hint and later reconstruct its disposal states.
pub(crate) enum AsyncGeneratorResourceScopeFinalizationSource {
    Sync { exit: u32 },
    Async(AsyncDisposableFinalizerPlanIr),
}

pub(crate) struct AsyncGeneratorResourceScopeSourceStates {
    execution: ResumableRegionProtocolIr,
    body: AsyncGeneratorSourceRange,
    finalization: AsyncGeneratorResourceScopeFinalizationSource,
    registrations: Vec<AsyncGeneratorResourceRegistrationSource>,
    suspensions: Vec<ResumableSuspensionPointIr>,
}

pub(crate) struct AsyncGeneratorScopedResourceSourceStates {
    execution: ResumableRegionProtocolIr,
    entry: u32,
    body_end: u32,
    exit: u32,
    registrations: Vec<AsyncGeneratorResourceRegistrationSource>,
    finalizer: Option<AsyncDisposableFinalizerPlanIr>,
}
impl AsyncGeneratorScopedResourceSourceStates {
    pub(crate) fn execution(&self) -> ResumableRegionProtocolIr {
        self.execution
    }
    pub(crate) fn entry(&self) -> u32 {
        self.entry
    }
    pub(crate) fn body_end(&self) -> u32 {
        self.body_end
    }
    pub(crate) fn exit(&self) -> u32 {
        self.exit
    }
    pub(crate) fn hint(&self) -> ResourceDisposalHintIr {
        if self
            .registrations
            .iter()
            .any(|source| source.hint() == ResourceDisposalHintIr::Async)
        {
            ResourceDisposalHintIr::Async
        } else {
            ResourceDisposalHintIr::Sync
        }
    }
    pub(crate) fn registrations(&self) -> &[AsyncGeneratorResourceRegistrationSource] {
        &self.registrations
    }
    pub(crate) fn finalizer(&self) -> Option<&AsyncDisposableFinalizerPlanIr> {
        self.finalizer.as_ref()
    }
}

pub(super) fn finish_scoped(
    entry: u32,
    body_end: u32,
    registrations: Vec<AsyncGeneratorResourceRegistrationSource>,
    states: &mut ResumableStateAllocator,
) -> Result<Option<AsyncGeneratorScopedResourceSourceStates>, AsyncGeneratorSourceError> {
    finish_scoped_for_protocol(
        entry,
        body_end,
        registrations,
        states,
        ResumableRegionProtocolIr::AsyncGenerator,
    )
}

pub(super) fn finish_scoped_for_protocol(
    entry: u32,
    body_end: u32,
    registrations: Vec<AsyncGeneratorResourceRegistrationSource>,
    states: &mut ResumableStateAllocator,
    execution: ResumableRegionProtocolIr,
) -> Result<Option<AsyncGeneratorScopedResourceSourceStates>, AsyncGeneratorSourceError> {
    if registrations.is_empty() {
        return Ok(None);
    }
    if states.current()
        != body_end
            .checked_add(1)
            .ok_or(AsyncGeneratorSourceError::StateOverflow)?
    {
        return Err(AsyncGeneratorSourceError::ForeignContinuation);
    }
    let asynchronous = registrations
        .iter()
        .any(|source| source.hint() == ResourceDisposalHintIr::Async);
    if asynchronous && execution == ResumableRegionProtocolIr::Generator {
        return Err(AsyncGeneratorSourceError::ForeignContinuation);
    }
    let finalizer = if asynchronous {
        Some(
            states
                .with_enclosing_scope(|states| states.finish_resource_finalizer(entry, body_end))?,
        )
    } else {
        None
    };
    Ok(Some(AsyncGeneratorScopedResourceSourceStates {
        execution,
        entry,
        body_end,
        exit: states.current(),
        registrations,
        finalizer,
    }))
}

pub(super) fn append_registration_variables(
    hint: ResourceDisposalHintIr,
    variables: &[Variable],
    states: &mut ResumableStateAllocator,
    registrations: &mut Vec<AsyncGeneratorResourceRegistrationSource>,
) -> Result<(), AsyncGeneratorSourceError> {
    append_registration_variables_for_protocol(
        hint,
        variables,
        states,
        registrations,
        ResumableRegionProtocolIr::AsyncGenerator,
    )
}

pub(super) fn append_registration_variables_for_protocol(
    hint: ResourceDisposalHintIr,
    variables: &[Variable],
    states: &mut ResumableStateAllocator,
    registrations: &mut Vec<AsyncGeneratorResourceRegistrationSource>,
    execution: ResumableRegionProtocolIr,
) -> Result<(), AsyncGeneratorSourceError> {
    if hint == ResourceDisposalHintIr::Async && execution == ResumableRegionProtocolIr::Generator {
        return Err(AsyncGeneratorSourceError::ForeignContinuation);
    }
    for variable in variables {
        if !matches!(variable.binding(), Binding::Identifier(_)) {
            return Err(AsyncGeneratorSourceError::ForeignContinuation);
        }
        for_of::append_expression(
            variable
                .init()
                .ok_or(AsyncGeneratorSourceError::ForeignContinuation)?,
            execution,
            states,
        )?;
        registrations.push(AsyncGeneratorResourceRegistrationSource {
            source: ResourceSourceIdentity::from_variable(variable),
            hint,
            register_state: states.current(),
        });
    }
    Ok(())
}

pub(super) fn append_control_items_for_protocol(
    items: &[StatementListItem],
    states: &mut ResumableStateAllocator,
    registrations: &mut Vec<AsyncGeneratorResourceRegistrationSource>,
    execution: ResumableRegionProtocolIr,
) -> Result<(), AsyncGeneratorSourceError> {
    for item in items {
        if let Some((hint, variables)) = declaration(item) {
            append_registration_variables_for_protocol(
                hint,
                variables,
                states,
                registrations,
                execution,
            )?;
        } else {
            append_item_for_protocol(item, states, execution)?;
        }
    }
    Ok(())
}
impl AsyncGeneratorResourceScopeSourceStates {
    pub(crate) fn execution(&self) -> ResumableRegionProtocolIr {
        self.execution
    }
    pub(crate) fn entry(&self) -> u32 {
        self.body.entry()
    }
    pub(crate) fn body(&self) -> AsyncGeneratorSourceRange {
        self.body
    }
    pub(crate) fn exit(&self) -> u32 {
        match &self.finalization {
            AsyncGeneratorResourceScopeFinalizationSource::Sync { exit } => *exit,
            AsyncGeneratorResourceScopeFinalizationSource::Async(plan) => plan.exit_state(),
        }
    }
    pub(crate) fn hint(&self) -> ResourceDisposalHintIr {
        match &self.finalization {
            AsyncGeneratorResourceScopeFinalizationSource::Sync { .. } => {
                ResourceDisposalHintIr::Sync
            }
            AsyncGeneratorResourceScopeFinalizationSource::Async(_) => {
                ResourceDisposalHintIr::Async
            }
        }
    }
    pub(crate) fn into_finalization(self) -> AsyncGeneratorResourceScopeFinalizationSource {
        self.finalization
    }
    pub(crate) fn registrations(&self) -> &[AsyncGeneratorResourceRegistrationSource] {
        &self.registrations
    }
    pub(crate) fn suspensions(&self) -> &[ResumableSuspensionPointIr] {
        &self.suspensions
    }
}
#[derive(Clone, Copy)]
pub(crate) struct AsyncGeneratorResourceScopeSource<'ast> {
    items: &'ast [StatementListItem],
    execution: ResumableRegionProtocolIr,
}
impl<'ast> AsyncGeneratorResourceScopeSource<'ast> {
    pub(crate) fn new(items: &'ast [StatementListItem]) -> Option<Self> {
        Self::for_protocol(items, ResumableRegionProtocolIr::AsyncGenerator)
    }
    pub(crate) fn for_protocol(
        items: &'ast [StatementListItem],
        execution: ResumableRegionProtocolIr,
    ) -> Option<Self> {
        match execution {
            ResumableRegionProtocolIr::Generator
                if items
                    .iter()
                    .any(|item| contains(item, ContainsSymbol::AwaitExpression)) =>
            {
                return None
            }
            ResumableRegionProtocolIr::Async
                if items
                    .iter()
                    .any(|item| contains(item, ContainsSymbol::YieldExpression)) =>
            {
                return None
            }
            ResumableRegionProtocolIr::Generator
            | ResumableRegionProtocolIr::Async
            | ResumableRegionProtocolIr::AsyncGenerator => {}
        }
        let mut has_registration = false;
        for item in items {
            if let Some((_, variables)) = declaration(item) {
                has_registration = true;
                if variables.is_empty()
                    || variables.iter().any(|variable| {
                        !matches!(variable.binding(), Binding::Identifier(_))
                            || variable.init().is_none()
                    })
                {
                    return None;
                }
            }
        }
        has_registration.then_some(())?;
        let source = Self { items, execution };
        let states = source.states(0)?;
        // An eager synchronous lexical scope retains its immediate disposal
        // owner. AwaitUsing still owns an asynchronous finalizer even when its
        // initializer and body do not suspend.
        if states.hint() == ResourceDisposalHintIr::Sync
            && states.body().entry() == states.body().end()
        {
            return None;
        }
        Some(source)
    }
    pub(crate) fn items(self) -> &'ast [StatementListItem] {
        self.items
    }
    pub(crate) fn execution(self) -> ResumableRegionProtocolIr {
        self.execution
    }
    pub(crate) fn states(self, entry: u32) -> Option<AsyncGeneratorResourceScopeSourceStates> {
        let mut states = ResumableStateAllocator::at(entry);
        let (body, finalization, registrations) =
            append_phases(self.items, &mut states, self.execution).ok()?;
        let (suspensions, _) = states.into_tape();
        Some(AsyncGeneratorResourceScopeSourceStates {
            execution: self.execution,
            body,
            finalization,
            registrations,
            suspensions,
        })
    }
}
pub(super) fn declaration(
    item: &StatementListItem,
) -> Option<(ResourceDisposalHintIr, &[Variable])> {
    let StatementListItem::Declaration(declaration) = item else {
        return None;
    };
    match declaration.as_ref() {
        Declaration::Lexical(LexicalDeclaration::Using(variables)) => {
            Some((ResourceDisposalHintIr::Sync, variables.as_ref()))
        }
        Declaration::Lexical(LexicalDeclaration::AwaitUsing(variables)) => {
            Some((ResourceDisposalHintIr::Async, variables.as_ref()))
        }
        _ => None,
    }
}
pub(super) fn append(
    items: &[StatementListItem],
    states: &mut ResumableStateAllocator,
) -> Result<(), AsyncGeneratorSourceError> {
    append_phases(items, states, ResumableRegionProtocolIr::AsyncGenerator).map(|_| ())
}

pub(crate) fn append_for_protocol(
    items: &[StatementListItem],
    execution: ResumableRegionProtocolIr,
    cursor: &mut u32,
    points: &mut Vec<ResumableSuspensionPointIr>,
) -> Option<()> {
    let mut states = ResumableStateAllocator::at(*cursor);
    append_phases(items, &mut states, execution).ok()?;
    *cursor = states.current();
    points.extend(states.into_tape().0);
    Some(())
}

fn append_item_for_protocol(
    item: &StatementListItem,
    states: &mut ResumableStateAllocator,
    execution: ResumableRegionProtocolIr,
) -> Result<(), AsyncGeneratorSourceError> {
    match execution {
        ResumableRegionProtocolIr::AsyncGenerator => {
            statement::append_item(item, states, AsyncGeneratorSourceDomain::FunctionBody)
        }
        ResumableRegionProtocolIr::Generator => {
            let mut cursor = states.current();
            let mut points = Vec::new();
            crate::generator_loop_source::append_classic_generator_items(
                std::slice::from_ref(item),
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
            crate::lowering_helpers::append_async_statement_protocol_items(
                std::slice::from_ref(item),
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

fn append_phases(
    items: &[StatementListItem],
    states: &mut ResumableStateAllocator,
    execution: ResumableRegionProtocolIr,
) -> Result<
    (
        AsyncGeneratorSourceRange,
        AsyncGeneratorResourceScopeFinalizationSource,
        Vec<AsyncGeneratorResourceRegistrationSource>,
    ),
    AsyncGeneratorSourceError,
> {
    let entry = states.current();
    let mut hint = ResourceDisposalHintIr::Sync;
    let mut registrations = Vec::new();
    states.with_resume_environment(ResumableResumeEnvironmentIr::InvocationOuter, |states| {
        for item in items {
            if let Some((resource_hint, variables)) = declaration(item) {
                if resource_hint == ResourceDisposalHintIr::Async {
                    hint = ResourceDisposalHintIr::Async;
                }
                append_registration_variables_for_protocol(
                    resource_hint,
                    variables,
                    states,
                    &mut registrations,
                    execution,
                )?;
            } else {
                append_item_for_protocol(item, states, execution)?;
            }
        }
        Ok(())
    })?;
    if registrations.is_empty() {
        return Err(AsyncGeneratorSourceError::ForeignContinuation);
    }
    let body = AsyncGeneratorSourceRange {
        entry,
        end: states.current(),
    };
    let finalization = match hint {
        ResourceDisposalHintIr::Sync => {
            states.reserve()?;
            AsyncGeneratorResourceScopeFinalizationSource::Sync {
                exit: states.current(),
            }
        }
        ResourceDisposalHintIr::Async => AsyncGeneratorResourceScopeFinalizationSource::Async(
            states.reserve_resource_finalizer(entry)?,
        ),
    };
    Ok((body, finalization, registrations))
}
