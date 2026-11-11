use super::*;
use crate::generator_loop_control::{collect_mixed_suspensions, GeneratorLoopControlError};
use crate::{BlockIr, ExprIr, GeneratorResumeModeIr, YieldForm};
use boa_ast::statement::Switch as AstSwitch;
use boa_ast::visitor::{VisitWith, Visitor};
use std::ops::ControlFlow;

const SOURCE: &str = "async function* g(){switch(await(yield 'discriminant')){case await(yield 'first'):yield 'first body';break;default:yield 'default body';case await(yield 'last'):await 0;yield 'last body';}}";
const LEXICAL_SOURCE: &str = "async function* g(){switch(await(yield 'head')){case 0:let local=1;function read(){return local;}yield read;break;default:yield 'default';}}";

fn with_plan(
    source: &str,
    consume: impl FnOnce(&AstSwitch, &AsyncGeneratorSwitchIr, &[OwnedEnvBindingIr]),
) {
    let parsed = scriptw_front::parse(source, scriptw_front::ParseOptions::script()).unwrap();
    let program = crate::lower(&parsed);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .find(|function| function.name == "g")
        .unwrap();
    let plan = function
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::AsyncGeneratorSwitch(plan) => Some(plan.as_ref()),
            _ => None,
        })
        .expect("actual source-produced mixed Switch");
    parsed
        .as_script()
        .unwrap()
        .with_compiler_session(|script, _| {
            struct Find<'ast>(Option<&'ast AstSwitch>);
            impl<'ast> Visitor<'ast> for Find<'ast> {
                type BreakTy = ();
                fn visit_statement(&mut self, source: &'ast boa_ast::Statement) -> ControlFlow<()> {
                    if let boa_ast::Statement::Switch(switch) = source {
                        self.0 = Some(switch);
                        return ControlFlow::Break(());
                    }
                    source.visit_with(self)
                }
            }
            let mut found = Find(None);
            let _ = script.visit_with(&mut found);
            consume(found.0.unwrap(), plan, &function.owned_env_bindings);
        });
}

fn states(source: &AstSwitch, plan: &AsyncGeneratorSwitchIr) -> AsyncGeneratorSwitchSourceStates {
    crate::async_generator_source::AsyncGeneratorSwitchSource::for_protocol(
        source,
        plan.execution(),
    )
    .unwrap()
    .states(plan.entry_state())
    .unwrap()
}

#[test]
fn plain_async_switch_consumes_conditional_reference_and_optional_call_operand_regions() {
    const ASYNC: &str = "async function g(target,receiver,flag){switch(flag ? await 2 : await 3){case target[await 0] &&= await 1:await 4;break;default:await 5;case receiver?.pick(await 6):let local=await 7;function read(){return local;}break;}}";
    with_plan(ASYNC, |source, plan, inventory| {
        assert_eq!(plan.execution(), ResumableRegionProtocolIr::Async);
        assert!(
            plan.resource().is_none(),
            "source operand ownership does not require a disposal capability"
        );
        assert_eq!(rebuild(source, plan, inventory).unwrap(), *plan);
        assert!(
            plan.lexical_environment().is_some(),
            "original captured CaseBlock cells"
        );
        assert_eq!(plan.cases().len(), 3);
        assert!(plan.cases()[1].selector().is_none());
        assert!(plan
            .regions()
            .all(|region| region.protocol() == ResumableRegionProtocolIr::Async));
        assert_eq!(
            plan.cases()[2].selector().unwrap().region().end_state() + 1,
            plan.fallback_state()
        );
        assert_eq!(
            plan.fallback_state() + 1,
            plan.cases()[0].body().entry_state()
        );
        let statement = StatementIr::AsyncGeneratorSwitch(Box::new(plan.clone()));
        assert_eq!(
            crate::async_switch::sequence_exit(
                std::slice::from_ref(&statement),
                plan.entry_state()
            ),
            Ok(plan.exit_state())
        );
        assert_eq!(
            crate::generator_loop_control::mixed_sequence_end(
                std::slice::from_ref(&statement),
                plan.entry_state()
            ),
            Err(GeneratorLoopControlError::ForeignContinuation)
        );
        let mut actual = Vec::new();
        crate::async_with::collect_async_suspensions(std::slice::from_ref(&statement), &mut actual);
        assert_eq!(actual, states(source, plan).suspensions());
        assert_eq!(actual.len(), 8);
        assert!(actual
            .iter()
            .all(|point| point.kind == crate::ResumableSuspensionKindIr::Await));
        let mut reordered = plan.clone();
        reordered.cases.swap(0, 2);
        assert_eq!(
            rebuild(source, &reordered, inventory),
            Err(GeneratorSwitchControlError::CaseOrder)
        );
        let mut missing = plan.discriminant().region().block().clone();
        missing.statements.pop();
        let region = ResumableRegionIr::new(
            missing,
            states(source, plan).discriminant(),
            ResumableRegionProtocolIr::Async,
        )
        .unwrap();
        let mut damaged = plan.clone();
        damaged.discriminant =
            ResumableExpressionIr::new(region, plan.discriminant().value().clone());
        assert_eq!(
            rebuild(source, &damaged, inventory),
            Err(GeneratorSwitchControlError::MissingDiscriminantPublication)
        );
    });
}

