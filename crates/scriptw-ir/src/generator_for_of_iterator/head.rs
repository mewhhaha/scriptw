use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GeneratorForOfAssignmentError {
    SpellableSink,
    HeadEnvironment,
    PersistentSink,
    InvalidPrefix,
    PrefixMismatch,
    SinkEscapesBody,
}

/// A prepared identifier or ordinary-property PutValue retains its actual
/// prefix until the plan binds it to the checked body. Neither domain can
/// substitute a raw property write or borrow the other's source proof.
#[must_use]
#[derive(Debug)]
pub(crate) struct GeneratorForOfAssignmentIr {
    value_name: String,
    prefix: StatementIr,
}

impl GeneratorForOfAssignmentIr {
    pub(crate) fn identifier<'a>(
        source_name: &str,
        value_name: String,
        prefix: &StatementIr,
        head_environment: Option<&ForInOfEnvironmentIr>,
        ignored: Option<&IdentifierWriteReferenceIr>,
        persistent_names: impl IntoIterator<Item = &'a str>,
        functions: &[FunctionIr],
    ) -> Result<Self, GeneratorForOfAssignmentError> {
        let ignored = ignored.is_some_and(|reference| {
            reference.name() == source_name
                && matches!(
                    reference.write_disposition(),
                    IdentifierWriteDisposition::IgnoreImmutableBinding
                )
        });
        Self::from_prefix(
            value_name,
            prefix,
            head_environment,
            persistent_names,
            functions,
            |write, value| identifier_write(write, source_name, value, ignored),
        )
    }

    pub(crate) fn ordinary_property<'a>(
        value_name: String,
        prefix: &StatementIr,
        head_environment: Option<&ForInOfEnvironmentIr>,
        persistent_names: impl IntoIterator<Item = &'a str>,
        functions: &[FunctionIr],
    ) -> Result<Self, GeneratorForOfAssignmentError> {
        Self::from_prefix(
            value_name,
            prefix,
            head_environment,
            persistent_names,
            functions,
            ordinary_property_write,
        )
    }

    fn from_prefix<'a>(
        value_name: String,
        prefix: &StatementIr,
        head_environment: Option<&ForInOfEnvironmentIr>,
        persistent_names: impl IntoIterator<Item = &'a str>,
        functions: &[FunctionIr],
        valid_write: impl FnOnce(&TypedExpr, &str) -> bool,
    ) -> Result<Self, GeneratorForOfAssignmentError> {
        if head_environment.is_some() {
            return Err(GeneratorForOfAssignmentError::HeadEnvironment);
        }
        validate_entry_local_sink(&value_name, persistent_names, functions)?;
        let StatementIr::DeclarationEvaluation(write) = prefix else {
            return Err(GeneratorForOfAssignmentError::InvalidPrefix);
        };
        if !valid_write(write, &value_name) {
            return Err(GeneratorForOfAssignmentError::InvalidPrefix);
        }
        Ok(Self {
            value_name,
            prefix: prefix.clone(),
        })
    }

    fn complete(
        self,
        body: &GeneratorForOfBodyIr,
    ) -> Result<GeneratorForOfIteratorHeadIr, GeneratorForOfAssignmentError> {
        let Some((prefix, rest)) = body.statements().split_first() else {
            return Err(GeneratorForOfAssignmentError::PrefixMismatch);
        };
        if *prefix != self.prefix {
            return Err(GeneratorForOfAssignmentError::PrefixMismatch);
        }
        if crate::ir::statements_reference_storage(rest, &self.value_name) {
            return Err(GeneratorForOfAssignmentError::SinkEscapesBody);
        }
        Ok(GeneratorForOfIteratorHeadIr::Assignment {
            value_name: self.value_name,
            iteration_environment: ResumableLoopIterationEnvironmentIr::StorageOnly,
        })
    }
}

fn validate_entry_local_sink<'a>(
    value_name: &str,
    persistent_names: impl IntoIterator<Item = &'a str>,
    functions: &[FunctionIr],
) -> Result<(), GeneratorForOfAssignmentError> {
    if !value_name
        .strip_prefix("$forof.assignment.")
        .is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(GeneratorForOfAssignmentError::SpellableSink);
    }
    if persistent_names.into_iter().any(|name| name == value_name)
        || functions.iter().any(|function| {
            let owned = &function.owned_env_bindings;
            let captured = &function.captured_bindings;
            owned.iter().any(|binding| binding.name == value_name)
                || captured.iter().any(|binding| binding.name == value_name)
        })
    {
        return Err(GeneratorForOfAssignmentError::PersistentSink);
    }
    Ok(())
}

/// The checked lexical initializer retains its full eager prefix until the
/// generator binds that exact prefix to the resumable body.
#[must_use]
#[derive(Debug)]
pub(crate) struct GeneratorForOfLexicalPatternIr {
    pattern: ValidatedResumableSyncForOfLexicalPatternIr,
}

