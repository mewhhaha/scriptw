//! Complete mixed Switch keeps the original two cells and one CaseBlock.

use crate::async_generator_loop_control::validate_tape;
use crate::async_generator_source::{AsyncGeneratorSourceRange, AsyncGeneratorSwitchSourceStates};
use crate::generator_loop_control::{
    checked_next_state, has_unowned_head_branch, mixed_sequence_end,
};
use crate::generator_switch::GeneratorSwitchControlError;
use crate::switch_storage::CheckedSwitchStorageIr;
use crate::{
    AsyncGeneratorLoopExpressionIr, AsyncGeneratorLoopRegionIr, LexicalEnvironmentIr,
    OwnedEnvBindingIr, ResumableExpressionIr, ResumableRegionIr, ResumableRegionProtocolIr,
    ResumableSuspensionPointIr, StatementIr, TypedExpr,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncGeneratorSwitchCaseIr {
    selector: Option<ResumableExpressionIr>,
    body: ResumableRegionIr,
}

impl AsyncGeneratorSwitchCaseIr {
    pub(crate) fn new(
        selector: Option<AsyncGeneratorLoopExpressionIr>,
        body: AsyncGeneratorLoopRegionIr,
    ) -> Result<Self, GeneratorSwitchControlError> {
        Self::new_complete(selector.map(Into::into), body.into())
    }
    pub(crate) fn new_complete(
        selector: Option<ResumableExpressionIr>,
        body: ResumableRegionIr,
    ) -> Result<Self, GeneratorSwitchControlError> {
        if body.block().lexical_environment.is_some() {
            return Err(GeneratorSwitchControlError::PerCaseEnvironment);
        }
        Ok(Self { selector, body })
    }
    pub fn selector(&self) -> Option<&ResumableExpressionIr> {
        self.selector.as_ref()
    }
    pub fn body(&self) -> &ResumableRegionIr {
        &self.body
    }
}

/// Complete discriminant/selection/fallthrough ranges and the original checked
/// retained storage are required before the mixed dispatcher can consume this.
#[must_use = "the checked mixed Switch must reach its actual statement"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncGeneratorSwitchIr {
    execution: ResumableRegionProtocolIr,
    discriminant: ResumableExpressionIr,
    storage: CheckedSwitchStorageIr,
    lexical_declarations: Vec<StatementIr>,
    cases: Vec<AsyncGeneratorSwitchCaseIr>,
    case_block_entry_state: u32,
    fallback_state: u32,
    exit_state: u32,
    suspensions: Vec<ResumableSuspensionPointIr>,
    resource: Option<crate::AsyncGeneratorScopedResourceIr>,
}

