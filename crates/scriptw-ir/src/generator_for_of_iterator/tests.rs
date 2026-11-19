use super::*;
use crate::generator_for_of_body::GeneratorForOfBodyError;

fn yielded(suspend_state: u32, resume_state: u32) -> StatementIr {
    StatementIr::GeneratorYield {
        value: TypedExpr::undefined(),
        form: YieldForm::Plain,
        suspend_state,
        resume_state,
        resume_mode: GeneratorResumeModeIr::Ignore,
    }
}

#[test]
fn body_rejects_discontinuous_or_absent_suspensions() {
    assert_eq!(
        GeneratorForOfBodyIr::new(vec![StatementIr::Empty], 0),
        Err(GeneratorForOfBodyError::YieldRequired),
    );
    assert_eq!(
        GeneratorForOfBodyIr::new(vec![yielded(0, 2)], 0),
        Err(GeneratorForOfBodyError::StateMismatch {
            expected: 1,
            actual: 2
        }),
    );
    assert_eq!(
        GeneratorForOfBodyIr::new(vec![yielded(u32::MAX, 0)], u32::MAX),
        Err(GeneratorForOfBodyError::StateOverflow { state: u32::MAX }),
    );
}

#[test]
fn exit_boundary_cannot_overflow_the_validated_body() {
    let head = ValidatedResumableSyncForOfBindingIr::new(
        "value",
        ForOfAssignmentIr {
            mode: BindingMode::Var,
            name: "value".into(),
        },
        None,
    )
    .expect("ordinary var head");
    let record = IteratorRecordIr::new(
        IteratorSlot::new("iterator".into()),
        NextMethodSlot::new("next".into()),
        DoneSlot::new("done".into()),
    );
    let body = GeneratorForOfBodyIr::new(vec![yielded(u32::MAX - 1, u32::MAX)], u32::MAX - 1)
        .expect("last representable yield transition");
    assert_eq!(
        GeneratorForOfIteratorPlanIr::new(
            GeneratorForOfIteratorHeadInputIr::Binding(head),
            record,
            body
        ),
        Err(GeneratorForOfIteratorPlanError::ExitStateOverflow {
            body_exit_state: u32::MAX
        }),
    );
}

#[test]
fn lexical_head_requires_the_correct_complete_iteration_environment() {
    let binding = ForOfAssignmentIr {
        mode: BindingMode::Let,
        name: "value".into(),
    };
    assert_eq!(
        ValidatedResumableSyncForOfBindingIr::new("value", binding.clone(), None),
        Err(
            ResumableSyncForOfBindingError::BindingHeadEnvironmentRequired {
                mode: BindingMode::Let,
                name: "value".into(),
            }
        ),
    );
    let invalid_environment = ForInOfEnvironmentIr {
        tdz_environment: None,
        iteration_environment: Some(LexicalEnvironmentIr {
            initialization: LexicalEnvironmentInitializationIr::Uninitialized,
            eval_environment: None,
            bindings: vec![OwnedEnvBindingIr {
                mutability: crate::EnvironmentBindingMutabilityIr::Mutable,
                name: "different".into(),
                slot: 0,
            }],
        }),
        tdz_binding_names: vec![TdzPlaceholderName::for_source_name("value").into_string()],
    };
    assert!(matches!(
        ValidatedResumableSyncForOfBindingIr::new("value", binding, Some(invalid_environment)),
        Err(ResumableSyncForOfBindingError::InvalidEnvironmentLayout(
            AsyncFunctionForOfIteratorEnvironmentError::BindingOutsideExpectedNames { .. },
        )),
    ));
}

fn block(statements: Vec<StatementIr>) -> BlockIr {
    BlockIr {
        statements,
        result_kind: ValueKind::Undefined,
        lexical_environment: None,
    }
}

#[test]
fn current_loop_branches_preserve_yield_states_through_blocks_and_eager_if() {
    let statements = vec![
        StatementIr::Continue { label: None },
        StatementIr::Block(block(vec![
            StatementIr::If {
                condition: TypedExpr::undefined(),
                then_branch: Box::new(StatementIr::Break { label: None }),
                else_branch: Some(Box::new(StatementIr::Continue { label: None })),
            },
            yielded(0, 1),
        ])),
        StatementIr::LexicalBlock(vec![StatementIr::Break { label: None }]),
    ];
    let body = GeneratorForOfBodyIr::new(statements.clone(), 0)
        .expect("current loop owns every unlabelled branch");
    assert_eq!(body.statements(), statements.as_slice());
    assert_eq!(body.entry_state(), 0);
    assert_eq!(body.exit_state(), 1);
}

