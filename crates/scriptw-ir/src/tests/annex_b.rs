#[test]
fn annex_b_block_functions_create_undefined_owner_bindings_and_copy_when_selected() {
    let program = lower_script(
        "if (false) { function unselected() {} } if (true) { function selected() {} }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    for name in ["unselected", "selected"] {
        assert!(script.global_bindings.iter().any(|binding| {
            binding.name == name && binding.declarations == GlobalDeclarationSetIr::Var
        }));
    }
    let copies = collect_annex_b_copies(&script.body);
    assert_eq!(copies.len(), 2);
    assert!(copies.iter().any(|(source, block, target)| {
        source == "unselected"
            && block.starts_with("$annexb.block.")
            && target
                == &AnnexBFunctionCopyTargetIr::ScriptGlobal {
                    name: source.clone(),
                }
    }));
    assert!(copies.iter().any(|(source, block, target)| {
        source == "selected"
            && block.starts_with("$annexb.block.")
            && target
                == &AnnexBFunctionCopyTargetIr::ScriptGlobal {
                    name: source.clone(),
                }
    }));
}

#[test]
fn annex_b_block_function_self_reference_captures_the_block_binding() {
    let program =
        lower_script("function owner() { { function f() { return f; } } return f; } owner();");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let function = script
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("block function should be lowered");
    assert!(function
        .captured_bindings
        .iter()
        .any(|binding| binding.name.starts_with("$annexb.block.")));
}

#[test]
fn script_annex_b_block_function_self_reference_captures_the_block_binding() {
    let program = lower_script("if (true) function f() { return f; } f();");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let function = script
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("block function should be lowered");
    assert!(function.captured_bindings.iter().any(|binding| {
        binding.name.starts_with("$annexb.block.") && binding.source_name == "f"
    }));
}

#[test]
fn annex_b_sibling_block_function_captures_the_block_binding() {
    let program = lower_script(
            "function owner() { let f = function () { return 2; }; { function f() { return 1; } function g() { return f(); } return g(); } } owner();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner function should be lowered");
    let sibling = script
        .functions
        .iter()
        .find(|function| function.name == "g")
        .expect("sibling block function should be lowered");
    let captured = sibling
        .captured_bindings
        .iter()
        .find(|binding| binding.name.starts_with("$annexb.block."))
        .expect("sibling should capture the block function binding");
    assert_ne!(captured.name, "f");
    assert!(!owner
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == captured.name));
    assert!(block_environment_owns_binding(
        &owner.body,
        &captured.name,
        captured.slot
    ));
}

#[test]
fn block_function_captures_same_block_let_binding() {
    let program = lower_script(
            "function owner() { 'use strict'; let value = 2; { let value = 1; function read() { return value; } return read(); } } owner();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner function should be lowered");
    let reader = script
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("block function should be lowered");
    let captured = reader
        .captured_bindings
        .iter()
        .find(|binding| binding.name.starts_with("$scoped.lex."))
        .expect("block function should capture the block lexical binding");
    assert!(!owner
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == captured.name));
    assert!(block_environment_owns_binding(
        &owner.body,
        &captured.name,
        captured.slot
    ));
}

#[test]
fn block_function_captures_same_block_class_binding() {
    let program = lower_script(
            "function owner() { 'use strict'; class Outer {} { class Local {} function read() { return Local; } return read(); } } owner();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner function should be lowered");
    let reader = script
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("block function should be lowered");
    let captured = reader
        .captured_bindings
        .iter()
        .find(|binding| binding.name.starts_with("$scoped.lex."))
        .expect("block function should capture the block class binding");
    assert!(!owner
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == captured.name));
    assert!(block_environment_owns_binding(
        &owner.body,
        &captured.name,
        captured.slot
    ));
}

#[test]
fn nested_block_function_capture_uses_the_nearest_shadowing_binding() {
    let program = lower_script(
            "function owner() { 'use strict'; let value = 0; { let value = 1; { const value = 2; function read() { return value; } return read(); } } } owner();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner function should be lowered");
    let reader = script
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("nested block function should be lowered");
    let captured = reader
        .captured_bindings
        .iter()
        .find(|binding| binding.name.starts_with("$scoped.lex."))
        .expect("nested block function should capture a scoped lexical binding");
    assert_ne!(captured.name, "value");
    assert!(!owner
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == captured.name));
    assert!(block_environment_owns_binding(
        &owner.body,
        &captured.name,
        captured.slot
    ));
}

