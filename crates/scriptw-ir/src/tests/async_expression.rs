#[path = "../../tests/common/statement_awaits.rs"]
mod statement_awaits;

#[test]
fn records_linear_async_await_resume_state() {
    let program = lower_script(
        "async function resume() {
                 let retained = 40;
                 let received;
                 received = await 2;
                 return retained + received;
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "resume")
        .expect("async function should be registered");

    assert_eq!(function.protocol, FunctionProtocolIr::Async);
    assert!(!function.protocol.is_constructable());
    let capture = captured_assignment(&function.body.statements);
    assert_eq!(capture.access(), IdentifierReferenceCaptureAccess::WriteOnly);
    let mut statements = Vec::new();
    staged_operand_statements(&function.body.statements, &mut statements);
    let (await_index, resumed) = statements.iter().enumerate().find_map(|(index, statement)| match statement {
        StatementIr::AsyncAwait { suspend_state: 0, resume_state: 1, resume_mode: AsyncResumeModeIr::AssignIdentifier(name), .. } => Some((index, name)),
        _ => None,
    }).expect("one original await boundary");
    assert!(statements[await_index + 1..].iter().any(|statement| matches!(statement,
        StatementIr::Expression(TypedExpr { expr: ExprIr::EnvironmentIdentifier(identifier), .. })
        if identifier.name == "received" && matches!(&identifier.operation,
            EnvironmentIdentifierOperationIr::PutCapturedReference { reference, value }
            if reference == capture.reference() && matches!(&value.expr, ExprIr::Identifier(name) if name == resumed)))));
}

#[test]
fn lowers_async_await_call_in_lexical_initializer() {
    let program = lower_script(
        "async function collect(Constructor) {
                 let result = await Array.fromAsync.call(Constructor, [1, 2]);
                 return result;
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "collect")
        .expect("async function should be registered");
    let StatementIr::LexicalBlock(statements) = &function.body.statements[0] else {
        panic!(
            "awaited lexical initializer should lower through a lexical block: {:?}",
            function.body.statements
        );
    };
    let [StatementIr::Lexical {
        name: received_name,
        ..
    }, StatementIr::AsyncAwait {
        resume_mode: AsyncResumeModeIr::AssignIdentifier(resume_name),
        ..
    }, StatementIr::Lexical {
        name: result_name,
        init: TypedExpr {
            expr: ExprIr::Identifier(init_name),
            ..
        },
        ..
    }] = statements.as_slice()
    else {
        panic!(
            "awaited lexical initializer should stage its resumed value: {:?}",
            function.body.statements
        );
    };

    assert_eq!(received_name, resume_name);
    assert_eq!(received_name, init_name);
    assert_eq!(result_name, "result");
}

#[test]
fn lowers_nested_async_await_in_expression_statement() {
    let program = lower_script(
        "async function inspect(promise) {
                 assert.sameValue((await promise).value, 1);
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "inspect")
        .expect("async function should be registered");
    let StatementIr::LexicalBlock(statements) = &function.body.statements[0] else {
        panic!(
            "nested await should lower through a lexical block: {:?}",
            function.body.statements
        );
    };

    assert!(statements.iter().any(|statement| matches!(
        statement,
        StatementIr::AsyncAwait {
            suspend_state: 0,
            resume_state: 1,
            resume_mode: AsyncResumeModeIr::AssignIdentifier(_),
            ..
        }
    )));
    assert!(matches!(
        statements.last(),
        Some(StatementIr::Expression(_))
    ));
}

#[test]
fn lowers_direct_await_elements_in_lexical_array_initializer() {
    let program = lower_script(
        "async function collect(before, first, between, second, after) {
                 const reports = [
                     before(),
                     await first(),
                     between(),
                     await second(),
                     after(),
                 ];
                 return reports;
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "collect")
        .expect("async function should be registered");
    let StatementIr::LexicalBlock(statements) = &function.body.statements[0] else {
        panic!(
            "awaited array initializer should lower through a lexical block: {:?}",
            function.body.statements
        );
    };
    let await_states = statements
        .iter()
        .filter_map(|statement| {
            let StatementIr::AsyncAwait {
                suspend_state,
                resume_state,
                ..
            } = statement
            else {
                return None;
            };
            Some((*suspend_state, *resume_state))
        })
        .collect::<Vec<_>>();
    assert_eq!(await_states, vec![(0, 1), (1, 2)]);
    let Some(StatementIr::Lexical {
        name,
        init: TypedExpr {
            expr: ExprIr::ArrayLiteral(elements),
            ..
        },
        ..
    }) = statements.last()
    else {
        panic!(
            "resumed array elements should initialize the declared binding: {:?}",
            function.body.statements
        );
    };
    assert_eq!(name, "reports");
    assert_eq!(elements.len(), 5);
    assert!(elements
        .iter()
        .all(|element| matches!(element.expr, ExprIr::Identifier(_))));
}

