//! A plain async With owns its complete outer head and original body phases.

use super::*;
use boa_ast::statement::With;
use boa_ast::visitor::{VisitWith, Visitor};
use boa_ast::StatementList;
use core::ops::ControlFlow;

mod statement;
pub(crate) use statement::append_protocol as append_async_statement_protocol_states;
pub(crate) use statement::expression_states as append_async_expression_protocol_states;
pub(crate) use statement::items as append_async_statement_protocol_items;
pub(crate) use statement::variables as append_async_variables;

/// Planning and lowering select the same specialized restart lifecycle. A
/// class has its own continuation scope even when its only visible phase is
/// one Await; that scope belongs to the complete classic-loop region.
pub(crate) fn plain_async_while_uses_eager_body(source: &WhileLoop) -> bool {
    struct ClassPhase;
    impl<'ast> Visitor<'ast> for ClassPhase {
        type BreakTy = ();
        fn visit_class_expression(&mut self, source: &'ast ClassExpression) -> ControlFlow<()> {
            if contains(source, ContainsSymbol::AwaitExpression) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        }
        fn visit_function_body(&mut self, _: &'ast FunctionBody) -> ControlFlow<()> {
            ControlFlow::Continue(())
        }
        fn visit_formal_parameter_list(&mut self, _: &'ast FormalParameterList) -> ControlFlow<()> {
            ControlFlow::Continue(())
        }
    }
    contains(source.condition(), ContainsSymbol::AwaitExpression)
        && !crate::lowering::synchronous_resource_loop::source_statement_suspends(source.body())
        && !contains_plain_async_phase(source.body())
        && source.condition().visit_with(&mut ClassPhase).is_continue()
}

pub(crate) struct AsyncWithSource<'ast> {
    source: &'ast With,
}

pub(crate) struct AsyncWithSourceStates {
    entry: u32,
    head_ready: u32,
    body_entry: u32,
    body_end: u32,
    exit: u32,
    awaits: Vec<(u32, u32)>,
}

impl AsyncWithSourceStates {
    pub(crate) fn entry(&self) -> u32 {
        self.entry
    }
    pub(crate) fn head_ready(&self) -> u32 {
        self.head_ready
    }
    pub(crate) fn body_entry(&self) -> u32 {
        self.body_entry
    }
    pub(crate) fn body_end(&self) -> u32 {
        self.body_end
    }
    pub(crate) fn exit(&self) -> u32 {
        self.exit
    }
    pub(crate) fn awaits(&self) -> &[(u32, u32)] {
        &self.awaits
    }
}

impl<'ast> AsyncWithSource<'ast> {
    pub(crate) fn new(source: &'ast With) -> Option<Self> {
        let checked = Self { source };
        checked.states(0)?;
        Some(checked)
    }
    pub(crate) fn source(&self) -> &'ast With {
        self.source
    }
    pub(crate) fn states(&self, entry: u32) -> Option<AsyncWithSourceStates> {
        let mut cursor = entry;
        let mut points = Vec::new();
        let (head_ready, body_entry, body_end) = self.append_phases(&mut cursor, &mut points)?;
        let exit = cursor;
        exit.checked_add(1)?;
        Some(AsyncWithSourceStates {
            entry,
            head_ready,
            body_entry,
            body_end,
            exit,
            awaits: points
                .into_iter()
                .map(|point| (point.suspend_state, point.resume_state))
                .collect(),
        })
    }
    pub(crate) fn append_protocol(
        &self,
        cursor: &mut u32,
        points: &mut Vec<ResumableSuspensionPointIr>,
    ) -> Option<()> {
        self.append_phases(cursor, points).map(|_| ())
    }
    fn append_phases(
        &self,
        cursor: &mut u32,
        points: &mut Vec<ResumableSuspensionPointIr>,
    ) -> Option<(u32, u32, u32)> {
        append_async_expression_protocol_states(self.source.expression(), cursor, points)?;
        let head_ready = *cursor;
        *cursor = cursor.checked_add(1)?;
        let body_entry = *cursor;
        append_async_statement_protocol_states(self.source.statement(), cursor, points, false)?;
        let body_end = *cursor;
        *cursor = cursor.checked_add(1)?;
        Some((head_ready, body_entry, body_end))
    }
}

/// The same actual FunctionBody boundary as the analyzed With owner. Foreign
/// loop bodies and nested activations do not lend their phases to an outer If.
pub(crate) fn contains_plain_async_phase<S: VisitWith + ?Sized>(source: &S) -> bool {
    struct Phases;
    impl<'ast> Visitor<'ast> for Phases {
        type BreakTy = ();
        fn visit_statement_list(&mut self, source: &'ast StatementList) -> ControlFlow<()> {
            if crate::async_generator_source::AsyncGeneratorResourceScopeSource::for_protocol(
                source.statements(),
                ResumableRegionProtocolIr::Async,
            )
            .is_some()
            {
                return ControlFlow::Break(());
            }
            source.visit_with(self)
        }
        fn visit_statement(&mut self, source: &'ast Statement) -> ControlFlow<()> {
            match source {
                Statement::With(_) => ControlFlow::Break(()),
                Statement::ForInLoop(source) => {
                    if crate::async_generator_source::AsyncGeneratorForInSource::for_execution(
                        source,
                        ResumableRegionProtocolIr::Async,
                    )
                    .is_some()
                    {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                }
                // Presence disables eager lowering; it never admits a region.
                // Every classic source reaches a continuation factory even
                // without Await. Replanning its descendants here would repeat
                // the same recursive work at every enclosing source boundary.
                Statement::WhileLoop(_) | Statement::DoWhileLoop(_) | Statement::ForLoop(_) => {
                    ControlFlow::Break(())
                }
                Statement::ForOfLoop(source) => {
                    if crate::async_generator_source::AsyncGeneratorForOfSource::for_execution(
                        source,
                        ResumableRegionProtocolIr::Async,
                    )
                    .is_some()
                    {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                }
                Statement::Var(_)
                | Statement::Empty
                | Statement::Debugger
                | Statement::Expression(_)
                | Statement::Block(_)
                | Statement::If(_)
                | Statement::Switch(_)
                | Statement::Labelled(_)
                | Statement::Break(_)
                | Statement::Continue(_)
                | Statement::Throw(_)
                | Statement::Try(_)
                | Statement::Return(_) => source.visit_with(self),
            }
        }
        fn visit_function_body(&mut self, _: &'ast FunctionBody) -> ControlFlow<()> {
            ControlFlow::Continue(())
        }
        fn visit_class_declaration(&mut self, _: &'ast ClassDeclaration) -> ControlFlow<()> {
            ControlFlow::Continue(())
        }
        fn visit_class_expression(&mut self, _: &'ast ClassExpression) -> ControlFlow<()> {
            ControlFlow::Continue(())
        }
    }
    source.visit_with(&mut Phases).is_break()
}
