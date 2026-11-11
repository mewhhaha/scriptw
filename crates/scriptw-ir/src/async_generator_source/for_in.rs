//! Complete mixed enumeration shares the original head evidence and owns each selected-key initializer.
use super::*;
use crate::lowering_helpers::{
    mixed_for_in_head_mode_and_kind, ForInSourceIdentity, GeneratorForInHeadKind,
    GeneratorForInHeadProof,
};
use boa_ast::statement::iteration::ForInLoop;

pub(crate) struct AsyncGeneratorForInSourceStates {
    source: ForInSourceIdentity,
    execution: ResumableRegionProtocolIr,
    mode: BindingMode,
    kind: GeneratorForInHeadKind,
    head: AsyncGeneratorSourceRange,
    advance: u32,
    initialization: AsyncGeneratorSourceRange,
    body: AsyncGeneratorSourceRange,
    exit: u32,
    suspensions: Vec<ResumableSuspensionPointIr>,
}

impl AsyncGeneratorForInSourceStates {
    pub(crate) fn execution(&self) -> ResumableRegionProtocolIr {
        self.execution
    }
    pub(crate) fn identity(&self) -> ForInSourceIdentity {
        self.source
    }
    pub(crate) fn entry(&self) -> u32 {
        self.head.entry()
    }
    pub(crate) fn head(&self) -> AsyncGeneratorSourceRange {
        self.head
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
    pub(crate) fn head_kind(&self) -> GeneratorForInHeadKind {
        self.kind
    }
    pub(crate) fn matches_head(&self, proof: &GeneratorForInHeadProof) -> bool {
        proof.matches_source(self.source, self.mode, self.kind)
    }
    pub(crate) fn suspensions(&self) -> &[ResumableSuspensionPointIr] {
        &self.suspensions
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AsyncGeneratorForInSource<'ast> {
    source: &'ast ForInLoop,
    execution: ResumableRegionProtocolIr,
}

impl<'ast> AsyncGeneratorForInSource<'ast> {
    #[cfg(test)]
    pub(crate) fn new(source: &'ast ForInLoop) -> Option<Self> {
        Self::for_execution(source, ResumableRegionProtocolIr::AsyncGenerator)
    }
    pub(crate) fn for_execution(
        source: &'ast ForInLoop,
        execution: ResumableRegionProtocolIr,
    ) -> Option<Self> {
        admits(source).then_some(())?;
        match execution {
            ResumableRegionProtocolIr::Generator
                if contains(source, ContainsSymbol::AwaitExpression) =>
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
    pub(crate) fn source(self) -> &'ast ForInLoop {
        self.source
    }
    pub(crate) fn execution(self) -> ResumableRegionProtocolIr {
        self.execution
    }
    pub(crate) fn checked_head(self, interner: &Interner) -> Option<GeneratorForInHeadProof> {
        GeneratorForInHeadProof::from_mixed_source(self.source, interner)
    }
    pub(crate) fn states(self, entry: u32) -> Option<AsyncGeneratorForInSourceStates> {
        let mut states = ResumableStateAllocator::at(entry);
        let (mode, kind) = mixed_for_in_head_mode_and_kind(self.source)?;
        let (head, advance, initialization, body) =
            append_phases(self.source, mode, self.execution, &mut states).ok()?;
        let exit = states.current();
        let (suspensions, _) = states.into_tape();
        Some(AsyncGeneratorForInSourceStates {
            source: ForInSourceIdentity::from_source(self.source),
            execution: self.execution,
            mode,
            kind,
            head,
            advance,
            initialization,
            body,
            exit,
            suspensions,
        })
    }
}

pub(super) fn admits(source: &ForInLoop) -> bool {
    mixed_for_in_head_mode_and_kind(source).is_some() && region::admits(source)
}

pub(super) fn append(
    source: &ForInLoop,
    states: &mut ResumableStateAllocator,
) -> Result<(), AsyncGeneratorSourceError> {
    if !admits(source) {
        return Err(AsyncGeneratorSourceError::ForeignContinuation);
    }
    let (mode, _) = mixed_for_in_head_mode_and_kind(source)
        .ok_or(AsyncGeneratorSourceError::ForeignContinuation)?;
    append_phases(
        source,
        mode,
        ResumableRegionProtocolIr::AsyncGenerator,
        states,
    )
    .map(|_| ())
}

pub(crate) fn append_for_execution(
    source: &ForInLoop,
    execution: ResumableRegionProtocolIr,
    cursor: &mut u32,
    points: &mut Vec<ResumableSuspensionPointIr>,
) -> Option<()> {
    let (mode, _) = mixed_for_in_head_mode_and_kind(source)?;
    let mut states = ResumableStateAllocator::at(*cursor);
    append_phases(source, mode, execution, &mut states).ok()?;
    *cursor = states.current();
    points.extend(states.into_tape().0);
    Some(())
}

fn append_phases(
    source: &ForInLoop,
    mode: BindingMode,
    execution: ResumableRegionProtocolIr,
    states: &mut ResumableStateAllocator,
) -> Result<
    (
        AsyncGeneratorSourceRange,
        u32,
        AsyncGeneratorSourceRange,
        AsyncGeneratorSourceRange,
    ),
    AsyncGeneratorSourceError,
> {
    let head = phase(states, |states| {
        if let IterableLoopInitializer::Var(variable) = source.initializer() {
            if let Some(initializer) = variable.init() {
                for_of::append_expression(initializer, execution, states)?;
            }
        }
        if matches!(mode, BindingMode::Let | BindingMode::Const) {
            states.with_enclosing_scope(|states| {
                for_of::append_expression(source.target(), execution, states)
            })
        } else {
            for_of::append_expression(source.target(), execution, states)
        }
    })?;
    let advance = states.current();
    states.reserve()?;
    let initialization = phase(states, |states| {
        states.with_enclosing_scope(|states| {
            for_of::append_initializer(source.initializer(), execution, states)
        })
    })?;
    let body = phase(states, |states| {
        states.with_enclosing_scope(|states| for_of::append_body(source.body(), execution, states))
    })?;
    Ok((head, advance, initialization, body))
}
