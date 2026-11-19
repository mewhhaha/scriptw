use super::*;
use boa_ast::expression::{Call, Expression, Identifier};
use boa_ast::visitor::{VisitWith, Visitor};
use boa_ast::StatementList;
use boa_interner::Sym;
use core::ops::ControlFlow;

/// A mixed whole owner must carry evidence from this exact analyzed With.
/// Only the closed FunctionBody census can mint the token from checked source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MixedAsyncGeneratorWithOwner {
    source_identity: usize,
}

impl MixedAsyncGeneratorWithOwner {
    fn from_checked_source(
        source: crate::async_generator_source::AsyncGeneratorWithSource<'_>,
    ) -> Self {
        Self {
            source_identity: source.source() as *const boa_ast::statement::With as usize,
        }
    }

    pub(crate) fn checked_source<'ast>(
        self,
        source: &'ast boa_ast::statement::With,
    ) -> Option<crate::async_generator_source::AsyncGeneratorWithSource<'ast>> {
        if self.source_identity != source as *const boa_ast::statement::With as usize {
            return None;
        }
        crate::async_generator_source::AsyncGeneratorWithSource::new(source)
    }
}

/// Only the actual checked FunctionBody ForIn can own complete mixed phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompleteResumableForInOwner {
    source_identity: crate::lowering_helpers::ForInSourceIdentity,
    execution: crate::ResumableRegionProtocolIr,
}

impl CompleteResumableForInOwner {
    fn from_checked_source(
        source: crate::async_generator_source::AsyncGeneratorForInSource<'_>,
    ) -> Self {
        Self {
            source_identity: crate::lowering_helpers::ForInSourceIdentity::from_source(
                source.source(),
            ),
            execution: source.execution(),
        }
    }

    pub(crate) fn checked_source<'ast>(
        self,
        source: &'ast boa_ast::statement::iteration::ForInLoop,
    ) -> Option<crate::async_generator_source::AsyncGeneratorForInSource<'ast>> {
        if self.source_identity != crate::lowering_helpers::ForInSourceIdentity::from_source(source)
        {
            return None;
        }
        crate::async_generator_source::AsyncGeneratorForInSource::for_execution(
            source,
            self.execution,
        )
    }
}

/// Only this actual checked source can keep a mixed iterator in FunctionBody.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompleteResumableForOfOwner {
    source_identity: crate::async_generator_source::AsyncGeneratorForOfSourceIdentity,
    execution: crate::ResumableRegionProtocolIr,
}
impl CompleteResumableForOfOwner {
    fn from_checked_source(
        source: crate::async_generator_source::AsyncGeneratorForOfSource<'_>,
    ) -> Self {
        Self {
            source_identity:
                crate::async_generator_source::AsyncGeneratorForOfSourceIdentity::from_source(
                    source.source(),
                ),
            execution: source.execution(),
        }
    }
    pub(crate) fn checked_source<'ast>(
        self,
        source: &'ast ForOfLoop,
    ) -> Option<crate::async_generator_source::AsyncGeneratorForOfSource<'ast>> {
        if self.source_identity
            != crate::async_generator_source::AsyncGeneratorForOfSourceIdentity::from_source(source)
        {
            return None;
        }
        crate::async_generator_source::AsyncGeneratorForOfSource::for_execution(
            source,
            self.execution,
        )
    }
}

/// Boa's `Contains` stops at method definitions, so it misses direct eval in
/// object and class method bodies. Eval visibility needs the whole tree.
pub(super) fn contains_direct_eval_call(script: &Script) -> bool {
    struct DirectEvalCalls;
    impl<'ast> Visitor<'ast> for DirectEvalCalls {
        type BreakTy = ();
        fn visit_call(&mut self, call: &'ast Call) -> ControlFlow<Self::BreakTy> {
            if matches!(call.function().flatten(), Expression::Identifier(identifier)
                if identifier.sym() == Sym::EVAL)
            {
                return ControlFlow::Break(());
            }
            call.visit_with(self)
        }
    }
    script.visit_with(&mut DirectEvalCalls).is_break()
}