#[test]
fn captured_block_bindings_use_nested_environment_hops_without_owner_activation() {
    let program = lower_script(
            "function owner() { { let outer = 1; { let inner = 2; function read() { return outer + inner; } return read; } } } owner();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner function should be lowered");
    let reader = script
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("reader function should be lowered");
    let outer = reader
        .captured_bindings
        .iter()
        .find(|binding| binding.source_name == "outer")
        .expect("outer block binding should be captured");
    let inner = reader
        .captured_bindings
        .iter()
        .find(|binding| binding.source_name == "inner")
        .expect("inner block binding should be captured");

    assert!(owner.owned_env_bindings.is_empty());
    assert_eq!(outer.hops, 1);
    assert_eq!(inner.hops, 0);
    assert!(block_environment_owns_binding(
        &owner.body,
        &outer.name,
        outer.slot
    ));
    assert!(block_environment_owns_binding(
        &owner.body,
        &inner.name,
        inner.slot
    ));
}

#[test]
fn captured_for_of_binding_uses_one_hop_from_the_body_block() {
    let program = lower_script(
            "function owner() { let saved; for (let value of [1]) { let body = 2; function read() { return value + body; } saved = read; } return saved; } owner();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let reader = script
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("reader function should be lowered");
    let capture_hops = reader
        .captured_bindings
        .iter()
        .map(|binding| (binding.source_name.as_str(), binding.hops))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(capture_hops.get("body"), Some(&0));
    assert_eq!(capture_hops.get("value"), Some(&1));
}

#[test]
fn captured_block_bindings_skip_to_owner_activation_when_it_exists() {
    let program = lower_script(
            "function owner(argument) { { let outer = 1; { let inner = 2; function read() { return argument + outer + inner; } return read; } } } owner(3);",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner function should be lowered");
    let reader = script
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("reader function should be lowered");
    let capture_hops = reader
        .captured_bindings
        .iter()
        .map(|binding| (binding.source_name.as_str(), binding.hops))
        .collect::<BTreeMap<_, _>>();

    assert!(owner
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == "argument"));
    assert_eq!(capture_hops.get("inner"), Some(&0));
    assert_eq!(capture_hops.get("outer"), Some(&1));
    assert_eq!(capture_hops.get("argument"), Some(&2));
}

#[test]
fn switch_case_block_functions_share_lexical_capture_aliases() {
    let program = lower_script(
            "function owner() { 'use strict'; let value = 0; switch (1) { case 1: let value = 1; case 2: function read() { return value; } return read(); } } owner();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner function should be lowered");
    let reader = script
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("case block function should be lowered");
    let captured = reader
        .captured_bindings
        .iter()
        .find(|binding| binding.name.starts_with("$scoped.lex."))
        .expect("case block function should capture the case lexical binding");
    assert!(owner.owned_env_bindings.is_empty());
    let StatementIr::Switch {
        lexical_environment,
        ..
    } = owner
        .body
        .statements
        .iter()
        .find(|statement| matches!(statement, StatementIr::Switch { .. }))
        .expect("owner should contain switch statement")
    else {
        panic!("expected switch statement");
    };
    assert!(lexical_environment.as_ref().is_some_and(|environment| {
        environment
            .bindings
            .iter()
            .any(|binding| binding.name == captured.name && binding.slot == captured.slot)
    }));
    assert!(block_environment_owns_binding(
        &owner.body,
        &captured.name,
        captured.slot
    ));
}

#[test]
fn switch_selector_reads_its_shared_lexical_environment_in_tdz() {
    let program = lower_script(
            "function select() { let value = 1; switch (value) { case value: let value = 2; } } select();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let select = script
        .functions
        .iter()
        .find(|function| function.name == "select")
        .expect("select function should be lowered");
    let StatementIr::Switch { cases, .. } = select
        .body
        .statements
        .iter()
        .find(|statement| matches!(statement, StatementIr::Switch { .. }))
        .expect("select should contain switch statement")
    else {
        panic!("expected switch statement");
    };
    let condition = cases[0]
        .condition
        .as_ref()
        .expect("case should have a selector");

    assert!(matches!(condition.expr, ExprIr::RuntimeThrow { .. }));
}

#[test]
fn block_shadow_read_before_declaration_uses_the_inner_tdz_binding() {
    let program =
        lower_script("function owner() { let value = 1; { value; let value = 2; } } owner();");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner function should be lowered");
    let block = owner
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Block(block) => Some(block),
            _ => None,
        })
        .expect("owner should contain the shadowing block");

    assert!(matches!(
        &block.statements[0],
        StatementIr::Expression(TypedExpr {
            expr: ExprIr::RuntimeThrow {
                name: NativeErrorKind::ReferenceError,
                ..
            },
            ..
        })
    ));
    let StatementIr::Lexical { name, .. } = &block.statements[1] else {
        panic!("expected the inner lexical declaration");
    };
    assert!(name.starts_with("$scoped.lex."));
}

