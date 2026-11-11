use super::*;

#[path = "generator_value_branch_source/branches.rs"]
mod branches;
pub(crate) use branches::{
    GeneratorValueBranchKind, GeneratorValueBranchSource, GeneratorValueRegionStates,
};
#[path = "generator_value_branch_source/optional_chain.rs"]
mod optional_chain;
pub(crate) use optional_chain::{
    GeneratorGroupedOptionalInvocationSource, GeneratorGroupedOptionalReferenceSource,
    GeneratorOptionalArgumentSource, GeneratorOptionalChainLink,
    GeneratorOptionalChainSource, GeneratorOptionalChainStates, GeneratorOptionalDeleteSource,
    GeneratorOptionalDeleteTerminal, GeneratorOptionalKeySource, GeneratorOptionalOperandSource,
};
#[path = "generator_value_branch_source/array_pattern.rs"]
mod array_pattern;
#[path = "generator_value_branch_source/object_pattern.rs"]
mod object_pattern;
pub(crate) use array_pattern::{GeneratorArrayPatternSource, GeneratorArrayPatternSourceStates};
#[path = "generator_value_branch_source/pattern_initializer.rs"]
mod pattern_initializer;
pub(crate) use object_pattern::GeneratorObjectPatternSource;
pub(crate) use pattern_initializer::GeneratorPatternInitializerSource;
#[path = "generator_value_branch_source/pattern_assignment.rs"]
mod pattern_assignment;
pub(crate) use pattern_assignment::GeneratorPatternAssignmentSource;

/// Ordinary generators admit staged value branches in their complete classic
/// control regions. Iterator bodies and async linear owners keep their grammar.
#[derive(Clone, Copy)]
pub(crate) enum GeneratorValueBranchAdmission {
    LinearOnly,
    OrdinaryOutsideLoops,
}

/// Delete's operand remains a Reference until the Delete consumer receives its
/// raw base/key. Only known value-producing syntax can use DeleteValue.
pub(crate) struct CheckedGeneratorDeleteSource<'ast>(GeneratorDeleteOperand<'ast>);

pub(crate) enum GeneratorDeleteOperand<'ast> {
    Super(&'ast boa_ast::expression::access::SuperPropertyAccess),
    Property {
        source: &'ast Expression,
        access: &'ast boa_ast::expression::access::SimplePropertyAccess,
    },
    Optional(GeneratorOptionalDeleteSource<'ast>),
    AwaitedOptional(&'ast Optional),
    Value(&'ast Expression),
}

impl<'ast> CheckedGeneratorDeleteSource<'ast> {
    pub(crate) fn new(source: &'ast Expression) -> Option<Self> {
        Self::for_optional(source, GeneratorOptionalDeleteSource::new)
    }
    pub(crate) fn new_mixed(source: &'ast Expression) -> Option<Self> {
        Self::for_optional(source, GeneratorOptionalDeleteSource::new_mixed)
    }
    pub(crate) fn new_async(mut source: &'ast Expression) -> Option<Self> {
        while let Expression::Parenthesized(group) = source {
            source = group.expression();
        }
        if let Expression::Optional(optional) = source {
            return match optional.chain().last()?.kind() {
                OptionalOperationKind::PrivatePropertyAccess { .. } => None,
                OptionalOperationKind::SimplePropertyAccess { .. }
                | OptionalOperationKind::Call { .. } => {
                    Some(Self(GeneratorDeleteOperand::AwaitedOptional(optional)))
                }
            };
        }
        Self::new(source)
    }
    fn for_optional(
        mut source: &'ast Expression,
        optional: impl FnOnce(&'ast Optional) -> Option<GeneratorOptionalDeleteSource<'ast>>,
    ) -> Option<Self> {
        while let Expression::Parenthesized(group) = source {
            source = group.expression();
        }
        let operand = match source {
            Expression::PropertyAccess(PropertyAccess::Simple(access)) => {
                GeneratorDeleteOperand::Property { source, access }
            }
            Expression::Optional(source) => GeneratorDeleteOperand::Optional(optional(source)?),
            Expression::PropertyAccess(PropertyAccess::Super(access)) => {
                GeneratorDeleteOperand::Super(access)
            }
            Expression::PropertyAccess(PropertyAccess::Private(_))
            | Expression::Identifier(_)
            | Expression::Parenthesized(_)
            | Expression::Spread(_)
            | Expression::FormalParameterList(_)
            | Expression::Debugger => return None,
            Expression::This(_)
            | Expression::Literal(_)
            | Expression::RegExpLiteral(_)
            | Expression::ArrayLiteral(_)
            | Expression::ObjectLiteral(_)
            | Expression::FunctionExpression(_)
            | Expression::ArrowFunction(_)
            | Expression::AsyncArrowFunction(_)
            | Expression::GeneratorExpression(_)
            | Expression::AsyncFunctionExpression(_)
            | Expression::AsyncGeneratorExpression(_)
            | Expression::ClassExpression(_)
            | Expression::TemplateLiteral(_)
            | Expression::New(_)
            | Expression::Call(_)
            | Expression::SuperCall(_)
            | Expression::ImportCall(_)
            | Expression::TaggedTemplate(_)
            | Expression::NewTarget(_)
            | Expression::ImportMeta(_)
            | Expression::Assign(_)
            | Expression::Unary(_)
            | Expression::Update(_)
            | Expression::Binary(_)
            | Expression::BinaryInPrivate(_)
            | Expression::Conditional(_)
            | Expression::Await(_)
            | Expression::Yield(_) => GeneratorDeleteOperand::Value(source),
        };
        Some(Self(operand))
    }

    pub(crate) fn into_operand(self) -> GeneratorDeleteOperand<'ast> {
        self.0
    }
}

/// The original logical selector and its complete admitted continuation stay
/// together. Its prefix completes before selection or either result arm.
pub(crate) struct StagedGeneratorSelector<'ast> {
    source: &'ast Expression,
    plan: GeneratorExpressionSourcePlan<'ast>,
}

impl<'ast> StagedGeneratorSelector<'ast> {
    fn new(source: &'ast Expression) -> Option<Self> {
        Some(Self {
            source,
            plan: GeneratorExpressionSourcePlan::new(
                source,
                GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
            )?,
        })
    }

    pub(crate) fn source(&self) -> &'ast Expression {
        self.source
    }
}

/// A closed compound assignment source. Capturing its actual original
/// Reference belongs to lowering; eager and selected logical operators retain
/// separate, complete suspension plans.
pub(crate) struct CheckedGeneratorCompoundAssignmentSource<'ast> {
    lhs: &'ast AssignTarget,
    rhs: &'ast Expression,
    operation: GeneratorCompoundAssignmentOperation,
}

