use super::*;
use boa_ast::StatementList;

pub(crate) use crate::async_generator_source::{
    AsyncGeneratorClassicLoopSource, AsyncGeneratorExpressionSource, AsyncGeneratorFunctionSource,
    AsyncGeneratorIfSource, AsyncGeneratorSourceDomain, AsyncGeneratorSourceError,
    AsyncGeneratorSourceRange,
};

pub(crate) use crate::async_pattern_source::{
    append_async_expression_states, AsyncArrayPatternSource, AsyncArrayPatternSourceStates,
    AsyncObjectPatternSource, AsyncPatternDefaultStates, AsyncPatternSource,
};

pub(crate) use crate::async_with_source::{
    append_async_statement_protocol_items, append_async_statement_protocol_states,
    contains_plain_async_phase, AsyncWithSource,
    AsyncWithSourceStates,
};

#[path = "generator_with_source.rs"]
mod generator_with_source;
pub(crate) use generator_with_source::{GeneratorWithSource, GeneratorWithSourceStates};
#[path = "generator_region_presence.rs"]
mod generator_region_presence;
pub(crate) use generator_region_presence::contains_ordinary_generator_phase_owner;
#[path = "generator_for_in_source.rs"]
mod generator_for_in_source;
pub(crate) use generator_for_in_source::{
    mixed_for_in_head_mode_and_kind, ForInSourceIdentity, GeneratorForInHeadKind,
    GeneratorForInHeadProof,
};

pub(crate) use crate::generator_value_branch_source::{
    CheckedGeneratorCompoundAssignmentSource, CheckedGeneratorDeleteSource,
    GeneratorArrayPatternSource, GeneratorArrayPatternSourceStates,
    GeneratorCompoundAssignmentOperation, GeneratorDeleteOperand, GeneratorExpressionSourcePlan,
    GeneratorGroupedOptionalInvocationSource, GeneratorGroupedOptionalReferenceSource,
    GeneratorObjectPatternSource, GeneratorOptionalArgumentSource,
    GeneratorOptionalChainLink, GeneratorOptionalChainSource, GeneratorOptionalChainStates,
    GeneratorOptionalDeleteSource, GeneratorOptionalDeleteTerminal, GeneratorOptionalKeySource,
    GeneratorOptionalOperandSource, GeneratorPatternAssignmentSource,
    GeneratorPatternInitializerSource, GeneratorValueBranchAdmission, GeneratorValueBranchKind,
    GeneratorValueBranchSource,
};

#[path = "generator_switch_source.rs"]
mod generator_switch_source;
pub(crate) use generator_switch_source::{
    append_generator_switch_suspensions, CheckedEmptyStatementCompletionSource,
    CheckedGeneratorSwitchSource, GeneratorSwitchSourceStates,
};

#[path = "generator_source_plan.rs"]
mod generator_source_plan;
pub(crate) use generator_source_plan::{
    append_generator_for_of_suspensions, append_generator_region_suspensions,
};
pub(crate) use generator_source_plan::{
    generator_for_of_source_shape_is_supported, GeneratorSuspensionRegion,
};

/// The normal Number/BigInt results an already-inferred primitive can produce
/// through ToNumeric. An untracked object can observably produce either kind.
pub(crate) fn numeric_domain(primitive: Option<&ValueInfo>) -> (bool, bool) {
    match primitive {
        Some(info) => {
            let has_number = [
                ValueKind::Undefined,
                ValueKind::Null,
                ValueKind::Boolean,
                ValueKind::Number,
                ValueKind::String,
            ]
            .into_iter()
            .any(|kind| info.possible_kinds.contains(kind));
            let has_bigint = info.possible_kinds.contains(ValueKind::BigInt);
            (has_number, has_bigint)
        }
        None => (true, true),
    }
}

/// Splices out the scope wrappers that hoisting an `await`/`yield` operand puts
/// around a loop body, so the suspension becomes a direct statement again.
///
/// `t += await p` lowers to
/// `LexicalBlock([Lexical $async.await.0, AsyncAwait, Expression(t += …)])`, and
/// `const v = await p` adds a second `Lexical` after the suspension. Both hide
/// the `AsyncAwait` one level down, where `split_resumable_loop_body` cannot see
/// it — the loop would then fall back to a straight-line `StatementIr::For`
/// holding a suspension the resumable dispatcher can never re-enter.
///
/// Only blocks that actually contain a suspension are flattened, so ordinary
/// nested scopes keep their structure. Flattening is safe for the ones that do:
/// `StatementIr::LexicalBlock` is a flat statement list in every backend (it
/// allocates bindings, it does not materialize an environment), and the names it
/// binds are already uniquified by the lowerer, so hoisting them into the loop
/// body scope cannot collide (ECMA-262 14.7 / 8.6 per-iteration bindings).
pub(crate) fn flatten_suspending_lexical_blocks(statements: Vec<StatementIr>) -> Vec<StatementIr> {
    if !statements.iter().any(block_contains_direct_suspension) {
        return statements;
    }
    let mut flattened = Vec::with_capacity(statements.len());
    for statement in statements {
        match statement {
            StatementIr::LexicalBlock(inner) if inner.iter().any(is_direct_suspension) => {
                flattened.extend(flatten_suspending_lexical_blocks(inner));
            }
            StatementIr::Block(block)
                if block.lexical_environment.is_none()
                    && block.statements.iter().any(is_direct_suspension) =>
            {
                flattened.extend(flatten_suspending_lexical_blocks(block.statements));
            }
            statement => flattened.push(statement),
        }
    }
    flattened
}

fn is_direct_suspension(statement: &StatementIr) -> bool {
    matches!(
        statement,
        StatementIr::GeneratorYield { .. }
            | StatementIr::AsyncAwait { .. }
            | StatementIr::AsyncModuleInstantiation
            | StatementIr::ResumableClassDefinition(_)
    )
}

fn block_contains_direct_suspension(statement: &StatementIr) -> bool {
    match statement {
        StatementIr::LexicalBlock(inner) => inner.iter().any(is_direct_suspension),
        StatementIr::Block(block) if block.lexical_environment.is_none() => {
            block.statements.iter().any(is_direct_suspension)
        }
        _ => false,
    }
}

pub(crate) fn function_name(
    interner: &Interner,
    function: &FunctionDeclaration,
    fallback: Option<&str>,
) -> String {
    fallback
        .map(ToString::to_string)
        .unwrap_or_else(|| interner.resolve_expect(function.name().sym()).to_string())
}

pub(crate) fn collect_simple_parameter_names(
    interner: &Interner,
    parameters: &FormalParameterList,
) -> Vec<String> {
    let mut names = Vec::with_capacity(parameters.as_ref().len());
    let mut seen = BTreeSet::new();
    for parameter in parameters.as_ref() {
        let Binding::Identifier(identifier) = parameter.variable().binding() else {
            return Vec::new();
        };
        if parameter.init().is_some() || parameter.is_rest_param() {
            return Vec::new();
        }
        let name = interner.resolve_expect(identifier.sym()).to_string();
        if seen.insert(name.clone()) {
            names.push(name);
        }
    }
    names
}

pub(crate) fn binding_parameter_storage_name(
    interner: &Interner,
    binding: &Binding,
    index: usize,
) -> String {
    match binding {
        Binding::Identifier(identifier) => interner.resolve_expect(identifier.sym()).to_string(),
        Binding::Pattern(_) => format!("$destructured.param.{index}"),
    }
}