#[test]
fn local_branches_in_yielding_finally_retain_exact_clause_states() {
    let plan = GeneratorTryPlanIr {
        entry_state: 0,
        try_exit_state: 2,
        catch_entry_state: None,
        catch_exit_state: None,
        finally_entry_state: Some(2),
        finally_exit_state: Some(4),
        exit_state: 4,
    };
    let statement = StatementIr::TryFinally {
        try_block: block(vec![yielded(0, 1), StatementIr::Continue { label: None }]),
        finally_block: block(vec![yielded(2, 3), StatementIr::Break { label: None }]),
        generator_plan: Some(plan),
        async_plan: None,
    };
    let body = GeneratorForOfBodyIr::new(vec![statement.clone()], 0)
        .expect("yielding finally retains current-loop completion ownership");
    assert_eq!(body.exit_state(), 4);
    assert_eq!(body.statements(), &[statement]);
    let mut invalid = plan;
    invalid.finally_exit_state = Some(5);
    assert_eq!(
        GeneratorForOfBodyIr::new(
            vec![StatementIr::TryFinally {
                try_block: block(vec![yielded(0, 1), StatementIr::Break { label: None }]),
                finally_block: block(vec![yielded(2, 3), StatementIr::Continue { label: None }]),
                generator_plan: Some(invalid),
                async_plan: None,
            }],
            0,
        ),
        Err(GeneratorForOfBodyError::StateMismatch {
            expected: 4,
            actual: 5
        }),
    );
}

#[test]
fn labelled_and_child_statement_branches_cannot_borrow_current_loop_ownership() {
    for branch in [
        StatementIr::Break { label: None },
        StatementIr::Continue { label: None },
    ] {
        for foreign in [
            StatementIr::While {
                condition: TypedExpr::undefined(),
                body: Box::new(branch.clone()),
            },
            StatementIr::For {
                init: None,
                test: None,
                update: None,
                body: Box::new(branch.clone()),
                lexical_environment: None,
            },
            StatementIr::For {
                init: Some(ForInitIr::Statements(vec![branch.clone()])),
                test: None,
                update: None,
                body: Box::new(StatementIr::Empty),
                lexical_environment: None,
            },
            StatementIr::Labelled {
                labels: vec!["other".into()],
                statement: Box::new(branch.clone()),
                async_plan: None,
            },
            StatementIr::Switch {
                discriminant: TypedExpr::undefined(),
                lexical_environment: None,
                lexical_declarations: vec![],
                cases: vec![SwitchCaseIr {
                    condition: None,
                    body: block(vec![branch.clone()]),
                }],
            },
            StatementIr::ParameterInitialization {
                parameter_index: 0,
                statements: vec![branch.clone()],
            },
        ] {
            assert_eq!(
                GeneratorForOfBodyIr::new(vec![yielded(0, 1), foreign], 0),
                Err(GeneratorForOfBodyError::ForeignBranchOwner),
            );
        }
    }
    for labelled in [
        StatementIr::Break {
            label: Some("outer".into()),
        },
        StatementIr::Continue {
            label: Some("outer".into()),
        },
    ] {
        assert_eq!(
            GeneratorForOfBodyIr::new(vec![yielded(0, 1), labelled], 0),
            Err(GeneratorForOfBodyError::ForeignBranchOwner),
        );
    }
}

#[test]
fn eager_child_owners_restore_current_loop_branch_context() {
    let body = GeneratorForOfBodyIr::new(
        vec![
            StatementIr::While {
                condition: TypedExpr::undefined(),
                body: Box::new(StatementIr::Empty),
            },
            StatementIr::Switch {
                discriminant: TypedExpr::undefined(),
                lexical_environment: None,
                lexical_declarations: vec![],
                cases: vec![SwitchCaseIr {
                    condition: None,
                    body: block(vec![]),
                }],
            },
            StatementIr::ParameterInitialization {
                parameter_index: 0,
                statements: vec![StatementIr::Empty],
            },
            StatementIr::Continue { label: None },
            yielded(0, 1),
            StatementIr::Break { label: None },
        ],
        0,
    )
    .expect("foreign eager owners retire before the current loop's next statement");
    assert_eq!(body.exit_state(), 1);
}

const ASSIGNMENT_SINK: &str = "$forof.assignment.7";

fn entry_value() -> TypedExpr {
    TypedExpr::from_info(
        ValueInfo::new(ValueKind::Dynamic),
        ExprIr::Identifier(ASSIGNMENT_SINK.into()),
    )
}

fn identifier_prefix() -> StatementIr {
    StatementIr::DeclarationEvaluation(TypedExpr::from_info(
        ValueInfo::new(ValueKind::Dynamic),
        ExprIr::AssignIdentifier {
            name: "original.cell".into(),
            value: Box::new(entry_value()),
        },
    ))
}

fn assignment_head(
    prefix: &StatementIr,
) -> Result<GeneratorForOfAssignmentIr, head::GeneratorForOfAssignmentError> {
    GeneratorForOfAssignmentIr::identifier(
        "target",
        ASSIGNMENT_SINK.into(),
        prefix,
        None,
        None,
        std::iter::empty(),
        &[],
    )
}

fn iterator_record() -> IteratorRecordIr {
    IteratorRecordIr::new(
        IteratorSlot::new("iterator".into()),
        NextMethodSlot::new("next".into()),
        DoneSlot::new("done".into()),
    )
}

