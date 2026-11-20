use super::*;

pub(super) fn append(
    source: &Expression,
    states: &mut ResumableStateAllocator,
) -> Result<(), AsyncGeneratorSourceError> {
    let mut visitor = ExpressionStates {
        states,
        owns_branches: true,
    };
    match visitor.visit_expression(source) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(error) => Err(error),
    }
}

pub(super) fn append_foreign(
    source: &Expression,
    states: &mut ResumableStateAllocator,
) -> Result<(), AsyncGeneratorSourceError> {
    states.with_resume_environment(ResumableResumeEnvironmentIr::SavedLexicalChain, |states| {
        let mut visitor = ExpressionStates {
            states,
            owns_branches: false,
        };
        match visitor.visit_expression(source) {
            ControlFlow::Continue(()) => Ok(()),
            ControlFlow::Break(error) => Err(error),
        }
    })
}

pub(super) fn branch_states(
    source: &Expression,
    entry: u32,
) -> Option<AsyncGeneratorIfSourceStates> {
    if let Expression::Assign(source) = source {
        let (then_source, else_source) = match source.op() {
            AssignOp::BoolAnd | AssignOp::Coalesce => (Some(source.rhs()), None),
            AssignOp::BoolOr => (None, Some(source.rhs())),
            _ => return None,
        };
        return branch_parts_states_with_condition(
            |states| append_assignment_reference(source.lhs(), states),
            then_source,
            else_source,
            entry,
        );
    }
    let (condition, then_source, else_source) = match source {
        Expression::Conditional(source) => (
            Some(source.condition()),
            Some(source.if_true()),
            Some(source.if_false()),
        ),
        Expression::Binary(source) => match source.op() {
            BinaryOp::Logical(LogicalOp::And | LogicalOp::Coalesce) => {
                (Some(source.lhs()), Some(source.rhs()), None)
            }
            BinaryOp::Logical(LogicalOp::Or) => (Some(source.lhs()), None, Some(source.rhs())),
            BinaryOp::Arithmetic(_)
            | BinaryOp::Relational(_)
            | BinaryOp::Bitwise(_)
            | BinaryOp::Comma => return None,
        },
        _ => return None,
    };
    branch_parts_states(condition, then_source, else_source, entry)
}

pub(super) fn branch_parts_states(
    condition: Option<&Expression>,
    then_source: Option<&Expression>,
    else_source: Option<&Expression>,
    entry: u32,
) -> Option<AsyncGeneratorIfSourceStates> {
    branch_parts_states_with_condition(
        |states| {
            if let Some(source) = condition {
                append(source, states)?;
            }
            Ok(())
        },
        then_source,
        else_source,
        entry,
    )
}

fn branch_parts_states_with_condition(
    condition: impl FnOnce(&mut ResumableStateAllocator) -> Result<(), AsyncGeneratorSourceError>,
    then_source: Option<&Expression>,
    else_source: Option<&Expression>,
    entry: u32,
) -> Option<AsyncGeneratorIfSourceStates> {
    let mut states = ResumableStateAllocator::at(entry);
    let (condition, then_branch, else_branch) =
        append_branches_with_condition(condition, then_source, else_source, &mut states).ok()?;
    let exit = states.current();
    let (suspensions, enclosing_scope_resume_states) = states.into_tape();
    Some(AsyncGeneratorIfSourceStates {
        condition,
        then_branch,
        else_branch,
        exit,
        suspensions,
        enclosing_scope_resume_states,
    })
}

pub(super) fn append_branches(
    condition: Option<&Expression>,
    then_source: Option<&Expression>,
    else_source: Option<&Expression>,
    states: &mut ResumableStateAllocator,
) -> Result<
    (
        AsyncGeneratorSourceRange,
        AsyncGeneratorSourceRange,
        AsyncGeneratorSourceRange,
    ),
    AsyncGeneratorSourceError,
> {
    append_branches_with_condition(
        |states| {
            if let Some(source) = condition {
                append(source, states)?;
            }
            Ok(())
        },
        then_source,
        else_source,
        states,
    )
}