pub(crate) fn collect_binding_names(
    interner: &Interner,
    binding: &Binding,
    names: &mut Vec<String>,
) {
    match binding {
        Binding::Identifier(identifier) => {
            names.push(interner.resolve_expect(identifier.sym()).to_string());
        }
        Binding::Pattern(Pattern::Object(pattern)) => {
            for element in pattern.bindings() {
                match element {
                    ObjectPatternElement::SingleName { ident, .. }
                    | ObjectPatternElement::RestProperty { ident } => {
                        names.push(interner.resolve_expect(ident.sym()).to_string());
                    }
                    ObjectPatternElement::Pattern { pattern, .. } => {
                        collect_binding_names(interner, &Binding::Pattern(pattern.clone()), names);
                    }
                    ObjectPatternElement::AssignmentPropertyAccess { .. }
                    | ObjectPatternElement::AssignmentRestPropertyAccess { .. } => {}
                }
            }
        }
        Binding::Pattern(Pattern::Array(pattern)) => {
            for element in pattern.bindings() {
                match element {
                    ArrayPatternElement::SingleName { ident, .. }
                    | ArrayPatternElement::SingleNameRest { ident } => {
                        names.push(interner.resolve_expect(ident.sym()).to_string());
                    }
                    ArrayPatternElement::Pattern { pattern, .. }
                    | ArrayPatternElement::PatternRest { pattern } => {
                        collect_binding_names(interner, &Binding::Pattern(pattern.clone()), names);
                    }
                    ArrayPatternElement::Elision
                    | ArrayPatternElement::PropertyAccess { .. }
                    | ArrayPatternElement::PropertyAccessRest { .. } => {}
                }
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct SupportedBoundName {
    pub(crate) source_name: String,
    pub(crate) span: boa_ast::Span,
}

pub(crate) fn supported_bound_names(
    interner: &Interner,
    binding: &Binding,
) -> Option<Vec<SupportedBoundName>> {
    // Nested array/object patterns and object rest properties all bind names, so the
    // walk has to recurse through both pattern shapes (ECMA-262 8.6 BoundNames).
    //
    // BoundNames is a purely syntactic function of the *binding* positions: a
    // computed property key (`{ [k]: v }`) contributes no bound name and does not
    // change the names bound by the rest of the pattern, so the key shape is
    // deliberately not inspected here. Whether a key can be *lowered* is decided by
    // the pattern lowering, not by this walk.
    fn collect<'a>(
        pattern: &'a Pattern,
        identifiers: &mut Vec<&'a boa_ast::expression::Identifier>,
    ) -> Option<()> {
        match pattern {
            Pattern::Object(pattern) => {
                for element in pattern.bindings() {
                    match element {
                        ObjectPatternElement::SingleName { ident, .. } => {
                            identifiers.push(ident);
                        }
                        ObjectPatternElement::RestProperty { ident } => identifiers.push(ident),
                        ObjectPatternElement::Pattern { pattern, .. } => {
                            collect(pattern, identifiers)?;
                        }
                        ObjectPatternElement::AssignmentPropertyAccess { .. }
                        | ObjectPatternElement::AssignmentRestPropertyAccess { .. } => return None,
                    }
                }
            }
            Pattern::Array(pattern) => {
                for element in pattern.bindings() {
                    match element {
                        ArrayPatternElement::SingleName { ident, .. }
                        | ArrayPatternElement::SingleNameRest { ident } => identifiers.push(ident),
                        ArrayPatternElement::Pattern { pattern, .. }
                        | ArrayPatternElement::PatternRest { pattern } => {
                            collect(pattern, identifiers)?;
                        }
                        ArrayPatternElement::Elision => {}
                        ArrayPatternElement::PropertyAccess { .. }
                        | ArrayPatternElement::PropertyAccessRest { .. } => return None,
                    }
                }
            }
        }
        Some(())
    }

    let identifiers = match binding {
        Binding::Identifier(identifier) => vec![identifier],
        Binding::Pattern(pattern) => {
            let mut identifiers = Vec::new();
            collect(pattern, &mut identifiers)?;
            identifiers
        }
    };

    Some(
        identifiers
            .into_iter()
            .map(|identifier| SupportedBoundName {
                source_name: interner.resolve_expect(identifier.sym()).to_string(),
                span: identifier.span(),
            })
            .collect(),
    )
}

pub(crate) fn function_declaration_key(function: &FunctionDeclaration) -> String {
    let span = function.linear_span();
    format!(
        "function-declaration:{}:{}",
        span.start().pos(),
        span.end().pos()
    )
}

pub(crate) fn generator_declaration_key(function: &GeneratorDeclaration) -> String {
    let span = function.linear_span();
    format!(
        "generator-declaration:{}:{}",
        span.start().pos(),
        span.end().pos()
    )
}

pub(crate) fn async_function_declaration_key(function: &AsyncFunctionDeclaration) -> String {
    let span = function.linear_span();
    format!(
        "async-function-declaration:{}:{}",
        span.start().pos(),
        span.end().pos()
    )
}

pub(crate) fn async_generator_declaration_key(function: &AsyncGeneratorDeclaration) -> String {
    let span = function.linear_span();
    format!(
        "async-generator-declaration:{}:{}",
        span.start().pos(),
        span.end().pos()
    )
}

/// Function declarations selected by Block/CaseBlock lexical instantiation.
/// Keep this separate from the Annex B declaration-candidate syntax walker.
pub(crate) fn lexically_scoped_statement_list_item_function_declaration(
    item: &StatementListItem,
) -> Option<&FunctionDeclaration> {
    match item {
        StatementListItem::Declaration(declaration) => match declaration.as_ref() {
            Declaration::FunctionDeclaration(function) => Some(function),
            _ => None,
        },
        StatementListItem::Statement(statement) => match statement.as_ref() {
            Statement::Labelled(labelled) => {
                lexically_scoped_labelled_function_declaration(labelled)
            }
            _ => None,
        },
    }
}

/// Every HoistableDeclaration participating in BlockDeclarationInstantiation.
/// Keeping the four source forms together prevents a block environment or its
/// initialization pass from quietly treating only ordinary functions as names.
pub(crate) enum BlockHoistableDeclaration<'a> {
    Ordinary(&'a FunctionDeclaration),
    Generator(&'a GeneratorDeclaration),
    Async(&'a AsyncFunctionDeclaration),
    AsyncGenerator(&'a AsyncGeneratorDeclaration),
}

impl<'a> BlockHoistableDeclaration<'a> {
    pub(crate) fn from_item(item: &'a StatementListItem) -> Option<Self> {
        match item {
            StatementListItem::Declaration(declaration) => match declaration.as_ref() {
                Declaration::FunctionDeclaration(function) => Some(Self::Ordinary(function)),
                Declaration::GeneratorDeclaration(function) => Some(Self::Generator(function)),
                Declaration::AsyncFunctionDeclaration(function) => Some(Self::Async(function)),
                Declaration::AsyncGeneratorDeclaration(function) => {
                    Some(Self::AsyncGenerator(function))
                }
                Declaration::Lexical(_) | Declaration::ClassDeclaration(_) => None,
            },
            StatementListItem::Statement(statement) => match statement.as_ref() {
                Statement::Labelled(labelled) => {
                    lexically_scoped_labelled_function_declaration(labelled).map(Self::Ordinary)
                }
                _ => None,
            },
        }
    }

    pub(crate) fn source_name(&self, interner: &Interner) -> String {
        let symbol = match self {
            Self::Ordinary(function) => function.name().sym(),
            Self::Generator(function) => function.name().sym(),
            Self::Async(function) => function.name().sym(),
            Self::AsyncGenerator(function) => function.name().sym(),
        };
        interner.resolve_expect(symbol).to_string()
    }

    pub(crate) fn name_span(&self) -> boa_ast::Span {
        match self {
            Self::Ordinary(function) => function.name().span(),
            Self::Generator(function) => function.name().span(),
            Self::Async(function) => function.name().span(),
            Self::AsyncGenerator(function) => function.name().span(),
        }
    }

    pub(crate) fn ordinary(&self) -> Option<&'a FunctionDeclaration> {
        match self {
            Self::Ordinary(function) => Some(function),
            Self::Generator(_) | Self::Async(_) | Self::AsyncGenerator(_) => None,
        }
    }
}

pub(crate) fn annex_b_block_storage_name(
    function: &FunctionDeclaration,
    source_name: &str,
) -> String {
    let span = function.linear_span();
    format!(
        "$annexb.block.{}.{}.{}",
        span.start().pos(),
        span.end().pos(),
        source_name
    )
}

pub(crate) fn scoped_lexical_binding_storage_name(
    source_name: &str,
    span: boa_ast::Span,
) -> String {
    format!(
        "$scoped.lex.{}.{}.{}.{}.{}",
        span.start().line_number(),
        span.start().column_number(),
        span.end().line_number(),
        span.end().column_number(),
        source_name
    )
}

pub(crate) fn class_name_binding_storage_name(source_name: &str, span: boa_ast::Span) -> String {
    format!(
        "$class.name.{}.{}.{}.{}.{}",
        span.start().line_number(),
        span.start().column_number(),
        span.end().line_number(),
        span.end().column_number(),
        source_name
    )
}

pub(crate) fn is_class_name_binding_storage_name(storage_name: &str) -> bool {
    storage_name.starts_with("$class.name.")
}

pub(crate) fn is_supported_parameter_binding(binding: &Binding) -> bool {
    fn is_supported_pattern(pattern: &Pattern) -> bool {
        match pattern {
            Pattern::Object(pattern) => pattern.bindings().iter().all(|element| match element {
                ObjectPatternElement::SingleName { .. }
                | ObjectPatternElement::RestProperty { .. } => true,
                ObjectPatternElement::Pattern { pattern, .. } => is_supported_pattern(pattern),
                ObjectPatternElement::AssignmentPropertyAccess { .. }
                | ObjectPatternElement::AssignmentRestPropertyAccess { .. } => false,
            }),
            Pattern::Array(pattern) => pattern.bindings().iter().all(|element| match element {
                ArrayPatternElement::Elision
                | ArrayPatternElement::SingleName { .. }
                | ArrayPatternElement::SingleNameRest { .. } => true,
                ArrayPatternElement::Pattern { pattern, .. }
                | ArrayPatternElement::PatternRest { pattern } => is_supported_pattern(pattern),
                ArrayPatternElement::PropertyAccess { .. }
                | ArrayPatternElement::PropertyAccessRest { .. } => false,
            }),
        }
    }

    match binding {
        Binding::Identifier(_) => true,
        Binding::Pattern(pattern) => is_supported_pattern(pattern),
    }
}

pub(crate) fn function_expression_key(function: &FunctionExpression) -> String {
    if let Some(span) = function.linear_span() {
        return format!("linear:{}:{}", span.start().pos(), span.end().pos());
    }
    let span = function.span();
    format!(
        "span:{}:{}:{}:{}",
        span.start().line_number(),
        span.start().column_number(),
        span.end().line_number(),
        span.end().column_number()
    )
}

pub(crate) fn generator_expression_key(function: &GeneratorExpression) -> String {
    let span = function.linear_span();
    format!(
        "generator-expression:{}:{}",
        span.start().pos(),
        span.end().pos()
    )
}

pub(crate) fn async_function_expression_key(function: &AsyncFunctionExpression) -> String {
    let span = function.linear_span();
    format!(
        "async-function-expression:{}:{}",
        span.start().pos(),
        span.end().pos()
    )
}

pub(crate) fn async_generator_expression_key(function: &AsyncGeneratorExpression) -> String {
    let span = function.linear_span();
    format!(
        "async-generator-expression:{}:{}",
        span.start().pos(),
        span.end().pos()
    )
}

pub(crate) fn generator_body_has_no_suspension(body: &FunctionBody) -> bool {
    !contains(body, ContainsSymbol::YieldExpression)
}

pub(crate) fn async_generator_resumable_plan(
    body: &FunctionBody,
) -> Result<ResumablePlanIr, AsyncGeneratorSourceError> {
    AsyncGeneratorFunctionSource::new(body).map(AsyncGeneratorFunctionSource::into_resumable_plan)
}

/// A source suspension shape that the generator state plan cannot represent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeneratorPlanRejection {
    /// A resume, join or final exclusive state count exceeds the state word.
    StateOverflow,
    /// A nested declaration (a function, class or lexical declaration at the top
    /// level of the body) contains a yield.
    YieldInDeclaration,
    /// A statement-level `yield`, or `x = yield …`, whose operand itself hides
    /// further suspensions the linear walk cannot order.
    YieldOperandNotDirect,
    /// `return <expression containing a yield>` whose expression cannot be
    /// staged into a sequence of suspensions.
    ReturnOperandNotStageable,
    /// `throw <expression containing a yield>` whose complete value has no
    /// source-owned continuation in the current region.
    ThrowOperandNotStageable,
    /// An expression statement whose value is discarded but whose yields cannot
    /// be flattened into a sequence.
    DiscardedYieldExpression,
    /// A bare block whose yields cannot be flattened into a sequence.
    DiscardedYieldBlock,
    /// The complete ordinary With head/body requires another continuation owner.
    WithSourceShape,
    /// The original ForIn head/body needs a distinct continuation owner.
    ForInSourceShape,
    /// A classic iteration head or body needs a different suspended expression,
    /// binding, iterator/resource or control owner than its admitted phases.
    ClassicLoopShape,
    /// `if (<condition containing a yield>)`.
    YieldInIfCondition,
    /// An `if` branch whose yields are not a countable direct sequence.
    IfBranchYieldNotDirect,
    /// A `try`, `catch` or `finally` block whose yields are not a countable
    /// direct sequence.
    YieldInTryStatement,
    /// Any other statement kind that contains a yield: `switch`, labelled
    /// statements, `do`-`while`, `for`-`in`, `for`-`of`, and so on.
    YieldInUnsupportedStatement,
    GeneratorForOfShape,
}

impl GeneratorPlanRejection {
    /// The reported reason. Every message names the *generator body* and the
    /// shape that failed, so a sweep groups these into families instead of into
    /// one bucket labelled "function or class declaration".
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::StateOverflow => {
                "generator body: suspension states exceed the representable state count"
            }
            Self::YieldInDeclaration => {
                "generator body: a nested declaration contains a yield, which has no linear \
                 suspension plan"
            }
            Self::YieldOperandNotDirect => {
                "generator body: a yield operand hides further suspensions, which has no linear \
                 suspension plan"
            }
            Self::ReturnOperandNotStageable => {
                "generator body: a `return` operand containing a yield cannot be staged into a \
                 linear suspension plan"
            }
            Self::ThrowOperandNotStageable => {
                "generator body: a `throw` operand containing a yield requires a complete value continuation"
            }
            Self::DiscardedYieldExpression => {
                "generator body: a discarded expression statement's yields cannot be flattened \
                 into a linear suspension plan"
            }
            Self::DiscardedYieldBlock => {
                "generator body: a block's yields cannot be flattened into a linear suspension \
                 plan"
            }
            Self::WithSourceShape => {
                "generator body: complete With source requires an owned staged head and environment lifetime"
            }
            Self::ForInSourceShape => {
                "generator body: complete ForIn source requires an eager per-key head and retained enumeration lifetime"
            }
            Self::ClassicLoopShape => {
                "generator body: classic loop source requires an owned staged expression, lexical binding and control region"
            }
            Self::YieldInIfCondition => {
                "generator body: a yield in an `if` condition has no linear suspension plan"
            }
            Self::IfBranchYieldNotDirect => {
                "generator body: an `if` branch whose yields are not a direct sequence has no \
                 linear suspension plan"
            }
            Self::YieldInTryStatement => {
                "generator body: a `try`/`catch`/`finally` block whose yields are not a direct \
                 sequence has no linear suspension plan"
            }
            Self::GeneratorForOfShape => {
                "generator body: yielding for-of requires an eager var/let/const identifier head and structured Yield body without nested resumable loops or foreign branch owners"
            }
            Self::YieldInUnsupportedStatement => {
                "generator body: a yield inside a statement kind with no resumable lowering \
                 (`switch`, a label, `do`-`while`, `for`-`in`, `for`-`of`) has no linear \
                 suspension plan"
            }
        }
    }
}

pub(crate) fn linear_generator_plan(body: &FunctionBody) -> Option<GeneratorPlanIr> {
    linear_generator_plan_with_reason(body).ok()
}

pub(crate) fn linear_generator_plan_with_reason(
    body: &FunctionBody,
) -> Result<GeneratorPlanIr, GeneratorPlanRejection> {
    let mut suspension_points = Vec::new();
    let mut current_state = 0u32;
    if crate::async_generator_source::AsyncGeneratorResourceScopeSource::for_protocol(
        body.statements(),
        ResumableRegionProtocolIr::Generator,
    )
    .is_some()
    {
        append_structured_generator_suspensions(
            body.statements(),
            &mut current_state,
            &mut suspension_points,
        )
        .ok_or(GeneratorPlanRejection::YieldInDeclaration)?;
        return finish_generator_plan(current_state, suspension_points);
    }
    for item in body.statements() {
        let StatementListItem::Statement(statement) = item else {
            if contains(item, ContainsSymbol::YieldExpression) {
                GeneratorExpressionSourcePlan::declaration(
                    item,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
                )
                .ok_or(GeneratorPlanRejection::YieldInDeclaration)?
                .append(&mut current_state, &mut suspension_points)
                .ok_or(GeneratorPlanRejection::StateOverflow)?;
            }
            continue;
        };
        let yield_expression = match statement.as_ref() {
            Statement::Expression(Expression::Yield(expression)) => Some((expression, true)),
            Statement::Expression(Expression::Assign(assignment))
                if assignment.op() == AssignOp::Assign
                    && matches!(
                        assignment.lhs(),
                        AssignTarget::Identifier(_) | AssignTarget::Access(_)
                    )
                    && !contains(assignment.lhs(), ContainsSymbol::YieldExpression) =>
            {
                match assignment.rhs() {
                    Expression::Yield(expression) => Some((expression, false)),
                    _ => None,
                }
            }
            Statement::Return(statement) => match statement.target() {
                Some(Expression::Yield(expression)) => Some((expression, true)),
                _ => None,
            },
            _ => None,
        };
        if let Some((yield_expression, nested_yield_allowed)) = yield_expression {
            if !nested_yield_allowed
                && yield_expression
                    .target()
                    .is_some_and(|target| contains(target, ContainsSymbol::YieldExpression))
            {
                return Err(GeneratorPlanRejection::YieldOperandNotDirect);
            }
            let expression = Expression::Yield(yield_expression.clone());
            GeneratorExpressionSourcePlan::new(
                &expression,
                GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
            )
            .ok_or(GeneratorPlanRejection::YieldOperandNotDirect)?
            .append(&mut current_state, &mut suspension_points)
            .ok_or(GeneratorPlanRejection::StateOverflow)?;
            continue;
        }
        if let Statement::Return(statement) = statement.as_ref() {
            if let Some(target) = statement
                .target()
                .filter(|target| contains(*target, ContainsSymbol::YieldExpression))
            {
                GeneratorExpressionSourcePlan::new(
                    target,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
                )
                .ok_or(GeneratorPlanRejection::ReturnOperandNotStageable)?
                .append(&mut current_state, &mut suspension_points)
                .ok_or(GeneratorPlanRejection::StateOverflow)?;
                continue;
            }
        }
        if let Statement::Throw(source) = statement.as_ref() {
            if contains(source.target(), ContainsSymbol::YieldExpression) {
                GeneratorExpressionSourcePlan::new(
                    source.target(),
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
                )
                .ok_or(GeneratorPlanRejection::ThrowOperandNotStageable)?
                .append(&mut current_state, &mut suspension_points)
                .ok_or(GeneratorPlanRejection::StateOverflow)?;
            }
            continue;
        }
        if let Statement::Var(source) = statement.as_ref() {
            if contains(source, ContainsSymbol::YieldExpression) {
                GeneratorExpressionSourcePlan::var_declaration(source)
                    .ok_or(GeneratorPlanRejection::YieldInDeclaration)?
                    .append(&mut current_state, &mut suspension_points)
                    .ok_or(GeneratorPlanRejection::StateOverflow)?;
            }
            continue;
        }
        if let Statement::Expression(expression) = statement.as_ref() {
            if contains(expression, ContainsSymbol::YieldExpression) {
                append_discarded_generator_expression_suspensions(
                    expression,
                    &mut current_state,
                    &mut suspension_points,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
                )
                .ok_or(GeneratorPlanRejection::DiscardedYieldExpression)?;
                continue;
            }
        }
        if let Statement::Block(block) = statement.as_ref() {
            if contains(block, ContainsSymbol::YieldExpression)
                || contains_ordinary_generator_phase_owner(block)
            {
                append_discarded_generator_block_suspensions(
                    block.statement_list().statements(),
                    &mut current_state,
                    &mut suspension_points,
                )
                .ok_or(GeneratorPlanRejection::DiscardedYieldBlock)?;
                continue;
            }
        }
        if let Statement::With(with) = statement.as_ref() {
            GeneratorWithSource::new(with)
                .and_then(|source| source.append(&mut current_state, &mut suspension_points))
                .ok_or(GeneratorPlanRejection::WithSourceShape)?;
            continue;
        }
        if let Statement::ForInLoop(source) = statement.as_ref() {
            append_generator_for_in_suspensions(source, &mut current_state, &mut suspension_points)
                .ok_or(GeneratorPlanRejection::ForInSourceShape)?;
            continue;
        }
        if let Statement::ForOfLoop(for_of) = statement.as_ref() {
            append_generator_for_of_suspensions(for_of, &mut current_state, &mut suspension_points)
                .ok_or(GeneratorPlanRejection::GeneratorForOfShape)?;
            continue;
        }
        if let Statement::Switch(source) = statement.as_ref() {
            if contains(source, ContainsSymbol::YieldExpression)
                || contains_ordinary_generator_phase_owner(source)
            {
                append_generator_switch_suspensions(
                    source,
                    &mut current_state,
                    &mut suspension_points,
                )
                .ok_or(GeneratorPlanRejection::YieldInUnsupportedStatement)?;
                continue;
            }
        }
        let classic_loop = match statement.as_ref() {
            Statement::ForLoop(source) => {
                Some(crate::generator_loop_source::ClassicGeneratorLoopSource::For(source))
            }
            Statement::WhileLoop(source) => {
                Some(crate::generator_loop_source::ClassicGeneratorLoopSource::While(source))
            }
            Statement::DoWhileLoop(source) => {
                Some(crate::generator_loop_source::ClassicGeneratorLoopSource::DoWhile(source))
            }
            _ => None,
        };
        if let Some(source) = classic_loop {
            source
                .append(&mut current_state, &mut suspension_points)
                .ok_or(GeneratorPlanRejection::ClassicLoopShape)?;
            continue;
        }
        if let Statement::Labelled(source) = statement.as_ref() {
            if matches!(
                source.item(),
                LabelledItem::Statement(
                    Statement::ForLoop(_)
                        | Statement::WhileLoop(_)
                        | Statement::DoWhileLoop(_)
                        | Statement::Switch(_)
                        | Statement::Labelled(_)
                        | Statement::With(_)
                )
            ) || contains_ordinary_generator_phase_owner(source)
            {
                crate::generator_loop_source::append_classic_generator_statement(
                    statement,
                    &mut current_state,
                    &mut suspension_points,
                )
                .ok_or(GeneratorPlanRejection::ClassicLoopShape)?;
                continue;
            }
        }
        if let Statement::If(if_statement) = statement.as_ref() {
            if ordinary_generator_if_requires_complete_owner(if_statement) {
                crate::generator_loop_source::append_classic_generator_statement(
                    statement,
                    &mut current_state,
                    &mut suspension_points,
                )
                .ok_or(GeneratorPlanRejection::IfBranchYieldNotDirect)?;
                continue;
            }
            if contains(if_statement.cond(), ContainsSymbol::YieldExpression) {
                return Err(GeneratorPlanRejection::YieldInIfCondition);
            }
            let then_yields = simple_generator_if_branch_yield_count(if_statement.body())
                .ok_or(GeneratorPlanRejection::IfBranchYieldNotDirect)?;
            let else_yields = match if_statement.else_node() {
                Some(branch) => simple_generator_if_branch_yield_count(branch)
                    .ok_or(GeneratorPlanRejection::IfBranchYieldNotDirect)?,
                None => 0,
            };
            let yield_count = then_yields
                .checked_add(else_yields)
                .and_then(|count| u32::try_from(count).ok())
                .ok_or(GeneratorPlanRejection::StateOverflow)?;
            if yield_count == 0 {
                continue;
            }
            for resume_offset in 1..=yield_count {
                suspension_points.push(GeneratorSuspensionPointIr {
                    suspend_state: current_state,
                    resume_state: current_state
                        .checked_add(resume_offset)
                        .ok_or(GeneratorPlanRejection::StateOverflow)?,
                });
            }
            current_state = current_state
                .checked_add(yield_count)
                .and_then(|state| state.checked_add(1))
                .ok_or(GeneratorPlanRejection::StateOverflow)?;
            continue;
        }
        if let Statement::Try(try_statement) = statement.as_ref() {
            append_structured_generator_suspensions(
                try_statement.block().statement_list().statements(),
                &mut current_state,
                &mut suspension_points,
            )
            .ok_or(GeneratorPlanRejection::YieldInTryStatement)?;
            current_state = current_state
                .checked_add(1)
                .ok_or(GeneratorPlanRejection::StateOverflow)?;
            if let Some(catch) = try_statement.catch() {
                crate::generator_loop_source::append_generator_catch_parameter(
                    catch.parameter(),
                    &mut current_state,
                    &mut suspension_points,
                )
                .ok_or(GeneratorPlanRejection::YieldInTryStatement)?;
                append_structured_generator_suspensions(
                    catch.block().statement_list().statements(),
                    &mut current_state,
                    &mut suspension_points,
                )
                .ok_or(GeneratorPlanRejection::YieldInTryStatement)?;
                current_state = current_state
                    .checked_add(1)
                    .ok_or(GeneratorPlanRejection::StateOverflow)?;
            }
            if let Some(finally) = try_statement.finally() {
                append_structured_generator_suspensions(
                    finally.block().statement_list().statements(),
                    &mut current_state,
                    &mut suspension_points,
                )
                .ok_or(GeneratorPlanRejection::YieldInTryStatement)?;
                current_state = current_state
                    .checked_add(1)
                    .ok_or(GeneratorPlanRejection::StateOverflow)?;
            }
            continue;
        }
        if contains(statement.as_ref(), ContainsSymbol::YieldExpression) {
            return Err(GeneratorPlanRejection::YieldInUnsupportedStatement);
        }
    }
    finish_generator_plan(current_state, suspension_points)
}

pub(crate) fn finish_generator_plan(
    final_state: u32,
    suspension_points: Vec<GeneratorSuspensionPointIr>,
) -> Result<GeneratorPlanIr, GeneratorPlanRejection> {
    let state_count = final_state
        .checked_add(1)
        .ok_or(GeneratorPlanRejection::StateOverflow)?;
    Ok(GeneratorPlanIr {
        entry_state: 0,
        state_count,
        suspension_points,
    })
}

fn append_structured_generator_suspensions(
    statements: &[StatementListItem],
    current_state: &mut u32,
    suspension_points: &mut Vec<GeneratorSuspensionPointIr>,
) -> Option<()> {
    append_generator_region_suspensions(
        statements,
        current_state,
        suspension_points,
        GeneratorSuspensionRegion::FunctionBody,
    )
}

fn direct_generator_yield_count(
    target: Option<&Expression>,
    nested_yield_allowed: bool,
) -> Option<u32> {
    let Some(target) = target else {
        return Some(1);
    };
    if !contains(target, ContainsSymbol::YieldExpression) {
        return Some(1);
    }
    if !nested_yield_allowed {
        return None;
    }
    staged_generator_expression_yield_count(target)?.checked_add(1)
}

pub(crate) fn class_evaluation_expressions<'a>(
    heritage: Option<&'a Expression>,
    elements: &'a [ClassElement],
) -> impl Iterator<Item = &'a Expression> {
    heritage
        .into_iter()
        .chain(elements.iter().filter_map(|element| {
            let name = match element {
                ClassElement::MethodDefinition(method) => match method.name() {
                    ClassElementName::PropertyName(name) => name,
                    ClassElementName::PrivateName(_) => return None,
                },
                ClassElement::FieldDefinition(field)
                | ClassElement::StaticFieldDefinition(field)
                | ClassElement::AccessorFieldDefinition(field)
                | ClassElement::StaticAccessorFieldDefinition(field) => field.name(),
                ClassElement::PrivateFieldDefinition(_)
                | ClassElement::PrivateStaticFieldDefinition(_)
                | ClassElement::StaticBlock(_) => return None,
            };
            match name {
                PropertyName::Computed(expression) => Some(expression),
                PropertyName::Literal(_) => None,
            }
        }))
}