#[test]
fn prepared_identifier_head_has_only_entry_local_storage_and_keeps_its_actual_prefix() {
    let prefix = identifier_prefix();
    let head = assignment_head(&prefix).expect("located mutable identifier write");
    let body =
        GeneratorForOfBodyIr::new(vec![prefix.clone(), yielded(0, 1)], 0).expect("checked body");
    let plan = GeneratorForOfIteratorPlanIr::new(
        GeneratorForOfIteratorHeadInputIr::Assignment(head),
        iterator_record(),
        body,
    )
    .expect("complete assignment head");
    assert_eq!(
        plan.value_storage(),
        GeneratorForOfIteratorValueStorageIr::EntryLocal {
            name: ASSIGNMENT_SINK
        }
    );
    assert!(plan.binding().is_none());
    assert!(plan.head_environment().is_none());
    assert_eq!(
        plan.iteration_environment(),
        &ResumableLoopIterationEnvironmentIr::StorageOnly
    );
    assert_eq!(plan.body().statements()[0], prefix);
    assert_eq!(plan.exit_state(), 2);
}

#[test]
fn identifier_head_preserves_global_runtime_environment_native_errors_and_justified_noop() {
    let prefixes = [
        ExprIr::GlobalPropertyWrite {
            name: "target".into(),
            value: Box::new(entry_value()),
            implicit: true,
            strictness: Strictness::Strict,
        },
        ExprIr::EnvironmentIdentifier(Box::new(EnvironmentIdentifierIr::current(
            "target".into(),
            Strictness::Strict,
            EnvironmentIdentifierOperationIr::Assign {
                value: Box::new(entry_value()),
            },
        ))),
    ];
    for expr in prefixes {
        let prefix = StatementIr::DeclarationEvaluation(TypedExpr::from_info(
            ValueInfo::new(ValueKind::Dynamic),
            expr,
        ));
        assert!(assignment_head(&prefix).is_ok());
    }
    for error in [
        IdentifierWriteErrorIr::UninitializedBinding,
        IdentifierWriteErrorIr::ImmutableBinding,
        IdentifierWriteErrorIr::ImmutableClassName,
    ] {
        let prefix = StatementIr::DeclarationEvaluation(TypedExpr::from_info(
            ValueInfo::new(ValueKind::Dynamic),
            ExprIr::Comma {
                lhs: Box::new(entry_value()),
                rhs: Box::new(TypedExpr::from_info(
                    ValueInfo::new(ValueKind::Dynamic),
                    ExprIr::RuntimeThrow {
                        name: error.kind(),
                        message: error.message(),
                    },
                )),
            },
        ));
        assert!(assignment_head(&prefix).is_ok(), "{error:?}");
    }
    let noop = StatementIr::DeclarationEvaluation(entry_value());
    assert_eq!(
        assignment_head(&noop).unwrap_err(),
        head::GeneratorForOfAssignmentError::InvalidPrefix
    );
    let ignored = IdentifierWriteReferenceIr::ignored_immutable_binding("target".into());
    assert!(GeneratorForOfAssignmentIr::identifier(
        "target",
        ASSIGNMENT_SINK.into(),
        &noop,
        None,
        Some(&ignored),
        std::iter::empty(),
        &[]
    )
    .is_ok());
    let unrelated = IdentifierWriteReferenceIr::ignored_immutable_binding("other".into());
    assert!(GeneratorForOfAssignmentIr::identifier(
        "target",
        ASSIGNMENT_SINK.into(),
        &noop,
        None,
        Some(&unrelated),
        std::iter::empty(),
        &[]
    )
    .is_err());
}

#[test]
fn prepared_identifier_head_rejects_source_binding_storage_and_unchecked_property_prefix() {
    let prefix = identifier_prefix();
    assert_eq!(
        GeneratorForOfAssignmentIr::identifier(
            "target",
            "sourceName".into(),
            &prefix,
            None,
            None,
            std::iter::empty(),
            &[]
        )
        .unwrap_err(),
        head::GeneratorForOfAssignmentError::SpellableSink
    );
    assert_eq!(
        GeneratorForOfAssignmentIr::identifier(
            "target",
            ASSIGNMENT_SINK.into(),
            &prefix,
            None,
            None,
            [ASSIGNMENT_SINK],
            &[]
        )
        .unwrap_err(),
        head::GeneratorForOfAssignmentError::PersistentSink
    );
    let environment = ForInOfEnvironmentIr {
        tdz_environment: None,
        iteration_environment: None,
        tdz_binding_names: vec![],
    };
    assert_eq!(
        GeneratorForOfAssignmentIr::identifier(
            "target",
            ASSIGNMENT_SINK.into(),
            &prefix,
            Some(&environment),
            None,
            std::iter::empty(),
            &[]
        )
        .unwrap_err(),
        head::GeneratorForOfAssignmentError::HeadEnvironment
    );
    let property = StatementIr::DeclarationEvaluation(TypedExpr::from_info(
        ValueInfo::new(ValueKind::Dynamic),
        ExprIr::PropertyWrite {
            target: Box::new(TypedExpr::undefined()),
            key: PropertyKeyIr::StaticString("target".into()),
            value: Box::new(entry_value()),
            strictness: Strictness::Strict,
        },
    ));
    assert_eq!(
        assignment_head(&property).unwrap_err(),
        head::GeneratorForOfAssignmentError::InvalidPrefix
    );
}