#[test]
fn block_self_initializer_uses_the_inner_tdz_binding() {
    let program =
        lower_script("function owner() { let value = 1; { let value = value; } } owner();");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner function should be lowered");
    let block = owner
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Block(block) => Some(block),
            _ => None,
        })
        .expect("owner should contain the shadowing block");
    let StatementIr::Lexical { init, .. } = &block.statements[0] else {
        panic!("expected the inner lexical declaration");
    };

    assert!(matches!(
        &init.expr,
        ExprIr::RuntimeThrow {
            name: NativeErrorKind::ReferenceError,
            ..
        }
    ));
}

#[test]
fn switch_later_selector_reads_its_shared_lexical_environment_in_tdz() {
    let program = lower_script(
        "function select() { switch (1) { case 0: let value = 1; break; case value: } } select();",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let select = script
        .functions
        .iter()
        .find(|function| function.name == "select")
        .expect("select function should be lowered");
    let StatementIr::Switch { cases, .. } = select
        .body
        .statements
        .iter()
        .find(|statement| matches!(statement, StatementIr::Switch { .. }))
        .expect("select should contain switch statement")
    else {
        panic!("expected switch statement");
    };
    let condition = cases[1]
        .condition
        .as_ref()
        .expect("second case should have a selector");

    assert!(matches!(
        &condition.expr,
        ExprIr::RuntimeThrow {
            name: NativeErrorKind::ReferenceError,
            ..
        }
    ));
}

#[test]
fn catch_block_functions_capture_catch_scope_lexical_bindings() {
    let program = lower_script(
            "function owner() { 'use strict'; let value = 0; try { throw 1; } catch (error) { let value = 1; function read() { return value; } return read(); } } owner();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner function should be lowered");
    let reader = script
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("catch block function should be lowered");
    let captured = reader
        .captured_bindings
        .iter()
        .find(|binding| binding.name.starts_with("$scoped.lex."))
        .expect("catch block function should capture the catch lexical binding");
    assert!(owner.owned_env_bindings.is_empty());
    assert!(block_environment_owns_binding(
        &owner.body,
        &captured.name,
        captured.slot
    ));
}

#[test]
fn lowers_parent_linked_try_catch_finally_environment_layouts_and_hops() {
    let program = lower_script(
            "function owner() { try { let tried = 1; function readTried() { return tried; } throw 2; } catch (error) { let handled = error; function readHandled() { return error + handled; } return readHandled; } finally { let cleaned = 3; function readCleaned() { return cleaned; } } } owner();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner function should be lowered");
    let StatementIr::TryCatchFinally {
        try_block,
        catch_parameter_environment,
        catch_block,
        finally_block,
        ..
    } = owner
        .body
        .statements
        .iter()
        .find(|statement| matches!(statement, StatementIr::TryCatchFinally { .. }))
        .expect("owner should contain try/catch/finally")
    else {
        panic!("expected try/catch/finally statement");
    };

    assert!(try_block.lexical_environment.is_some());
    assert!(catch_parameter_environment.is_some());
    assert!(catch_block.lexical_environment.is_some());
    assert!(finally_block.lexical_environment.is_some());

    let handled = script
        .functions
        .iter()
        .find(|function| function.name == "readHandled")
        .expect("catch reader should be lowered");
    let capture_hops = handled
        .captured_bindings
        .iter()
        .map(|binding| (binding.source_name.as_str(), binding.hops))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(capture_hops.get("handled"), Some(&0));
    assert_eq!(capture_hops.get("error"), Some(&1));
}

#[test]
fn strict_block_bindings_do_not_leak_to_sibling_functions() {
    let program = lower_script(
            "function owner() { 'use strict'; { function hidden() { return 1; } } function outside() { return typeof hidden; } return outside(); } owner();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let outside = script
        .functions
        .iter()
        .find(|function| function.name == "outside")
        .expect("sibling function should be lowered");
    assert!(outside.captured_bindings.is_empty());
}

#[test]
fn annex_b_function_owner_parameter_and_arguments_bindings_block_outer_copies() {
    for (source, expected_target) in [
        ("function owner(f) { { function f() {} } return f; }", None),
        (
            "function owner() { { function arguments() {} } return arguments; }",
            Some(AnnexBFunctionCopyTargetIr::FunctionVariableEnvironment {
                name: "arguments".into(),
            }),
        ),
        (
            "function owner(arguments) { { function arguments() {} } return arguments; }",
            None,
        ),
        (
            "function owner() { function arguments() {} { function arguments() {} } return arguments; }",
            Some(AnnexBFunctionCopyTargetIr::OwnerBinding {
                storage_name: "arguments".into(),
            }),
        ),
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let script = program.script.as_ref().expect("script IR should exist");
        let copies = script
            .functions
            .iter()
            .flat_map(|function| collect_annex_b_copies(&function.body))
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), usize::from(expected_target.is_some()), "{source}");
        if let Some(expected_target) = expected_target {
            assert_eq!(copies[0].2, expected_target, "{source}");
        }
    }
}