/// Current property's evaluation operands, before its actual definition. Method
/// bodies belong to their own invocation; only a computed name evaluates here.
pub(crate) fn generator_object_property_operands(
    property: &PropertyDefinition,
) -> Option<(Option<&Expression>, Option<&Expression>)> {
    Some(match property {
        PropertyDefinition::Property(PropertyName::Computed(key), value) => {
            (Some(key), Some(value))
        }
        PropertyDefinition::Property(PropertyName::Literal(_), value)
        | PropertyDefinition::SpreadObject(value) => (None, Some(value)),
        PropertyDefinition::MethodDefinition(method) => {
            let key = match method.name() {
                PropertyName::Computed(key) => Some(key),
                PropertyName::Literal(_) => None,
            };
            (key, None)
        }
        PropertyDefinition::IdentifierReference(_) => (None, None),
        PropertyDefinition::CoverInitializedName(_, _) => return None,
    })
}

fn staged_generator_property_reference_yield_count(access: &PropertyAccess) -> Option<u32> {
    let (count, key) = match access {
        PropertyAccess::Simple(access) => (
            staged_generator_expression_yield_count(access.target())?,
            access.field(),
        ),
        PropertyAccess::Private(access) => {
            return staged_generator_expression_yield_count(access.target())
        }
        PropertyAccess::Super(access) => (0, access.field()),
    };
    match key {
        PropertyAccessField::Expr(key) => {
            count.checked_add(staged_generator_expression_yield_count(key)?)
        }
        PropertyAccessField::Const(_) => Some(count),
    }
}

