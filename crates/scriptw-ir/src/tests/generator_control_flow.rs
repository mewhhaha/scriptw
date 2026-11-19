#[test]
fn nonyielding_try_in_conditional_owns_states_before_a_yielding_finalizer() {
    let program = lower_script(
        "function* conditional(flag) {
            try {
                if (flag) { try { throw 1; } catch (error) { flag = error; } }
                else { flag = 0; }
                return flag;
            } finally { yield 'cleanup'; }
        }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .find(|function| function.name == "conditional")
        .unwrap();
    let StatementIr::TryFinally {
        try_block,
        generator_plan: Some(_),
        ..
    } = &function.body.statements[0]
    else {
        panic!("outer finalizer owns its continuation");
    };
    let branch = try_block
        .statements
        .iter()
        .find_map(|statement| {
            let StatementIr::LexicalBlock(prefix) = statement else {
                return None;
            };
            prefix.iter().find_map(|statement| {
                if let StatementIr::OrdinaryGeneratorIf(branch) = statement {
                    Some(branch)
                } else {
                    None
                }
            })
        })
        .expect("the conditional's lexical prefix must retain its complete state owner");
    assert!(
        branch
            .then_branch()
            .block()
            .statements
            .iter()
            .any(|statement| {
                matches!(
                    statement,
                    StatementIr::TryCatch {
                        generator_plan: Some(_),
                        ..
                    }
                )
            }),
        "the selected branch retains the complete try/catch owner"
    );
    assert_eq!(
        function
            .generator_plan
            .as_ref()
            .unwrap()
            .suspension_points
            .len(),
        1,
        "the conditional adds control states without inventing yields"
    );
}

#[test]
fn records_generator_try_catch_finally_resume_state_boundaries() {
    let program = lower_script(
        "function* sequence() {
                 try { yield 1; }
                 catch (error) { yield error; }
                 finally { yield 3; }
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "sequence")
        .expect("generator should be registered");

    assert_eq!(
        function.generator_plan,
        Some(GeneratorPlanIr {
            entry_state: 0,
            state_count: 7,
            suspension_points: vec![
                GeneratorSuspensionPointIr {
                    suspend_state: 0,
                    resume_state: 1,
                },
                GeneratorSuspensionPointIr {
                    suspend_state: 2,
                    resume_state: 3,
                },
                GeneratorSuspensionPointIr {
                    suspend_state: 4,
                    resume_state: 5,
                },
            ],
        })
    );
    let StatementIr::TryCatchFinally { generator_plan, .. } = &function.body.statements[0] else {
        panic!("expected generator try/catch/finally statement");
    };
    assert_eq!(
        *generator_plan,
        Some(GeneratorTryPlanIr {
            entry_state: 0,
            try_exit_state: 2,
            catch_entry_state: Some(2),
            catch_exit_state: Some(4),
            finally_entry_state: Some(4),
            finally_exit_state: Some(6),
            exit_state: 6,
        })
    );
}

#[test]
fn records_nested_generator_try_finally_resume_state_boundaries() {
    let program = lower_script(
        "function* nested() {
                 try {
                     try { yield 1; }
                     finally { yield 2; }
                 } finally { yield 3; }
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "nested")
        .expect("generator should be registered");

    assert_eq!(
        function.generator_plan,
        Some(GeneratorPlanIr {
            entry_state: 0,
            state_count: 8,
            suspension_points: vec![
                GeneratorSuspensionPointIr {
                    suspend_state: 0,
                    resume_state: 1,
                },
                GeneratorSuspensionPointIr {
                    suspend_state: 2,
                    resume_state: 3,
                },
                GeneratorSuspensionPointIr {
                    suspend_state: 5,
                    resume_state: 6,
                },
            ],
        })
    );
    let StatementIr::TryFinally {
        try_block,
        generator_plan,
        ..
    } = &function.body.statements[0]
    else {
        panic!("expected outer generator try/finally statement");
    };
    assert_eq!(
        *generator_plan,
        Some(GeneratorTryPlanIr {
            entry_state: 0,
            try_exit_state: 5,
            catch_entry_state: None,
            catch_exit_state: None,
            finally_entry_state: Some(5),
            finally_exit_state: Some(7),
            exit_state: 7,
        })
    );
    let StatementIr::TryFinally { generator_plan, .. } = &try_block.statements[0] else {
        panic!("expected nested generator try/finally statement");
    };
    assert_eq!(
        *generator_plan,
        Some(GeneratorTryPlanIr {
            entry_state: 0,
            try_exit_state: 2,
            catch_entry_state: None,
            catch_exit_state: None,
            finally_entry_state: Some(2),
            finally_exit_state: Some(4),
            exit_state: 4,
        })
    );
}

