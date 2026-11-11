//! Closed protocol views reuse the original complete statement validators.
use crate::async_generator_source::AsyncGeneratorSourceRange;
use crate::generator_loop_control::{GeneratorLoopControlError, GeneratorLoopSourceRange};
use crate::{
    AsyncGeneratorLoopExpressionIr, AsyncGeneratorLoopRegionIr, BlockIr, GeneratorLoopExpressionIr,
    GeneratorLoopRegionIr, ResumableResumeEnvironmentIr, ResumableSuspensionKindIr,
    ResumableSuspensionPointIr, TypedExpr,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumableRegionProtocolIr {
    Generator,
    Async,
    AsyncGenerator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RegionOwner {
    Generator(GeneratorLoopRegionIr),
    Async {
        block: BlockIr,
        entry: u32,
        end: u32,
    },
    AsyncGenerator(AsyncGeneratorLoopRegionIr),
    IterationResource {
        region: AsyncGeneratorLoopRegionIr,
        protocol: ResumableRegionProtocolIr,
    },
    Resource {
        block: BlockIr,
        entry: u32,
        end: u32,
        protocol: ResumableRegionProtocolIr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumableRegionIr(RegionOwner);
impl ResumableRegionIr {
    pub(crate) fn from_checked_resource(
        body: crate::async_generator_resource::CheckedResourceBody,
    ) -> Self {
        let (block, range, protocol) = body.into_protocol_parts();
        Self(RegionOwner::Resource {
            block,
            entry: range.entry(),
            end: range.end(),
            protocol,
        })
    }
    pub(crate) fn new(
        block: BlockIr,
        range: AsyncGeneratorSourceRange,
        protocol: ResumableRegionProtocolIr,
    ) -> Result<Self, GeneratorLoopControlError> {
        Ok(Self(match protocol {
            ResumableRegionProtocolIr::Generator => {
                RegionOwner::Generator(GeneratorLoopRegionIr::new(
                    block,
                    GeneratorLoopSourceRange {
                        entry: range.entry(),
                        end: range.end(),
                    },
                )?)
            }
            ResumableRegionProtocolIr::Async => {
                let end = crate::async_switch::sequence_exit(&block.statements, range.entry())
                    .map_err(|_| GeneratorLoopControlError::ForeignContinuation)?;
                if end != range.end() {
                    return Err(GeneratorLoopControlError::StateMismatch {
                        expected: range.end(),
                        actual: end,
                    });
                }
                RegionOwner::Async {
                    block,
                    entry: range.entry(),
                    end,
                }
            }
            ResumableRegionProtocolIr::AsyncGenerator => {
                RegionOwner::AsyncGenerator(AsyncGeneratorLoopRegionIr::new(block, range)?)
            }
        }))
    }
    pub(crate) fn from_checked_iteration_resource(
        region: AsyncGeneratorLoopRegionIr,
        resource: &crate::AsyncGeneratorForOfResourceIr,
        states: &crate::async_generator_source::AsyncGeneratorForOfResourceSourceStates,
        protocol: ResumableRegionProtocolIr,
    ) -> Option<Self> {
        if !resource.matches_source(states) || region.block().statements.len() != 1 {
            return None;
        }
        let crate::StatementIr::AsyncGeneratorResourceRegistration(registration) =
            &region.block().statements[0]
        else {
            return None;
        };
        if registration.capability_binding() != resource.capability_binding()
            || registration.source() != states.registration()
        {
            return None;
        }
        Some(Self(RegionOwner::IterationResource { region, protocol }))
    }
    pub fn protocol(&self) -> ResumableRegionProtocolIr {
        match self.0 {
            RegionOwner::Generator(_) => ResumableRegionProtocolIr::Generator,
            RegionOwner::Async { .. } => ResumableRegionProtocolIr::Async,
            RegionOwner::AsyncGenerator(_) => ResumableRegionProtocolIr::AsyncGenerator,
            RegionOwner::IterationResource { protocol, .. }
            | RegionOwner::Resource { protocol, .. } => protocol,
        }
    }
    pub fn block(&self) -> &BlockIr {
        match &self.0 {
            RegionOwner::Generator(region) => region.block(),
            RegionOwner::Async { block, .. } | RegionOwner::Resource { block, .. } => block,
            RegionOwner::AsyncGenerator(region) | RegionOwner::IterationResource { region, .. } => {
                region.block()
            }
        }
    }
    pub fn entry_state(&self) -> u32 {
        match &self.0 {
            RegionOwner::Generator(region) => region.entry_state(),
            RegionOwner::Async { entry, .. } | RegionOwner::Resource { entry, .. } => *entry,
            RegionOwner::AsyncGenerator(region) | RegionOwner::IterationResource { region, .. } => {
                region.entry_state()
            }
        }
    }
    pub fn end_state(&self) -> u32 {
        match &self.0 {
            RegionOwner::Generator(region) => region.end_state(),
            RegionOwner::Async { end, .. } | RegionOwner::Resource { end, .. } => *end,
            RegionOwner::AsyncGenerator(region) | RegionOwner::IterationResource { region, .. } => {
                region.end_state()
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn as_mixed(&self) -> Option<&AsyncGeneratorLoopRegionIr> {
        match &self.0 {
            RegionOwner::AsyncGenerator(region)
            | RegionOwner::IterationResource {
                region,
                protocol: ResumableRegionProtocolIr::AsyncGenerator,
            } => Some(region),
            RegionOwner::Generator(_)
            | RegionOwner::Async { .. }
            | RegionOwner::IterationResource { .. }
            | RegionOwner::Resource { .. } => None,
        }
    }
    pub(crate) fn collect_suspensions(&self, output: &mut Vec<ResumableSuspensionPointIr>) {
        match &self.0 {
            RegionOwner::Generator(region) => {
                let mut points = Vec::new();
                crate::generator_loop_control::collect_suspensions(
                    &region.block().statements,
                    &mut points,
                );
                output.extend(points.into_iter().map(|point| ResumableSuspensionPointIr {
                    kind: ResumableSuspensionKindIr::Yield,
                    suspend_state: point.suspend_state,
                    resume_state: point.resume_state,
                    resume_environment: ResumableResumeEnvironmentIr::InvocationOuter,
                }));
            }
            RegionOwner::Async { block, .. } => {
                crate::async_with::collect_async_suspensions(&block.statements, output);
            }
            RegionOwner::AsyncGenerator(region) => {
                crate::generator_loop_control::collect_mixed_suspensions(
                    &region.block().statements,
                    output,
                )
            }
            // The sole checked registration reads the retained incoming cell;
            // source suspension belongs to the iterator head/body, never this op.
            RegionOwner::IterationResource { .. } => {}
            RegionOwner::Resource {
                block, protocol, ..
            } => crate::async_generator_resource::collect_resource_suspensions(
                &block.statements,
                *protocol,
                output,
            ),
        }
    }
}
impl From<AsyncGeneratorLoopRegionIr> for ResumableRegionIr {
    fn from(value: AsyncGeneratorLoopRegionIr) -> Self {
        Self(RegionOwner::AsyncGenerator(value))
    }
}
impl From<GeneratorLoopRegionIr> for ResumableRegionIr {
    fn from(value: GeneratorLoopRegionIr) -> Self {
        Self(RegionOwner::Generator(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumableExpressionIr {
    region: ResumableRegionIr,
    value: TypedExpr,
}
impl ResumableExpressionIr {
    pub(crate) fn new(region: ResumableRegionIr, value: TypedExpr) -> Self {
        Self { region, value }
    }
    pub fn region(&self) -> &ResumableRegionIr {
        &self.region
    }
    pub fn value(&self) -> &TypedExpr {
        &self.value
    }
}
impl From<AsyncGeneratorLoopExpressionIr> for ResumableExpressionIr {
    fn from(value: AsyncGeneratorLoopExpressionIr) -> Self {
        Self {
            region: value.region().clone().into(),
            value: value.value().clone(),
        }
    }
}
impl From<GeneratorLoopExpressionIr> for ResumableExpressionIr {
    fn from(value: GeneratorLoopExpressionIr) -> Self {
        Self {
            region: value.region().clone().into(),
            value: value.value().clone(),
        }
    }
}