#[test]
fn rejects_composite_and_spread_awaited_lexical_array_initializers() {
    let composite = lower_script(
        "async function collect(source) {
                 const reports = [consume(await source)];
                 return reports;
             }",
    );
    assert!(composite.is_wasm_supported());

    let spread = lower_script(
        "async function collect(source, rest) {
                 const reports = [await source, ...rest];
                 return reports;
             }",
    );
    assert!(!spread.is_wasm_supported());
    assert!(spread.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("async lexical array initializer spread")
    }));
}

#[test]
fn lowers_eager_arithmetic_await_declaration_initializers() {
    let program = lower_script(
        "async function calculate(x, first, second) {
                 let lexical = await first() * x;
                 var variable = -(await second()) + lexical;
                 return variable;
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "calculate")
        .expect("async function should be registered");
    let await_states = function
        .body
        .statements
        .iter()
        .flat_map(|statement| match statement {
            StatementIr::LexicalBlock(statements) => statements.as_slice(),
            statement => std::slice::from_ref(statement),
        })
        .filter_map(|statement| {
            let StatementIr::AsyncAwait {
                suspend_state,
                resume_state,
                ..
            } = statement
            else {
                return None;
            };
            Some((*suspend_state, *resume_state))
        })
        .collect::<Vec<_>>();

    assert_eq!(await_states, vec![(0, 1), (1, 2)]);
    let StatementIr::LexicalBlock(var_statements) = &function.body.statements[1] else {
        panic!(
            "awaited var initializer should preserve its predeclared binding: {:?}",
            function.body.statements
        );
    };
    assert!(matches!(
        var_statements.first(),
        Some(StatementIr::Var(declarators))
            if matches!(
                declarators.as_slice(),
                [VarDeclaratorIr { name, init: None }] if name == "variable"
            )
    ));
    assert!(matches!(
        var_statements.last(),
        Some(StatementIr::DeclarationEvaluation(TypedExpr {
            expr: ExprIr::AssignIdentifier { name, .. },
            ..
        })) if name == "variable"
    ));
}

