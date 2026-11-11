//! One source walk owns discrimination, lazy selection and normal fallthrough.
use super::*;
use crate::generator_loop_control::{checked_next_state, GeneratorLoopSourceRange};
use crate::generator_loop_source::append_classic_generator_items;

enum EmptyStatementCompletionSource<'ast> {
    Statement { _source: &'ast Statement },
    Declaration { _source: &'ast Declaration },
    CatchParameter { _source: &'ast Binding },
}

/// Actual source syntax proves that the whole lowered item has an Empty
/// normal completion. Generated expression prefixes cannot publish their
/// temporary values as the surrounding StatementList's value.
pub(crate) struct CheckedEmptyStatementCompletionSource<'ast> {
    _source: EmptyStatementCompletionSource<'ast>,
}

impl<'ast> CheckedEmptyStatementCompletionSource<'ast> {
    pub(crate) fn from_catch_parameter(source: &'ast Binding) -> Self {
        Self {
            _source: EmptyStatementCompletionSource::CatchParameter { _source: source },
        }
    }
    pub(crate) fn from_statement(source: &'ast Statement) -> Option<Self> {
        match source {
            Statement::Var(_) | Statement::Empty | Statement::Debugger => Some(Self {
                _source: EmptyStatementCompletionSource::Statement { _source: source },
            }),
            Statement::Expression(_)
            | Statement::Block(_)
            | Statement::If(_)
            | Statement::WhileLoop(_)
            | Statement::DoWhileLoop(_)
            | Statement::ForLoop(_)
            | Statement::ForOfLoop(_)
            | Statement::ForInLoop(_)
            | Statement::Switch(_)
            | Statement::Labelled(_)
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Throw(_)
            | Statement::Try(_)
            | Statement::Return(_)
            | Statement::With(_) => None,
        }
    }

    pub(crate) fn from_declaration(source: &'ast Declaration) -> Self {
        match source {
            Declaration::Lexical(_)
            | Declaration::FunctionDeclaration(_)
            | Declaration::GeneratorDeclaration(_)
            | Declaration::AsyncFunctionDeclaration(_)
            | Declaration::AsyncGeneratorDeclaration(_)
            | Declaration::ClassDeclaration(_) => Self {
                _source: EmptyStatementCompletionSource::Declaration { _source: source },
            },
        }
    }
}

/// The source plan has no raw constructor outside this module. Its ranges are
/// consumed by both function admission and the actual ordinary Switch lowerer.
pub(crate) struct GeneratorSwitchSourceStates {
    discriminant: GeneratorLoopSourceRange,
    case_block_entry: u32,
    selectors: Box<[Option<GeneratorLoopSourceRange>]>,
    bodies: Box<[GeneratorLoopSourceRange]>,
    fallback_state: u32,
    exit: u32,
    suspensions: Vec<GeneratorSuspensionPointIr>,
}

impl GeneratorSwitchSourceStates {
    pub(crate) fn discriminant(&self) -> GeneratorLoopSourceRange {
        self.discriminant
    }
    pub(crate) fn case_block_entry(&self) -> u32 {
        self.case_block_entry
    }
    pub(crate) fn selectors(&self) -> &[Option<GeneratorLoopSourceRange>] {
        &self.selectors
    }
    pub(crate) fn bodies(&self) -> &[GeneratorLoopSourceRange] {
        &self.bodies
    }
    pub(crate) fn fallback_state(&self) -> u32 {
        self.fallback_state
    }
    pub(crate) fn exit(&self) -> u32 {
        self.exit
    }
    pub(crate) fn suspensions(&self) -> &[GeneratorSuspensionPointIr] {
        &self.suspensions
    }
    pub(crate) fn into_suspensions(self) -> Vec<GeneratorSuspensionPointIr> {
        self.suspensions
    }
}

#[must_use = "the checked Switch source must be lowered or appended to its function plan"]
pub(crate) struct CheckedGeneratorSwitchSource<'ast> {
    source: &'ast AstSwitch,
    states: GeneratorSwitchSourceStates,
}

impl<'ast> CheckedGeneratorSwitchSource<'ast> {
    pub(crate) fn new(source: &'ast AstSwitch, entry: u32) -> Option<Self> {
        if !(contains(source, ContainsSymbol::YieldExpression)
            || crate::lowering_helpers::contains_ordinary_generator_phase_owner(source))
            || contains(source, ContainsSymbol::AwaitExpression)
        {
            return None;
        }
        let mut cursor = entry;
        let mut suspensions = Vec::new();
        let mut expression_region =
            |source: &Expression, cursor: &mut u32| -> Option<GeneratorLoopSourceRange> {
                let entry = *cursor;
                GeneratorExpressionSourcePlan::new(
                    source,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
                )?
                .append(cursor, &mut suspensions)?;
                let end = *cursor;
                *cursor = checked_next_state(end).ok()?;
                Some(GeneratorLoopSourceRange { entry, end })
            };
        // GetValue of the full discriminant precedes CaseBlock instantiation.
        let discriminant = expression_region(source.val(), &mut cursor)?;
        let case_block_entry = cursor;
        let mut defaults = 0_u32;
        let mut selectors = Vec::with_capacity(source.cases().len());
        for case in source.cases() {
            selectors.push(match case.condition() {
                Some(source) => Some(expression_region(source, &mut cursor)?),
                None => {
                    defaults = defaults.checked_add(1)?;
                    if defaults > 1 {
                        return None;
                    }
                    None
                }
            });
        }
        // Default can be selected only after every actual selector failed,
        // including selectors after the default clause's source position.
        let fallback_state = cursor;
        cursor = checked_next_state(cursor).ok()?;
        let mut bodies = Vec::with_capacity(source.cases().len());
        for case in source.cases() {
            let entry = cursor;
            append_classic_generator_items(
                case.body().statements(),
                &mut cursor,
                &mut suspensions,
            )?;
            let end = cursor;
            cursor = checked_next_state(end).ok()?;
            bodies.push(GeneratorLoopSourceRange { entry, end });
        }
        // The containing function owns a final normal state after Switch exit.
        checked_next_state(cursor).ok()?;
        Some(Self {
            source,
            states: GeneratorSwitchSourceStates {
                discriminant,
                case_block_entry,
                selectors: selectors.into_boxed_slice(),
                bodies: bodies.into_boxed_slice(),
                fallback_state,
                exit: cursor,
                suspensions,
            },
        })
    }
    #[cfg(test)]
    pub(crate) fn states(&self) -> &GeneratorSwitchSourceStates {
        &self.states
    }
    pub(crate) fn into_parts(self) -> (&'ast AstSwitch, GeneratorSwitchSourceStates) {
        (self.source, self.states)
    }
}

pub(crate) fn append_generator_switch_suspensions(
    source: &AstSwitch,
    cursor: &mut u32,
    points: &mut Vec<GeneratorSuspensionPointIr>,
) -> Option<()> {
    if crate::async_generator_source::switch_has_direct_resources(source) {
        let mut typed = Vec::new();
        crate::async_generator_source::append_resource_switch_for_protocol(
            source,
            ResumableRegionProtocolIr::Generator,
            cursor,
            &mut typed,
        )?;
        points.extend(typed.into_iter().map(|point| GeneratorSuspensionPointIr {
            suspend_state: point.suspend_state,
            resume_state: point.resume_state,
        }));
        return Some(());
    }
    let (_, states) = CheckedGeneratorSwitchSource::new(source, *cursor)?.into_parts();
    let exit = states.exit();
    points.extend(states.into_suspensions());
    *cursor = exit;
    Some(())
}