#[test]
fn async_case_source_keeps_bare_await_and_nested_empty_declarations_distinct() {
    const SOURCE:&str="async function g(marker){switch(await 0){case await 0:41;await marker;const local=await 7;default:try{99;break;}finally{const empty=await 8;}case await 1:break;}}";
    with_plan(SOURCE, |source, plan, inventory| {
        assert_eq!(rebuild(source, plan, inventory).unwrap(), *plan);
        let first = &plan.cases()[0].body().block().statements;
        assert!(
            first.iter().any(|statement| matches!(
                statement,
                StatementIr::AsyncAwait {
                    resume_mode: crate::AsyncResumeModeIr::Ignore,
                    ..
                }
            )),
            "actual source Await publishes its received normal value"
        );
        assert!(first.iter().any(|statement|matches!(statement,StatementIr::EmptyStatementCompletion(item) if {
            let mut points=Vec::new();crate::async_with::collect_async_suspensions(std::slice::from_ref(item.statement()),&mut points);points.len()==1
        })),"actual declaration retains one suppressed staged Await as Empty");
        let StatementIr::TryFinally {
            finally_block,
            async_plan: Some(_),
            ..
        } = &plan.cases()[1].body().block().statements[0]
        else {
            panic!("actual pending-completion finalizer");
        };
        assert!(finally_block
            .statements
            .iter()
            .any(|statement| matches!(statement, StatementIr::EmptyStatementCompletion(_))));
        assert!(
            plan.cases()[2].selector().unwrap().region().entry_state()
                < plan.cases()[0].body().entry_state(),
            "later test stays before all body regions and is skipped on fallthrough"
        );
    });
}

fn rebuild(
    source: &AstSwitch,
    plan: &AsyncGeneratorSwitchIr,
    inventory: &[OwnedEnvBindingIr],
) -> Result<AsyncGeneratorSwitchIr, GeneratorSwitchControlError> {
    AsyncGeneratorSwitchIr::new_complete(
        states(source, plan),
        plan.discriminant.clone(),
        plan.discriminant_binding().clone(),
        plan.lexical_environment().cloned(),
        plan.lexical_declarations.clone(),
        plan.cases.clone(),
        plan.value_binding().clone(),
        plan.resource.clone(),
        inventory,
    )
}

fn change_head(
    source: &AstSwitch,
    plan: &AsyncGeneratorSwitchIr,
    change: impl FnOnce(&mut BlockIr),
) -> AsyncGeneratorSwitchIr {
    let mut changed = plan.clone();
    let mut block = plan.discriminant.region().block().clone();
    change(&mut block);
    changed.discriminant = AsyncGeneratorLoopExpressionIr::new(
        AsyncGeneratorLoopRegionIr::new(block, states(source, plan).discriminant()).unwrap(),
        plan.discriminant.value().clone(),
    )
    .into();
    changed
}

#[test]
fn mixed_switch_constructor_consumes_actual_default_middle_phases_and_both_protocols() {
    with_plan(SOURCE, |source, plan, inventory| {
        assert_eq!(rebuild(source, plan, inventory).unwrap(), *plan);
        assert!(inventory.contains(plan.discriminant_binding()));
        assert!(inventory.contains(plan.value_binding()));
        assert_ne!(plan.discriminant_binding().slot, plan.value_binding().slot);
        let [first, default, last] = plan.cases() else {
            panic!("three actual cases");
        };
        assert!(
            first.selector().is_some() && default.selector().is_none() && last.selector().is_some()
        );
        assert_eq!(
            last.selector().unwrap().region().end_state() + 1,
            plan.fallback_state()
        );
        assert_eq!(plan.fallback_state() + 1, first.body().entry_state());
        assert_eq!(first.body().end_state() + 1, default.body().entry_state());
        assert_eq!(default.body().end_state() + 1, last.body().entry_state());
        assert_eq!(last.body().end_state() + 1, plan.exit_state());
        let statement = StatementIr::AsyncGeneratorSwitch(Box::new(plan.clone()));
        let statements = std::slice::from_ref(&statement);
        assert_eq!(
            crate::generator_loop_control::mixed_sequence_end(statements, plan.entry_state()),
            Ok(plan.exit_state())
        );
        assert_eq!(
            crate::generator_loop_control::sequence_end(statements, plan.entry_state()),
            Err(GeneratorLoopControlError::ForeignContinuation)
        );
        let mut actual = Vec::new();
        collect_mixed_suspensions(statements, &mut actual);
        assert_eq!(actual, states(source, plan).suspensions());
        assert_eq!(actual.len(), 10);
    });
}