pub(crate) fn staged_generator_expression_yield_count(expression: &Expression) -> Option<u32> {
    if !contains(expression, ContainsSymbol::YieldExpression) {
        return Some(0);
    }
    match expression {
        Expression::Parenthesized(parenthesized) => {
            staged_generator_expression_yield_count(parenthesized.expression())
        }
        Expression::Binary(binary) if !matches!(binary.op(), BinaryOp::Logical(_)) => {
            staged_generator_expression_yield_count(binary.lhs())?
                .checked_add(staged_generator_expression_yield_count(binary.rhs())?)
        }
        Expression::Unary(unary) if unary.op() == UnaryOp::Delete => {
            match CheckedGeneratorDeleteSource::new(unary.target())?.into_operand() {
                GeneratorDeleteOperand::Property { access, .. } => {
                    let count = staged_generator_expression_yield_count(access.target())?;
                    match access.field() {
                        PropertyAccessField::Const(_) => Some(count),
                        PropertyAccessField::Expr(key) => {
                            count.checked_add(staged_generator_expression_yield_count(key)?)
                        }
                    }
                }
                GeneratorDeleteOperand::Optional(_)
                | GeneratorDeleteOperand::AwaitedOptional(_) => None,
                GeneratorDeleteOperand::Super(access) => match access.field() {
                    PropertyAccessField::Const(_) => Some(0),
                    PropertyAccessField::Expr(key) => staged_generator_expression_yield_count(key),
                },
                GeneratorDeleteOperand::Value(source) => {
                    staged_generator_expression_yield_count(source)
                }
            }
        }
        Expression::Unary(unary) => staged_generator_expression_yield_count(unary.target()),
        Expression::TemplateLiteral(template) => {
            template
                .elements()
                .iter()
                .try_fold(0u32, |count, element| match element {
                    TemplateElement::String(_) => Some(count),
                    TemplateElement::Expr(value) => {
                        count.checked_add(staged_generator_expression_yield_count(value)?)
                    }
                })
        }
        Expression::Yield(yield_expression) => {
            let nested_count = match yield_expression.target() {
                Some(target) => staged_generator_expression_yield_count(target)?,
                None => 0,
            };
            nested_count.checked_add(1)
        }
        Expression::Call(call) => {
            if contains(expression, ContainsSymbol::AwaitExpression) {
                return None;
            }
            staged_generator_invocation_argument_yield_count(
                staged_generator_invocation_reference_yield_count(call.function())?,
                call.args(),
            )
        }
        Expression::New(construct) => {
            if contains(expression, ContainsSymbol::AwaitExpression) {
                return None;
            }
            staged_generator_invocation_argument_yield_count(
                staged_generator_invocation_reference_yield_count(construct.constructor())?,
                construct.arguments(),
            )
        }
        Expression::TaggedTemplate(template) => {
            if contains(expression, ContainsSymbol::AwaitExpression) {
                return None;
            }
            staged_generator_invocation_argument_yield_count(
                staged_generator_invocation_reference_yield_count(template.tag())?,
                template.exprs(),
            )
        }
        Expression::PropertyAccess(access) => {
            staged_generator_property_reference_yield_count(access)
        }
        Expression::BinaryInPrivate(source) => {
            staged_generator_expression_yield_count(source.rhs())
        }
        Expression::Update(source) => match source.target() {
            UpdateTarget::Identifier(_) => Some(0),
            UpdateTarget::PropertyAccess(access) => {
                staged_generator_property_reference_yield_count(access)
            }
            UpdateTarget::WebCompatCall(call) => staged_generator_invocation_argument_yield_count(
                staged_generator_invocation_reference_yield_count(call.function())?,
                call.args(),
            ),
        },
        Expression::ImportCall(source) => {
            let count = staged_generator_expression_yield_count(source.argument())?;
            match source.options() {
                Some(options) => {
                    count.checked_add(staged_generator_expression_yield_count(options)?)
                }
                None => Some(count),
            }
        }
        Expression::Assign(assignment)
            if matches!(assignment.lhs(), AssignTarget::WebCompatCall(_)) =>
        {
            let AssignTarget::WebCompatCall(call) = assignment.lhs() else {
                unreachable!()
            };
            staged_generator_invocation_argument_yield_count(
                staged_generator_invocation_reference_yield_count(call.function())?,
                call.args(),
            )
        }
        Expression::Assign(assignment)
            if CheckedGeneratorCompoundAssignmentSource::new(assignment).is_some() =>
        {
            let source = CheckedGeneratorCompoundAssignmentSource::new(assignment)?;
            // A selected RHS has joins that a linear suspension count cannot own.
            if source.logical_operation().is_some() {
                return None;
            }
            let count = match source.lhs() {
                AssignTarget::Identifier(_) => 0,
                AssignTarget::Access(access) => {
                    staged_generator_property_reference_yield_count(access)?
                }
                AssignTarget::Pattern(_) | AssignTarget::WebCompatCall(_) => return None,
            };
            count.checked_add(staged_generator_expression_yield_count(source.rhs())?)
        }
        Expression::Assign(assignment) if matches!(assignment.lhs(), AssignTarget::Pattern(_)) => {
            // A selected default owns a structured region, not a linear count.
            if contains(assignment.lhs(), ContainsSymbol::YieldExpression) {
                return None;
            }
            let source = GeneratorPatternAssignmentSource::new(
                assignment,
                GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
            )?;
            staged_generator_expression_yield_count(source.rhs())
        }
        Expression::Assign(assignment) if assignment.op() == AssignOp::Assign => {
            if matches!(assignment.lhs(), AssignTarget::Identifier(_)) {
                return staged_generator_expression_yield_count(assignment.rhs());
            }
            let AssignTarget::Access(access) = assignment.lhs() else {
                return None;
            };
            let count = staged_generator_property_reference_yield_count(access)?;
            count.checked_add(staged_generator_expression_yield_count(assignment.rhs())?)
        }
        Expression::ClassExpression(class) => class_evaluation_expressions(
            class.super_ref(),
            class.elements(),
        )
        .try_fold(0u32, |count, operand| {
            count.checked_add(staged_generator_expression_yield_count(operand)?)
        }),
        Expression::ArrayLiteral(array) => {
            array
                .as_ref()
                .iter()
                .try_fold(0u32, |count, element| match element {
                    Some(Expression::Spread(spread)) => {
                        count.checked_add(staged_generator_expression_yield_count(spread.target())?)
                    }
                    Some(element) => {
                        count.checked_add(staged_generator_expression_yield_count(element)?)
                    }
                    None => Some(count),
                })
        }
        Expression::ObjectLiteral(object) => {
            object
                .properties()
                .iter()
                .try_fold(0u32, |mut count, property| {
                    let (key, value) = generator_object_property_operands(property)?;
                    for source in key.into_iter().chain(value) {
                        count =
                            count.checked_add(staged_generator_expression_yield_count(source)?)?;
                    }
                    Some(count)
                })
        }
        expression if contains(expression, ContainsSymbol::YieldExpression) => None,
        _ => Some(0),
    }
}

fn staged_generator_invocation_reference_yield_count(source: &Expression) -> Option<u32> {
    match source {
        Expression::Parenthesized(parenthesized) => {
            staged_generator_invocation_reference_yield_count(parenthesized.expression())
        }
        Expression::PropertyAccess(PropertyAccess::Private(access)) => {
            staged_generator_expression_yield_count(access.target())
        }
        Expression::PropertyAccess(PropertyAccess::Super(access)) => match access.field() {
            PropertyAccessField::Const(_) => Some(0),
            PropertyAccessField::Expr(key) => staged_generator_expression_yield_count(key),
        },
        // Guarded chain Yields cannot become a flat single-arm count.
        // Grouped Property References and Call Values enter through the same
        // complete checked source planner, never this single-arm count.
        Expression::Optional(optional) if contains(optional, ContainsSymbol::YieldExpression) => {
            None
        }
        source => staged_generator_expression_yield_count(source),
    }
}

fn staged_generator_invocation_argument_yield_count(
    callee_count: u32,
    arguments: &[Expression],
) -> Option<u32> {
    arguments.iter().try_fold(callee_count, |count, argument| {
        let operand = match argument {
            Expression::Spread(spread) => spread.target(),
            argument => argument,
        };
        count.checked_add(staged_generator_expression_yield_count(operand)?)
    })
}