#[test]
fn generator_activation_owns_bindings_that_survive_a_yield() {
    let program = lower_script(
            "function viaEval() { eval(''); } function* activation(parameter) { let local = parameter; if (local) { yield local; local += arguments[0]; } }",
        );
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    let function = script
        .functions
        .iter()
        .find(|function| function.name == "activation")
        .expect("generator should be registered");
    let owned_names = function
        .owned_env_bindings
        .iter()
        .map(|binding| binding.name.as_str())
        .collect::<BTreeSet<_>>();
    assert!(owned_names.contains("parameter"));
    assert!(owned_names.contains("local"));
    let StatementIr::LexicalBlock(prefix) = &function.body.statements[1] else {
        panic!("the environment-owning conditional must expose its checked continuation");
    };
    let branch = prefix
        .iter()
        .find_map(|statement| {
            if let StatementIr::OrdinaryGeneratorIf(branch) = statement {
                Some(branch)
            } else {
                None
            }
        })
        .expect("the original yielding conditional owns its selected and skipped states");
    assert!(
        branch.then_branch().block().lexical_environment.is_some(),
        "the original named Environment Record must survive the yield"
    );
    let mut points = Vec::new();
    crate::generator_loop_control::collect_suspensions(
        &branch.then_branch().block().statements,
        &mut points,
    );
    assert_eq!(
        points.len(),
        1,
        "the original conditional retains exactly one yield"
    );
    assert_eq!(
        function.generator_plan.as_ref().unwrap().suspension_points,
        points,
        "function metadata must agree with the checked body continuation"
    );
    assert_eq!(
        function.generator_plan.as_ref().unwrap().state_count,
        branch.exit_state() + 1
    );
}

#[test]
fn stages_generator_return_calls_across_argument_yields() {
    let program = lower_script(
            "const generator = function* g() { return (function(value) { return value + 1; }(yield)); };",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "g")
        .expect("generator should be registered");
    assert_eq!(function.generator_plan.as_ref().unwrap().state_count, 2);
    assert!(matches!(
        function.body.statements.as_slice(),
        [StatementIr::LexicalBlock(_)]
    ));
}

#[test]
fn stages_generator_object_spreads_in_source_order() {
    let program = lower_script(
            "const generator = function* g() { yield { ...yield yield, ...(function(value) { return {...value}; }(yield)), ...yield }; };",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "g")
        .expect("generator should be registered");
    assert_eq!(function.generator_plan.as_ref().unwrap().state_count, 6);
    assert!(matches!(
        function.body.statements.as_slice(),
        [StatementIr::LexicalBlock(_)]
    ));
}