#[test]
fn mandatory_plan_rejects_sink_reads_writes_retained_receiver_and_resume_target() {
    let prefix = identifier_prefix();
    let mut resumed_yield = yielded(0, 1);
    if let StatementIr::GeneratorYield { resume_mode, .. } = &mut resumed_yield {
        *resume_mode = GeneratorResumeModeIr::AssignIdentifier(ASSIGNMENT_SINK.into());
    }
    let escapes = [
        StatementIr::Expression(entry_value()),
        StatementIr::Expression(TypedExpr::from_info(
            ValueInfo::new(ValueKind::Dynamic),
            ExprIr::AssignIdentifier {
                name: ASSIGNMENT_SINK.into(),
                value: Box::new(TypedExpr::undefined()),
            },
        )),
        StatementIr::Expression(TypedExpr::from_info(
            ValueInfo::new(ValueKind::Dynamic),
            ExprIr::MaterializeBinding {
                name: ASSIGNMENT_SINK.into(),
                value: Box::new(TypedExpr::undefined()),
                body: Box::new(TypedExpr::undefined()),
            },
        )),
        StatementIr::Expression(TypedExpr::from_info(
            ValueInfo::new(ValueKind::Dynamic),
            ExprIr::CallIndirect {
                direct_eval: None,
                callee: Box::new(TypedExpr::undefined()),
                this_arg: Some(Box::new(entry_value())),
                args: vec![],
                static_regexp_compilation: None,
            },
        )),
    ];
    for escape in escapes {
        let head = assignment_head(&prefix).expect("checked prefix");
        let body = GeneratorForOfBodyIr::new(vec![prefix.clone(), yielded(0, 1), escape], 0)
            .expect("valid state sequence");
        assert_eq!(
            GeneratorForOfIteratorPlanIr::new(
                GeneratorForOfIteratorHeadInputIr::Assignment(head),
                iterator_record(),
                body
            ),
            Err(GeneratorForOfIteratorPlanError::InvalidAssignment(
                head::GeneratorForOfAssignmentError::SinkEscapesBody
            ))
        );
    }
    let head = assignment_head(&prefix).unwrap();
    let body = GeneratorForOfBodyIr::new(vec![prefix, resumed_yield], 0).unwrap();
    assert!(matches!(
        GeneratorForOfIteratorPlanIr::new(
            GeneratorForOfIteratorHeadInputIr::Assignment(head),
            iterator_record(),
            body
        ),
        Err(GeneratorForOfIteratorPlanError::InvalidAssignment(
            head::GeneratorForOfAssignmentError::SinkEscapesBody
        ))
    ));
}

#[test]
fn plan_cannot_reassociate_checked_prefix_or_retain_sink_as_iterator_record() {
    let prefix = identifier_prefix();
    let head = assignment_head(&prefix).unwrap();
    let body = GeneratorForOfBodyIr::new(vec![StatementIr::Empty, yielded(0, 1)], 0).unwrap();
    assert!(matches!(
        GeneratorForOfIteratorPlanIr::new(
            GeneratorForOfIteratorHeadInputIr::Assignment(head),
            iterator_record(),
            body
        ),
        Err(GeneratorForOfIteratorPlanError::InvalidAssignment(
            head::GeneratorForOfAssignmentError::PrefixMismatch
        ))
    ));
    let head = assignment_head(&prefix).unwrap();
    let body = GeneratorForOfBodyIr::new(vec![prefix, yielded(0, 1)], 0).unwrap();
    let record = IteratorRecordIr::new(
        IteratorSlot::new(ASSIGNMENT_SINK.into()),
        NextMethodSlot::new("next".into()),
        DoneSlot::new("done".into()),
    );
    assert!(matches!(
        GeneratorForOfIteratorPlanIr::new(
            GeneratorForOfIteratorHeadInputIr::Assignment(head),
            record,
            body
        ),
        Err(GeneratorForOfIteratorPlanError::InvalidAssignment(
            head::GeneratorForOfAssignmentError::PersistentSink
        ))
    ));
}

#[test]
fn prepared_identifier_head_rejects_capture_and_owned_environment_registration() {
    let unit = scriptw_front::parse(
        "function holder() {}",
        scriptw_front::ParseOptions::script(),
    )
    .unwrap();
    let program = crate::lower(&unit);
    let mut function = program
        .script
        .unwrap()
        .functions
        .into_iter()
        .find(|function| function.name == "holder")
        .unwrap();
    let prefix = identifier_prefix();
    function.owned_env_bindings.push(OwnedEnvBindingIr {
        mutability: crate::EnvironmentBindingMutabilityIr::Mutable,
        name: ASSIGNMENT_SINK.into(),
        slot: 0,
    });
    assert_eq!(
        GeneratorForOfAssignmentIr::identifier(
            "target",
            ASSIGNMENT_SINK.into(),
            &prefix,
            None,
            None,
            std::iter::empty(),
            &[function.clone()]
        )
        .unwrap_err(),
        head::GeneratorForOfAssignmentError::PersistentSink
    );
    function.owned_env_bindings.clear();
    function.captured_bindings.push(CapturedBindingIr {
        name: ASSIGNMENT_SINK.into(),
        source_name: "target".into(),
        mode: BindingMode::Let,
        slot: 0,
        hops: 0,
    });
    assert_eq!(
        GeneratorForOfAssignmentIr::identifier(
            "target",
            ASSIGNMENT_SINK.into(),
            &prefix,
            None,
            None,
            std::iter::empty(),
            &[function]
        )
        .unwrap_err(),
        head::GeneratorForOfAssignmentError::PersistentSink
    );
}