#[test]
fn annex_b_arguments_formals_block_copy_and_parameter_expressions_keep_late_creation() {
    for source in [
        "function owner(value = arguments.length) { function arguments() {} { function arguments() {} } return arguments; }",
        "function owner(read = () => arguments) { function arguments() {} try {} catch (arguments) { { function arguments() {} } } return arguments; }",
    ] {
        let program = lower_script(source);
        assert!(program.is_wasm_supported(), "{source}: {:?}", program.diagnostics);
        let owner = program.script.as_ref().unwrap().functions.iter()
            .find(|function| function.name == "owner").unwrap();
        let copies = collect_annex_b_copies(&owner.body);
        assert_eq!(copies.len(), 1, "{source}");
        assert_eq!(copies[0].2, AnnexBFunctionCopyTargetIr::FunctionVariableEnvironment {
            name: "arguments".into(),
        }, "{source}");
        let body = owner.body.statements.iter().find_map(|statement| match statement {
            StatementIr::Block(body) if body.lexical_environment.as_ref().is_some_and(|environment|
                matches!(environment.initialization, LexicalEnvironmentInitializationIr::FunctionBody { .. })) => Some(body),
            _ => None,
        }).expect("parameter expressions retain their distinct body record");
        assert!(body.lexical_environment.as_ref().unwrap().bindings.iter()
            .any(|binding| binding.name == "arguments"), "{source}");
    }
    let program =
        lower_script("function owner(arguments) { { function arguments() {} } return arguments; }");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().unwrap();
    assert!(script
        .functions
        .iter()
        .all(|function| collect_annex_b_copies(&function.body).is_empty()));

    let program = lower_script(
        "function owner(read = () => arguments) { const later = () => arguments; { function arguments() {} } return [read(), later()]; }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().unwrap();
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .unwrap();
    let copies = collect_annex_b_copies(&owner.body);
    assert_eq!(copies.len(), 1);
    assert_eq!(
        copies[0].2,
        AnnexBFunctionCopyTargetIr::FunctionVariableEnvironment {
            name: "arguments".into(),
        }
    );
    let body = owner
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Block(body)
                if body
                    .lexical_environment
                    .as_ref()
                    .is_some_and(|environment| {
                        matches!(
                            environment.initialization,
                            LexicalEnvironmentInitializationIr::FunctionBody { .. }
                        )
                    }) =>
            {
                Some(body)
            }
            _ => None,
        })
        .expect("parameter expressions require a separate body environment");
    assert!(!body
        .lexical_environment
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .any(|binding| binding.name == "arguments"));
}

#[test]
fn annex_b_top_level_lexical_binding_blocks_outer_copy() {
    let program = lower_script("let f = 1; { function f() {} } f;");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    assert!(collect_annex_b_copies(&script.body).is_empty());
    assert!(!script
        .global_bindings
        .iter()
        .any(|binding| binding.name == "f"));
}