#[test]
fn mixed_switch_constructor_requires_two_unique_original_invocation_cells() {
    with_plan(SOURCE, |source, plan, inventory| {
        let mut missing = inventory.to_vec();
        missing.retain(|row| row != plan.value_binding());
        let mut duplicated = inventory.to_vec();
        duplicated.push(plan.discriminant_binding().clone());
        let mut alias = inventory.to_vec();
        let mut row = plan.value_binding().clone();
        row.name = "foreign.alias".into();
        alias.push(row);
        for rows in [missing, duplicated, alias] {
            assert_eq!(
                rebuild(source, plan, &rows),
                Err(GeneratorSwitchControlError::UnallocatedBinding)
            );
        }
        assert_eq!(
            AsyncGeneratorSwitchIr::new_complete(
                states(source, plan),
                plan.discriminant.clone(),
                plan.discriminant_binding().clone(),
                plan.lexical_environment().cloned(),
                plan.lexical_declarations.clone(),
                plan.cases.clone(),
                plan.discriminant_binding().clone(),
                plan.resource.clone(),
                inventory
            ),
            Err(GeneratorSwitchControlError::AliasedBindings)
        );
    });
}

#[test]
fn case_block_lexical_resource_cannot_lose_or_rebind_its_source_capability() {
    use crate::async_generator_resource::{
        AsyncGeneratorResourceError, AsyncGeneratorResourceScopeIr,
    };
    use crate::async_generator_source::AsyncGeneratorResourceScopeSource;

    const DIRECT_RESOURCE: &str = "async function* g(input){switch(await(yield 0)){case 0:using first=input;yield first;case 1:await using second=input;yield ()=>second;}}";
    let error = scriptw_front::parse(DIRECT_RESOURCE, scriptw_front::ParseOptions::script())
        .expect_err("resource declarations require a lexical block inside a case");
    let scriptw_front::ParseCode::Early(code) = error.diagnostic().code else {
        panic!("direct CaseBlock resources must remain an early error");
    };
    assert_eq!(
        code.code(),
        scriptw_front::EarlyErrorCode::SwitchClauseUsingDeclaration
    );

    const RESOURCE: &str = "async function* g(input){switch(await(yield 0)){case 0:{using first=input;yield first;await using second=input;yield ()=>second;}}}";
    with_plan(RESOURCE, |source, plan, inventory| {
        assert_eq!(rebuild(source, plan, inventory).unwrap(), *plan);
        assert!(plan.resource().is_none(), "the lexical block owns disposal");
        let case = &plan.cases()[0];
        let StatementIr::Block(block) = &case.body().block().statements[0] else {
            panic!("actual owning lexical block");
        };
        let StatementIr::AsyncGeneratorResourceScope(resource) = &block.statements[0] else {
            panic!("the complete lexical resource scope");
        };
        assert_eq!(resource.capacity(), 2);
        assert_eq!(resource.entry_state(), case.body().entry_state());
        assert_eq!(resource.exit_state(), case.body().end_state());
        let rebuild_resource =
            |source: &AstSwitch, capability: OwnedEnvBindingIr, inventory: &[OwnedEnvBindingIr]| {
                let boa_ast::StatementListItem::Statement(block) =
                    &source.cases()[0].body().statements()[0]
                else {
                    panic!("actual source lexical block");
                };
                let boa_ast::Statement::Block(block) = block.as_ref() else {
                    panic!("actual source lexical block");
                };
                AsyncGeneratorResourceScopeIr::new(
                    AsyncGeneratorResourceScopeSource::for_protocol(
                        block.statement_list().statements(),
                        plan.execution(),
                    )
                    .unwrap()
                    .states(resource.entry_state())
                    .unwrap(),
                    capability,
                    resource.body().block().clone(),
                    inventory,
                )
            };
        assert_eq!(
            rebuild_resource(source, resource.capability_binding().clone(), inventory).unwrap(),
            **resource
        );
        let mut erased = case.body().block().clone();
        erased.statements.clear();
        assert!(
            ResumableRegionIr::new(erased, states(source, plan).bodies()[0], plan.execution(),)
                .is_err()
        );
        let absent: Vec<_> = inventory
            .iter()
            .filter(|row| *row != resource.capability_binding())
            .cloned()
            .collect();
        assert_eq!(
            rebuild_resource(source, resource.capability_binding().clone(), &absent),
            Err(AsyncGeneratorResourceError::InvalidCapability)
        );
        assert_eq!(
            rebuild_resource(source, plan.discriminant_binding().clone(), inventory),
            Err(AsyncGeneratorResourceError::InvalidRegistration)
        );
        with_plan(RESOURCE, |foreign, _, _| {
            assert_eq!(
                rebuild_resource(foreign, resource.capability_binding().clone(), inventory),
                Err(AsyncGeneratorResourceError::UnconsumedSourceRegistration)
            );
        });
    });
}