#[test]
fn sink_query_includes_retained_environments_and_destructuring_write_targets() {
    let pattern = ObjectDestructuringPatternIr {
        properties: vec![ObjectDestructuringPropertyIr {
            key: DestructuringPropertyKeyIr::Static("value".into()),
            target: DestructuringTargetIr::AssignmentIdentifier(
                IdentifierWriteReferenceIr::mutable_binding(ASSIGNMENT_SINK.into()),
            ),
            default: None,
        }],
        rest: None,
    };
    let escapes = [
        StatementIr::Block(BlockIr {
            statements: vec![],
            result_kind: ValueKind::Undefined,
            lexical_environment: Some(LexicalEnvironmentIr {
                initialization: LexicalEnvironmentInitializationIr::Uninitialized,
                eval_environment: None,
                bindings: vec![OwnedEnvBindingIr {
                    mutability: crate::EnvironmentBindingMutabilityIr::Mutable,
                    name: ASSIGNMENT_SINK.into(),
                    slot: 0,
                }],
            }),
        }),
        StatementIr::Expression(TypedExpr::from_info(
            ValueInfo::new(ValueKind::Dynamic),
            ExprIr::ObjectDestructure {
                value: Box::new(TypedExpr::undefined()),
                pattern: Box::new(pattern),
            },
        )),
    ];
    for escape in escapes {
        let prefix = identifier_prefix();
        let head = assignment_head(&prefix).unwrap();
        let body = GeneratorForOfBodyIr::new(vec![prefix, yielded(0, 1), escape], 0).unwrap();
        assert!(matches!(
            GeneratorForOfIteratorPlanIr::new(
                GeneratorForOfIteratorHeadInputIr::Assignment(head),
                iterator_record(),
                body
            ),
            Err(GeneratorForOfIteratorPlanError::InvalidAssignment(
                head::GeneratorForOfAssignmentError::SinkEscapesBody
            ))
        ));
    }
}

#[test]
fn prepared_identifier_head_rejects_mistyped_or_narrowed_iterator_value() {
    for operand in [
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::Identifier(ASSIGNMENT_SINK.into()),
        ),
        TypedExpr {
            possible_kinds: KindSet::from_kind(ValueKind::Number),
            ..entry_value()
        },
    ] {
        let prefix = StatementIr::DeclarationEvaluation(TypedExpr::from_info(
            ValueInfo::new(ValueKind::Dynamic),
            ExprIr::AssignIdentifier {
                name: "original.cell".into(),
                value: Box::new(operand),
            },
        ));
        assert_eq!(
            assignment_head(&prefix).unwrap_err(),
            head::GeneratorForOfAssignmentError::InvalidPrefix
        );
    }
}

fn property_prefix(base: TypedExpr, key: PropertyKeyIr, rhs: TypedExpr) -> StatementIr {
    StatementIr::DeclarationEvaluation(
        crate::reference::OrdinaryPropertyReferencePlan::new(
            Box::new(base),
            key,
            Strictness::Strict,
        )
        .plain_assignment(rhs, PropertyHookTargets::default()),
    )
}

fn property_head(
    prefix: &StatementIr,
) -> Result<GeneratorForOfAssignmentIr, head::GeneratorForOfAssignmentError> {
    GeneratorForOfAssignmentIr::ordinary_property(
        ASSIGNMENT_SINK.into(),
        prefix,
        None,
        std::iter::empty(),
        &[],
    )
}

#[test]
fn ordinary_property_head_retains_fused_raw_reference_and_entry_only_value() {
    for (base, key) in [
        (
            TypedExpr::undefined(),
            PropertyKeyIr::StaticString("value".into()),
        ),
        (
            TypedExpr::from_info(
                ValueInfo::new(ValueKind::Number),
                ExprIr::Number(42.0f64.to_bits()),
            ),
            PropertyKeyIr::StringExpr(Box::new(TypedExpr::from_info(
                ValueInfo::new(ValueKind::Dynamic),
                ExprIr::Identifier("raw.key".into()),
            ))),
        ),
    ] {
        let prefix = property_prefix(base.clone(), key.clone(), entry_value());
        assert!(
            assignment_head(&prefix).is_err(),
            "an identifier proof cannot borrow a property prefix"
        );
        let head = property_head(&prefix).expect("one ordinary Reference");
        let body = GeneratorForOfBodyIr::new(vec![prefix.clone(), yielded(0, 1)], 0).unwrap();
        let plan = GeneratorForOfIteratorPlanIr::new(
            GeneratorForOfIteratorHeadInputIr::Assignment(head),
            iterator_record(),
            body,
        )
        .unwrap();
        assert_eq!(
            plan.value_storage(),
            GeneratorForOfIteratorValueStorageIr::EntryLocal {
                name: ASSIGNMENT_SINK
            }
        );
        assert!(plan.binding().is_none());
        assert!(plan.head_environment().is_none());
        assert_eq!(
            plan.iteration_environment(),
            &ResumableLoopIterationEnvironmentIr::StorageOnly
        );
        let StatementIr::DeclarationEvaluation(write) = &plan.body().statements()[0] else {
            panic!("entry prefix");
        };
        let ExprIr::OrdinaryPropertyAssignment(reference) = &write.expr else {
            panic!("fused Reference");
        };
        assert_eq!(reference.base_and_receiver(), &base);
        assert_eq!(reference.referenced_name(), &key);
        assert_eq!(reference.rhs(), &entry_value());
        assert_eq!(reference.strictness(), Strictness::Strict);
        assert_eq!(plan.exit_state(), 2);
    }
}