impl GeneratorForOfLexicalPatternIr {
    pub(crate) fn new<'a>(
        pattern: ValidatedResumableSyncForOfLexicalPatternIr,
        persistent_names: impl IntoIterator<Item = &'a str>,
        functions: &[FunctionIr],
    ) -> Result<Self, GeneratorForOfAssignmentError> {
        validate_entry_local_sink(pattern.value_name(), persistent_names, functions)?;
        let [StatementIr::DeclarationEvaluation(initializer)] = pattern.initialization() else {
            return Err(GeneratorForOfAssignmentError::InvalidPrefix);
        };
        let mut nested = Vec::new();
        let value = match &initializer.expr {
            ExprIr::ArrayDestructure {
                value,
                pattern,
                evaluation: ArrayDestructuringEvaluationIr::BindingInitialization,
            } => {
                pattern.visit_expressions(&mut |expr| {
                    nested.push(StatementIr::DeclarationEvaluation(expr.clone()))
                });
                value
            }
            ExprIr::ObjectDestructure { value, pattern } => {
                pattern.visit_expressions(&mut |expr| {
                    nested.push(StatementIr::DeclarationEvaluation(expr.clone()))
                });
                value
            }
            _ => return Err(GeneratorForOfAssignmentError::InvalidPrefix),
        };
        if !sink(value, pattern.value_name())
            || !crate::ir::entry_local_storage_has_only_dynamic_reads(
                pattern.initialization(),
                pattern.value_name(),
            )
            || crate::ir::statements_reference_storage(&nested, pattern.value_name())
        {
            return Err(GeneratorForOfAssignmentError::InvalidPrefix);
        }
        Ok(Self { pattern })
    }

    fn complete(
        self,
        body: &GeneratorForOfBodyIr,
    ) -> Result<GeneratorForOfIteratorHeadIr, GeneratorForOfAssignmentError> {
        let count = self.pattern.initialization().len();
        let Some(prefix) = body.statements().get(..count) else {
            return Err(GeneratorForOfAssignmentError::PrefixMismatch);
        };
        if prefix != self.pattern.initialization() {
            return Err(GeneratorForOfAssignmentError::PrefixMismatch);
        }
        if crate::ir::statements_reference_storage(
            &body.statements()[count..],
            self.pattern.value_name(),
        ) {
            return Err(GeneratorForOfAssignmentError::SinkEscapesBody);
        }
        Ok(GeneratorForOfIteratorHeadIr::LexicalPattern(self.pattern))
    }
}

fn sink(expr: &TypedExpr, name: &str) -> bool {
    expr.kind == ValueKind::Dynamic
        && expr.possible_kinds == KindSet::all_runtime_tags()
        && matches!(&expr.expr, ExprIr::Identifier(storage) if storage == name)
}

fn ordinary_property_write(expr: &TypedExpr, value: &str) -> bool {
    let ExprIr::OrdinaryPropertyAssignment(assignment) = &expr.expr else {
        return false;
    };
    if !sink(assignment.rhs(), value) {
        return false;
    }
    let mut reference_operands = vec![StatementIr::DeclarationEvaluation(
        assignment.base_and_receiver().clone(),
    )];
    match assignment.referenced_name() {
        PropertyKeyIr::StringExpr(key) | PropertyKeyIr::ArrayIndex(key) => {
            reference_operands.push(StatementIr::DeclarationEvaluation((**key).clone()));
        }
        PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => {}
    }
    !crate::ir::statements_reference_storage(&reference_operands, value)
}

fn identifier_write(expr: &TypedExpr, source: &str, value: &str, ignored: bool) -> bool {
    match &expr.expr {
        ExprIr::AssignIdentifier {
            name,
            value: operand,
        } => name != value && sink(operand, value),
        ExprIr::GlobalPropertyWrite {
            name,
            value: operand,
            ..
        } => name == source && sink(operand, value),
        ExprIr::EnvironmentIdentifier(identifier) => {
            identifier.name == source
                && matches!(&identifier.operation,
                    EnvironmentIdentifierOperationIr::Assign { value: operand }
                        | EnvironmentIdentifierOperationIr::AssignWithGlobalFallback { value: operand, .. }
                    if sink(operand, value))
        }
        ExprIr::Comma { lhs, rhs } => {
            sink(lhs, value)
                && matches!(&rhs.expr, ExprIr::RuntimeThrow { name, message }
                if [IdentifierWriteErrorIr::UninitializedBinding, IdentifierWriteErrorIr::ImmutableBinding, IdentifierWriteErrorIr::ImmutableClassName]
                    .into_iter().any(|error| *name == error.kind() && *message == error.message()))
        }
        ExprIr::Identifier(_) => ignored && sink(expr, value),
        ExprIr::Conditional {
            condition,
            then_expr,
            else_expr,
        } => {
            let ExprIr::SpecOperation {
                operation: SpecOperationIr::WithEnvironmentHasBinding,
                operands,
            } = &condition.expr
            else {
                return false;
            };
            let [object, name] = operands.as_slice() else {
                return false;
            };
            matches!(&name.expr, ExprIr::String(name) if name == source)
                && with_write(then_expr, object, source, value)
                && identifier_write(else_expr, source, value, ignored)
        }
        _ => false,
    }
}

