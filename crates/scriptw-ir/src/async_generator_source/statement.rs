use super::*;

/// Analysis and source allocation share the actual resource-suffix boundary.
pub(crate) fn item_enters_foreign_suffix(item: &StatementListItem) -> bool {
    async_generator_await_using_is_admitted(item) || sync_using_declaration(item)
}

pub(super) fn append_items(
    items: &[StatementListItem],
    states: &mut ResumableStateAllocator,
    domain: AsyncGeneratorSourceDomain,
) -> Result<(), AsyncGeneratorSourceError> {
    if domain == AsyncGeneratorSourceDomain::FunctionBody
        && AsyncGeneratorResourceScopeSource::new(items).is_some()
    {
        return resource::append(items, states);
    }
    let mut domain = domain;
    let mut async_disposable_scope_count = 0;
    for item in items {
        let admitted_disposal = async_generator_await_using_is_admitted(item);
        let mut visitor = StatementStates { states, domain };
        match visitor.visit_statement_list_item(item) {
            ControlFlow::Continue(()) => {}
            ControlFlow::Break(error) => return Err(error),
        }
        if item_enters_foreign_suffix(item) {
            if admitted_disposal {
                async_disposable_scope_count += 1;
            }
            // This actual suffix keeps its original resource finalizer owner.
            domain = AsyncGeneratorSourceDomain::ForeignIteratorBody;
        }
    }
    for _ in 0..async_disposable_scope_count {
        states.reserve_async_disposable_finalizer()?;
    }
    Ok(())
}

pub(super) fn append_item(
    item: &StatementListItem,
    states: &mut ResumableStateAllocator,
    domain: AsyncGeneratorSourceDomain,
) -> Result<(), AsyncGeneratorSourceError> {
    let mut visitor = StatementStates { states, domain };
    match visitor.visit_statement_list_item(item) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(error) => Err(error),
    }
}

pub(super) fn append(
    source: &Statement,
    states: &mut ResumableStateAllocator,
    domain: AsyncGeneratorSourceDomain,
) -> Result<(), AsyncGeneratorSourceError> {
    let mut visitor = StatementStates { states, domain };
    match visitor.visit_statement(source) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(error) => Err(error),
    }
}

pub(super) fn append_variables(
    source: &[Variable],
    states: &mut ResumableStateAllocator,
) -> Result<(), AsyncGeneratorSourceError> {
    for variable in source {
        if let Some(source) = variable.init() {
            expression::append(source, states)?;
        }
        if let Binding::Pattern(pattern) = variable.binding() {
            pattern::append_if_suspended(pattern, states)?;
        }
    }
    Ok(())
}