#[test]
fn generator_branch_lexical_declaration_has_a_structured_resume_plan() {
    let program =
        lower_script("function* scopedBranch(flag) { if (flag) { let value = 1; yield value; } }");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "scopedBranch")
        .expect("generator should be registered");
    let [StatementIr::GeneratorIf {
        then_before_yield,
        then_yield_statement: Some(yield_statement),
        entry_state: 0,
        then_resume_state: Some(1),
        else_resume_state: None,
        exit_state: 2,
        ..
    }] = function.body.statements.as_slice()
    else {
        panic!(
            "expected a resumable lexical branch: {:?}",
            function.body.statements
        );
    };
    let [StatementIr::Lexical {
        mode: BindingMode::Let,
        name,
        ..
    }] = then_before_yield.as_slice()
    else {
        panic!("expected the branch's lexical declaration: {then_before_yield:?}");
    };
    assert!(matches!(
        yield_statement.as_ref(),
        StatementIr::GeneratorYield {
            value: TypedExpr { expr: ExprIr::Identifier(yielded_name), .. },
            suspend_state: 0,
            resume_state: 1,
            ..
        } if yielded_name == name
    ));
    assert!(function
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == *name));
}

#[test]
fn nested_generator_return_operands_retain_values_across_the_source_yield() {
    let source = "function* nestedOperand() { return 1 + (yield 2); }";
    let program = lower_script(source);
    assert!(
        program.is_wasm_supported(),
        "{source}: {:?}",
        program.diagnostics
    );
    let function = program
        .script
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .find(|function| function.name == "nestedOperand")
        .unwrap();
    assert_eq!(
        function.generator_plan,
        Some(GeneratorPlanIr {
            entry_state: 0,
            state_count: 2,
            suspension_points: vec![GeneratorSuspensionPointIr {
                suspend_state: 0,
                resume_state: 1,
            }],
        })
    );
    let mut statements = Vec::new();
    staged_operand_statements(&function.body.statements, &mut statements);
    let (yield_index, received) = statements
        .iter()
        .enumerate()
        .find_map(|(index, statement)| match statement {
            StatementIr::GeneratorYield {
                value:
                    TypedExpr {
                        expr: ExprIr::Number(value),
                        ..
                    },
                resume_mode: GeneratorResumeModeIr::AssignIdentifier(received),
                ..
            } if *value == 2f64.to_bits() => Some((index, received)),
            _ => None,
        })
        .expect("the source yield retains its resumed value");
    let (return_index, lhs, rhs) = statements
        .iter()
        .enumerate()
        .find_map(|(index, statement)| {
            let StatementIr::Return(TypedExpr {
                expr: ExprIr::CoerciveAdd { lhs, rhs },
                ..
            }) = statement
            else {
                return None;
            };
            let (ExprIr::Identifier(lhs), ExprIr::Identifier(rhs)) = (&lhs.expr, &rhs.expr) else {
                panic!("addition consumes the two retained source operands");
            };
            Some((index, lhs, rhs))
        })
        .expect("source addition follows the yield");
    let lhs_index = statements
        .iter()
        .position(|statement| {
            matches!(statement,
        StatementIr::Lexical { name, init: TypedExpr { expr: ExprIr::Number(value), .. }, .. }
            if name == lhs && *value == 1f64.to_bits())
        })
        .unwrap();
    let rhs_index = statements
        .iter()
        .position(|statement| {
            matches!(statement,
        StatementIr::Lexical { name, init: TypedExpr { expr: ExprIr::Identifier(value), .. }, .. }
            if name == rhs && value == received)
        })
        .unwrap();
    assert!(lhs_index < yield_index && yield_index < rhs_index && rhs_index < return_index);
    for name in [lhs, rhs, received] {
        assert_eq!(
            function
                .owned_env_bindings
                .iter()
                .filter(|binding| &binding.name == name)
                .count(),
            1
        );
    }
}

#[test]
fn lowers_a_loop_body_lexical_declaration_across_a_suspension() {
    // This shape used to be rejected alongside the cases above. A loop body
    // that redeclares a lexical binding on every iteration and suspends
    // while it is live now gets a structured resume plan, so it lowers
    // instead of being refused. Verified end to end: the generator yields
    // 0, 2, 4 and then completes.
    let program = lower_script(
        "function* g() { let i = 0; while (i < 3) { let d = i * 2; yield d; i += 1; } }",
    );
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    let function = script
        .functions
        .iter()
        .find(|function| function.name == "g")
        .expect("generator should be registered");
    assert!(function.generator_plan.is_some());
}