#[derive(Clone, Copy)]
pub(crate) enum GeneratorCompoundAssignmentOperation {
    Arithmetic(ArithmeticOp),
    Bitwise(BitwiseBinaryOp),
    Logical(LogicalBinaryOp),
}

/// Complete selected-RHS ranges, shared by source admission and lowering.
pub(crate) struct GeneratorLogicalAssignmentStates {
    pub(crate) entry: u32,
    pub(crate) selected: crate::generator_loop_control::GeneratorLoopSourceRange,
    pub(crate) skipped: crate::generator_loop_control::GeneratorLoopSourceRange,
    pub(crate) exit: u32,
    suspensions: Vec<GeneratorSuspensionPointIr>,
}

impl<'ast> CheckedGeneratorCompoundAssignmentSource<'ast> {
    pub(crate) fn new(source: &'ast boa_ast::expression::operator::Assign) -> Option<Self> {
        match source.lhs() {
            AssignTarget::Identifier(_) | AssignTarget::Access(_) => {}
            AssignTarget::Pattern(_) | AssignTarget::WebCompatCall(_) => return None,
        }
        let operation = match source.op() {
            AssignOp::Add => GeneratorCompoundAssignmentOperation::Arithmetic(ArithmeticOp::Add),
            AssignOp::Sub => GeneratorCompoundAssignmentOperation::Arithmetic(ArithmeticOp::Sub),
            AssignOp::Mul => GeneratorCompoundAssignmentOperation::Arithmetic(ArithmeticOp::Mul),
            AssignOp::Div => GeneratorCompoundAssignmentOperation::Arithmetic(ArithmeticOp::Div),
            AssignOp::Mod => GeneratorCompoundAssignmentOperation::Arithmetic(ArithmeticOp::Mod),
            AssignOp::Exp => GeneratorCompoundAssignmentOperation::Arithmetic(ArithmeticOp::Exp),
            AssignOp::And => GeneratorCompoundAssignmentOperation::Bitwise(BitwiseBinaryOp::And),
            AssignOp::Or => GeneratorCompoundAssignmentOperation::Bitwise(BitwiseBinaryOp::Or),
            AssignOp::Xor => GeneratorCompoundAssignmentOperation::Bitwise(BitwiseBinaryOp::Xor),
            AssignOp::Shl => GeneratorCompoundAssignmentOperation::Bitwise(BitwiseBinaryOp::Shl),
            AssignOp::Shr => GeneratorCompoundAssignmentOperation::Bitwise(BitwiseBinaryOp::Shr),
            AssignOp::Ushr => GeneratorCompoundAssignmentOperation::Bitwise(BitwiseBinaryOp::UShr),
            AssignOp::BoolAnd => {
                GeneratorCompoundAssignmentOperation::Logical(LogicalBinaryOp::And)
            }
            AssignOp::BoolOr => GeneratorCompoundAssignmentOperation::Logical(LogicalBinaryOp::Or),
            AssignOp::Coalesce => {
                GeneratorCompoundAssignmentOperation::Logical(LogicalBinaryOp::Coalesce)
            }
            AssignOp::Assign => return None,
        };
        Some(Self {
            lhs: source.lhs(),
            rhs: source.rhs(),
            operation,
        })
    }