#[test]
fn ordinary_property_head_rejects_raw_write_other_domains_and_mistyped_value() {
    let raw = StatementIr::DeclarationEvaluation(TypedExpr::from_info(
        ValueInfo::new(ValueKind::Dynamic),
        ExprIr::PropertyWrite {
            target: Box::new(TypedExpr::undefined()),
            key: PropertyKeyIr::StaticString("value".into()),
            value: Box::new(entry_value()),
            strictness: Strictness::Strict,
        },
    ));
    for prefix in [
        raw,
        identifier_prefix(),
        StatementIr::DeclarationEvaluation(entry_value()),
    ] {
        assert_eq!(
            property_head(&prefix).unwrap_err(),
            head::GeneratorForOfAssignmentError::InvalidPrefix
        );
    }
    for rhs in [
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::Identifier(ASSIGNMENT_SINK.into()),
        ),
        TypedExpr {
            possible_kinds: KindSet::from_kind(ValueKind::Number),
            ..entry_value()
        },
        TypedExpr::undefined(),
    ] {
        let prefix = property_prefix(
            TypedExpr::undefined(),
            PropertyKeyIr::StaticString("value".into()),
            rhs,
        );
        assert_eq!(
            property_head(&prefix).unwrap_err(),
            head::GeneratorForOfAssignmentError::InvalidPrefix
        );
    }
}

#[test]
fn ordinary_property_reference_cannot_read_write_or_retain_the_iterator_value() {
    let write_sink = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Dynamic),
        ExprIr::AssignIdentifier {
            name: ASSIGNMENT_SINK.into(),
            value: Box::new(TypedExpr::undefined()),
        },
    );
    let retained_sink = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Dynamic),
        ExprIr::CallIndirect {
            direct_eval: None,
            callee: Box::new(TypedExpr::undefined()),
            this_arg: Some(Box::new(entry_value())),
            args: vec![],
            static_regexp_compilation: None,
        },
    );
    let materialized_sink = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Dynamic),
        ExprIr::MaterializeBinding {
            name: ASSIGNMENT_SINK.into(),
            value: Box::new(TypedExpr::undefined()),
            body: Box::new(TypedExpr::undefined()),
        },
    );
    for operand in [entry_value(), write_sink, retained_sink, materialized_sink] {
        for (base, key) in [
            (operand.clone(), PropertyKeyIr::StaticString("value".into())),
            (
                TypedExpr::undefined(),
                PropertyKeyIr::StringExpr(Box::new(operand.clone())),
            ),
            (
                TypedExpr::undefined(),
                PropertyKeyIr::ArrayIndex(Box::new(operand.clone())),
            ),
        ] {
            let prefix = property_prefix(base, key, entry_value());
            assert_eq!(
                property_head(&prefix).unwrap_err(),
                head::GeneratorForOfAssignmentError::InvalidPrefix
            );
        }
    }
}

#[test]
fn ordinary_property_head_uses_shared_mandatory_lifetime_and_prefix_body_pairing() {
    let prefix = property_prefix(
        TypedExpr::undefined(),
        PropertyKeyIr::StaticString("value".into()),
        entry_value(),
    );
    assert_eq!(
        GeneratorForOfAssignmentIr::ordinary_property(
            ASSIGNMENT_SINK.into(),
            &prefix,
            None,
            [ASSIGNMENT_SINK],
            &[],
        )
        .unwrap_err(),
        head::GeneratorForOfAssignmentError::PersistentSink
    );
    let environment = ForInOfEnvironmentIr {
        tdz_environment: None,
        iteration_environment: None,
        tdz_binding_names: vec![],
    };
    assert_eq!(
        GeneratorForOfAssignmentIr::ordinary_property(
            ASSIGNMENT_SINK.into(),
            &prefix,
            Some(&environment),
            std::iter::empty(),
            &[],
        )
        .unwrap_err(),
        head::GeneratorForOfAssignmentError::HeadEnvironment
    );
    let different = property_prefix(
        TypedExpr::undefined(),
        PropertyKeyIr::StaticString("other".into()),
        entry_value(),
    );
    let body = GeneratorForOfBodyIr::new(vec![different, yielded(0, 1)], 0).unwrap();
    assert!(matches!(
        GeneratorForOfIteratorPlanIr::new(
            GeneratorForOfIteratorHeadInputIr::Assignment(property_head(&prefix).unwrap()),
            iterator_record(),
            body,
        ),
        Err(GeneratorForOfIteratorPlanError::InvalidAssignment(
            head::GeneratorForOfAssignmentError::PrefixMismatch
        ))
    ));
    let mut resumed = yielded(0, 1);
    if let StatementIr::GeneratorYield { resume_mode, .. } = &mut resumed {
        *resume_mode = GeneratorResumeModeIr::AssignIdentifier(ASSIGNMENT_SINK.into());
    }
    let body = GeneratorForOfBodyIr::new(vec![prefix.clone(), resumed], 0).unwrap();
    assert!(matches!(
        GeneratorForOfIteratorPlanIr::new(
            GeneratorForOfIteratorHeadInputIr::Assignment(property_head(&prefix).unwrap()),
            iterator_record(),
            body,
        ),
        Err(GeneratorForOfIteratorPlanError::InvalidAssignment(
            head::GeneratorForOfAssignmentError::SinkEscapesBody
        ))
    ));
}