fn append_discarded_generator_block_suspensions(
    statements: &[StatementListItem],
    current_state: &mut u32,
    suspension_points: &mut Vec<GeneratorSuspensionPointIr>,
) -> Option<()> {
    for item in statements {
        let StatementListItem::Statement(statement) = item else {
            if contains(item, ContainsSymbol::YieldExpression) {
                GeneratorExpressionSourcePlan::declaration(
                    item,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
                )?
                .append(current_state, suspension_points)?;
            }
            continue;
        };
        match statement.as_ref() {
            Statement::Var(source) => {
                if contains(source, ContainsSymbol::YieldExpression) {
                    GeneratorExpressionSourcePlan::var_declaration(source)?
                        .append(current_state, suspension_points)?;
                }
            }
            Statement::Expression(expression) => {
                append_discarded_generator_expression_suspensions(
                    expression,
                    current_state,
                    suspension_points,
                    GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
                )?;
            }
            Statement::Return(source) => {
                if let Some(target) = source.target() {
                    GeneratorExpressionSourcePlan::new(
                        target,
                        GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
                    )?
                    .append(current_state, suspension_points)?;
                }
            }
            Statement::Throw(source) => GeneratorExpressionSourcePlan::new(
                source.target(),
                GeneratorValueBranchAdmission::OrdinaryOutsideLoops,
            )?
            .append(current_state, suspension_points)?,
            Statement::With(source) => {
                GeneratorWithSource::new(source)?.append(current_state, suspension_points)?;
            }
            Statement::ForInLoop(source) => {
                append_generator_for_in_suspensions(source, current_state, suspension_points)?;
            }
            Statement::If(source) if ordinary_generator_if_requires_complete_owner(source) => {
                crate::generator_loop_source::append_classic_generator_statement(
                    statement,
                    current_state,
                    suspension_points,
                )?;
            }
            Statement::Block(block) => append_discarded_generator_block_suspensions(
                block.statement_list().statements(),
                current_state,
                suspension_points,
            )?,
            Statement::ForOfLoop(for_of) => {
                append_generator_for_of_suspensions(for_of, current_state, suspension_points)?;
            }
            Statement::Switch(source)
                if contains(source, ContainsSymbol::YieldExpression)
                    || contains_ordinary_generator_phase_owner(source) =>
            {
                append_generator_switch_suspensions(source, current_state, suspension_points)?;
            }
            Statement::ForLoop(source) => {
                crate::generator_loop_source::ClassicGeneratorLoopSource::For(source)
                    .append(current_state, suspension_points)?
            }
            Statement::WhileLoop(source) => {
                crate::generator_loop_source::ClassicGeneratorLoopSource::While(source)
                    .append(current_state, suspension_points)?
            }
            Statement::DoWhileLoop(source) => {
                crate::generator_loop_source::ClassicGeneratorLoopSource::DoWhile(source)
                    .append(current_state, suspension_points)?
            }
            Statement::Labelled(source)
                if matches!(
                    source.item(),
                    LabelledItem::Statement(
                        Statement::ForLoop(_)
                            | Statement::WhileLoop(_)
                            | Statement::DoWhileLoop(_)
                            | Statement::Switch(_)
                            | Statement::Labelled(_)
                            | Statement::With(_)
                    )
                ) || contains_ordinary_generator_phase_owner(source) =>
            {
                crate::generator_loop_source::append_classic_generator_statement(
                    statement,
                    current_state,
                    suspension_points,
                )?
            }
            Statement::Try(_) => append_structured_generator_suspensions(
                std::slice::from_ref(item),
                current_state,
                suspension_points,
            )?,
            statement if contains(statement, ContainsSymbol::YieldExpression) => return None,
            _ => {}
        }
    }
    Some(())
}

pub(crate) fn append_discarded_generator_expression_suspensions(
    expression: &Expression,
    current_state: &mut u32,
    suspension_points: &mut Vec<GeneratorSuspensionPointIr>,
    admission: GeneratorValueBranchAdmission,
) -> Option<()> {
    let mut ungrouped = expression;
    while let Expression::Parenthesized(group) = ungrouped {
        ungrouped = group.expression();
    }
    if contains(expression, ContainsSymbol::YieldExpression)
        && (matches!(
            ungrouped,
            Expression::Unary(_) | Expression::TemplateLiteral(_)
        ) || matches!(ungrouped, Expression::Binary(binary) if !matches!(binary.op(), BinaryOp::Logical(_))))
    {
        return GeneratorExpressionSourcePlan::new(expression, admission)?
            .append(current_state, suspension_points);
    }
    if matches!(
        ungrouped,
        Expression::Conditional(_) | Expression::Optional(_)
    ) || matches!(ungrouped, Expression::Binary(binary) if matches!(binary.op(), BinaryOp::Logical(_)))
    {
        if let Some(plan) = GeneratorExpressionSourcePlan::new(expression, admission) {
            return plan.append(current_state, suspension_points);
        }
    }
    match expression {
        Expression::Parenthesized(parenthesized) => {
            append_discarded_generator_expression_suspensions(
                parenthesized.expression(),
                current_state,
                suspension_points,
                admission,
            )
        }
        Expression::Yield(_) => GeneratorExpressionSourcePlan::new(expression, admission)?
            .append(current_state, suspension_points),
        Expression::ArrayLiteral(array) => {
            for element in array.as_ref().iter().flatten() {
                if matches!(element, Expression::Spread(_)) {
                    return None;
                }
                append_discarded_generator_expression_suspensions(
                    element,
                    current_state,
                    suspension_points,
                    admission,
                )?;
            }
            Some(())
        }
        Expression::Binary(binary) if binary.op() == BinaryOp::Comma => {
            append_discarded_generator_expression_suspensions(
                binary.lhs(),
                current_state,
                suspension_points,
                admission,
            )?;
            append_discarded_generator_expression_suspensions(
                binary.rhs(),
                current_state,
                suspension_points,
                admission,
            )
        }
        Expression::Binary(binary) if binary.op() == BinaryOp::Arithmetic(ArithmeticOp::Add) => {
            append_discarded_generator_expression_suspensions(
                binary.lhs(),
                current_state,
                suspension_points,
                admission,
            )?;
            append_discarded_generator_expression_suspensions(
                binary.rhs(),
                current_state,
                suspension_points,
                admission,
            )
        }
        Expression::Conditional(conditional) => {
            let mut condition = conditional.condition();
            while let Expression::Parenthesized(parenthesized) = condition {
                condition = parenthesized.expression();
            }
            let Expression::Yield(condition_yield) = condition else {
                return None;
            };
            if direct_generator_yield_count(condition_yield.target(), true)? != 1 {
                return None;
            }
            let condition_suspend_state = *current_state;
            *current_state = current_state.checked_add(1)?;
            suspension_points.push(GeneratorSuspensionPointIr {
                suspend_state: condition_suspend_state,
                resume_state: *current_state,
            });

            let branch_entry_state = *current_state;
            for (resume_offset, branch) in [conditional.if_true(), conditional.if_false()]
                .into_iter()
                .enumerate()
            {
                let mut branch = branch;
                while let Expression::Parenthesized(parenthesized) = branch {
                    branch = parenthesized.expression();
                }
                let Expression::Yield(branch_yield) = branch else {
                    return None;
                };
                if direct_generator_yield_count(branch_yield.target(), true)? != 1 {
                    return None;
                }
                suspension_points.push(GeneratorSuspensionPointIr {
                    suspend_state: branch_entry_state,
                    resume_state: branch_entry_state
                        .checked_add(u32::try_from(resume_offset).ok()?)?
                        .checked_add(1)?,
                });
            }
            *current_state = branch_entry_state.checked_add(3)?;
            Some(())
        }
        Expression::Assign(assignment)
            if assignment.op() == AssignOp::Assign
                && matches!(assignment.lhs(), AssignTarget::Identifier(_))
                && matches!(assignment.rhs(), Expression::TemplateLiteral(template) if contains(template, ContainsSymbol::YieldExpression)) =>
        {
            let Expression::TemplateLiteral(template) = assignment.rhs() else {
                return None;
            };
            for element in template.elements() {
                let TemplateElement::Expr(expression) = element else {
                    continue;
                };
                if !contains(expression, ContainsSymbol::YieldExpression) {
                    continue;
                }
                let mut expression = expression;
                while let Expression::Parenthesized(parenthesized) = expression {
                    expression = parenthesized.expression();
                }
                let Expression::Yield(yield_expression) = expression else {
                    return None;
                };
                if direct_generator_yield_count(yield_expression.target(), true)? != 1 {
                    return None;
                }
                let suspend_state = *current_state;
                *current_state = current_state.checked_add(1)?;
                suspension_points.push(GeneratorSuspensionPointIr {
                    suspend_state,
                    resume_state: *current_state,
                });
            }
            Some(())
        }
        Expression::Assign(assignment)
            if assignment.op() == AssignOp::Assign
                && matches!(assignment.lhs(), AssignTarget::Identifier(_)) =>
        {
            append_discarded_generator_expression_suspensions(
                assignment.rhs(),
                current_state,
                suspension_points,
                admission,
            )
        }
        Expression::ClassExpression(_)
        | Expression::ObjectLiteral(_)
        | Expression::Call(_)
        | Expression::New(_)
        | Expression::TaggedTemplate(_)
        | Expression::PropertyAccess(_)
        | Expression::Update(_)
        | Expression::Assign(_) => GeneratorExpressionSourcePlan::new(expression, admission)?
            .append(current_state, suspension_points),
        expression if contains(expression, ContainsSymbol::YieldExpression) => None,
        _ => Some(()),
    }
}

pub(crate) fn ordinary_generator_if_requires_complete_owner(source: &If) -> bool {
    contains(source.cond(), ContainsSymbol::YieldExpression)
        || simple_generator_if_branch_yield_count(source.body()).is_none()
        || source
            .else_node()
            .is_some_and(|branch| simple_generator_if_branch_yield_count(branch).is_none())
        || contains_ordinary_generator_phase_owner(source)
}

fn simple_generator_if_branch_yield_count(branch: &Statement) -> Option<usize> {
    let statements = match branch {
        Statement::Block(block) => block.statement_list().statements(),
        _ => {
            return match branch {
                Statement::Expression(Expression::Yield(expression))
                    if !expression.delegate()
                        && !expression.target().is_some_and(|target| {
                            contains(target, ContainsSymbol::YieldExpression)
                        }) =>
                {
                    Some(1)
                }
                statement if contains(statement, ContainsSymbol::YieldExpression) => None,
                _ => Some(0),
            };
        }
    };
    let mut yield_count = 0usize;
    let mut has_declaration = false;
    let mut declarations_are_supported = true;
    for item in statements {
        let StatementListItem::Statement(statement) = item else {
            if contains(item, ContainsSymbol::YieldExpression) {
                return None;
            }
            has_declaration = true;
            declarations_are_supported &= generator_loop_body_declaration_is_supported(item);
            continue;
        };
        match statement.as_ref() {
            Statement::Expression(Expression::Yield(expression))
                if !expression.delegate()
                    && !expression.target().is_some_and(|target| {
                        contains(target, ContainsSymbol::YieldExpression)
                    }) =>
            {
                yield_count += 1;
            }
            statement if contains(statement, ContainsSymbol::YieldExpression) => return None,
            _ => {}
        }
    }
    if yield_count == 0 {
        return Some(0);
    }
    if yield_count != 1
        || has_declaration
            && (!declarations_are_supported
                || generator_loop_has_unsupported_construct(branch, true))
    {
        return None;
    }
    Some(1)
}

fn generator_loop_body_declaration_is_supported(item: &StatementListItem) -> bool {
    let StatementListItem::Declaration(declaration) = item else {
        return false;
    };
    if contains(item, ContainsSymbol::YieldExpression) {
        return false;
    }
    matches!(
        declaration.as_ref(),
        Declaration::Lexical(LexicalDeclaration::Let(_) | LexicalDeclaration::Const(_))
    )
}