/// Check the existing selected Object-ER PutValue template, including its
/// fresh HasProperty check. This admits no ordinary source property head.
fn with_write(expr: &TypedExpr, object: &TypedExpr, source: &str, value: &str) -> bool {
    let ExprIr::MaterializeBinding {
        name,
        value: operand,
        body,
    } = &expr.expr
    else {
        return false;
    };
    if !sink(operand, value) {
        return false;
    }
    let checked_set = |write: &TypedExpr| {
        matches!(&write.expr,
        ExprIr::PropertyWrite { target, key: PropertyKeyIr::StaticString(key), value: rhs, .. }
        if **target == *object && key == source && sink(rhs, name))
    };
    let recheck = |expr: &TypedExpr| {
        matches!(&expr.expr,
        ExprIr::SpecOperation { operation: SpecOperationIr::HasProperty, operands }
        if operands.len() == 2 && operands[0] == *object
            && matches!(&operands[1].expr, ExprIr::String(name) if name == source))
    };
    match &body.expr {
        ExprIr::MaterializeBinding {
            value: check,
            body: write,
            ..
        } => recheck(check) && checked_set(write),
        ExprIr::Conditional {
            condition,
            then_expr,
            else_expr,
        } => {
            recheck(condition)
                && checked_set(then_expr)
                && matches!(
                    &else_expr.expr,
                    ExprIr::RuntimeThrow {
                        name: NativeErrorKind::ReferenceError,
                        ..
                    }
                )
        }
        _ => false,
    }
}

#[derive(Debug)]
pub(crate) enum GeneratorForOfIteratorHeadInputIr {
    Binding(ValidatedResumableSyncForOfBindingIr),
    Assignment(GeneratorForOfAssignmentIr),
    LexicalPattern(GeneratorForOfLexicalPatternIr),
}

impl GeneratorForOfIteratorHeadInputIr {
    pub(super) fn complete(
        self,
        body: &GeneratorForOfBodyIr,
    ) -> Result<GeneratorForOfIteratorHeadIr, GeneratorForOfAssignmentError> {
        match self {
            Self::Binding(binding) => Ok(GeneratorForOfIteratorHeadIr::Binding(binding)),
            Self::Assignment(assignment) => assignment.complete(body),
            Self::LexicalPattern(pattern) => pattern.complete(body),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GeneratorForOfIteratorHeadIr {
    Binding(ValidatedResumableSyncForOfBindingIr),
    LexicalPattern(ValidatedResumableSyncForOfLexicalPatternIr),
    Assignment {
        value_name: String,
        iteration_environment: ResumableLoopIterationEnvironmentIr,
    },
}

impl GeneratorForOfIteratorHeadIr {
    pub(super) fn storage(&self) -> GeneratorForOfIteratorValueStorageIr<'_> {
        match self {
            Self::Binding(binding) => match binding.storage() {
                ResumableSyncForOfBindingStorageIr::Activation(binding) => {
                    GeneratorForOfIteratorValueStorageIr::Activation(binding)
                }
                ResumableSyncForOfBindingStorageIr::IterationEnvironment(binding) => {
                    GeneratorForOfIteratorValueStorageIr::IterationEnvironment(binding)
                }
            },
            Self::LexicalPattern(pattern) => GeneratorForOfIteratorValueStorageIr::EntryLocal {
                name: pattern.value_name(),
            },
            Self::Assignment { value_name, .. } => {
                GeneratorForOfIteratorValueStorageIr::EntryLocal { name: value_name }
            }
        }
    }
    pub(super) fn binding(&self) -> Option<&ForOfAssignmentIr> {
        match self {
            Self::Binding(binding) => Some(binding.binding()),
            Self::Assignment { .. } | Self::LexicalPattern(_) => None,
        }
    }
    pub(super) fn head_environment(&self) -> Option<&ForInOfEnvironmentIr> {
        match self {
            Self::Binding(binding) => binding.head_environment(),
            Self::LexicalPattern(pattern) => Some(pattern.head_environment()),
            Self::Assignment { .. } => None,
        }
    }
    pub(super) fn head_binding_environment(&self) -> Option<(BindingMode, &ForInOfEnvironmentIr)> {
        match self {
            Self::Binding(binding) => binding
                .head_environment()
                .map(|environment| (binding.binding().mode, environment)),
            Self::LexicalPattern(pattern) => Some((pattern.mode(), pattern.head_environment())),
            Self::Assignment { .. } => None,
        }
    }
    pub(super) fn iteration_environment(&self) -> &ResumableLoopIterationEnvironmentIr {
        match self {
            Self::Binding(binding) => binding.iteration_environment(),
            Self::LexicalPattern(pattern) => pattern.iteration_environment(),
            Self::Assignment {
                iteration_environment,
                ..
            } => iteration_environment,
        }
    }
}