struct StatementStates<'a> {
    states: &'a mut ResumableStateAllocator,
    domain: AsyncGeneratorSourceDomain,
}
fn control(
    result: Result<(), AsyncGeneratorSourceError>,
) -> ControlFlow<AsyncGeneratorSourceError> {
    match result {
        Ok(()) => ControlFlow::Continue(()),
        Err(error) => ControlFlow::Break(error),
    }
}
impl<'ast> Visitor<'ast> for StatementStates<'_> {
    type BreakTy = AsyncGeneratorSourceError;
    fn visit_statement_list(&mut self, source: &'ast StatementList) -> ControlFlow<Self::BreakTy> {
        control(append_items(source.statements(), self.states, self.domain))
    }
    fn visit_expression(&mut self, source: &'ast Expression) -> ControlFlow<Self::BreakTy> {
        control(match self.domain {
            AsyncGeneratorSourceDomain::FunctionBody => expression::append(source, self.states),
            AsyncGeneratorSourceDomain::ForeignIteratorBody => {
                expression::append_foreign(source, self.states)
            }
        })
    }
    fn visit_statement(&mut self, source: &'ast Statement) -> ControlFlow<Self::BreakTy> {
        if self.domain == AsyncGeneratorSourceDomain::ForeignIteratorBody {
            // The original scalar iterator/resource source authority remains
            // intact. A newly ranged child cannot be buried in this owner.
            if matches!(source, Statement::Block(_) | Statement::Try(_)) {
                let domain = self.domain;
                return control(self.states.with_enclosing_scope(|states| {
                    let mut visitor = StatementStates { states, domain };
                    match source.visit_with(&mut visitor) {
                        ControlFlow::Continue(()) => Ok(()),
                        ControlFlow::Break(error) => Err(error),
                    }
                }));
            }
            return source.visit_with(self);
        }
        match source {
            Statement::Block(source) => control(self.states.with_enclosing_scope(|states| {
                states.with_resume_environment(
                    ResumableResumeEnvironmentIr::InvocationOuter,
                    |states| {
                        append_items(
                            source.statement_list().statements(),
                            states,
                            AsyncGeneratorSourceDomain::FunctionBody,
                        )
                    },
                )
            })),
            Statement::ForLoop(source) => {
                control(AsyncGeneratorClassicLoopSource::For(source).append(self.states))
            }
            Statement::WhileLoop(source) => {
                control(AsyncGeneratorClassicLoopSource::While(source).append(self.states))
            }
            Statement::DoWhileLoop(source) => {
                control(AsyncGeneratorClassicLoopSource::DoWhile(source).append(self.states))
            }
            Statement::If(source) => control(classic_loop::append_if(source, self.states)),
            Statement::Try(source) => control(self.states.with_enclosing_scope(|states| {
                states.with_resume_environment(
                    ResumableResumeEnvironmentIr::InvocationOuter,
                    |states| {
                        append_items(
                            source.block().statement_list().statements(),
                            states,
                            AsyncGeneratorSourceDomain::FunctionBody,
                        )?;
                        states.reserve()?;
                        if let Some(source) = source.catch() {
                            if let Some(Binding::Pattern(pattern)) = source.parameter() {
                                pattern::append_if_suspended(pattern, states)?;
                            }
                            append_items(
                                source.block().statement_list().statements(),
                                states,
                                AsyncGeneratorSourceDomain::FunctionBody,
                            )?;
                            states.reserve()?;
                        }
                        if let Some(source) = source.finally() {
                            append_items(
                                source.block().statement_list().statements(),
                                states,
                                AsyncGeneratorSourceDomain::FunctionBody,
                            )?;
                            states.reserve()?;
                        }
                        Ok(())
                    },
                )
            })),
            Statement::Switch(source) => control(switch::append(source, self.states)),
            Statement::With(source) => control(with::append(source, self.states)),
            Statement::ForInLoop(source) if for_in::admits(source) => {
                control(for_in::append(source, self.states))
            }
            Statement::ForInLoop(source) if has_suspension(&**source) => {
                ControlFlow::Break(AsyncGeneratorSourceError::ForeignContinuation)
            }
            Statement::ForInLoop(_) => {
                let old = self.domain;
                self.domain = AsyncGeneratorSourceDomain::ForeignIteratorBody;
                let result = source.visit_with(self);
                self.domain = old;
                result
            }
            Statement::ForOfLoop(source) if for_of::admits(source) => {
                control(for_of::append(source, self.states))
            }
            Statement::Empty
            | Statement::Var(_)
            | Statement::Expression(_)
            | Statement::Return(_)
            | Statement::Throw(_)
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::ForOfLoop(_)
            | Statement::Labelled(_)
            | Statement::Debugger => source.visit_with(self),
        }
    }
    fn visit_variable_list(
        &mut self,
        source: &'ast boa_ast::declaration::VariableList,
    ) -> ControlFlow<Self::BreakTy> {
        match self.domain {
            AsyncGeneratorSourceDomain::FunctionBody => {
                control(append_variables(source.as_ref(), self.states))
            }
            AsyncGeneratorSourceDomain::ForeignIteratorBody => source.visit_with(self),
        }
    }
    fn visit_return(&mut self, source: &'ast AstReturn) -> ControlFlow<Self::BreakTy> {
        if let Some(source) = source.target() {
            self.visit_expression(source)?;
            // Return settles its completed operand through the original Await.
            control(match self.domain {
                AsyncGeneratorSourceDomain::FunctionBody => {
                    self.states.suspend(ResumableSuspensionKindIr::Await)
                }
                AsyncGeneratorSourceDomain::ForeignIteratorBody => {
                    self.states.with_resume_environment(
                        ResumableResumeEnvironmentIr::SavedLexicalChain,
                        |states| states.suspend(ResumableSuspensionKindIr::Await),
                    )
                }
            })?;
        }
        ControlFlow::Continue(())
    }
    fn visit_for_of_loop(&mut self, source: &'ast ForOfLoop) -> ControlFlow<Self::BreakTy> {
        let old = self.domain;
        self.domain = AsyncGeneratorSourceDomain::ForeignIteratorBody;
        source.initializer().visit_with(self)?;
        source.iterable().visit_with(self)?;
        if source.r#await() {
            control(self.states.with_resume_environment(
                ResumableResumeEnvironmentIr::SavedLexicalChain,
                |states| states.suspend(ResumableSuspensionKindIr::ForAwaitNext),
            ))?;
        }
        let result = self.visit_statement(source.body());
        self.domain = old;
        result?;
        if source.r#await() {
            // The real next/close protocol requires a separate close suspend
            // state; retain the original reservation before ForAwaitClose.
            control(self.states.reserve())?;
            control(self.states.with_resume_environment(
                ResumableResumeEnvironmentIr::SavedLexicalChain,
                |states| states.suspend(ResumableSuspensionKindIr::ForAwaitClose),
            ))?;
        }
        ControlFlow::Continue(())
    }
    fn visit_function_body(&mut self, _: &'ast FunctionBody) -> ControlFlow<Self::BreakTy> {
        ControlFlow::Continue(())
    }
    fn visit_formal_parameter_list(
        &mut self,
        _: &'ast FormalParameterList,
    ) -> ControlFlow<Self::BreakTy> {
        ControlFlow::Continue(())
    }
    fn visit_class_declaration(
        &mut self,
        source: &'ast ClassDeclaration,
    ) -> ControlFlow<Self::BreakTy> {
        for source in class_evaluation_expressions(source.super_ref(), source.elements()) {
            self.visit_expression(source)?;
        }
        ControlFlow::Continue(())
    }
    fn visit_class_expression(
        &mut self,
        source: &'ast ClassExpression,
    ) -> ControlFlow<Self::BreakTy> {
        for source in class_evaluation_expressions(source.super_ref(), source.elements()) {
            self.visit_expression(source)?;
        }
        ControlFlow::Continue(())
    }
    fn visit_object_method_definition(
        &mut self,
        source: &'ast ObjectMethodDefinition,
    ) -> ControlFlow<Self::BreakTy> {
        source.name().visit_with(self)
    }
}

fn async_generator_await_using_is_admitted(item: &StatementListItem) -> bool {
    let StatementListItem::Declaration(declaration) = item else {
        return false;
    };
    let Declaration::Lexical(LexicalDeclaration::AwaitUsing(list)) = declaration.as_ref() else {
        return false;
    };
    list.as_ref().iter().all(|variable| {
        matches!(variable.binding(), Binding::Identifier(_))
            && variable
                .init()
                .is_some_and(|source| !has_suspension(source))
    })
}

fn sync_using_declaration(item: &StatementListItem) -> bool {
    matches!(item, StatementListItem::Declaration(source) if matches!(source.as_ref(), Declaration::Lexical(LexicalDeclaration::Using(_))))
}