/// Annex B can create an `arguments` variable binding when a block declaration
/// executes, rather than during entry. Its references need live named-record
/// resolution even when the source never calls eval.
pub(super) fn contains_annex_b_arguments_candidate(script: &Script) -> bool {
    struct ArgumentsDeclarations;
    impl<'ast> Visitor<'ast> for ArgumentsDeclarations {
        type BreakTy = ();
        fn visit_function_declaration(
            &mut self,
            declaration: &'ast boa_ast::function::FunctionDeclaration,
        ) -> ControlFlow<Self::BreakTy> {
            if declaration.name().sym() == Sym::ARGUMENTS {
                return ControlFlow::Break(());
            }
            declaration.visit_with(self)
        }
    }
    script.visit_with(&mut ArgumentsDeclarations).is_break()
}

pub(super) fn source_identifiers(script: &Script, interner: &Interner) -> BTreeSet<String> {
    struct SourceIdentifiers<'a> {
        interner: &'a Interner,
        names: BTreeSet<String>,
    }
    impl<'ast> Visitor<'ast> for SourceIdentifiers<'_> {
        type BreakTy = core::convert::Infallible;
        fn visit_identifier(&mut self, identifier: &'ast Identifier) -> ControlFlow<Self::BreakTy> {
            self.names
                .insert(self.interner.resolve_expect(identifier.sym()).to_string());
            ControlFlow::Continue(())
        }
    }
    let mut visitor = SourceIdentifiers {
        interner,
        names: BTreeSet::new(),
    };
    let _ = script.visit_with(&mut visitor);
    visitor.names
}

/// With and ForIn function-body phases stay separate from foreign ForOf bodies.
/// IteratorBody is sticky throughout its nested statements; nested callables and
/// class bodies have their own analyzed execution owners.
struct WithContinuationOwners {
    execution: FunctionExecutionKind,
    region: crate::lowering_helpers::GeneratorSuspensionRegion,
    owners: BTreeMap<usize, WithContinuationOwner>,
    for_in_owners: BTreeMap<usize, ForInContinuationOwner>,
    for_of_owners: BTreeMap<usize, CompleteResumableForOfOwner>,
}

impl<'ast> Visitor<'ast> for WithContinuationOwners {
    type BreakTy = core::convert::Infallible;

    fn visit_statement_list(&mut self, source: &'ast StatementList) -> ControlFlow<Self::BreakTy> {
        let previous = self.region;
        let execution = match self.execution {
            FunctionExecutionKind::Generator => Some(crate::ResumableRegionProtocolIr::Generator),
            FunctionExecutionKind::Async => Some(crate::ResumableRegionProtocolIr::Async),
            FunctionExecutionKind::AsyncGenerator => {
                Some(crate::ResumableRegionProtocolIr::AsyncGenerator)
            }
            FunctionExecutionKind::Ordinary => None,
        };
        let owns_resources = previous
            == crate::lowering_helpers::GeneratorSuspensionRegion::FunctionBody
            && execution.is_some_and(|execution| {
                crate::async_generator_source::AsyncGeneratorResourceScopeSource::for_protocol(
                    source.statements(),
                    execution,
                )
                .is_some()
            });
        for item in source.statements() {
            self.visit_statement_list_item(item)?;
            if self.execution == FunctionExecutionKind::AsyncGenerator
                && !owns_resources
                && crate::async_generator_source::item_enters_foreign_suffix(item)
            {
                self.region = crate::lowering_helpers::GeneratorSuspensionRegion::IteratorBody;
            }
        }
        self.region = previous;
        ControlFlow::Continue(())
    }