fn append_branches_with_condition(
    condition: impl FnOnce(&mut ResumableStateAllocator) -> Result<(), AsyncGeneratorSourceError>,
    then_source: Option<&Expression>,
    else_source: Option<&Expression>,
    states: &mut ResumableStateAllocator,
) -> Result<
    (
        AsyncGeneratorSourceRange,
        AsyncGeneratorSourceRange,
        AsyncGeneratorSourceRange,
    ),
    AsyncGeneratorSourceError,
> {
    let condition = phase(states, condition)?;
    let then_branch = phase(states, |states| {
        if let Some(source) = then_source {
            append(source, states)?;
        }
        Ok(())
    })?;
    let else_branch = phase(states, |states| {
        if let Some(source) = else_source {
            append(source, states)?;
        }
        Ok(())
    })?;
    Ok((condition, then_branch, else_branch))
}

/// The condition owns evaluation of the actual assignment Reference. Its base
/// and raw key precede GetValue and the selected RHS in one source-state tape.
fn append_assignment_reference(
    source: &AssignTarget,
    states: &mut ResumableStateAllocator,
) -> Result<(), AsyncGeneratorSourceError> {
    match source {
        AssignTarget::Identifier(_) => Ok(()),
        AssignTarget::Access(PropertyAccess::Simple(access)) => {
            append(access.target(), states)?;
            if let PropertyAccessField::Expr(key) = access.field() {
                append(key, states)?;
            }
            Ok(())
        }
        AssignTarget::Access(PropertyAccess::Private(access)) => append(access.target(), states),
        AssignTarget::Access(PropertyAccess::Super(access)) => {
            if let PropertyAccessField::Expr(key) = access.field() {
                append(key, states)?;
            }
            Ok(())
        }
        AssignTarget::Pattern(_) | AssignTarget::WebCompatCall(_) => {
            Err(AsyncGeneratorSourceError::UnsupportedExpression)
        }
    }
}