pub(crate) fn simple_resumable_await_loop_body_is_supported(body: &Statement) -> bool {
    if generator_loop_has_unsupported_construct(body, false) {
        return false;
    }
    let statements = match body {
        Statement::Block(block) => block.statement_list().statements(),
        statement => {
            return matches!(
                statement,
                Statement::Expression(Expression::Await(await_expression))
                    if !contains(
                        await_expression.target(),
                        ContainsSymbol::AwaitExpression
                    ) && !contains(
                        await_expression.target(),
                        ContainsSymbol::YieldExpression
                    )
            );
        }
    };
    let mut await_count = 0usize;
    for item in statements {
        let StatementListItem::Statement(statement) = item else {
            if contains(item, ContainsSymbol::AwaitExpression)
                || contains(item, ContainsSymbol::YieldExpression)
            {
                return false;
            }
            continue;
        };
        match statement.as_ref() {
            Statement::Expression(Expression::Await(await_expression))
                if !contains(await_expression.target(), ContainsSymbol::AwaitExpression)
                    && !contains(await_expression.target(), ContainsSymbol::YieldExpression) =>
            {
                await_count += 1;
            }
            statement
                if contains(statement, ContainsSymbol::AwaitExpression)
                    || contains(statement, ContainsSymbol::YieldExpression) =>
            {
                return false;
            }
            _ => {}
        }
    }
    await_count > 0
}

struct GeneratorLoopShapeVisitor {
    reject_nested_functions: bool,
}