#[test]
fn annex_b_blocked_candidates_do_not_create_script_owner_bindings() {
    for (shape, source) in [
            (
                "block",
                "{ let f; { function f() {} } } typeof f; f; function outside() { return typeof f; }",
            ),
            (
                "switch",
                "switch (0) { default: let f; { function f() {} } } typeof f; f; function outside() { return typeof f; }",
            ),
        ] {
            let mut interner = Interner::default();
            let scope = Scope::new_global();
            let parsed_script = Parser::new(Source::from_bytes(source.as_bytes()))
                .parse_script(&scope, &mut interner)
                .expect("script should parse");
            let analysis = AnalysisBuilder::default().finish(&parsed_script, &interner, source);
            assert!(
                !analysis.owner_plans[SCRIPT_OWNER_ID]
                    .root_bindings
                    .contains("f"),
                "{shape}: blocked Annex B candidate must not create a script owner binding"
            );

            let program = lower_script(source);
            assert!(
                program.is_wasm_supported(),
                "{shape}: {:?}",
                program.diagnostics
            );
            let script = program.script.as_ref().expect("script IR should exist");
            assert!(
                !script
                    .global_bindings
                    .iter()
                    .any(|binding| binding.name == "f"),
                "{shape}: blocked Annex B candidate must not create a script global binding"
            );
            assert!(
                collect_annex_b_copies(&script.body)
                    .iter()
                    .all(|(source, _, _)| source != "f"),
                "{shape}: blocked Annex B candidate must not copy to the variable environment"
            );
            assert!(
                script.body.statements.iter().any(|statement| matches!(
                    statement,
                    StatementIr::Expression(TypedExpr {
                        expr: ExprIr::TypeOfUnresolvedIdentifier { .. },
                        ..
                    })
                )),
                "{shape}: outer typeof f must be unresolved"
            );
            assert!(
                script.body.statements.iter().any(|statement| matches!(
                    statement,
                    StatementIr::Expression(TypedExpr {
                        expr: ExprIr::GlobalIdentifierRead { name },
                        ..
                    }) if name == "f"
                )),
                "{shape}: outer f read must use runtime global resolution"
            );

            let outside = script
                .functions
                .iter()
                .find(|function| function.name == "outside")
                .expect("outside function should be lowered");
            assert!(
                outside.captured_bindings.is_empty(),
                "{shape}: outside callback must not capture blocked f"
            );
            assert!(
                outside.body.statements.iter().any(|statement| matches!(
                    statement,
                    StatementIr::Return(TypedExpr {
                        expr: ExprIr::TypeOfUnresolvedIdentifier { .. },
                        ..
                    })
                )),
                "{shape}: outside callback typeof f must be unresolved"
            );
        }
}

#[test]
fn annex_b_existing_var_and_function_bindings_are_reused() {
    let program = lower_script(
            "function owner() { var f = 1; { function f() {} } function g() {} { function g() {} } return f; }",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner should be lowered");
    let copies = collect_annex_b_copies(&owner.body);
    assert_eq!(copies.len(), 2);
    assert!(copies.iter().any(|(source, _, target)| source == "f"
        && target
            == &AnnexBFunctionCopyTargetIr::OwnerBinding {
                storage_name: "f".to_string(),
            }));
    assert!(copies.iter().any(|(source, _, target)| source == "g"
        && target
            == &AnnexBFunctionCopyTargetIr::OwnerBinding {
                storage_name: "g".to_string(),
            }));
}

#[test]
fn annex_b_copy_bypasses_a_same_named_catch_binding() {
    let program = lower_script(
        "function owner() { try { throw 1; } catch (f) { { function f() {} } } return f; }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner should be lowered");
    let copies = collect_annex_b_copies(&owner.body);
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].0, "f");
    assert_eq!(
        copies[0].2,
        AnnexBFunctionCopyTargetIr::OwnerBinding {
            storage_name: "f".to_string()
        }
    );
}

#[test]
fn annex_b_duplicate_declarations_share_the_last_block_binding() {
    let program = lower_script(
        "function owner() { { function f() { return 1; } function f() { return 2; } } return f; }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner should be lowered");
    let copies = collect_annex_b_copies(&owner.body);
    assert_eq!(copies.len(), 2);
    assert_eq!(copies[0].1, copies[1].1);
}