#[test]
fn mixed_switch_constructor_requires_one_complete_discriminant_publication() {
    with_plan(SOURCE, |source, plan, inventory| {
        let missing = change_head(source, plan, |block| {
            block.statements.pop();
        });
        let duplicate = change_head(source, plan, |block| {
            block
                .statements
                .push(block.statements.last().unwrap().clone());
        });
        let mut foreign = plan.clone();
        foreign.discriminant = ResumableExpressionIr::new(
            plan.discriminant.region().clone(),
            TypedExpr::from_info(
                plan.discriminant.value().value_info(),
                ExprIr::Identifier("foreign.head".into()),
            ),
        );
        for changed in [missing, duplicate, foreign] {
            assert_eq!(
                rebuild(source, &changed, inventory),
                Err(GeneratorSwitchControlError::MissingDiscriminantPublication)
            );
        }
    });
}

#[test]
fn mixed_switch_constructor_rejects_missing_reordered_and_duplicate_default_cases() {
    with_plan(SOURCE, |source, plan, inventory| {
        let mut missing = plan.clone();
        missing.cases.pop();
        assert_eq!(
            rebuild(source, &missing, inventory),
            Err(GeneratorSwitchControlError::CaseCount)
        );
        let mut reordered = plan.clone();
        reordered.cases.swap(0, 2);
        assert_eq!(
            rebuild(source, &reordered, inventory),
            Err(GeneratorSwitchControlError::CaseOrder)
        );
        let mut defaults = plan.clone();
        defaults.cases[2].selector = None;
        assert_eq!(
            rebuild(source, &defaults, inventory),
            Err(GeneratorSwitchControlError::DuplicateDefault)
        );
    });
}

#[test]
fn mixed_switch_constructor_rejects_equal_extent_protocol_changes_and_head_branches() {
    with_plan(SOURCE, |source, plan, inventory| {
        let changed = change_head(source, plan, |block| {
            let statement = block
                .statements
                .iter_mut()
                .find(|statement| matches!(statement, StatementIr::AsyncAwait { .. }))
                .unwrap();
            let StatementIr::AsyncAwait {
                value,
                suspend_state,
                resume_state,
                ..
            } = statement.clone()
            else {
                unreachable!()
            };
            *statement = StatementIr::GeneratorYield {
                value,
                suspend_state,
                resume_state,
                form: YieldForm::Plain,
                resume_mode: GeneratorResumeModeIr::Ignore,
            };
        });
        assert_eq!(
            rebuild(source, &changed, inventory),
            Err(GeneratorSwitchControlError::Continuation(
                GeneratorLoopControlError::UnconsumedSourceSuspension
            ))
        );
        let branch = change_head(source, plan, |block| {
            block
                .statements
                .insert(0, StatementIr::Break { label: None });
        });
        assert_eq!(
            rebuild(source, &branch, inventory),
            Err(GeneratorSwitchControlError::UnownedHeadBranch)
        );
    });
}

#[test]
fn mixed_switch_constructor_rejects_per_case_environment_and_invocation_name_capture() {
    with_plan(LEXICAL_SOURCE, |source, plan, inventory| {
        let environment = plan
            .lexical_environment()
            .expect("actual shared captured CaseBlock")
            .clone();
        let mut block = plan.cases[0].body.block().clone();
        block.lexical_environment = Some(environment.clone());
        let body =
            AsyncGeneratorLoopRegionIr::new(block, states(source, plan).bodies()[0]).unwrap();
        assert_eq!(
            AsyncGeneratorSwitchCaseIr::new(None, body),
            Err(GeneratorSwitchControlError::PerCaseEnvironment)
        );
        let mut foreign = environment;
        foreign.bindings[0].name = plan.value_binding().name.clone();
        assert_eq!(
            AsyncGeneratorSwitchIr::new_complete(
                states(source, plan),
                plan.discriminant.clone(),
                plan.discriminant_binding().clone(),
                Some(foreign),
                plan.lexical_declarations.clone(),
                plan.cases.clone(),
                plan.value_binding().clone(),
                plan.resource.clone(),
                inventory
            ),
            Err(GeneratorSwitchControlError::InvalidCaseBlockEnvironment)
        );
    });
}