    pub(crate) fn lhs(&self) -> &'ast AssignTarget {
        self.lhs
    }
    pub(crate) fn rhs(&self) -> &'ast Expression {
        self.rhs
    }
    pub(crate) fn logical_operation(&self) -> Option<LogicalBinaryOp> {
        match self.operation {
            GeneratorCompoundAssignmentOperation::Logical(operation) => Some(operation),
            GeneratorCompoundAssignmentOperation::Arithmetic(_)
            | GeneratorCompoundAssignmentOperation::Bitwise(_) => None,
        }
    }
    pub(crate) fn logical_states(&self, entry: u32) -> Option<GeneratorLogicalAssignmentStates> {
        self.logical_operation()?;
        let plan = GeneratorExpressionSourcePlan::new(
            self.rhs,
            GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
        )?;
        let selected_entry = entry.checked_add(1)?;
        let mut cursor = selected_entry;
        let mut suspensions = Vec::new();
        plan.append(&mut cursor, &mut suspensions)?;
        let selected = crate::generator_loop_control::GeneratorLoopSourceRange {
            entry: selected_entry,
            end: cursor,
        };
        let skipped_entry = cursor.checked_add(1)?;
        let skipped = crate::generator_loop_control::GeneratorLoopSourceRange {
            entry: skipped_entry,
            end: skipped_entry,
        };
        Some(GeneratorLogicalAssignmentStates {
            entry,
            selected,
            skipped,
            exit: skipped_entry.checked_add(1)?,
            suspensions,
        })
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        &'ast AssignTarget,
        &'ast Expression,
        GeneratorCompoundAssignmentOperation,
    ) {
        (self.lhs, self.rhs, self.operation)
    }
}