impl AsyncGeneratorSwitchIr {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        source: AsyncGeneratorSwitchSourceStates,
        discriminant: AsyncGeneratorLoopExpressionIr,
        discriminant_binding: OwnedEnvBindingIr,
        lexical_environment: Option<LexicalEnvironmentIr>,
        lexical_declarations: Vec<StatementIr>,
        cases: Vec<AsyncGeneratorSwitchCaseIr>,
        value_binding: OwnedEnvBindingIr,
        resource: Option<crate::AsyncGeneratorScopedResourceIr>,
        inventory: &[OwnedEnvBindingIr],
    ) -> Result<Self, GeneratorSwitchControlError> {
        if source.execution() != ResumableRegionProtocolIr::AsyncGenerator {
            return Err(GeneratorSwitchControlError::CaseOrder);
        }
        Self::new_complete(
            source,
            discriminant.into(),
            discriminant_binding,
            lexical_environment,
            lexical_declarations,
            cases,
            value_binding,
            resource,
            inventory,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_complete(
        source: AsyncGeneratorSwitchSourceStates,
        discriminant: ResumableExpressionIr,
        discriminant_binding: OwnedEnvBindingIr,
        lexical_environment: Option<LexicalEnvironmentIr>,
        lexical_declarations: Vec<StatementIr>,
        cases: Vec<AsyncGeneratorSwitchCaseIr>,
        value_binding: OwnedEnvBindingIr,
        resource: Option<crate::AsyncGeneratorScopedResourceIr>,
        inventory: &[OwnedEnvBindingIr],
    ) -> Result<Self, GeneratorSwitchControlError> {
        let execution = source.execution();
        if discriminant.region().protocol() != execution
            || cases.iter().any(|case| {
                case.body().protocol() != execution
                    || case
                        .selector()
                        .is_some_and(|selector| selector.region().protocol() != execution)
            })
            || (execution == ResumableRegionProtocolIr::Generator && resource.is_none())
        {
            return Err(GeneratorSwitchControlError::CaseOrder);
        }
        if source.selectors().len() != source.bodies().len() || cases.len() != source.bodies().len()
        {
            return Err(GeneratorSwitchControlError::CaseCount);
        }
        if cases.iter().filter(|case| case.selector.is_none()).count() > 1 {
            return Err(GeneratorSwitchControlError::DuplicateDefault);
        }
        let storage = CheckedSwitchStorageIr::new(
            discriminant.region().block(),
            discriminant.value(),
            discriminant_binding,
            value_binding,
            lexical_environment,
            inventory,
        )?;
        if cases
            .iter()
            .filter_map(|case| case.selector.as_ref())
            .any(|selector| selector.region().block().lexical_environment.is_some())
        {
            return Err(GeneratorSwitchControlError::InvalidCaseBlockEnvironment);
        }
        if discriminant
            .region()
            .block()
            .statements
            .iter()
            .any(has_unowned_head_branch)
            || lexical_declarations.iter().any(has_unowned_head_branch)
            || cases
                .iter()
                .filter_map(|case| case.selector.as_ref())
                .any(|selector| {
                    selector
                        .region()
                        .block()
                        .statements
                        .iter()
                        .any(has_unowned_head_branch)
                })
        {
            return Err(GeneratorSwitchControlError::UnownedHeadBranch);
        }
        if !matches_range(discriminant.region(), source.discriminant())
            || checked_next_state(discriminant.region().end_state())? != source.case_block_entry()
            || sequence_end_for(&lexical_declarations, source.case_block_entry(), execution)?
                != source.case_block_entry()
        {
            return Err(GeneratorSwitchControlError::CaseOrder);
        }
        let mut cursor = source.case_block_entry();
        for (case, expected) in cases.iter().zip(source.selectors()) {
            match (case.selector.as_ref(), expected) {
                (Some(selector), Some(expected))
                    if matches_range(selector.region(), *expected)
                        && selector.region().entry_state() == cursor =>
                {
                    cursor = checked_next_state(selector.region().end_state())?;
                }
                (None, None) => {}
                (Some(_), Some(_)) | (Some(_), None) | (None, Some(_)) => {
                    return Err(GeneratorSwitchControlError::CaseOrder);
                }
            }
        }
        if cursor != source.fallback_state() {
            return Err(GeneratorSwitchControlError::CaseOrder);
        }
        cursor = checked_next_state(cursor)?;
        for (case, expected) in cases.iter().zip(source.bodies()) {
            if case.body.block().lexical_environment.is_some() {
                return Err(GeneratorSwitchControlError::PerCaseEnvironment);
            }
            if !matches_range(&case.body, *expected) || case.body.entry_state() != cursor {
                return Err(GeneratorSwitchControlError::CaseOrder);
            }
            cursor = checked_next_state(case.body.end_state())?;
        }
        match (source.resource(), resource.as_ref()) {
            (None, None) if cursor == source.exit() => {}
            (Some(states), Some(resource))
                if resource.execution() == execution
                    && resource.matches_resumable_regions(
                        states,
                        &cases
                            .iter()
                            .filter_map(|case| {
                                case.selector.as_ref().map(|selector| selector.region())
                            })
                            .chain(cases.iter().map(|case| case.body()))
                            .collect::<Vec<_>>(),
                        inventory,
                    )
                    && resource.entry_state() == source.case_block_entry()
                    && resource.body_end_state().checked_add(1) == Some(cursor)
                    && resource.exit_state() == source.exit()
                    && [storage.discriminant_binding(), storage.value_binding()]
                        .iter()
                        .all(|binding| {
                            binding.name != resource.capability_binding().name
                                && binding.slot != resource.capability_binding().slot
                        })
                    && storage.lexical_environment().is_none_or(|environment| {
                        environment
                            .bindings
                            .iter()
                            .all(|binding| binding.name != resource.capability_binding().name)
                    }) => {}
            _ => return Err(GeneratorSwitchControlError::CaseOrder),
        }
        let mut plan = Self {
            execution,
            discriminant,
            storage,
            lexical_declarations,
            cases,
            case_block_entry_state: source.case_block_entry(),
            fallback_state: source.fallback_state(),
            exit_state: source.exit(),
            suspensions: Vec::new(),
            resource,
        };
        let mut actual = Vec::new();
        for region in plan.regions() {
            region.collect_suspensions(&mut actual);
        }
        validate_tape(source.suspensions(), &actual)?;
        plan.suspensions = actual;
        Ok(plan)
    }

    pub fn discriminant(&self) -> &ResumableExpressionIr {
        &self.discriminant
    }
    pub const fn execution(&self) -> ResumableRegionProtocolIr {
        self.execution
    }
    pub fn discriminant_binding(&self) -> &OwnedEnvBindingIr {
        self.storage.discriminant_binding()
    }
    pub fn value_binding(&self) -> &OwnedEnvBindingIr {
        self.storage.value_binding()
    }
    pub fn resource(&self) -> Option<&crate::AsyncGeneratorScopedResourceIr> {
        self.resource.as_ref()
    }
    pub fn lexical_environment(&self) -> Option<&LexicalEnvironmentIr> {
        self.storage.lexical_environment()
    }
    pub fn lexical_declarations(&self) -> &[StatementIr] {
        &self.lexical_declarations
    }
    pub fn cases(&self) -> &[AsyncGeneratorSwitchCaseIr] {
        &self.cases
    }
    pub fn entry_state(&self) -> u32 {
        self.discriminant.region().entry_state()
    }
    pub const fn case_block_entry_state(&self) -> u32 {
        self.case_block_entry_state
    }
    pub const fn fallback_state(&self) -> u32 {
        self.fallback_state
    }
    pub const fn exit_state(&self) -> u32 {
        self.exit_state
    }
    pub fn regions(&self) -> impl Iterator<Item = &ResumableRegionIr> {
        std::iter::once(self.discriminant.region())
            .chain(
                self.cases
                    .iter()
                    .filter_map(|case| case.selector.as_ref().map(|selector| selector.region())),
            )
            .chain(self.cases.iter().map(|case| &case.body))
    }
    pub fn expressions(&self) -> impl Iterator<Item = &TypedExpr> {
        std::iter::once(self.discriminant.value()).chain(
            self.cases
                .iter()
                .filter_map(|case| case.selector.as_ref().map(|selector| selector.value())),
        )
    }
    pub(crate) fn suspensions(&self) -> &[ResumableSuspensionPointIr] {
        &self.suspensions
    }
}

fn matches_range(region: &ResumableRegionIr, source: AsyncGeneratorSourceRange) -> bool {
    region.entry_state() == source.entry() && region.end_state() == source.end()
}

fn sequence_end_for(
    statements: &[StatementIr],
    state: u32,
    execution: ResumableRegionProtocolIr,
) -> Result<u32, GeneratorSwitchControlError> {
    match execution {
        ResumableRegionProtocolIr::Generator => {
            crate::generator_loop_control::sequence_end(statements, state)
        }
        ResumableRegionProtocolIr::Async => crate::async_switch::sequence_exit(statements, state)
            .map_err(|_| {
                crate::generator_loop_control::GeneratorLoopControlError::ForeignContinuation
            }),
        ResumableRegionProtocolIr::AsyncGenerator => mixed_sequence_end(statements, state),
    }
    .map_err(Into::into)
}