#[test]
fn stages_eager_arithmetic_operands_before_later_awaits() {
    let program = lower_script(
        "async function calculate(before, first, between, second) {
                 const result =
                     before() + await first() * between() + await second();
                 return result;
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "calculate")
        .expect("async function should be registered");
    let StatementIr::LexicalBlock(statements) = &function.body.statements[0] else {
        panic!(
            "composite await initializer should lower through a lexical block: {:?}",
            function.body.statements
        );
    };
    let await_indexes = statements
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| {
            matches!(statement, StatementIr::AsyncAwait { .. }).then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(await_indexes.len(), 2);
    assert!(matches!(
        statements.first(),
        Some(StatementIr::Lexical {
            name,
            init:
                TypedExpr {
                    expr: ExprIr::CallIndirect { .. },
                    ..
                },
            ..
        }) if name.starts_with("$async.binary.lhs.")
    ));
    assert!(await_indexes[0] > 0);
    assert!(statements[await_indexes[0] + 1..await_indexes[1]]
        .iter()
        .any(|statement| matches!(
            statement,
            StatementIr::Lexical { name, .. }
                if name.starts_with("$async.binary.lhs.")
        )));
    assert!(matches!(
        statements.last(),
        Some(StatementIr::Lexical { name, .. }) if name == "result"
    ));
}

#[test]
fn branch_sensitive_await_declaration_initializers_retain_complete_selected_plans() {
    for (source, awaits) in [
        ("async function inspect(source) { let value = (source[await 0] &&= await source); }", 2),
        ("async function inspect(source) { var value = ((await source).value ||= await source); }", 2),
        ("async function inspect(source, key) { let value = source?.(await key); }", 1),
    ] {
        let program = lower_script(source);
        assert!(program.is_wasm_supported(), "{source}: {:?}", program.diagnostics);
        let function = program.script.as_ref().unwrap().functions.iter().find(|function| function.name == "inspect").unwrap();
        statement_awaits::assert_count(function, awaits);
    }
}

#[test]
fn lowers_awaited_optional_chain_in_var_initializer() {
    let program = lower_script(
        "async function inspect(source, key) {
                 var result = await source?.[key()];
                 return result;
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "inspect")
        .expect("async function should be registered");
    let StatementIr::LexicalBlock(statements) = &function.body.statements[0] else {
        panic!(
            "awaited optional chain should lower through a lexical block: {:?}",
            function.body.statements
        );
    };

    assert!(statements.iter().any(|statement| matches!(
        statement,
        StatementIr::AsyncAwait {
            suspend_state: 0,
            resume_state: 1,
            resume_mode: AsyncResumeModeIr::AssignIdentifier(_),
            ..
        }
    )));
    assert!(matches!(
        statements.first(),
        Some(StatementIr::Var(declarators))
            if matches!(
                declarators.as_slice(),
                [VarDeclaratorIr {
                    name,
                    init: None,
                }] if name == "result"
            )
    ));
}

#[test]
fn awaited_var_redeclaration_discards_static_binding_facts() {
    let program = lower_script(
        "async function inspect(promise) {
                 var value = 'old';
                 var value = await promise;
                 return value.length;
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "inspect")
        .expect("async function should be registered");
    let return_value = function
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Return(value) => Some(value),
            _ => None,
        })
        .expect("async function should return the awaited value length");

    assert!(matches!(
        return_value.expr,
        ExprIr::SpecOperation {
            operation: SpecOperationIr::GetV,
            ..
        }
    ));
}

#[test]
fn skips_await_in_statically_nullish_optional_chain_key() {
    let program = lower_script(
        "async function inspect(reject) {
                 assert.sameValue(undefined?.[await reject()], undefined);
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "inspect")
        .expect("async function should be registered");
    let StatementIr::LexicalBlock(statements) = &function.body.statements[0] else {
        panic!(
            "optional chain should retain its expression statement: {:?}",
            function.body.statements
        );
    };

    assert!(
        !statements
            .iter()
            .any(|statement| matches!(statement, StatementIr::AsyncAwait { .. })),
        "short-circuited key must not suspend: {statements:?}"
    );
    // The call retains its callee and receiver before evaluating arguments.
    // The nullish argument still evaluates its base and never enters the key.
    assert!(
        matches!(statements.last(), Some(StatementIr::Expression(_))),
        "optional chain should still end in its expression statement: {statements:?}"
    );
    assert!(
        statements.iter().rev().skip(1).all(|statement| matches!(statement, StatementIr::Lexical { .. })),
        "callee, receiver and arguments are retained before the call: {statements:?}"
    );
    assert!(statements.iter().any(|statement| matches!(statement,
        StatementIr::Lexical { init: TypedExpr { expr: ExprIr::MaterializeBinding { value, body, .. }, .. }, .. }
        if matches!(&value.expr, ExprIr::GlobalPropertyRead { name } if name == "undefined") && matches!(body.expr, ExprIr::Undefined))));
}

#[test]
fn skips_await_in_side_effecting_statically_nullish_optional_chain_key() {
    let program = lower_script(
        "async function inspect(reject) {
                 let calls = 0;
                 let value = (calls += 1, undefined)?.[await reject()];
                 return value;
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "inspect")
        .expect("async function should be registered");
    assert!(
        !function
            .body
            .statements
            .iter()
            .any(|statement| matches!(statement, StatementIr::AsyncAwait { .. })),
        "short-circuited key must not suspend: {:?}",
        function.body.statements
    );
    assert!(matches!(
        &function.body.statements[1],
        StatementIr::Lexical {
            init: TypedExpr {
                expr: ExprIr::MaterializeBinding { .. },
                ..
            },
            ..
        }
    ));
}