struct ExpressionStates<'a> {
    states: &'a mut ResumableStateAllocator,
    owns_branches: bool,
}
impl ExpressionStates<'_> {
    fn suspension(
        &mut self,
        kind: ResumableSuspensionKindIr,
    ) -> ControlFlow<AsyncGeneratorSourceError> {
        match self.states.suspend(kind) {
            Ok(()) => ControlFlow::Continue(()),
            Err(error) => ControlFlow::Break(error),
        }
    }
    fn branch(&mut self, source: &Expression) -> ControlFlow<AsyncGeneratorSourceError> {
        let Some(plan) = branch_states(source, self.states.current()) else {
            return ControlFlow::Break(AsyncGeneratorSourceError::UnsupportedExpression);
        };
        self.states.append_region(
            plan.exit,
            plan.suspensions,
            plan.enclosing_scope_resume_states,
        );
        ControlFlow::Continue(())
    }
}
impl<'ast> Visitor<'ast> for ExpressionStates<'_> {
    type BreakTy = AsyncGeneratorSourceError;
    // Logical operators that own suspensions are branch regions; each inner
    // node of a chain must be seen. (Chains in generator and async bodies are
    // bounded by the parser's nesting limit.)
    const VISITS_BINARY_CHAINS_ITERATIVELY: bool = false;
    fn visit_expression(&mut self, source: &'ast Expression) -> ControlFlow<Self::BreakTy> {
        if !has_suspension(source) {
            return ControlFlow::Continue(());
        }
        if !self.owns_branches && !matches!(source, Expression::Await(_) | Expression::Yield(_)) {
            return source.visit_with(self);
        }
        match source {
            Expression::Await(source) => {
                self.visit_expression(source.target())?;
                self.suspension(ResumableSuspensionKindIr::Await)
            }
            Expression::Yield(source) => {
                if let Some(source) = source.target() {
                    self.visit_expression(source)?;
                }
                // Yield adoption and delegation use their existing native protocol;
                // they do not fabricate an additional source Await point.
                self.suspension(ResumableSuspensionKindIr::Yield)
            }
            Expression::Conditional(_) => self.branch(source),
            Expression::Binary(binary) if matches!(binary.op(), BinaryOp::Logical(_)) => {
                self.branch(source)
            }
            Expression::Assign(assignment)
                if matches!(assignment.lhs(), AssignTarget::WebCompatCall(_)) =>
            {
                let AssignTarget::WebCompatCall(call) = assignment.lhs() else {
                    unreachable!()
                };
                // The actual Call completes before the mandated ReferenceError;
                // the RHS is never evaluated and reserves no continuation.
                call.visit_with(self)
            }
            Expression::Assign(assignment)
                if matches!(
                    assignment.op(),
                    AssignOp::BoolAnd | AssignOp::BoolOr | AssignOp::Coalesce
                ) =>
            {
                let Some(plan) = branch_states(source, self.states.current()) else {
                    return ControlFlow::Break(AsyncGeneratorSourceError::UnsupportedExpression);
                };
                self.states.append_region(
                    plan.exit,
                    plan.suspensions,
                    plan.enclosing_scope_resume_states,
                );
                ControlFlow::Continue(())
            }
            Expression::Assign(source) => {
                if source.op() == AssignOp::Assign {
                    if let AssignTarget::Pattern(pattern) = source.lhs() {
                        self.visit_expression(source.rhs())?;
                        return match pattern::append_if_suspended(pattern, self.states) {
                            Ok(()) => ControlFlow::Continue(()),
                            Err(error) => ControlFlow::Break(error),
                        };
                    }
                }
                if !matches!(
                    source.lhs(),
                    AssignTarget::Identifier(_) | AssignTarget::Access(_)
                ) {
                    return ControlFlow::Break(AsyncGeneratorSourceError::UnsupportedExpression);
                }
                source.visit_with(self)
            }
            Expression::Unary(source) if source.op() == UnaryOp::Delete => {
                // Reference-aware Delete keeps the original owner. A mixed
                // operand is not admitted as a fabricated Value/Get.
                if CheckedGeneratorDeleteSource::new_mixed(source.target()).is_none() {
                    return ControlFlow::Break(AsyncGeneratorSourceError::UnsupportedExpression);
                }
                source.visit_with(self)
            }
            Expression::Optional(source) => match optional::append(source, self.states) {
                Ok(()) => ControlFlow::Continue(()),
                Err(error) => ControlFlow::Break(error),
            },
            Expression::Parenthesized(_)
            | Expression::Binary(_)
            | Expression::Unary(_)
            | Expression::PropertyAccess(_)
            | Expression::Call(_)
            | Expression::New(_)
            | Expression::TaggedTemplate(_)
            | Expression::TemplateLiteral(_)
            | Expression::ArrayLiteral(_)
            | Expression::ObjectLiteral(_)
            | Expression::ClassExpression(_)
            | Expression::Spread(_)
            | Expression::BinaryInPrivate(_)
            | Expression::ImportCall(_)
            | Expression::Update(_) => source.visit_with(self),
            Expression::This(_)
            | Expression::Identifier(_)
            | Expression::Literal(_)
            | Expression::RegExpLiteral(_)
            | Expression::FunctionExpression(_)
            | Expression::ArrowFunction(_)
            | Expression::AsyncArrowFunction(_)
            | Expression::GeneratorExpression(_)
            | Expression::AsyncFunctionExpression(_)
            | Expression::AsyncGeneratorExpression(_)
            | Expression::NewTarget(_)
            | Expression::ImportMeta(_) => ControlFlow::Continue(()),
            Expression::SuperCall(_)
            | Expression::FormalParameterList(_)
            | Expression::Debugger => {
                ControlFlow::Break(AsyncGeneratorSourceError::UnsupportedExpression)
            }
        }
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
    fn visit_class_element(&mut self, source: &'ast ClassElement) -> ControlFlow<Self::BreakTy> {
        if let ClassElement::MethodDefinition(method) = source {
            if let ClassElementName::PropertyName(name) = method.name() {
                name.visit_with(self)?;
            }
            return ControlFlow::Continue(());
        }
        source.visit_with(self)
    }
    fn visit_object_method_definition(
        &mut self,
        source: &'ast ObjectMethodDefinition,
    ) -> ControlFlow<Self::BreakTy> {
        source.name().visit_with(self)
    }
}