enum GeneratorExpressionSuspensionStep<'ast> {
    LinearYield,
    ValueBranch(GeneratorValueBranchSource<'ast>),
    OptionalChain(GeneratorOptionalChainSource<'ast>),
    LogicalAssignment(CheckedGeneratorCompoundAssignmentSource<'ast>),
    PatternAssignment(GeneratorPatternAssignmentSource<'ast>),
    PatternInitializer(GeneratorPatternInitializerSource<'ast>),
}

/// An expression's admitted suspension order, rather than a count that loses
/// selected arms and joins. Only this checked source walk can create its steps.
pub(crate) struct GeneratorExpressionSourcePlan<'ast> {
    steps: Vec<GeneratorExpressionSuspensionStep<'ast>>,
}

impl<'ast> GeneratorExpressionSourcePlan<'ast> {
    pub(crate) fn new(
        source: &'ast Expression,
        admission: GeneratorValueBranchAdmission,
    ) -> Option<Self> {
        let mut plan = Self { steps: Vec::new() };
        plan.expression(source, admission)?;
        Some(plan)
    }

    pub(crate) fn declaration(
        source: &'ast StatementListItem,
        admission: GeneratorValueBranchAdmission,
    ) -> Option<Self> {
        let StatementListItem::Declaration(source) = source else {
            return None;
        };
        let mut plan = Self { steps: Vec::new() };
        match source.as_ref() {
            Declaration::Lexical(
                source @ (LexicalDeclaration::Let(_) | LexicalDeclaration::Const(_)),
            ) => {
                for variable in source.variable_list().as_ref() {
                    plan.variable(variable, admission)?;
                }
            }
            Declaration::ClassDeclaration(source) => {
                for expression in
                    class_evaluation_expressions(source.super_ref(), source.elements())
                {
                    plan.expression(expression, GeneratorValueBranchAdmission::LinearOnly)?;
                }
            }
            _ => return None,
        }
        Some(plan)
    }

    pub(crate) fn var_declaration(source: &'ast VarDeclaration) -> Option<Self> {
        let mut plan = Self { steps: Vec::new() };
        for variable in source.0.as_ref() {
            plan.variable(
                variable,
                GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
            )?;
        }
        Some(plan)
    }

    fn variable(
        &mut self,
        variable: &'ast Variable,
        admission: GeneratorValueBranchAdmission,
    ) -> Option<()> {
        match variable.binding() {
            Binding::Identifier(_) => {}
            Binding::Pattern(_) => {
                if !matches!(
                    admission,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops
                ) || contains(variable.binding(), ContainsSymbol::AwaitExpression)
                {
                    return None;
                }
                if variable
                    .init()
                    .is_some_and(|source| contains(source, ContainsSymbol::YieldExpression))
                    || contains(variable.binding(), ContainsSymbol::YieldExpression)
                {
                    let source = GeneratorPatternInitializerSource::new(variable, admission)?;
                    self.steps
                        .push(GeneratorExpressionSuspensionStep::PatternInitializer(
                            source,
                        ));
                    return Some(());
                }
            }
        }
        if let Some(source) = variable.init() {
            self.expression(source, admission)?;
        }
        Some(())
    }

    pub(crate) fn append(
        self,
        current_state: &mut u32,
        points: &mut Vec<GeneratorSuspensionPointIr>,
    ) -> Option<()> {
        // Validate overflow for the complete source before publishing any point.
        let mut cursor = *current_state;
        let mut admitted = Vec::new();
        for step in self.steps {
            match step {
                GeneratorExpressionSuspensionStep::LinearYield => {
                    let resume_state = cursor.checked_add(1)?;
                    admitted.push(GeneratorSuspensionPointIr {
                        suspend_state: cursor,
                        resume_state,
                    });
                    cursor = resume_state;
                }
                GeneratorExpressionSuspensionStep::ValueBranch(source) => {
                    source.append(&mut cursor, &mut admitted)?;
                }
                GeneratorExpressionSuspensionStep::LogicalAssignment(source) => {
                    let states = source.logical_states(cursor)?;
                    admitted.extend(states.suspensions);
                    cursor = states.exit;
                }
                GeneratorExpressionSuspensionStep::OptionalChain(source) => {
                    source.append(&mut cursor, &mut admitted)?;
                }
                GeneratorExpressionSuspensionStep::PatternAssignment(source) => {
                    source.append(&mut cursor, &mut admitted)?;
                }
                GeneratorExpressionSuspensionStep::PatternInitializer(source) => {
                    source.append(&mut cursor, &mut admitted)?;
                }
            }
        }
        points.extend(admitted);
        *current_state = cursor;
        Some(())
    }

    fn expression(
        &mut self,
        source: &'ast Expression,
        admission: GeneratorValueBranchAdmission,
    ) -> Option<()> {
        if contains(source, ContainsSymbol::AwaitExpression) {
            return None;
        }
        if !contains(source, ContainsSymbol::YieldExpression) {
            return Some(());
        }
        match source {
            Expression::Parenthesized(group) => self.expression(group.expression(), admission),
            Expression::Optional(source)
                if matches!(
                    admission,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops
                ) =>
            {
                self.steps
                    .push(GeneratorExpressionSuspensionStep::OptionalChain(
                        GeneratorOptionalChainSource::new(source)?,
                    ));
                Some(())
            }
            source @ Expression::Conditional(_)
                if matches!(
                    admission,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops
                ) =>
            {
                self.steps
                    .push(GeneratorExpressionSuspensionStep::ValueBranch(
                        GeneratorValueBranchSource::new(source)?,
                    ));
                Some(())
            }
            source @ Expression::Binary(binary) if matches!(binary.op(), BinaryOp::Logical(_)) => {
                if !matches!(
                    admission,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops
                ) {
                    return None;
                }
                self.steps
                    .push(GeneratorExpressionSuspensionStep::ValueBranch(
                        GeneratorValueBranchSource::new(source)?,
                    ));
                Some(())
            }
            Expression::Binary(source) => {
                self.expression(source.lhs(), admission)?;
                self.expression(source.rhs(), admission)
            }
            Expression::Unary(source) if source.op() == UnaryOp::Delete => {
                match CheckedGeneratorDeleteSource::new(source.target())?.into_operand() {
                    GeneratorDeleteOperand::Property { access, .. } => {
                        self.expression(access.target(), admission)?;
                        self.key(access.field(), admission)
                    }
                    GeneratorDeleteOperand::Super(access) => self.key(access.field(), admission),
                    GeneratorDeleteOperand::Optional(source) => {
                        if !matches!(
                            admission,
                            GeneratorValueBranchAdmission::OrdinaryOutsideLoops
                        ) {
                            return None;
                        }
                        self.steps
                            .push(GeneratorExpressionSuspensionStep::OptionalChain(
                                source.into_chain(),
                            ));
                        Some(())
                    }
                    GeneratorDeleteOperand::Value(source) => self.expression(source, admission),
                    GeneratorDeleteOperand::AwaitedOptional(_) => None,
                }
            }
            Expression::Unary(source) => self.expression(source.target(), admission),
            Expression::TemplateLiteral(source) => {
                for element in source.elements() {
                    if let TemplateElement::Expr(source) = element {
                        self.expression(source, admission)?;
                    }
                }
                Some(())
            }
            Expression::Yield(source) => {
                if let Some(target) = source.target() {
                    self.expression(target, admission)?;
                }
                self.steps
                    .push(GeneratorExpressionSuspensionStep::LinearYield);
                Some(())
            }
            Expression::Call(source) => {
                self.invocation_reference(source.function(), admission)?;
                self.arguments(source.args(), admission)
            }
            Expression::New(source) => {
                self.constructor_value(source.constructor(), admission)?;
                self.arguments(source.arguments(), admission)
            }
            Expression::TaggedTemplate(source) => {
                self.invocation_reference(source.tag(), admission)?;
                self.arguments(source.exprs(), admission)
            }
            Expression::PropertyAccess(PropertyAccess::Simple(source)) => {
                self.expression(source.target(), admission)?;
                self.key(source.field(), admission)
            }
            Expression::PropertyAccess(PropertyAccess::Private(source)) => {
                self.expression(source.target(), admission)
            }
            Expression::PropertyAccess(PropertyAccess::Super(source)) => {
                self.key(source.field(), admission)
            }
            Expression::BinaryInPrivate(source) => self.expression(source.rhs(), admission),
            Expression::Update(source) => match source.target() {
                UpdateTarget::Identifier(_) => Some(()),
                UpdateTarget::PropertyAccess(source) => self.property_reference(source, admission),
                UpdateTarget::WebCompatCall(source) => {
                    self.invocation_reference(source.function(), admission)?;
                    self.arguments(source.args(), admission)
                }
            },
            Expression::ImportCall(source) => {
                self.expression(source.argument(), admission)?;
                if let Some(options) = source.options() {
                    self.expression(options, admission)?;
                }
                Some(())
            }
            Expression::Assign(source)
                if matches!(source.lhs(), AssignTarget::WebCompatCall(_)) =>
            {
                let AssignTarget::WebCompatCall(call) = source.lhs() else {
                    unreachable!()
                };
                // The original Annex B Call completes and throws ReferenceError
                // before the assignment RHS can be evaluated.
                self.invocation_reference(call.function(), admission)?;
                self.arguments(call.args(), admission)
            }
            Expression::Assign(source)
                if matches!(
                    admission,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops
                ) && CheckedGeneratorCompoundAssignmentSource::new(source).is_some() =>
            {
                let source = CheckedGeneratorCompoundAssignmentSource::new(source)?;
                // Base and raw key evaluation finish before the selected RHS
                // owner allocates its branch entry and continuation ranges.
                self.assignment_reference(source.lhs(), admission)?;
                if source.logical_operation().is_some() {
                    // Validate the complete RHS before admitting a selected owner.
                    GeneratorExpressionSourcePlan::new(source.rhs(), admission)?;
                    self.steps
                        .push(GeneratorExpressionSuspensionStep::LogicalAssignment(source));
                    Some(())
                } else {
                    self.expression(source.rhs(), admission)
                }
            }
            Expression::Assign(source) if source.op() == AssignOp::Assign => {
                if matches!(source.lhs(), AssignTarget::Pattern(_)) {
                    let source = GeneratorPatternAssignmentSource::new(source, admission)?;
                    self.steps
                        .push(GeneratorExpressionSuspensionStep::PatternAssignment(source));
                    return Some(());
                }
                if matches!(source.lhs(), AssignTarget::Identifier(_))
                    && matches!(
                        admission,
                        GeneratorValueBranchAdmission::OrdinaryOutsideLoops
                    )
                {
                    return self.expression(source.rhs(), admission);
                }
                let AssignTarget::Access(target) = source.lhs() else {
                    return None;
                };
                self.property_reference(target, admission)?;
                self.expression(source.rhs(), admission)
            }
            Expression::ArrayLiteral(source) => {
                for element in source.as_ref().iter().flatten() {
                    let element = match element {
                        Expression::Spread(spread) => spread.target(),
                        value => value,
                    };
                    self.expression(element, admission)?;
                }
                Some(())
            }
            Expression::ObjectLiteral(source) => {
                for property in source.properties() {
                    let (key, value) = generator_object_property_operands(property)?;
                    for source in key.into_iter().chain(value) {
                        self.expression(source, admission)?;
                    }
                }
                Some(())
            }
            Expression::ClassExpression(source) => {
                for expression in
                    class_evaluation_expressions(source.super_ref(), source.elements())
                {
                    self.expression(expression, GeneratorValueBranchAdmission::LinearOnly)?;
                }
                Some(())
            }
            _ => None,
        }
    }

    fn assignment_reference(
        &mut self,
        source: &'ast AssignTarget,
        admission: GeneratorValueBranchAdmission,
    ) -> Option<()> {
        match source {
            AssignTarget::Identifier(_) => Some(()),
            AssignTarget::Access(source) => self.property_reference(source, admission),
            AssignTarget::Pattern(_) | AssignTarget::WebCompatCall(_) => None,
        }
    }

    fn property_reference(
        &mut self,
        source: &'ast PropertyAccess,
        admission: GeneratorValueBranchAdmission,
    ) -> Option<()> {
        match source {
            PropertyAccess::Simple(source) => {
                self.expression(source.target(), admission)?;
                self.key(source.field(), admission)
            }
            PropertyAccess::Private(source) => self.expression(source.target(), admission),
            PropertyAccess::Super(source) => self.key(source.field(), admission),
        }
    }

    fn key(
        &mut self,
        source: &'ast PropertyAccessField,
        admission: GeneratorValueBranchAdmission,
    ) -> Option<()> {
        match source {
            PropertyAccessField::Const(_) => Some(()),
            PropertyAccessField::Expr(source) => self.expression(source, admission),
        }
    }

    fn invocation_reference(
        &mut self,
        source: &'ast Expression,
        admission: GeneratorValueBranchAdmission,
    ) -> Option<()> {
        if let Some(source) = GeneratorGroupedOptionalInvocationSource::new(source, admission) {
            // The complete conditional chain, rather than a flattened count,
            // owns its guarded yields and joins before unconditional operands.
            self.steps
                .push(GeneratorExpressionSuspensionStep::OptionalChain(
                    source.into_chain(),
                ));
            return Some(());
        }
        match source {
            Expression::Parenthesized(group) => {
                self.invocation_reference(group.expression(), admission)
            }
            Expression::PropertyAccess(PropertyAccess::Private(source)) => {
                self.expression(source.target(), admission)
            }
            Expression::PropertyAccess(PropertyAccess::Super(source)) => {
                self.key(source.field(), admission)
            }
            Expression::Optional(source) if contains(source, ContainsSymbol::YieldExpression) => {
                None
            }
            source => self.expression(source, admission),
        }
    }

    /// Construct consumes a callee Value, so a completed optional chain may
    /// supply it. Preserve the existing private/super Reference capture walks
    /// for all other constructor forms instead of replacing their admission.
    fn constructor_value(
        &mut self,
        source: &'ast Expression,
        admission: GeneratorValueBranchAdmission,
    ) -> Option<()> {
        match source {
            Expression::Parenthesized(group) => {
                self.constructor_value(group.expression(), admission)
            }
            Expression::Optional(_) => self.expression(source, admission),
            source => self.invocation_reference(source, admission),
        }
    }

    fn arguments(
        &mut self,
        source: &'ast [Expression],
        admission: GeneratorValueBranchAdmission,
    ) -> Option<()> {
        for argument in source {
            let source = match argument {
                Expression::Spread(spread) => spread.target(),
                value => value,
            };
            self.expression(source, admission)?;
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_optional_expression<R>(
        source: &str,
        consume: impl for<'ast> FnOnce(&'ast Expression) -> R,
    ) -> R {
        let parsed = scriptw_front::parse(source, scriptw_front::ParseOptions::script())
            .expect("finite generator source parses");
        parsed
            .as_script()
            .expect("script")
            .with_compiler_session(|script, _| {
                struct Find<'ast>(Option<&'ast Expression>);
                impl<'ast> Visitor<'ast> for Find<'ast> {
                    type BreakTy = ();
                    fn visit_expression(&mut self, source: &'ast Expression) -> ControlFlow<()> {
                        if matches!(source, Expression::Optional(_)) {
                            self.0 = Some(source);
                            return ControlFlow::Break(());
                        }
                        source.visit_with(self)
                    }
                }
                let mut found = Find(None);
                let _ = script.visit_with(&mut found);
                consume(found.0.expect("actual optional source"))
            })
    }

    #[test]
    fn optional_operands_share_exact_ordered_scalar_layouts_with_the_source_plan() {
        with_optional_expression(
            "function* values(base, argument) { return base?.[yield 1](argument, yield 2, ...(yield 3)).value; }",
            |expression| {
                let Expression::Optional(optional) = expression else { unreachable!() };
                let source = GeneratorOptionalChainSource::new(optional).expect("complete chain");
                assert!(source.states(7).unwrap().finish().is_none(), "unfinished cursor cannot publish");
                let mut states = source.states(7).unwrap();
                assert_eq!(states.exit(), 19);
                let mut layouts = Vec::new();
                while let Some(layout) = states.next() {
                    layouts.push((layout.entry(), layout.then_arm().entry, layout.then_arm().end,
                        layout.else_arm().entry, layout.else_arm().end, layout.exit()));
                }
                assert_eq!(layouts, [(7, 8, 8, 9, 10, 11), (11, 12, 12, 13, 14, 15), (15, 16, 16, 17, 18, 19)]);
                assert_eq!(states.finish(), Some(19));
                let (_, links) = source.into_parts();
                let mut links = links.into_iter();
                let GeneratorOptionalChainLink::Property(key) = links.next().unwrap() else { panic!("first key") };
                let (GeneratorOptionalKeySource::Computed(key), shorted) = key.into_parts() else { panic!("computed key") };
                assert!(shorted);
                assert!(key.suspends());
                let GeneratorOptionalChainLink::Call(call) = links.next().unwrap() else { panic!("actual Call") };
                let (syntax, arguments, shorted) = call.into_parts();
                assert!(!shorted);
                assert_eq!(syntax.len(), arguments.len());
                assert_eq!(arguments.iter().map(|argument| match argument {
                    GeneratorOptionalArgumentSource::Value(source) | GeneratorOptionalArgumentSource::Spread(source) => source.suspends(),
                }).collect::<Vec<_>>(), [false, true, true]);
                assert!(matches!(arguments.last(), Some(GeneratorOptionalArgumentSource::Spread(_))));
                assert!(matches!(syntax.last(), Some(Expression::Spread(_))));
                assert!(matches!(links.next(), Some(GeneratorOptionalChainLink::Property(_))));
                assert!(links.next().is_none());
                let mut current = 7;
                let mut points = Vec::new();
                GeneratorExpressionSourcePlan::new(expression, GeneratorValueBranchAdmission::OrdinaryOutsideLoops)
                    .unwrap().append(&mut current, &mut points).unwrap();
                assert_eq!(current, 19);
                assert_eq!(points.iter().map(|point| (point.suspend_state, point.resume_state)).collect::<Vec<_>>(),
                    [(9, 10), (13, 14), (17, 18)]);
                assert!(GeneratorExpressionSourcePlan::new(expression, GeneratorValueBranchAdmission::LinearOnly).is_none());
            },
        );
    }

    #[test]
    fn optional_chain_overflow_is_atomic_and_final_state_count_is_representable() {
        with_optional_expression(
            "function* values(base) { return base?.[yield 1]; }",
            |expression| {
                let Expression::Optional(optional) = expression else {
                    unreachable!()
                };
                let source = GeneratorOptionalChainSource::new(optional).unwrap();
                let mut states = source
                    .states(u32::MAX - 5)
                    .expect("largest complete one-operand layout");
                assert_eq!(states.next().unwrap().exit(), u32::MAX - 1);
                let exit = states.finish().unwrap();
                assert_eq!(
                    finish_generator_plan(exit, Vec::new()).unwrap().state_count,
                    u32::MAX
                );
                assert!(
                    source.states(u32::MAX - 4).is_none(),
                    "fitting joins cannot overflow final state count"
                );
            },
        );
        with_optional_expression(
            "function* values(base) { return base?.[yield 1](yield 2); }",
            |expression| {
                let existing = GeneratorSuspensionPointIr {
                    suspend_state: 0,
                    resume_state: 1,
                };
                let mut points = vec![existing.clone()];
                let mut current = u32::MAX - 4;
                let plan = GeneratorExpressionSourcePlan::new(
                    expression,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
                )
                .unwrap();
                assert!(plan.append(&mut current, &mut points).is_none());
                assert_eq!(current, u32::MAX - 4);
                assert_eq!(points, [existing]);
            },
        );
    }

    #[test]
    fn optional_source_retains_complete_base_link_and_operand_suspensions() {
        for (source, admitted) in [
            ("function* values(base) { return base?.method(1); }", false),
            (
                "function* values(base) { return (yield base)?.method(yield 1); }",
                true,
            ),
            (
                "function* values(base) { return base?.method((yield 1) + (yield 2)); }",
                true,
            ),
            (
                "function* values(base, flag) { return base?.method(flag ? (yield 1) : 0); }",
                true,
            ),
            (
                "function* values(base) { return base?.method(yield* [1]); }",
                true,
            ),
            (
                "function* values(base) { return base?.method(yield (yield 1)); }",
                true,
            ),
            (
                "class C { #value; *values(base) { return base?.[yield 1].#value; } }",
                true,
            ),
            (
                "class C extends Object { *values() { return super.method?.(yield 1); } }",
                true,
            ),
            (
                "async function* values(base) { return base?.method(yield await 1); }",
                false,
            ),
        ] {
            with_optional_expression(source, |expression| {
                let Expression::Optional(optional) = expression else {
                    unreachable!()
                };
                assert_eq!(
                    GeneratorOptionalChainSource::new(optional).is_some(),
                    admitted,
                    "{source}"
                );
            });
        }
    }

    #[test]
    fn selected_resumes_reserve_a_fresh_join_even_at_the_state_limit() {
        with_optional_expression(
            "function* values(base) { return base?.[yield 1]; }",
            |expression| {
                let Expression::Optional(optional) = expression else {
                    unreachable!()
                };
                let source = GeneratorOptionalChainSource::new(optional).unwrap();
                let (_, links) = source.into_parts();
                let GeneratorOptionalChainLink::Property(key) = links.into_iter().next().unwrap()
                else {
                    unreachable!()
                };
                let (GeneratorOptionalKeySource::Computed(key), _) = key.into_parts() else {
                    unreachable!()
                };
                let states = GeneratorValueRegionStates::new(
                    u32::MAX - 6,
                    Some(key.source()),
                    Some(key.source()),
                )
                .expect("distinct complete arms and fresh join fit");
                assert_eq!(states.then_arm().entry, u32::MAX - 5);
                assert_eq!(states.then_arm().end, u32::MAX - 4);
                assert_eq!(states.else_arm().entry, u32::MAX - 3);
                assert_eq!(states.else_arm().end, u32::MAX - 2);
                assert_eq!(states.exit(), u32::MAX - 1);
                assert_eq!(
                    finish_generator_plan(states.exit(), Vec::new())
                        .unwrap()
                        .state_count,
                    u32::MAX
                );
                assert!(GeneratorValueRegionStates::new(
                    u32::MAX - 5,
                    Some(key.source()),
                    Some(key.source())
                )
                .is_none());
                assert!(
                    GeneratorValueRegionStates::new(u32::MAX - 4, None, Some(key.source()))
                        .is_none()
                );
                let eager = GeneratorValueRegionStates::new(0, None, None).unwrap();
                assert_eq!((eager.then_arm().entry, eager.then_arm().end), (1, 1));
                assert_eq!((eager.else_arm().entry, eager.else_arm().end), (2, 2));
                assert_eq!(eager.exit(), 3);
            },
        );
    }

    #[test]
    fn source_append_does_not_publish_a_partial_prefix_on_overflow() {
        let plan = GeneratorExpressionSourcePlan {
            steps: vec![
                GeneratorExpressionSuspensionStep::LinearYield,
                GeneratorExpressionSuspensionStep::LinearYield,
            ],
        };
        let mut cursor = u32::MAX - 1;
        let existing = GeneratorSuspensionPointIr {
            suspend_state: 0,
            resume_state: 1,
        };
        let mut points = vec![existing.clone()];
        assert!(plan.append(&mut cursor, &mut points).is_none());
        assert_eq!(cursor, u32::MAX - 1);
        assert_eq!(points, [existing]);
    }
}