    fn visit_statement(&mut self, statement: &'ast Statement) -> ControlFlow<Self::BreakTy> {
        use crate::lowering_helpers::GeneratorSuspensionRegion;

        match statement {
            Statement::WhileLoop(_) | Statement::DoWhileLoop(_) | Statement::ForLoop(_)
                if self.execution == FunctionExecutionKind::Async =>
            {
                let previous = self.region;
                use crate::async_generator_source::PlainAsyncClassicLoopSource;
                let checked = match statement {
                    Statement::WhileLoop(source) => {
                        PlainAsyncClassicLoopSource::While(source).plan(0)
                    }
                    Statement::DoWhileLoop(source) => {
                        PlainAsyncClassicLoopSource::DoWhile(source).plan(0)
                    }
                    Statement::ForLoop(source) => PlainAsyncClassicLoopSource::For(source).plan(0),
                    _ => unreachable!("the outer match selected a classic loop"),
                };
                if previous != GeneratorSuspensionRegion::FunctionBody || checked.is_none() {
                    self.region = GeneratorSuspensionRegion::IteratorBody;
                }
                let result = statement.visit_with(self);
                self.region = previous;
                return result;
            }
            Statement::ForOfLoop(source) => {
                let previous = self.region;
                let execution = match self.execution {
                    FunctionExecutionKind::Generator => {
                        Some(crate::ResumableRegionProtocolIr::Generator)
                    }
                    FunctionExecutionKind::Async => Some(crate::ResumableRegionProtocolIr::Async),
                    FunctionExecutionKind::AsyncGenerator => {
                        Some(crate::ResumableRegionProtocolIr::AsyncGenerator)
                    }
                    FunctionExecutionKind::Ordinary => None,
                };
                let checked = (previous == GeneratorSuspensionRegion::FunctionBody)
                    .then(|| {
                        execution.and_then(|execution| {
                            crate::async_generator_source::AsyncGeneratorForOfSource::for_execution(
                                source, execution,
                            )
                        })
                    })
                    .flatten();
                if let Some(checked) = checked {
                    self.for_of_owners.insert(
                        source as *const ForOfLoop as usize,
                        CompleteResumableForOfOwner::from_checked_source(checked),
                    );
                } else {
                    self.region = GeneratorSuspensionRegion::IteratorBody;
                }
                let result = statement.visit_with(self);
                self.region = previous;
                return result;
            }
            Statement::Switch(source)
                if self.execution == FunctionExecutionKind::AsyncGenerator =>
            {
                let previous = self.region;
                // Only the actual checked whole source may keep the enclosing
                // FunctionBody domain. Foreign ancestry remains sticky even
                // when the same Switch shape would be lawful on its own.
                if previous != GeneratorSuspensionRegion::FunctionBody
                    || crate::async_generator_source::AsyncGeneratorSwitchSource::new(source)
                        .is_none()
                {
                    self.region = GeneratorSuspensionRegion::IteratorBody;
                }
                let result = statement.visit_with(self);
                self.region = previous;
                return result;
            }
            Statement::ForInLoop(source) => {
                let owner = match self.region {
                    GeneratorSuspensionRegion::FunctionBody
                        if !matches!(
                            source.initializer(),
                            IterableLoopInitializer::WebCompatCall(_)
                        ) =>
                    {
                        let execution = match self.execution {
                            FunctionExecutionKind::Generator => {
                                Some(crate::ResumableRegionProtocolIr::Generator)
                            }
                            FunctionExecutionKind::Async => {
                                Some(crate::ResumableRegionProtocolIr::Async)
                            }
                            FunctionExecutionKind::AsyncGenerator => {
                                Some(crate::ResumableRegionProtocolIr::AsyncGenerator)
                            }
                            FunctionExecutionKind::Ordinary => None,
                        };
                        execution.and_then(|execution|crate::async_generator_source::AsyncGeneratorForInSource::for_execution(source,execution))
                            .map(CompleteResumableForInOwner::from_checked_source)
                            .map(ForInContinuationOwner::CompleteWhole)
                            .unwrap_or(ForInContinuationOwner::ImmediateOrLinear)
                    }
                    GeneratorSuspensionRegion::FunctionBody
                    | GeneratorSuspensionRegion::IteratorBody => {
                        ForInContinuationOwner::ImmediateOrLinear
                    }
                };
                self.for_in_owners.insert(
                    source as *const boa_ast::statement::iteration::ForInLoop as usize,
                    owner,
                );
                if matches!(
                    source.initializer(),
                    IterableLoopInitializer::WebCompatCall(_)
                ) {
                    return ControlFlow::Continue(());
                }
                if !matches!(owner, ForInContinuationOwner::CompleteWhole(_)) {
                    // Only the checked actual source keeps FunctionBody.
                    // Unsupported heads and foreign ancestry retain their
                    // original eager/linear owner and body domain.
                    let previous = self.region;
                    self.region = GeneratorSuspensionRegion::IteratorBody;
                    let result = statement.visit_with(self);
                    self.region = previous;
                    return result;
                }
            }
            Statement::With(with) => {
                let owner = match self.region {
                    GeneratorSuspensionRegion::FunctionBody => match self.execution {
                        FunctionExecutionKind::Generator => WithContinuationOwner::OrdinaryWhole,
                        FunctionExecutionKind::Async => WithContinuationOwner::PlainAsyncWhole,
                        FunctionExecutionKind::AsyncGenerator => {
                            crate::async_generator_source::AsyncGeneratorWithSource::new(with)
                                .map(MixedAsyncGeneratorWithOwner::from_checked_source)
                                .map(WithContinuationOwner::MixedAsyncGeneratorWhole)
                                .unwrap_or(WithContinuationOwner::LinearResumable)
                        }
                        FunctionExecutionKind::Ordinary => {
                            unreachable!(
                                "only admitted complete function protocols visit this census"
                            )
                        }
                    },
                    GeneratorSuspensionRegion::IteratorBody => {
                        WithContinuationOwner::LinearResumable
                    }
                };
                self.owners
                    .insert(with as *const boa_ast::statement::With as usize, owner);
            }
            Statement::Var(_)
            | Statement::Empty
            | Statement::Debugger
            | Statement::Expression(_)
            | Statement::Block(_)
            | Statement::If(_)
            | Statement::WhileLoop(_)
            | Statement::DoWhileLoop(_)
            | Statement::ForLoop(_)
            | Statement::Switch(_)
            | Statement::Labelled(_)
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Throw(_)
            | Statement::Try(_)
            | Statement::Return(_) => {}
        }
        statement.visit_with(self)
    }