fn lexical_pattern_environment() -> ForInOfEnvironmentIr {
    ForInOfEnvironmentIr {
        tdz_environment: None,
        iteration_environment: Some(LexicalEnvironmentIr {
            initialization: LexicalEnvironmentInitializationIr::Uninitialized,
            eval_environment: None,
            bindings: vec![OwnedEnvBindingIr {
                mutability: crate::EnvironmentBindingMutabilityIr::Mutable,
                name: "iteration.value".into(),
                slot: 0,
            }],
        }),
        tdz_binding_names: vec![TdzPlaceholderName::for_source_name("value").into_string()],
    }
}

fn lexical_pattern_prefix(mode: BindingMode) -> StatementIr {
    StatementIr::DeclarationEvaluation(TypedExpr::from_info(
        ValueInfo::undefined(),
        ExprIr::ObjectDestructure {
            value: Box::new(entry_value()),
            pattern: Box::new(ObjectDestructuringPatternIr {
                properties: vec![ObjectDestructuringPropertyIr {
                    key: DestructuringPropertyKeyIr::Static("value".into()),
                    target: DestructuringTargetIr::Binding {
                        mode,
                        name: "iteration.value".into(),
                    },
                    default: None,
                }],
                rest: None,
            }),
        },
    ))
}

fn checked_lexical_pattern(
    mode: BindingMode,
    prefix: StatementIr,
) -> ValidatedResumableSyncForOfLexicalPatternIr {
    ValidatedResumableSyncForOfLexicalPatternIr::new(
        mode,
        ASSIGNMENT_SINK.into(),
        vec!["iteration.value".into()],
        vec![TdzPlaceholderName::for_source_name("value").into_string()],
        vec![prefix],
        Some(lexical_pattern_environment()),
    )
    .expect("matching lexical initializer and fresh environment")
}

#[test]
fn lexical_pattern_head_consumes_real_mode_environment_and_initializer_before_yield() {
    for mode in [BindingMode::Let, BindingMode::Const] {
        let prefix = lexical_pattern_prefix(mode);
        let pattern = checked_lexical_pattern(mode, prefix.clone());
        let head = GeneratorForOfLexicalPatternIr::new(pattern, std::iter::empty(), &[])
            .expect("private dynamic entry sink");
        let body = GeneratorForOfBodyIr::new(vec![prefix.clone(), yielded(0, 1)], 0)
            .expect("initializer precedes the first suspension");
        let plan = GeneratorForOfIteratorPlanIr::new(
            GeneratorForOfIteratorHeadInputIr::LexicalPattern(head),
            iterator_record(),
            body,
        )
        .expect("completed lexical pattern head");
        assert_eq!(
            plan.value_storage(),
            GeneratorForOfIteratorValueStorageIr::EntryLocal {
                name: ASSIGNMENT_SINK
            }
        );
        assert!(plan.binding().is_none());
        assert_eq!(plan.head_binding_environment().unwrap().0, mode);
        assert_eq!(plan.body().statements()[0], prefix);
        let ResumableLoopIterationEnvironmentIr::FreshPerIteration(environment) =
            plan.iteration_environment()
        else {
            panic!("source bindings require their complete fresh environment");
        };
        assert_eq!(environment.bindings[0].name, "iteration.value");
        assert_eq!(environment.bindings[0].slot, 0);
        assert_eq!(plan.exit_state(), 2);
    }
}

