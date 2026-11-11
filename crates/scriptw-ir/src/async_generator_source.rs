//! One checked source allocator owns mixed async-generator continuation ranges.
use super::*;
use boa_ast::visitor::{VisitWith, Visitor};
use boa_ast::StatementList;
use std::ops::ControlFlow;

mod allocator;
mod classic_loop;
mod plain_async_classic;
pub(crate) use plain_async_classic::PlainAsyncClassicLoopSource;
mod expression;
mod for_in;
mod for_of;
mod optional;
mod pattern;
mod region;
mod resource;
mod statement;
mod switch;
mod with;

use allocator::ResumableStateAllocator;
pub(crate) use classic_loop::{AsyncGeneratorClassicLoopSource, AsyncGeneratorIfSource};
pub(crate) use for_in::append_for_execution as append_complete_for_in_source;
pub(crate) use for_in::{AsyncGeneratorForInSource, AsyncGeneratorForInSourceStates};
pub(crate) use for_of::append_for_execution as append_complete_for_of_source;
pub(crate) use for_of::{
    AsyncGeneratorForOfResourceSourceStates, AsyncGeneratorForOfSource,
    AsyncGeneratorForOfSourceIdentity, AsyncGeneratorForOfSourceStates,
};
pub(crate) use optional::AsyncGeneratorOptionalChainStates;
pub(crate) use pattern::default_states as async_generator_pattern_default_states;
#[cfg(test)]
pub(crate) use pattern::AsyncGeneratorArrayPatternSource;
pub(crate) use pattern::{AsyncGeneratorArrayPatternSourceStates, AsyncGeneratorPatternSource};
pub(crate) use resource::append_for_protocol as append_resource_scope_for_protocol;
pub(crate) use resource::{
    AsyncGeneratorResourceRegistrationSource, AsyncGeneratorResourceScopeFinalizationSource,
    AsyncGeneratorResourceScopeSource, AsyncGeneratorResourceScopeSourceStates,
    AsyncGeneratorScopedResourceSourceStates,
};
pub(crate) use statement::item_enters_foreign_suffix;
pub(crate) use switch::append_for_protocol as append_resource_switch_for_protocol;
pub(crate) use switch::has_async_operands as switch_has_async_operands;
pub(crate) use switch::has_direct_resources as switch_has_direct_resources;
pub(crate) use switch::{AsyncGeneratorSwitchSource, AsyncGeneratorSwitchSourceStates};
pub(crate) use with::{AsyncGeneratorWithSource, AsyncGeneratorWithSourceStates};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AsyncGeneratorSourceDomain {
    FunctionBody,
    ForeignIteratorBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AsyncGeneratorSourceError {
    StateOverflow,
    UnsupportedExpression,
    ForeignContinuation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AsyncGeneratorSourceRange {
    entry: u32,
    end: u32,
}
impl AsyncGeneratorSourceRange {
    pub(crate) fn entry(self) -> u32 {
        self.entry
    }
    pub(crate) fn end(self) -> u32 {
        self.end
    }
}

pub(crate) struct AsyncGeneratorLoopSourceStates {
    execution: ResumableRegionProtocolIr,
    kind: GeneratorLoopKindIr,
    initialization: Option<AsyncGeneratorSourceRange>,
    test: AsyncGeneratorSourceRange,
    body: AsyncGeneratorSourceRange,
    update: Option<AsyncGeneratorSourceRange>,
    resource: Option<AsyncGeneratorScopedResourceSourceStates>,
    exit: u32,
    suspensions: Vec<ResumableSuspensionPointIr>,
}
impl AsyncGeneratorLoopSourceStates {
    pub(crate) fn execution(&self) -> ResumableRegionProtocolIr {
        self.execution
    }
    pub(crate) fn kind(&self) -> GeneratorLoopKindIr {
        self.kind
    }
    pub(crate) fn initialization(&self) -> Option<AsyncGeneratorSourceRange> {
        self.initialization
    }
    pub(crate) fn test(&self) -> AsyncGeneratorSourceRange {
        self.test
    }
    pub(crate) fn body(&self) -> AsyncGeneratorSourceRange {
        self.body
    }
    pub(crate) fn update(&self) -> Option<AsyncGeneratorSourceRange> {
        self.update
    }
    pub(crate) fn exit(&self) -> u32 {
        self.exit
    }
    pub(crate) fn resource(&self) -> Option<&AsyncGeneratorScopedResourceSourceStates> {
        self.resource.as_ref()
    }
    pub(crate) fn suspensions(&self) -> &[ResumableSuspensionPointIr] {
        &self.suspensions
    }
}

pub(crate) struct AsyncGeneratorIfSourceStates {
    condition: AsyncGeneratorSourceRange,
    then_branch: AsyncGeneratorSourceRange,
    else_branch: AsyncGeneratorSourceRange,
    exit: u32,
    suspensions: Vec<ResumableSuspensionPointIr>,
    enclosing_scope_resume_states: Vec<u32>,
}
impl AsyncGeneratorIfSourceStates {
    pub(crate) fn condition(&self) -> AsyncGeneratorSourceRange {
        self.condition
    }
    pub(crate) fn then_branch(&self) -> AsyncGeneratorSourceRange {
        self.then_branch
    }
    pub(crate) fn else_branch(&self) -> AsyncGeneratorSourceRange {
        self.else_branch
    }
    pub(crate) fn exit(&self) -> u32 {
        self.exit
    }
    pub(crate) fn suspensions(&self) -> &[ResumableSuspensionPointIr] {
        &self.suspensions
    }
}

/// A function receives only the plan from its actual checked source walk.
pub(crate) struct AsyncGeneratorFunctionSource {
    state_count: u32,
    suspension_points: Vec<ResumableSuspensionPointIr>,
    enclosing_scope_resume_states: Vec<u32>,
}
impl AsyncGeneratorFunctionSource {
    pub(crate) fn new(body: &FunctionBody) -> Result<Self, AsyncGeneratorSourceError> {
        let mut states = ResumableStateAllocator::at(0);
        statement::append_items(
            body.statement_list().statements(),
            &mut states,
            AsyncGeneratorSourceDomain::FunctionBody,
        )?;
        let (state_count, suspension_points, enclosing_scope_resume_states) = states.finish()?;
        Ok(Self {
            state_count,
            suspension_points,
            enclosing_scope_resume_states,
        })
    }
    pub(crate) fn into_parts(self) -> (u32, Vec<ResumableSuspensionPointIr>, Vec<u32>) {
        (
            self.state_count,
            self.suspension_points,
            self.enclosing_scope_resume_states,
        )
    }
    pub(crate) fn into_resumable_plan(self) -> ResumablePlanIr {
        ResumablePlanIr::from_checked_source(self)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AsyncGeneratorExpressionSource<'ast> {
    source: &'ast Expression,
}
impl<'ast> AsyncGeneratorExpressionSource<'ast> {
    pub(crate) fn new(source: &'ast Expression) -> Option<Self> {
        let mut states = ResumableStateAllocator::at(0);
        expression::append(source, &mut states).ok()?;
        Some(Self { source })
    }
    pub(crate) fn expression(self) -> &'ast Expression {
        self.source
    }
    pub(crate) fn branch_states(self, entry: u32) -> Option<AsyncGeneratorIfSourceStates> {
        expression::branch_states(self.source, entry)
    }
}

fn phase(
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

fn has_suspension<T: VisitWith>(source: &T) -> bool {
    contains(source, ContainsSymbol::AwaitExpression)
        || contains(source, ContainsSymbol::YieldExpression)
}