    fn visit_function_body(&mut self, _: &'ast FunctionBody) -> ControlFlow<Self::BreakTy> {
        ControlFlow::Continue(())
    }

    fn visit_class_declaration(&mut self, _: &'ast ClassDeclaration) -> ControlFlow<Self::BreakTy> {
        ControlFlow::Continue(())
    }

    fn visit_class_expression(&mut self, _: &'ast ClassExpression) -> ControlFlow<Self::BreakTy> {
        ControlFlow::Continue(())
    }
}

impl AnalysisBuilder<'_> {
    pub(super) fn finalize_with_continuation_owners(&mut self) {
        let sources = self
            .function_plans
            .values()
            .map(|function| (function.protocol.execution_kind(), function.body))
            .chain(
                self.class_method_bodies
                    .iter()
                    .map(|(owner, body)| (self.owner_plans[owner].execution_kind, *body)),
            )
            .filter(|(execution, _)| {
                matches!(
                    execution,
                    FunctionExecutionKind::Generator
                        | FunctionExecutionKind::Async
                        | FunctionExecutionKind::AsyncGenerator
                )
            })
            .collect::<Vec<_>>();
        for (execution, body) in sources {
            let mut visitor = WithContinuationOwners {
                execution,
                region: crate::lowering_helpers::GeneratorSuspensionRegion::FunctionBody,
                owners: BTreeMap::new(),
                for_in_owners: BTreeMap::new(),
                for_of_owners: BTreeMap::new(),
            };
            let _ = body.visit_with(&mut visitor);
            // Retain the original per-key lexical record only for a checked
            // mixed source owner. Its source BoundNames keep their existing
            // TDZ and iteration mappings, without activation duplicates.
            for source in visitor.for_of_owners.keys() {
                if let Some(environment) = self.for_in_of_iteration_environment_ids.get(source) {
                    self.complete_resumable_for_of_iteration_environment_ids
                        .insert(*environment);
                }
            }
            self.complete_for_of_owners.extend(visitor.for_of_owners);
            for (source, owner) in visitor.for_in_owners {
                let assigned = self
                    .for_in_continuation_owners
                    .get_mut(&source)
                    .expect("source ForIn must have its actual analyzed continuation owner");
                *assigned = owner;
            }
            for (source, continuation_owner) in visitor.owners {
                let environment_id = self.with_environment_ids[&source];
                self.with_object_environment_plans
                    .get_mut(&environment_id)
                    .expect("source With must name its analyzed Object Environment Record")
                    .continuation_owner = continuation_owner;
            }
        }
    }

    pub(super) fn finalize_eval_environment_roles(&mut self, interner: &Interner) {
        let global_variable_environment = self
            .script_instantiation
            .has_global_variable_environment(self.owner_plans[SCRIPT_OWNER_ID].strict);
        let borrowed_variable_environment = matches!(
            &self.script_instantiation,
            ScriptInstantiation::Prepared(PreparedScriptKind::DirectEval(_))
        ) && !self.owner_plans[SCRIPT_OWNER_ID].strict;
        for environment in self.environment_plans.values_mut().filter(|environment| {
            environment.eval_visible
                || (environment.kind == EnvironmentKind::WithObject
                    && !environment.owned_env_slots.is_empty())
        }) {
            if environment.kind == EnvironmentKind::WithObject {
                let object = &self.with_object_environment_plans[&environment.id];
                environment.eval_environment = Some(EvalEnvironmentRoleIr::WithObject {
                    object_slot: environment.owned_env_slots[object.binding_name.as_str()],
                });
                continue;
            }
            let parameter_names = &self.owner_plans[&environment.owner_id].parameter_names;
            let bindings = environment
                .owned_env_slots
                .iter()
                .filter_map(|(storage_name, slot)| {
                    let source_name = if storage_name == LEXICAL_ARGUMENTS_NAME {
                        if environment.binding_storage_names.contains("arguments") {
                            return None;
                        }
                        "arguments".to_string()
                    } else if let Some(source_name) = self.source_names_by_storage.get(storage_name)
                    {
                        source_name.clone()
                    } else if self.source_identifiers.contains(storage_name) {
                        storage_name.clone()
                    } else {
                        return None;
                    };
                    let mode = environment.binding_modes[storage_name];
                    if matches!(
                        environment.kind,
                        EnvironmentKind::Activation | EnvironmentKind::FunctionParameters
                    ) && mode == BindingMode::Const
                        && self
                            .function_plans
                            .get(&environment.owner_id)
                            .is_some_and(|plan| {
                                plan.self_binding_name.as_deref() == Some(source_name.as_str())
                                    && (self.owner_plans[&environment.owner_id]
                                        .body_environment_id
                                        .is_some()
                                        || !lexically_declared_names(plan.body).into_iter().any(
                                            |name| {
                                                interner.resolve_expect(name).to_string()
                                                    == source_name
                                            },
                                        ))
                            })
                    {
                        return None;
                    }
                    let declaration =
                        if environment.kind == EnvironmentKind::NamedFunctionExpression {
                            EvalBindingDeclarationIr::NamedFunctionExpression
                        } else if matches!(
                            environment.kind,
                            EnvironmentKind::Activation | EnvironmentKind::FunctionParameters
                        ) && parameter_names.contains(&source_name)
                        {
                            EvalBindingDeclarationIr::Parameter
                        } else if mode == BindingMode::Var
                            || storage_name == LEXICAL_ARGUMENTS_NAME
                            || (matches!(
                                environment.kind,
                                EnvironmentKind::Activation
                                    | EnvironmentKind::FunctionParameters
                                    | EnvironmentKind::FunctionBody
                            ) && self.owner_plans[&environment.owner_id]
                                .function_bindings
                                .contains_key(&source_name))
                        {
                            EvalBindingDeclarationIr::Variable
                        } else {
                            EvalBindingDeclarationIr::Lexical
                        };
                    if environment.owner_id == SCRIPT_OWNER_ID
                        && (global_variable_environment || borrowed_variable_environment)
                        && declaration == EvalBindingDeclarationIr::Variable
                    {
                        return None;
                    }
                    Some(EvalVisibleBindingIr {
                        source_name,
                        slot: *slot,
                        mode,
                        declaration,
                    })
                })
                .collect();
            let kind = match environment.kind {
                EnvironmentKind::Activation
                    if environment.owner_id == SCRIPT_OWNER_ID && borrowed_variable_environment =>
                {
                    EvalDeclarativeEnvironmentKindIr::Lexical
                }
                EnvironmentKind::Activation
                    if environment.owner_id == SCRIPT_OWNER_ID && global_variable_environment =>
                {
                    EvalDeclarativeEnvironmentKindIr::GlobalLexical
                }
                EnvironmentKind::Activation | EnvironmentKind::FunctionParameters
                    if self.owner_plans[&environment.owner_id]
                        .parameter_eval_environment_id
                        .is_some() =>
                {
                    EvalDeclarativeEnvironmentKindIr::Parameters
                }
                EnvironmentKind::Activation
                | EnvironmentKind::FunctionParameters
                | EnvironmentKind::FunctionBody
                | EnvironmentKind::ParameterEvalVariable => {
                    EvalDeclarativeEnvironmentKindIr::Variable
                }
                EnvironmentKind::CatchParameter => EvalDeclarativeEnvironmentKindIr::Catch,
                EnvironmentKind::SimpleCatchParameter => {
                    EvalDeclarativeEnvironmentKindIr::SimpleCatch
                }
                EnvironmentKind::Block
                | EnvironmentKind::NamedFunctionExpression
                | EnvironmentKind::ClassName
                | EnvironmentKind::SwitchCaseBlock
                | EnvironmentKind::ForLexicalHead
                | EnvironmentKind::ForInOfTdzHead
                | EnvironmentKind::ForInOfIteration => EvalDeclarativeEnvironmentKindIr::Lexical,
                EnvironmentKind::WithObject => unreachable!("object record handled above"),
            };
            environment.eval_environment =
                Some(EvalEnvironmentRoleIr::Declarative { kind, bindings });
        }
    }
}

#[cfg(test)]
#[path = "eval_environment/mixed_with_tests.rs"]
mod mixed_with_tests;

#[cfg(test)]
#[path = "eval_environment/mixed_switch_tests.rs"]
mod mixed_switch_tests;

#[cfg(test)]
#[path = "eval_environment/mixed_for_in_tests.rs"]
mod mixed_for_in_tests;

impl Analysis<'_> {
    pub(crate) fn owner_eval_environment(&self, owner_id: &str) -> Option<EvalEnvironmentRoleIr> {
        self.owner_plans.get(owner_id).and_then(|owner| {
            self.environment_plans[&owner.activation_environment_id]
                .eval_environment
                .clone()
        })
    }
}

pub(super) fn parameter_names(
    interner: &Interner,
    parameters: &[boa_ast::function::FormalParameter],
) -> BTreeSet<String> {
    parameters
        .iter()
        .flat_map(|parameter| {
            let mut names = Vec::new();
            collect_binding_names(interner, parameter.variable().binding(), &mut names);
            names
        })
        .collect()
}