#[test]
fn lexical_pattern_rejects_missing_cells_wrong_modes_and_assignment_initialization() {
    let mode = BindingMode::Const;
    let prefix = lexical_pattern_prefix(mode);
    let mut environment = lexical_pattern_environment();
    environment.iteration_environment = None;
    assert!(matches!(
        ValidatedResumableSyncForOfLexicalPatternIr::new(
            mode,
            ASSIGNMENT_SINK.into(),
            vec!["iteration.value".into()],
            environment.tdz_binding_names.clone(),
            vec![prefix.clone()],
            Some(environment),
        ),
        Err(AsyncFunctionForOfIteratorPlanError::LexicalPatternIterationNamesMismatch { .. })
    ));
    assert!(matches!(
        ValidatedResumableSyncForOfLexicalPatternIr::new(
            mode,
            ASSIGNMENT_SINK.into(),
            vec!["iteration.value".into()],
            vec![TdzPlaceholderName::for_source_name("value").into_string()],
            vec![lexical_pattern_prefix(BindingMode::Let)],
            Some(lexical_pattern_environment()),
        ),
        Err(
            AsyncFunctionForOfIteratorPlanError::InvalidLexicalPatternInitialization(
                AsyncFunctionForOfIteratorInitializationError::ModeMismatch { .. }
            )
        )
    ));
    let assignment = StatementIr::DeclarationEvaluation(TypedExpr::from_info(
        ValueInfo::undefined(),
        ExprIr::ArrayDestructure {
            value: Box::new(entry_value()),
            pattern: ArrayDestructuringPatternIr {
                elements: vec![],
                protocol: ArrayPatternProtocol::ARRAY_DESTRUCTURING,
            },
            evaluation: ArrayDestructuringEvaluationIr::AssignmentEvaluation,
        },
    ));
    assert!(matches!(
        ValidatedResumableSyncForOfLexicalPatternIr::new(
            mode,
            ASSIGNMENT_SINK.into(),
            vec![],
            vec![],
            vec![assignment],
            Some(ForInOfEnvironmentIr {
                tdz_environment: None,
                iteration_environment: None,
                tdz_binding_names: vec![]
            }),
        ),
        Err(
            AsyncFunctionForOfIteratorPlanError::InvalidLexicalPatternInitialization(
                AsyncFunctionForOfIteratorInitializationError::ArrayAssignmentEvaluation { .. }
            )
        )
    ));
}

#[test]
fn lexical_pattern_sink_is_dynamic_eager_unretained_and_cannot_escape_the_prefix() {
    let mode = BindingMode::Let;
    for invalid in [
        ValueInfo::new(ValueKind::Number),
        ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: KindSet::from_kind(ValueKind::Number),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        },
    ] {
        let mut prefix = lexical_pattern_prefix(mode);
        let StatementIr::DeclarationEvaluation(TypedExpr {
            expr: ExprIr::ObjectDestructure { value, .. },
            ..
        }) = &mut prefix
        else {
            unreachable!()
        };
        **value = TypedExpr::from_info(invalid, ExprIr::Identifier(ASSIGNMENT_SINK.into()));
        assert_eq!(
            GeneratorForOfLexicalPatternIr::new(
                checked_lexical_pattern(mode, prefix),
                std::iter::empty(),
                &[]
            )
            .unwrap_err(),
            head::GeneratorForOfAssignmentError::InvalidPrefix
        );
    }
    let mut prefix = lexical_pattern_prefix(mode);
    let StatementIr::DeclarationEvaluation(TypedExpr {
        expr: ExprIr::ObjectDestructure { pattern, .. },
        ..
    }) = &mut prefix
    else {
        unreachable!()
    };
    pattern.properties[0].default = Some(entry_value());
    assert_eq!(
        GeneratorForOfLexicalPatternIr::new(
            checked_lexical_pattern(mode, prefix),
            std::iter::empty(),
            &[]
        )
        .unwrap_err(),
        head::GeneratorForOfAssignmentError::InvalidPrefix
    );
    let prefix = lexical_pattern_prefix(mode);
    assert_eq!(
        GeneratorForOfLexicalPatternIr::new(
            checked_lexical_pattern(mode, prefix.clone()),
            [ASSIGNMENT_SINK],
            &[]
        )
        .unwrap_err(),
        head::GeneratorForOfAssignmentError::PersistentSink
    );
    let head = GeneratorForOfLexicalPatternIr::new(
        checked_lexical_pattern(mode, prefix.clone()),
        std::iter::empty(),
        &[],
    )
    .unwrap();
    let body = GeneratorForOfBodyIr::new(vec![yielded(0, 1), prefix.clone()], 0).unwrap();
    assert_eq!(
        GeneratorForOfIteratorPlanIr::new(
            GeneratorForOfIteratorHeadInputIr::LexicalPattern(head),
            iterator_record(),
            body
        )
        .unwrap_err(),
        GeneratorForOfIteratorPlanError::InvalidAssignment(
            head::GeneratorForOfAssignmentError::PrefixMismatch
        )
    );
    let head = GeneratorForOfLexicalPatternIr::new(
        checked_lexical_pattern(mode, prefix.clone()),
        std::iter::empty(),
        &[],
    )
    .unwrap();
    let body = GeneratorForOfBodyIr::new(
        vec![
            prefix,
            yielded(0, 1),
            StatementIr::DeclarationEvaluation(entry_value()),
        ],
        0,
    )
    .unwrap();
    assert_eq!(
        GeneratorForOfIteratorPlanIr::new(
            GeneratorForOfIteratorHeadInputIr::LexicalPattern(head),
            iterator_record(),
            body
        )
        .unwrap_err(),
        GeneratorForOfIteratorPlanError::InvalidAssignment(
            head::GeneratorForOfAssignmentError::SinkEscapesBody
        )
    );
}