impl GeneratorLoopShapeVisitor {
    fn visit_nested_function(&self) -> ControlFlow<()> {
        if self.reject_nested_functions {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}

impl<'ast> Visitor<'ast> for GeneratorLoopShapeVisitor {
    type BreakTy = ();

    fn visit_break(&mut self, _statement: &'ast AstBreak) -> ControlFlow<Self::BreakTy> {
        ControlFlow::Break(())
    }

    fn visit_continue(&mut self, _statement: &'ast AstContinue) -> ControlFlow<Self::BreakTy> {
        ControlFlow::Break(())
    }

    fn visit_function_declaration(
        &mut self,
        _function: &'ast FunctionDeclaration,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_generator_declaration(
        &mut self,
        _function: &'ast GeneratorDeclaration,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_async_function_declaration(
        &mut self,
        _function: &'ast AsyncFunctionDeclaration,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_async_generator_declaration(
        &mut self,
        _function: &'ast AsyncGeneratorDeclaration,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_function_expression(
        &mut self,
        _function: &'ast FunctionExpression,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_generator_expression(
        &mut self,
        _function: &'ast GeneratorExpression,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_async_function_expression(
        &mut self,
        _function: &'ast AsyncFunctionExpression,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_async_generator_expression(
        &mut self,
        _function: &'ast AsyncGeneratorExpression,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_arrow_function(
        &mut self,
        _function: &'ast ArrowFunction,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_async_arrow_function(
        &mut self,
        _function: &'ast AsyncArrowFunction,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_class_declaration(
        &mut self,
        _class: &'ast ClassDeclaration,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_class_expression(
        &mut self,
        _class: &'ast ClassExpression,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }

    fn visit_object_method_definition(
        &mut self,
        _method: &'ast ObjectMethodDefinition,
    ) -> ControlFlow<Self::BreakTy> {
        self.visit_nested_function()
    }
}

fn generator_loop_has_unsupported_construct<N: VisitWith + ?Sized>(
    loop_statement: &N,
    reject_nested_functions: bool,
) -> bool {
    let mut visitor = GeneratorLoopShapeVisitor {
        reject_nested_functions,
    };
    loop_statement.visit_with(&mut visitor).is_break()
}

/// `break`/`continue` anywhere inside a resumable loop body is rejected: the
/// body is re-entered one iteration per invocation, so a branch out of it has no
/// wasm control frame to land in.
pub(crate) fn generator_loop_has_unsupported_control<N: VisitWith + ?Sized>(
    loop_statement: &N,
    reject_nested_functions: bool,
) -> bool {
    generator_loop_has_unsupported_construct(loop_statement, reject_nested_functions)
}

pub(crate) fn generator_function_is_aot_supported(
    body: &FunctionBody,
    _parameters: &FormalParameterList,
) -> bool {
    linear_generator_plan(body).is_some()
}

pub(crate) fn arrow_function_key(function: &ArrowFunction) -> String {
    let span = function.linear_span();
    format!("linear:{}:{}", span.start().pos(), span.end().pos())
}

pub(crate) fn async_arrow_function_key(function: &AsyncArrowFunction) -> String {
    let span = function.linear_span();
    format!("async-arrow:{}:{}", span.start().pos(), span.end().pos())
}

pub(crate) fn object_method_key(method: &ObjectMethodDefinition) -> String {
    let span = method.linear_span();
    format!("object-method:{}:{}", span.start().pos(), span.end().pos())
}

pub(crate) const fn object_method_protocol(kind: MethodDefinitionKind) -> ObjectMethodProtocolIr {
    match kind {
        MethodDefinitionKind::Ordinary => {
            ObjectMethodProtocolIr::Method(FunctionExecutionKind::Ordinary)
        }
        MethodDefinitionKind::Generator => {
            ObjectMethodProtocolIr::Method(FunctionExecutionKind::Generator)
        }
        MethodDefinitionKind::Async => ObjectMethodProtocolIr::Method(FunctionExecutionKind::Async),
        MethodDefinitionKind::AsyncGenerator => {
            ObjectMethodProtocolIr::Method(FunctionExecutionKind::AsyncGenerator)
        }
        MethodDefinitionKind::Get => ObjectMethodProtocolIr::Getter,
        MethodDefinitionKind::Set => ObjectMethodProtocolIr::Setter,
    }
}

pub(crate) fn for_in_loop_binding_storage_name(
    for_in: &boa_ast::statement::iteration::ForInLoop,
    source_name: &str,
) -> String {
    let span = for_in.target().span();
    format!(
        "$forin.lex.{}.{}.{}.{}.{}",
        span.start().line_number(),
        span.start().column_number(),
        span.end().line_number(),
        span.end().column_number(),
        source_name
    )
}

// `tdz_binding_storage_name` lived here. It is now
// `binding_lifecycle::TdzPlaceholderName::for_source_name`, the sole constructor
// of the `$tdz.` name domain; a bare `String` is no longer accepted where a
// placeholder name is wanted.

pub(crate) fn for_of_loop_binding_storage_name(for_of: &ForOfLoop, source_name: &str) -> String {
    let span = for_of.iterable().span();
    format!(
        "$forof.lex.{}.{}.{}.{}.{}",
        span.start().line_number(),
        span.start().column_number(),
        span.end().line_number(),
        span.end().column_number(),
        source_name
    )
}

pub(crate) fn class_method_key(method: &ClassMethodDefinition) -> String {
    let span = method.linear_span();
    format!("class-method:{}:{}", span.start().pos(), span.end().pos())
}

pub(crate) fn class_method_debug_key(key: &PropertyKeyIr) -> String {
    key.static_name().unwrap_or("<computed>").to_string()
}

pub(crate) fn class_field_debug_key(key: &ClassFieldKeyIr) -> String {
    match key {
        ClassFieldKeyIr::Public(name) => name.clone(),
        ClassFieldKeyIr::ComputedPublic(slot) => format!("<computed:{slot}>"),
        ClassFieldKeyIr::Private(private_name_id) => private_data_key(*private_name_id),
    }
}

pub(crate) fn class_constructor_key(function: &FunctionExpression) -> String {
    format!("class-constructor:{}", function_expression_key(function))
}

pub(crate) fn class_default_constructor_key(span: boa_ast::LinearSpan) -> String {
    format!(
        "class-default-constructor:{}:{}",
        span.start().pos(),
        span.end().pos()
    )
}

pub(crate) fn class_field_initializer_key(initializer: &Expression) -> String {
    let span = initializer.span();
    format!(
        "class-field-initializer:{}:{}:{}:{}",
        span.start().line_number(),
        span.start().column_number(),
        span.end().line_number(),
        span.end().column_number()
    )
}

pub(crate) fn class_static_block_key(block: &StaticBlockBody) -> String {
    let span = block.statements().span();
    format!(
        "class-static-block:{}:{}:{}:{}",
        span.start().line_number(),
        span.start().column_number(),
        span.end().line_number(),
        span.end().column_number()
    )
}

/// The parser's source owner is UTF-16. UTF-8 is retained only for compiler
/// unit fixtures and original module byte-range recovery; it is never decoded
/// as an internal string spelling.
#[derive(Debug, Clone, Copy)]
pub(crate) enum CompilerSourceText<'a> {
    Utf8(&'a str),
    Utf16(&'a [u16]),
}

impl<'a> From<&'a str> for CompilerSourceText<'a> {
    fn from(source: &'a str) -> Self {
        Self::Utf8(source)
    }
}

fn source_slice_from_utf16_span(
    source_text: CompilerSourceText<'_>,
    span: boa_ast::LinearSpan,
) -> String {
    match source_text {
        CompilerSourceText::Utf8(source) => {
            let slice = &source[source_byte_range_from_utf16_span(source, span)];
            encode_js_string_utf16(&slice.encode_utf16().collect::<Vec<_>>())
        }
        CompilerSourceText::Utf16(source) => {
            encode_js_string_utf16(&source[span.start().pos()..span.end().pos()])
        }
    }
}

pub(crate) fn source_byte_range_from_utf16_span(
    source_text: &str,
    span: boa_ast::LinearSpan,
) -> std::ops::Range<usize> {
    let mut utf16_offset = 0;
    let mut start_byte = None;
    for (byte_offset, width) in source_text
        .char_indices()
        .map(|(byte_offset, character)| (byte_offset, character.len_utf16()))
        .chain(std::iter::once((source_text.len(), 0)))
    {
        if utf16_offset == span.start().pos() {
            start_byte = Some(byte_offset);
        }
        if utf16_offset == span.end().pos() {
            let start_byte = start_byte.expect("parser source span starts at a UTF-16 boundary");
            return start_byte..byte_offset;
        }
        utf16_offset += width;
    }
    panic!("parser source span {span:?} must end within the source text");
}

pub(crate) fn function_source_slice(
    function: &FunctionDeclaration,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = function.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn generator_declaration_source_slice(
    function: &GeneratorDeclaration,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = function.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn async_function_declaration_source_slice(
    function: &AsyncFunctionDeclaration,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = function.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn async_generator_declaration_source_slice(
    function: &AsyncGeneratorDeclaration,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = function.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn async_function_expression_source_slice(
    function: &AsyncFunctionExpression,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = function.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn async_generator_expression_source_slice(
    function: &AsyncGeneratorExpression,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = function.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn function_expression_source_slice(
    function: &FunctionExpression,
    source_text: CompilerSourceText<'_>,
) -> String {
    if let Some(span) = function.linear_span() {
        return source_slice_from_utf16_span(source_text, span);
    }
    String::new()
}

pub(crate) fn generator_expression_source_slice(
    function: &GeneratorExpression,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = function.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn arrow_function_source_slice(
    function: &ArrowFunction,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = function.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn async_arrow_function_source_slice(
    function: &AsyncArrowFunction,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = function.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn object_method_source_slice(
    method: &ObjectMethodDefinition,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = method.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn class_method_source_slice(
    method: &ClassMethodDefinition,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = method.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn class_expression_source_slice(
    class: &ClassExpression,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = class.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn class_declaration_source_slice(
    class: &ClassDeclaration,
    source_text: CompilerSourceText<'_>,
) -> String {
    let span = class.linear_span();
    source_slice_from_utf16_span(source_text, span)
}

pub(crate) fn private_name_key(interner: &Interner, name: PrivateName) -> String {
    interner.resolve_expect(name.description()).to_string()
}

/// LexicallyScopedDeclarations of a LabelledStatement: only the immediate
/// LabelledItem::FunctionDeclaration contributes a declaration. An inner label
/// is LabelledItem::Statement and contributes nothing to a Block or CaseBlock.
/// TopLevelVarScopedDeclarations deliberately uses the separate chain walker.
pub(crate) fn lexically_scoped_labelled_function_declaration(
    labelled: &AstLabelled,
) -> Option<&FunctionDeclaration> {
    match labelled.item() {
        LabelledItem::FunctionDeclaration(function) => Some(function),
        LabelledItem::Statement(_) => None,
    }
}

/// Top-level declaration collection and syntax traversal may unwrap labels.
/// Do not use this walker to collect Block/CaseBlock lexical declarations.
pub(crate) fn labelled_function_declaration(
    labelled: &AstLabelled,
) -> Option<&FunctionDeclaration> {
    let mut item = labelled.item();
    loop {
        match item {
            LabelledItem::Statement(Statement::Labelled(next)) => {
                item = next.item();
            }
            LabelledItem::Statement(_) => return None,
            LabelledItem::FunctionDeclaration(function) => return Some(function),
        }
    }
}

pub(crate) fn labelled_base_statement<'b>(labelled: &'b AstLabelled) -> Option<&'b Statement> {
    let mut item = labelled.item();
    loop {
        match item {
            LabelledItem::Statement(Statement::Labelled(next)) => {
                item = next.item();
            }
            LabelledItem::Statement(statement) => return Some(statement),
            LabelledItem::FunctionDeclaration(_) => return None,
        }
    }
}

pub(crate) fn contains_async_property_assignment(expression: &Expression) -> bool {
    struct PropertyAssignment;
    impl<'ast> Visitor<'ast> for PropertyAssignment {
        type BreakTy = ();

        fn visit_expression(&mut self, expression: &'ast Expression) -> ControlFlow<()> {
            if !contains(expression, ContainsSymbol::AwaitExpression) {
                return ControlFlow::Continue(());
            }
            if matches!(expression, Expression::Assign(assignment)
                if assignment.op() == AssignOp::Assign
                    && matches!(assignment.lhs(), AssignTarget::Access(PropertyAccess::Simple(_))))
            {
                return ControlFlow::Break(());
            }
            expression.visit_with(self)
        }
    }
    PropertyAssignment.visit_expression(expression).is_break()
}

/// True when hoisting the `await`s out of `expression` would change *which* of
/// them run.
///
/// An async body suspends only in statement position: the dispatcher re-enters
/// the function and resumes at the statement matching the stored state, so the
/// lowerer rewrites `await x` into a `let` plus an `AsyncAwait` statement
/// placed *before* the statement that used it. That prefix runs
/// unconditionally and in order, so it can only carry suspensions the
/// expression itself always reaches.
///
/// The right operand of `&&`/`||`/`??` (and their compound assignments), both
/// arms of `?:`, and every link after a short-circuiting `?.` are reached only
/// on some paths. Only the plain async branch owner admits `?:` arms, logical RHS values
/// and checked Property/Call optional tails.
/// Other branch positions remain refused instead of being hoisted onto a path
/// the program never takes.
///
/// Anything else evaluates its operands unconditionally, left to right, so the
/// walk recurses through it. Forms that are not recognised are reported as
/// conditional whenever they contain an `await` at all, so a shape this
/// function has not been taught about refuses instead of miscompiling.
#[derive(Clone, Copy)]
pub(crate) enum AwaitBranchOwner {
    UnconditionalPrefix,
    PlainAsyncBranch,
}

/// The actual property and Call links admitted to the plain async tail.
#[must_use]
pub(crate) struct AwaitedOptionalChainSource<'ast> {
    target: &'ast Expression,
    links: Vec<AwaitedOptionalChainLink<'ast>>,
}

pub(crate) enum AwaitedOptionalChainLink<'ast> {
    Property(AwaitedOptionalPropertyLink<'ast>),
    Private { field: PrivateName, shorted: bool },
    Call(AwaitedOptionalCallLink<'ast>),
}

pub(crate) struct AwaitedOptionalPropertyLink<'ast> {
    field: &'ast PropertyAccessField,
    shorted: bool,
}

pub(crate) struct AwaitedOptionalCallLink<'ast> {
    arguments: &'ast [Expression],
    shorted: bool,
}

pub(crate) struct AwaitedOptionalChainTail<'ast> {
    links: std::vec::IntoIter<AwaitedOptionalChainLink<'ast>>,
}

/// The actual terminal source decides whether grouping preserves a property
/// Reference or has already consumed it in a Call. The pair cannot be minted
/// from an independently selected chain and result mode.
pub(crate) struct AwaitedGroupedOptionalChainSource<'ast> {
    chain: AwaitedOptionalChainSource<'ast>,
    terminal: AwaitedOptionalChainTerminal,
}

pub(crate) enum AwaitedOptionalChainTerminal {
    PropertyReference,
    CallValue,
}

impl<'ast> AwaitedGroupedOptionalChainSource<'ast> {
    pub(crate) fn into_parts(
        self,
    ) -> (
        AwaitedOptionalChainSource<'ast>,
        AwaitedOptionalChainTerminal,
    ) {
        (self.chain, self.terminal)
    }
}

impl<'ast> AwaitedOptionalChainSource<'ast> {
    pub(crate) fn from_grouped_invocation(
        source: &'ast Expression,
    ) -> Option<AwaitedGroupedOptionalChainSource<'ast>> {
        let Expression::Parenthesized(group) = source else {
            return None;
        };
        let mut source = group.expression();
        while let Expression::Parenthesized(group) = source {
            source = group.expression();
        }
        let Expression::Optional(optional) = source else {
            return None;
        };
        if !contains(optional, ContainsSymbol::AwaitExpression) {
            return None;
        }
        let terminal = match optional.chain().last()?.kind() {
            OptionalOperationKind::SimplePropertyAccess { .. }
            | OptionalOperationKind::PrivatePropertyAccess { .. } => {
                AwaitedOptionalChainTerminal::PropertyReference
            }
            OptionalOperationKind::Call { .. } => AwaitedOptionalChainTerminal::CallValue,
        };
        // Grouping retains the actual terminal kind even when only the target
        // suspends. Both roles consume the same complete tail; ordinary value
        // admission below still requires an awaited link.
        let chain = Self::from_links(optional)?;
        Some(AwaitedGroupedOptionalChainSource { chain, terminal })
    }

    pub(crate) fn new(source: &'ast Optional) -> Option<Self> {
        if !source
            .chain()
            .iter()
            .any(|link| contains(link, ContainsSymbol::AwaitExpression))
        {
            return None;
        }
        Self::from_links(source)
    }

    pub(crate) fn for_delete(source: &'ast Optional) -> Option<Self> {
        if !contains(source, ContainsSymbol::AwaitExpression)
            || !matches!(
                source.chain().last()?.kind(),
                OptionalOperationKind::SimplePropertyAccess { .. }
            )
        {
            return None;
        }
        Self::from_links(source)
    }

    fn from_links(source: &'ast Optional) -> Option<Self> {
        if !source.chain().first().is_some_and(|link| link.shorted()) {
            return None;
        }
        // A first Call consumes either a grouped property Reference or a
        // completed Call Value. Validate that exact source recursively before
        // any prefix entry can allocate continuation states.
        if source
            .chain()
            .first()
            .is_some_and(|link| matches!(link.kind(), OptionalOperationKind::Call { .. }))
            && awaited_optional_reference(source.target())
            && Self::from_grouped_invocation(source.target()).is_none()
        {
            return None;
        }
        let mut links = Vec::with_capacity(source.chain().len());
        for operation in source.chain() {
            links.push(match operation.kind() {
                OptionalOperationKind::SimplePropertyAccess { field } => {
                    AwaitedOptionalChainLink::Property(AwaitedOptionalPropertyLink {
                        field,
                        shorted: operation.shorted(),
                    })
                }
                OptionalOperationKind::Call { args } => {
                    AwaitedOptionalChainLink::Call(AwaitedOptionalCallLink {
                        arguments: args,
                        shorted: operation.shorted(),
                    })
                }
                OptionalOperationKind::PrivatePropertyAccess { field } => {
                    AwaitedOptionalChainLink::Private {
                        field: *field,
                        shorted: operation.shorted(),
                    }
                }
            });
        }
        Some(Self {
            target: source.target(),
            links,
        })
    }

    pub(crate) fn has_calls(&self) -> bool {
        self.links
            .iter()
            .any(|link| matches!(link, AwaitedOptionalChainLink::Call(_)))
    }

    pub(crate) fn into_parts(self) -> (&'ast Expression, AwaitedOptionalChainTail<'ast>) {
        (
            self.target,
            AwaitedOptionalChainTail {
                links: self.links.into_iter(),
            },
        )
    }
}

impl AwaitedOptionalChainLink<'_> {
    pub(crate) fn shorted(&self) -> bool {
        match self {
            Self::Property(link) => link.shorted,
            Self::Private { shorted, .. } => *shorted,
            Self::Call(link) => link.shorted,
        }
    }
}

impl<'ast> AwaitedOptionalPropertyLink<'ast> {
    pub(crate) fn into_parts(self) -> (&'ast PropertyAccessField, bool) {
        (self.field, self.shorted)
    }
}

impl<'ast> AwaitedOptionalCallLink<'ast> {
    pub(crate) fn into_arguments(self) -> &'ast [Expression] {
        self.arguments
    }
}

impl<'ast> AwaitedOptionalChainTail<'ast> {
    pub(crate) fn is_empty(&self) -> bool {
        self.links.as_slice().is_empty()
    }

    pub(crate) fn starts_with_call(&self) -> bool {
        matches!(
            self.links.as_slice().first(),
            Some(AwaitedOptionalChainLink::Call(_))
        )
    }

    pub(crate) fn has_calls(&self) -> bool {
        self.links
            .as_slice()
            .iter()
            .any(|link| matches!(link, AwaitedOptionalChainLink::Call(_)))
    }

    pub(crate) fn suspends(&self) -> bool {
        self.links.as_slice().iter().any(|link| match link {
            AwaitedOptionalChainLink::Property(link) => match link.field {
                PropertyAccessField::Const(_) => false,
                PropertyAccessField::Expr(key) => {
                    contains(key.as_ref(), ContainsSymbol::AwaitExpression)
                }
            },
            AwaitedOptionalChainLink::Call(link) => link
                .arguments
                .iter()
                .any(|argument| contains(argument, ContainsSymbol::AwaitExpression)),
            AwaitedOptionalChainLink::Private { .. } => false,
        })
    }

    pub(crate) fn next(mut self) -> Option<(AwaitedOptionalChainLink<'ast>, Self)> {
        self.links.next().map(|link| (link, self))
    }
}

/// Identify a direct suspended optional operand of Call, tag or delete.
/// Its terminal source determines Reference versus Value ownership. An outer
/// ordinary member has its own owner and is deliberately not matched here.
pub(crate) fn awaited_optional_reference(source: &Expression) -> bool {
    match source {
        Expression::Parenthesized(group) => awaited_optional_reference(group.expression()),
        Expression::Optional(optional) => contains(optional, ContainsSymbol::AwaitExpression),
        _ => false,
    }
}

fn awaited_optional_call_reference_requires_owner(
    source: &Expression,
    owner: AwaitBranchOwner,
) -> bool {
    if !awaited_optional_reference(source) {
        return false;
    }
    match owner {
        AwaitBranchOwner::UnconditionalPrefix => true,
        AwaitBranchOwner::PlainAsyncBranch => {
            AwaitedOptionalChainSource::from_grouped_invocation(source).is_none()
        }
    }
}

pub(crate) fn await_requires_branch_owner(
    expression: &Expression,
    owner: AwaitBranchOwner,
) -> bool {
    if !contains(expression, ContainsSymbol::AwaitExpression) {
        return false;
    }
    match expression {
        Expression::Parenthesized(parenthesized) => {
            await_requires_branch_owner(parenthesized.expression(), owner)
        }
        Expression::Await(await_expression) => {
            await_requires_branch_owner(await_expression.target(), owner)
        }
        Expression::Unary(unary) => {
            if unary.op() == UnaryOp::Delete && awaited_optional_reference(unary.target()) {
                let mut target = unary.target();
                while let Expression::Parenthesized(group) = target {
                    target = group.expression();
                }
                let Expression::Optional(optional) = target else {
                    unreachable!("actual optional operand");
                };
                if matches!(
                    optional.chain().last().map(|link| link.kind()),
                    Some(OptionalOperationKind::SimplePropertyAccess { .. })
                ) {
                    return match owner {
                        AwaitBranchOwner::UnconditionalPrefix => true,
                        AwaitBranchOwner::PlainAsyncBranch => {
                            AwaitedOptionalChainSource::for_delete(optional).is_none()
                                || await_requires_branch_owner(optional.target(), owner)
                                || optional.chain().iter().any(|link| match link.kind() {
                                    OptionalOperationKind::SimplePropertyAccess {
                                        field: PropertyAccessField::Expr(key),
                                    } => await_requires_branch_owner(key, owner),
                                    OptionalOperationKind::SimplePropertyAccess {
                                        field: PropertyAccessField::Const(_),
                                    }
                                    | OptionalOperationKind::PrivatePropertyAccess { .. } => false,
                                    OptionalOperationKind::Call { args } => args
                                        .iter()
                                        .any(|value| await_requires_branch_owner(value, owner)),
                                })
                        }
                    };
                }
            }
            await_requires_branch_owner(unary.target(), owner)
        }
        Expression::Update(update) => match update.target() {
            UpdateTarget::Identifier(_) => false,
            UpdateTarget::PropertyAccess(access) => {
                property_access_await_requires_branch_owner(access, owner)
            }
            UpdateTarget::WebCompatCall(call) => call
                .args()
                .iter()
                .any(|expression| await_requires_branch_owner(expression, owner)),
        },
        Expression::Binary(binary) => match binary.op() {
            // 13.13/13.14: the right operand is evaluated only when the left
            // one does not already decide the result.
            BinaryOp::Logical(_) => match owner {
                AwaitBranchOwner::UnconditionalPrefix => {
                    contains(binary.rhs(), ContainsSymbol::AwaitExpression)
                        || await_requires_branch_owner(binary.lhs(), owner)
                }
                AwaitBranchOwner::PlainAsyncBranch => {
                    await_requires_branch_owner(binary.lhs(), owner)
                        || await_requires_branch_owner(binary.rhs(), owner)
                }
            },
            _ => {
                await_requires_branch_owner(binary.lhs(), owner)
                    || await_requires_branch_owner(binary.rhs(), owner)
            }
        },
        Expression::BinaryInPrivate(binary) => await_requires_branch_owner(binary.rhs(), owner),
        Expression::Conditional(conditional) => match owner {
            AwaitBranchOwner::UnconditionalPrefix => {
                contains(conditional.if_true(), ContainsSymbol::AwaitExpression)
                    || contains(conditional.if_false(), ContainsSymbol::AwaitExpression)
                    || await_requires_branch_owner(conditional.condition(), owner)
            }
            AwaitBranchOwner::PlainAsyncBranch => {
                await_requires_branch_owner(conditional.condition(), owner)
                    || await_requires_branch_owner(conditional.if_true(), owner)
                    || await_requires_branch_owner(conditional.if_false(), owner)
            }
        },
        Expression::Assign(assign) => match assign.op() {
            AssignOp::BoolAnd | AssignOp::BoolOr | AssignOp::Coalesce => match owner {
                AwaitBranchOwner::UnconditionalPrefix => {
                    contains(assign.rhs(), ContainsSymbol::AwaitExpression)
                        || assign_target_await_requires_branch_owner(assign.lhs(), owner)
                }
                AwaitBranchOwner::PlainAsyncBranch => {
                    !matches!(
                        assign.lhs(),
                        AssignTarget::Identifier(_) | AssignTarget::Access(_)
                    ) || assign_target_await_requires_branch_owner(assign.lhs(), owner)
                        || await_requires_branch_owner(assign.rhs(), owner)
                }
            },
            _ => {
                assign_target_await_requires_branch_owner(assign.lhs(), owner)
                    || await_requires_branch_owner(assign.rhs(), owner)
            }
        },
        Expression::Call(call) => {
            awaited_optional_call_reference_requires_owner(call.function(), owner)
                || await_requires_branch_owner(call.function(), owner)
                || call
                    .args()
                    .iter()
                    .any(|expression| await_requires_branch_owner(expression, owner))
        }
        Expression::New(new_expression) => {
            await_requires_branch_owner(new_expression.constructor(), owner)
                || new_expression
                    .arguments()
                    .iter()
                    .any(|expression| await_requires_branch_owner(expression, owner))
        }
        Expression::SuperCall(call) => match owner {
            AwaitBranchOwner::UnconditionalPrefix => true,
            AwaitBranchOwner::PlainAsyncBranch => call
                .arguments()
                .iter()
                .any(|argument| await_requires_branch_owner(argument, owner)),
        },
        Expression::PropertyAccess(access) => {
            property_access_await_requires_branch_owner(access, owner)
        }
        // Every shorted link guards the entire remaining suffix. Only the
        // checked source can enter the existing async branch owner.
        Expression::Optional(optional) => match owner {
            AwaitBranchOwner::UnconditionalPrefix => {
                optional
                    .chain()
                    .iter()
                    .any(|operation| contains(operation, ContainsSymbol::AwaitExpression))
                    || await_requires_branch_owner(optional.target(), owner)
            }
            AwaitBranchOwner::PlainAsyncBranch => {
                let chain_suspends = optional
                    .chain()
                    .iter()
                    .any(|operation| contains(operation, ContainsSymbol::AwaitExpression));
                if !chain_suspends {
                    // Target-only Await precedes the synchronous chain. Its
                    // existing emitter retains optional Call/private semantics.
                    return await_requires_branch_owner(optional.target(), owner);
                }
                AwaitedOptionalChainSource::new(optional).is_none()
                    || await_requires_branch_owner(optional.target(), owner)
                    || optional
                        .chain()
                        .iter()
                        .any(|operation| match operation.kind() {
                            OptionalOperationKind::SimplePropertyAccess { field } => match field {
                                PropertyAccessField::Const(_) => false,
                                PropertyAccessField::Expr(key) => {
                                    await_requires_branch_owner(key, owner)
                                }
                            },
                            OptionalOperationKind::Call { args } => args
                                .iter()
                                .any(|argument| await_requires_branch_owner(argument, owner)),
                            OptionalOperationKind::PrivatePropertyAccess { .. } => false,
                        })
            }
        },
        Expression::ArrayLiteral(array) => array
            .as_ref()
            .iter()
            .flatten()
            .any(|expression| await_requires_branch_owner(expression, owner)),
        Expression::ObjectLiteral(object) => object
            .properties()
            .iter()
            .any(|property| object_property_await_requires_branch_owner(property, owner)),
        Expression::Spread(spread) => await_requires_branch_owner(spread.target(), owner),
        Expression::TemplateLiteral(template) => template
            .elements()
            .iter()
            .filter_map(|element| match element {
                TemplateElement::Expr(expression) => Some(expression),
                TemplateElement::String(_) => None,
            })
            .any(|expression| await_requires_branch_owner(expression, owner)),
        Expression::TaggedTemplate(template) => {
            awaited_optional_call_reference_requires_owner(template.tag(), owner)
                || await_requires_branch_owner(template.tag(), owner)
                || template
                    .exprs()
                    .iter()
                    .any(|expression| await_requires_branch_owner(expression, owner))
        }
        Expression::ImportCall(call) => {
            await_requires_branch_owner(call.argument(), owner)
                || call
                    .options()
                    .is_some_and(|expression| await_requires_branch_owner(expression, owner))
        }
        Expression::ClassExpression(class) => {
            class_evaluation_expressions(class.super_ref(), class.elements())
                .any(|expression| await_requires_branch_owner(expression, owner))
        }
        // `contains` proved an `await` is in there, and this walk cannot show
        // it is always reached.
        _ => true,
    }
}

fn property_access_await_requires_branch_owner(
    access: &PropertyAccess,
    owner: AwaitBranchOwner,
) -> bool {
    match access {
        PropertyAccess::Simple(access) => {
            await_requires_branch_owner(access.target(), owner)
                || match access.field() {
                    PropertyAccessField::Const(_) => false,
                    PropertyAccessField::Expr(key) => await_requires_branch_owner(key, owner),
                }
        }
        PropertyAccess::Private(access) => await_requires_branch_owner(access.target(), owner),
        PropertyAccess::Super(access) => match access.field() {
            PropertyAccessField::Const(_) => false,
            PropertyAccessField::Expr(key) => await_requires_branch_owner(key, owner),
        },
    }
}

fn assign_target_await_requires_branch_owner(
    target: &AssignTarget,
    owner: AwaitBranchOwner,
) -> bool {
    match target {
        AssignTarget::Identifier(_) => false,
        AssignTarget::Access(access) => property_access_await_requires_branch_owner(access, owner),
        AssignTarget::Pattern(pattern) => {
            contains(pattern, ContainsSymbol::AwaitExpression)
                && !(matches!(owner, AwaitBranchOwner::PlainAsyncBranch)
                    && AsyncPatternSource::new(pattern).is_some())
        }
        AssignTarget::WebCompatCall(call) => call
            .args()
            .iter()
            .any(|expression| await_requires_branch_owner(expression, owner)),
    }
}

fn object_property_await_requires_branch_owner(
    property: &PropertyDefinition,
    owner: AwaitBranchOwner,
) -> bool {
    match property {
        PropertyDefinition::IdentifierReference(_) => false,
        PropertyDefinition::Property(name, value) => {
            property_name_await_requires_branch_owner(name, owner)
                || await_requires_branch_owner(value, owner)
        }
        PropertyDefinition::SpreadObject(source) => await_requires_branch_owner(source, owner),
        PropertyDefinition::MethodDefinition(method) => {
            property_name_await_requires_branch_owner(method.name(), owner)
        }
        PropertyDefinition::CoverInitializedName(_, _) => true,
    }
}

fn property_name_await_requires_branch_owner(name: &PropertyName, owner: AwaitBranchOwner) -> bool {
    match name {
        PropertyName::Literal(_) => false,
        PropertyName::Computed(key) => await_requires_branch_owner(key, owner),
    }
}

/// Annex B's invalid Reference head throws eagerly and never enters its body.
/// Its actual operands must still complete without an unowned suspension.
pub(crate) fn append_generator_for_in_suspensions(
    source: &boa_ast::statement::iteration::ForInLoop,
    cursor: &mut u32,
    points: &mut Vec<GeneratorSuspensionPointIr>,
) -> Option<()> {
    if matches!(
        source.initializer(),
        IterableLoopInitializer::WebCompatCall(_)
    ) {
        return (!contains(source.initializer(), ContainsSymbol::YieldExpression)
            && !contains(source.initializer(), ContainsSymbol::AwaitExpression)
            && !contains(source.target(), ContainsSymbol::YieldExpression)
            && !contains(source.target(), ContainsSymbol::AwaitExpression))
        .then_some(());
    }
    crate::async_generator_source::AsyncGeneratorForInSource::for_execution(
        source,
        ResumableRegionProtocolIr::Generator,
    )?;
    let mut actual = Vec::new();
    crate::async_generator_source::append_complete_for_in_source(
        source,
        ResumableRegionProtocolIr::Generator,
        cursor,
        &mut actual,
    )?;
    points.extend(actual.into_iter().map(|point| GeneratorSuspensionPointIr {
        suspend_state: point.suspend_state,
        resume_state: point.resume_state,
    }));
    Some(())
}