#[test]
fn annex_b_switch_declarations_share_one_case_block_binding() {
    let program = lower_script(
            "function owner(v) { switch (v) { case 0: function f() { return 1; } break; default: function f() { return 2; } } return f; }",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner should be lowered");
    let switch = owner
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Switch {
                lexical_declarations,
                cases,
                ..
            } => Some((lexical_declarations, cases)),
            _ => None,
        })
        .expect("switch should be lowered");
    assert_eq!(switch.0.len(), 1);
    assert_eq!(collect_annex_b_copies(&owner.body).len(), 2);
}

#[test]
fn nested_labelled_function_declaration_remains_block_scoped() {
    let program = lower_script(
            "function owner() { var result; { label: function f() { return 6; } result = f(); } return result; }",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner should be lowered");
    assert!(collect_annex_b_copies(&owner.body).is_empty());
    assert!(script.functions.iter().any(|function| function.name == "f"));
}

#[test]
fn annex_b_for_lexical_binding_blocks_outer_copy() {
    let program = lower_script(
            "function owner() { for (let f;;) { if (false) function _f() {} else function f() {} break; } return typeof f; }",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner should be lowered");
    let copies = collect_annex_b_copies(&owner.body);
    assert!(copies.iter().any(|(source, _, _)| source == "_f"));
    assert!(copies.iter().all(|(source, _, _)| source != "f"));
}

#[test]
fn annex_b_labelled_block_function_shares_storage_with_direct_redeclaration() {
    let program = lower_script(
        "function test() { { function f() { return 1; } l: function f() { return 2; } } return f(); } test(); \
         { function g() { return 1; } m: function g() { return 2; } }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let copies = collect_annex_b_copies(&script.body);
    assert!(copies.iter().any(|(source, _, _)| source == "g"));
}

#[test]
fn labelled_lexical_selector_stops_at_a_statement_instead_of_unwrapping_labels() {
    let source = "single: function first() {} outer: inner: function chained() {}";
    let mut interner = Interner::default();
    let scope = Scope::new_global();
    let parsed = Parser::new(Source::from_bytes(source.as_bytes()))
        .parse_script(&scope, &mut interner)
        .expect("sloppy labelled declarations should parse");
    let items = parsed.statements().statements();
    assert!(lexically_scoped_statement_list_item_function_declaration(&items[0]).is_some());
    assert!(lexically_scoped_statement_list_item_function_declaration(&items[1]).is_none());
}

#[test]
fn labelled_block_and_case_environments_keep_only_direct_lexical_label_items() {
    for source in [
        "function owner() { { label: function single() {} a: b: function chained() {} function read() { return [single, typeof chained]; } eval('typeof single'); } }",
        "function owner() { switch (0) { case 0: label: function single() {} a: b: function chained() {} function read() { return [single, typeof chained]; } eval('typeof single'); } }",
    ] {
        with_script_analysis(source, |analysis| {
            let read = analysis
                .function_plans
                .values()
                .find(|function| function.name == "read")
                .expect("the reader must be analyzed");
            assert!(read.captures.values().any(|capture| capture.source_name == "single"), "{source}");
            assert!(!read.captures.values().any(|capture| capture.source_name == "chained"), "{source}");
            let environment = analysis
                .environment_plans
                .values()
                .find(|environment| {
                    matches!(
                        environment.kind,
                        EnvironmentKind::Block | EnvironmentKind::SwitchCaseBlock
                    ) && environment
                        .binding_storage_names
                        .iter()
                        .any(|name| name.ends_with(".single"))
                })
                .expect("the single-labelled function needs a lexical environment");
            assert!(!environment
                .binding_storage_names
                .iter()
                .any(|name| name.ends_with(".chained")), "{source}");
            let Some(EvalEnvironmentRoleIr::Declarative { bindings, .. }) =
                &environment.eval_environment
            else {
                panic!("the direct-eval-visible lexical record must be retained: {source}");
            };
            assert!(bindings.iter().any(|binding| binding.source_name == "single"));
            assert!(bindings.iter().all(|binding| binding.source_name != "chained"));
        });
    }
}

#[test]
fn labelled_case_initialization_omits_chains_without_changing_annex_b_copies() {
    let program = lower_script(
        "function owner() { switch (0) { case 0: label: function single() {} a: b: function chained() {} } return typeof chained; }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR");
    let owner = script.functions.iter().find(|f| f.name == "owner").unwrap();
    let declarations = owner
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Switch {
                lexical_declarations,
                ..
            } => Some(lexical_declarations),
            _ => None,
        })
        .expect("switch IR");
    assert_eq!(declarations.len(), 1);
    assert!(matches!(&declarations[0], StatementIr::Lexical { name, .. }
        if name.ends_with(".single")));
    assert!(collect_annex_b_copies(&owner.body).is_empty());
    assert!(!owner
        .owned_env_bindings
        .iter()
        .any(|binding| { binding.name == "chained" || binding.name.ends_with(".chained") }));
}

#[test]
fn labelled_chains_keep_script_and_function_top_level_var_hoisting() {
    let source = "root(); a: b: function root() { return 17; } function owner() { inner(); c: d: function inner() { return 19; } }";
    let program = lower_script(source);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR");
    assert!(script.global_bindings.iter().any(|binding| {
        binding.name == "root" && binding.declarations == GlobalDeclarationSetIr::Function
    }));
    with_script_analysis(source, |analysis| {
        let owner = function_owner_plan_by_name(analysis, "owner");
        assert!(owner.root_bindings.contains("inner"));
        assert!(owner.function_bindings.contains_key("inner"));
    });
}

#[test]
fn labelled_chains_do_not_reserve_activation_cells_visible_to_direct_eval() {
    for (source, function_owner) in [
        (
            "let visible = 83; { eval('visible'); a: b: function visible() {} function ordinary() {} }",
            false,
        ),
        (
            "function owner() { let visible = 83; { eval('visible'); a: b: function visible() {} function ordinary() {} } }",
            true,
        ),
        (
            "function owner(value = 0) { let visible = 83; { eval('visible'); a: b: function visible() {} function ordinary() {} } }",
            true,
        ),
    ] {
        with_script_analysis(source, |analysis| {
            let owner = if function_owner {
                function_owner_plan_by_name(analysis, "owner")
            } else {
                &analysis.owner_plans[SCRIPT_OWNER_ID]
            };
            let body = &analysis.environment_plans
                [&owner.body_environment_id.unwrap_or(owner.activation_environment_id)];
            assert!(body.binding_storage_names.contains("visible"), "{source}");
            assert!(body.binding_storage_names.iter().all(|name| {
                !name.starts_with("$annexb.block.") || !name.ends_with(".visible")
            }), "a nonlexical chain must not reserve an uninitialized cell: {source}");
            let Some(EvalEnvironmentRoleIr::Declarative { bindings, .. }) =
                &body.eval_environment
            else {
                panic!("the actual outer binding must remain visible to eval: {source}");
            };
            assert_eq!(
                bindings.iter().filter(|binding| binding.source_name == "visible").count(),
                1,
                "the named record must not contain duplicate source names: {source}"
            );
            assert!(analysis.annex_b_function_plans.values().all(|plan| {
                plan.source_name != "visible"
            }));
            assert!(analysis.annex_b_function_plans.values().any(|plan| {
                plan.source_name == "ordinary" && plan.copy_to_variable_environment
            }), "ordinary eligible copies must remain intact: {source}");
            assert_eq!(
                analysis.annex_b_function_plans.values()
                    .filter(|plan| plan.copy_to_variable_environment)
                    .map(|plan| plan.source_name.as_str())
                    .collect::<Vec<_>>(),
                vec!["ordinary"],
                "the eligible copy set must stay exact: {source}"
            );
        });
    }
}

#[test]
fn labelled_case_chains_do_not_reserve_activation_cells_visible_to_direct_eval() {
    with_script_analysis(
        "function owner() { let visible = 83; switch (0) { case 0: eval('visible'); a: b: function visible() {} function ordinary() {} } }",
        |analysis| {
            let owner = function_owner_plan_by_name(analysis, "owner");
            let activation = &analysis.environment_plans[&owner.activation_environment_id];
            assert!(activation.binding_storage_names.contains("visible"));
            assert!(activation.binding_storage_names.iter().all(|name| {
                !name.starts_with("$annexb.block.") || !name.ends_with(".visible")
            }));
            let Some(EvalEnvironmentRoleIr::Declarative { bindings, .. }) =
                &activation.eval_environment
            else {
                panic!("the activation has a real direct-eval-visible lexical binding");
            };
            assert_eq!(bindings.iter().filter(|binding| binding.source_name == "visible").count(), 1);
            assert!(analysis.annex_b_function_plans.values().all(|plan| plan.source_name != "visible"));
            assert!(analysis.annex_b_function_plans.values().any(|plan| {
                plan.source_name == "ordinary" && plan.copy_to_variable_environment
            }));
            assert_eq!(
                analysis.annex_b_function_plans.values()
                    .filter(|plan| plan.copy_to_variable_environment)
                    .map(|plan| plan.source_name.as_str())
                    .collect::<Vec<_>>(),
                vec!["ordinary"]
            );
        },
    );
}

#[test]
fn all_block_hoistable_forms_have_captured_eval_visible_lexical_bindings() {
    for source in [
        "'use strict'; function owner() { { eval('0'); function plain() {} function* gen() { yield 1; } async function task() {} async function* stream() { yield 2; } function read() { return [plain, gen, task, stream]; } } }",
        "'use strict'; function owner() { switch (0) { case 0: eval('0'); function plain() {} function* gen() { yield 1; } async function task() {} async function* stream() { yield 2; } function read() { return [plain, gen, task, stream]; } } }",
    ] {
        with_script_analysis(source, |analysis| {
            let reader = analysis.function_plans.values().find(|function| function.name == "read").expect("reader owner");
            for name in ["plain", "gen", "task", "stream"] {
                assert!(reader.captures.values().any(|capture| capture.source_name == name), "missing capture {name}: {source}");
            }
            let environment = analysis.environment_plans.values().find(|environment| {
                matches!(environment.kind, EnvironmentKind::Block | EnvironmentKind::SwitchCaseBlock)
                    && environment.binding_storage_names.iter().any(|name| name.ends_with(".gen"))
            }).expect("generator's actual lexical environment");
            let Some(EvalEnvironmentRoleIr::Declarative { bindings, .. }) = &environment.eval_environment else {
                panic!("all hoistable bindings must remain visible to eval");
            };
            for name in ["plain", "gen", "task", "stream"] {
                assert!(bindings.iter().any(|binding| binding.source_name == name), "missing runtime binding {name}: {source}");
            }
        });
    }
}

#[test]
fn case_block_instantiates_all_hoistable_forms_once_before_case_selection() {
    let program = lower_script("'use strict'; function owner() { switch (0) { case 0: gen(); task(); stream(); break; case 1: function plain() {} function* gen() { yield 1; } async function task() {} async function* stream() { yield 2; } } }");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR");
    let owner = script
        .functions
        .iter()
        .find(|function| function.name == "owner")
        .expect("owner IR");
    let declarations = owner
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Switch {
                lexical_declarations,
                ..
            } => Some(lexical_declarations),
            _ => None,
        })
        .expect("switch IR");
    assert_eq!(declarations.len(), 4);
    for source_name in ["plain", "gen", "task", "stream"] {
        assert_eq!(declarations.iter().filter(|declaration| matches!(declaration, StatementIr::Lexical { name, init, .. }
            if name.ends_with(&format!(".{source_name}")) && matches!(&init.expr, ExprIr::FunctionValue(_)))).count(), 1);
    }
}

#[test]
fn annex_b_implicit_arguments_exclusion_obeys_parameter_expression_precedence() {
    // Preserve the historical test ID while checking the pinned distinction:
    // formal arguments parameters exclude copying; implicit arguments do not.
    for (source, copies) in [
        ("function owner(x) { { function arguments() {} } }", 1),
        ("function owner(...rest) { { function arguments() {} } }", 1),
        (
            "function owner(arguments) { { function arguments() {} } }",
            0,
        ),
        (
            "function owner(x = 1) { function arguments() {} { function arguments() {} } }",
            1,
        ),
        (
            "function owner() { function arguments() {} { function arguments() {} } }",
            1,
        ),
        ("const owner = () => { { function arguments() {} } };", 1),
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let owner = program
            .script
            .as_ref()
            .unwrap()
            .functions
            .iter()
            .find(|function| function.name == "owner")
            .unwrap();
        assert_eq!(
            collect_annex_b_copies(&owner.body).len(),
            copies,
            "{source}"
        );
    }
}
