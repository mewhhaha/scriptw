#[test]
fn analysis_tracks_nested_block_environment_ownership() {
    with_script_analysis(
            "\"use strict\"; { let outer = 1; { const inner = 2; function read() { return outer + inner; } } }",
            |analysis| {
                let script_activation =
                    analysis.owner_plans[SCRIPT_OWNER_ID].activation_environment_id;
                let outer = environment_with_binding_suffix(analysis, ".outer");
                let inner = environment_with_binding_suffix(analysis, ".inner");

                assert_eq!(outer.kind, EnvironmentKind::Block);
                assert_eq!(
                    outer
                        .parent_cursor
                        .as_ref()
                        .map(|cursor| cursor.environment_id),
                    Some(script_activation)
                );
                assert_eq!(inner.kind, EnvironmentKind::Block);
                assert_eq!(
                    inner
                        .parent_cursor
                        .as_ref()
                        .map(|cursor| cursor.environment_id),
                    Some(outer.id)
                );
                assert_physical_binding_owner(
                    analysis,
                    binding_with_suffix(outer, ".outer"),
                    outer,
                );
                assert_physical_binding_owner(
                    analysis,
                    binding_with_suffix(inner, ".inner"),
                    inner,
                );

                let read = function_owner_plan_by_name(analysis, "read");
                assert_eq!(read.definition_environment_cursor.environment_id, inner.id);
            },
        );
}

#[test]
fn analysis_orders_with_objects_and_declarative_environments_at_function_definition() {
    with_script_analysis(
            "var writer; with (outer) { let x = 0; with (inner) writer = function write() { 'use strict'; x = 2; }; }",
            |analysis| {
                let function = analysis
                    .function_plans
                    .values()
                    .find(|function| function.name == "write")
                    .expect("nested strict function should be planned");
                let mut cursor = Some(
                    analysis.owner_plans[&function.id]
                        .definition_environment_cursor
                        .clone(),
                );
                let mut kinds = Vec::new();
                while let Some(current) = cursor {
                    let environment = &analysis.environment_plans[&current.environment_id];
                    kinds.push(environment.kind);
                    cursor = environment.parent_cursor.clone();
                }
                assert_eq!(
                    &kinds[..5],
                    &[
                        EnvironmentKind::NamedFunctionExpression,
                        EnvironmentKind::WithObject,
                        EnvironmentKind::Block,
                        EnvironmentKind::WithObject,
                        EnvironmentKind::Activation,
                    ]
                );

                let with_captures = function
                    .captures
                    .values()
                    .filter(|capture| {
                        analysis.environment_plans[&capture.environment_id].kind
                            == EnvironmentKind::WithObject
                    })
                    .collect::<Vec<_>>();
                assert_eq!(with_captures.len(), 2);
                for capture in with_captures {
                    assert!(capture.source_name.starts_with("$with.object."));
                    assert_eq!(
                        analysis.with_object_environment_plans[&capture.environment_id]
                            .binding_name
                            .as_str(),
                        capture.source_name.as_str()
                    );
                    let environment = &analysis.environment_plans[&capture.environment_id];
                    assert_eq!(
                        environment.owned_env_slots.get(&capture.source_name),
                        Some(&capture.slot)
                    );
                }
            },
        );
}

#[test]
fn with_object_expression_functions_keep_the_outer_definition_cursor() {
    with_script_analysis(
            "var body; with ((function objectExpression() { return {}; })()) body = function body() {};",
            |analysis| {
                let object_expression = function_owner_plan_by_name(analysis, "objectExpression");
                let body = function_owner_plan_by_name(analysis, "body");
                let object_name = &analysis.environment_plans[&object_expression
                    .definition_environment_cursor.environment_id];
                let body_name = &analysis.environment_plans[&body
                    .definition_environment_cursor.environment_id];
                assert_eq!(object_name.kind, EnvironmentKind::NamedFunctionExpression);
                assert_eq!(body_name.kind, EnvironmentKind::NamedFunctionExpression);
                assert_eq!(
                    object_name.parent_cursor.as_ref().unwrap().environment_id,
                    analysis.owner_plans[SCRIPT_OWNER_ID].activation_environment_id
                );
                assert_eq!(
                    analysis.environment_plans[&body_name.parent_cursor.as_ref().unwrap()
                        .environment_id].kind,
                    EnvironmentKind::WithObject
                );
            },
        );
}

#[test]
fn nested_strict_with_assignment_reads_the_production_hidden_capture() {
    let program = lower_script(
        "var x = 91; var scope = { x: 1 }; var write; with (scope) write = function write() { 'use strict'; x = 2; };",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let write = script
        .functions
        .iter()
        .find(|function| function.name == "write")
        .expect("nested strict function should be lowered");
    let with_capture = write
        .captured_bindings
        .iter()
        .find(|binding| binding.name.starts_with("$with.object."))
        .expect("nested strict function must capture its Object Environment Record");
    assert!(block_environment_owns_binding(
        &script.body,
        &with_capture.name,
        with_capture.slot,
    ));
    let assignment = write
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Expression(expression)
                if matches!(&expression.expr, ExprIr::EnvironmentIdentifier(_)) =>
            {
                Some(expression)
            }
            _ => None,
        })
        .expect("with assignment should retain its initial resolution");
    let ExprIr::EnvironmentIdentifier(identifier) = &assignment.expr else {
        unreachable!()
    };
    assert_eq!(identifier.strictness, Strictness::Strict);
    assert_eq!(
        identifier.resolution_start(),
        EnvironmentIdentifierResolutionStart::GlobalEnvironment
    );
    let EnvironmentIdentifierOperationIr::AssignWithGlobalFallback { selection, value } =
        &identifier.operation
    else {
        panic!("the global fallback must resolve before its RHS");
    };
    assert!(matches!(&value.expr, ExprIr::Number(value) if *value == 2.0_f64.to_bits()));
    let ExprIr::Conditional { condition, .. } = &selection.expr else {
        panic!("the retained selection must branch on the captured Object Environment");
    };
    let ExprIr::SpecOperation {
        operation: SpecOperationIr::WithEnvironmentHasBinding,
        operands,
    } = &condition.expr
    else {
        panic!("Object Environment HasBinding must guard the selected write");
    };
    assert!(matches!(
        &operands[0].expr,
        ExprIr::Identifier(name) if name == &with_capture.name
    ));
}

#[test]
fn resumable_function_capture_of_with_object_environment_is_explicitly_rejected() {
    let program = lower_script(
        "var generator; with ({ x: 1 }) generator = function* generator() { yield x; };",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().unwrap();
    let function = script
        .functions
        .iter()
        .find(|function| function.name == "generator")
        .unwrap();
    assert_eq!(
        function.protocol.execution_kind(),
        FunctionExecutionKind::Generator
    );
    let capture = function
        .captured_bindings
        .iter()
        .find(|binding| binding.name.starts_with("$with.object."))
        .expect("the generator retains the original hidden Object Environment cell");
    assert!(block_environment_owns_binding(
        &script.body,
        &capture.name,
        capture.slot
    ));
    assert_eq!(
        function
            .generator_plan
            .as_ref()
            .unwrap()
            .suspension_points
            .len(),
        1
    );
    let mut statements = Vec::new();
    identifier_reference_rows(&function.body.statements, &mut statements);
    assert!(
        statements.into_iter().any(|statement| {
            let value = match statement {
                StatementIr::Lexical { init, .. } | StatementIr::Expression(init) => init,
                StatementIr::GeneratorYield { value, .. } => value,
                _ => return false,
            };
            matches!(&value.expr, ExprIr::Conditional { condition, .. }
            if matches!(&condition.expr, ExprIr::SpecOperation {
                operation: SpecOperationIr::WithEnvironmentHasBinding, operands,
            } if matches!(&operands[0].expr, ExprIr::Identifier(name) if name == &capture.name)))
        }),
        "resumed reads use the captured original Object Environment"
    );
}

#[test]
fn analysis_shares_one_environment_for_switch_cases() {
    with_script_analysis(
        "switch (0) { case 0: let first = 1; break; default: const second = 2; }",
        |analysis| {
            let first = environment_with_binding_suffix(analysis, ".first");
            let second = environment_with_binding_suffix(analysis, ".second");
            let script_activation = analysis.owner_plans[SCRIPT_OWNER_ID].activation_environment_id;

            assert_eq!(first.kind, EnvironmentKind::SwitchCaseBlock);
            assert_eq!(first.id, second.id);
            assert_eq!(analysis.switch_environment_ids.len(), 1);
            assert!(analysis
                .switch_environment_ids
                .values()
                .any(|environment_id| environment_id == &first.id));
            assert_eq!(
                first
                    .parent_cursor
                    .as_ref()
                    .map(|cursor| cursor.environment_id),
                Some(script_activation)
            );
            assert_physical_binding_owner(analysis, binding_with_suffix(first, ".first"), first);
            assert_physical_binding_owner(analysis, binding_with_suffix(second, ".second"), second);
        },
    );
}

#[test]
fn analysis_separates_try_catch_parameter_catch_body_and_finally_blocks() {
    with_script_analysis(
            "\"use strict\"; try { let tried = 1; } catch (error) { let handled = error; function reader() { return error + handled; } } finally { const cleaned = 1; }",
            |analysis| {
                let script_activation =
                    analysis.owner_plans[SCRIPT_OWNER_ID].activation_environment_id;
                let tried = environment_with_binding_suffix(analysis, ".tried");
                let error = environment_with_binding_suffix(analysis, ".error");
                let handled = environment_with_binding_suffix(analysis, ".handled");
                let cleaned = environment_with_binding_suffix(analysis, ".cleaned");

                assert_eq!(tried.kind, EnvironmentKind::Block);
                assert_eq!(error.kind, EnvironmentKind::SimpleCatchParameter);
                assert_eq!(handled.kind, EnvironmentKind::Block);
                assert_eq!(cleaned.kind, EnvironmentKind::Block);
                assert_eq!(analysis.catch_parameter_environment_ids.len(), 1);
                assert!(
                    analysis
                        .catch_parameter_environment_ids
                        .values()
                        .any(|environment_id| environment_id == &error.id)
                );
                for environment in [tried, handled, cleaned] {
                    assert!(
                        analysis
                            .block_environment_ids
                            .values()
                            .any(|environment_id| environment_id == &environment.id)
                    );
                }
                for environment in [tried, error, cleaned] {
                    assert_eq!(
                        environment
                            .parent_cursor
                            .as_ref()
                            .map(|cursor| cursor.environment_id),
                        Some(script_activation)
                    );
                }
                assert_eq!(
                    handled
                        .parent_cursor
                        .as_ref()
                        .map(|cursor| cursor.environment_id),
                    Some(error.id)
                );
                assert_physical_binding_owner(
                    analysis,
                    binding_with_suffix(tried, ".tried"),
                    tried,
                );
                assert_physical_binding_owner(
                    analysis,
                    binding_with_suffix(error, ".error"),
                    error,
                );
                assert_physical_binding_owner(
                    analysis,
                    binding_with_suffix(handled, ".handled"),
                    handled,
                );
                assert_physical_binding_owner(
                    analysis,
                    binding_with_suffix(cleaned, ".cleaned"),
                    cleaned,
                );

                let reader = function_owner_plan_by_name(analysis, "reader");
                assert_eq!(
                    reader.definition_environment_cursor.environment_id,
                    handled.id
                );
            },
        );
}

#[test]
fn analysis_tracks_classic_for_lexical_head_environment() {
    with_script_analysis(
            "\"use strict\"; for (let index = 0; index < 1; index++) { const value = index; function read() { return index + value; } }",
            |analysis| {
                let script_activation =
                    analysis.owner_plans[SCRIPT_OWNER_ID].activation_environment_id;
                let index = environment_with_binding_suffix(analysis, ".index");
                let value = environment_with_binding_suffix(analysis, ".value");

                assert_eq!(index.kind, EnvironmentKind::ForLexicalHead);
                assert_eq!(value.kind, EnvironmentKind::Block);
                assert!(
                    analysis
                        .for_lexical_environment_ids
                        .values()
                        .any(|environment_id| *environment_id == index.id)
                );
                assert_eq!(
                    index
                        .parent_cursor
                        .as_ref()
                        .map(|cursor| cursor.environment_id),
                    Some(script_activation)
                );
                assert_eq!(
                    value
                        .parent_cursor
                        .as_ref()
                        .map(|cursor| cursor.environment_id),
                    Some(index.id)
                );
                assert_physical_binding_owner(
                    analysis,
                    binding_with_suffix(index, ".index"),
                    index,
                );
                assert_physical_binding_owner(
                    analysis,
                    binding_with_suffix(value, ".value"),
                    value,
                );

                let read = function_owner_plan_by_name(analysis, "read");
                assert_eq!(read.definition_environment_cursor.environment_id, value.id);
            },
        );
}

#[test]
fn analysis_tracks_for_in_and_for_of_tdz_and_iteration_environments() {
    with_script_analysis(
            "\"use strict\"; for (let property in property) { const inRead = () => property; } for (const value of value) { const ofRead = () => value; }",
            |analysis| {
                let script_activation =
                    analysis.owner_plans[SCRIPT_OWNER_ID].activation_environment_id;
                let property_tdz = analysis
                    .environment_plans
                    .values()
                    .find(|environment| {
                        environment.kind == EnvironmentKind::ForInOfTdzHead
                            && environment.binding_storage_names.contains("$tdz.property")
                    })
                    .expect("for-in TDZ environment should be planned");
                let property_iteration = analysis
                    .environment_plans
                    .values()
                    .find(|environment| {
                        environment.kind == EnvironmentKind::ForInOfIteration
                            && environment.binding_storage_names.iter().any(|binding| {
                                binding.starts_with("$forin.lex.") && binding.ends_with(".property")
                            })
                    })
                    .expect("for-in iteration environment should be planned");
                let value_tdz = analysis
                    .environment_plans
                    .values()
                    .find(|environment| {
                        environment.kind == EnvironmentKind::ForInOfTdzHead
                            && environment.binding_storage_names.contains("$tdz.value")
                    })
                    .expect("for-of TDZ environment should be planned");
                let value_iteration = analysis
                    .environment_plans
                    .values()
                    .find(|environment| {
                        environment.kind == EnvironmentKind::ForInOfIteration
                            && environment.binding_storage_names.iter().any(|binding| {
                                binding.starts_with("$forof.lex.") && binding.ends_with(".value")
                            })
                    })
                    .expect("for-of iteration environment should be planned");

                for environment in [property_tdz, value_tdz] {
                    assert!(
                        analysis
                            .for_in_of_tdz_environment_ids
                            .values()
                            .any(|environment_id| *environment_id == environment.id)
                    );
                }
                for environment in [property_iteration, value_iteration] {
                    assert!(
                        analysis
                            .for_in_of_iteration_environment_ids
                            .values()
                            .any(|environment_id| *environment_id == environment.id)
                    );
                }

                assert_eq!(
                    property_iteration
                        .parent_cursor
                        .as_ref()
                        .map(|cursor| cursor.environment_id),
                    Some(script_activation)
                );
                assert_eq!(
                    property_tdz
                        .parent_cursor
                        .as_ref()
                        .map(|cursor| cursor.environment_id),
                    Some(script_activation)
                );
                assert_eq!(
                    value_iteration
                        .parent_cursor
                        .as_ref()
                        .map(|cursor| cursor.environment_id),
                    Some(script_activation)
                );
                assert_eq!(
                    value_tdz
                        .parent_cursor
                        .as_ref()
                        .map(|cursor| cursor.environment_id),
                    Some(script_activation)
                );
                assert_physical_binding_owner(analysis, "$tdz.property", property_tdz);
                assert_physical_binding_owner(analysis, "$tdz.value", value_tdz);
                let property_binding = property_iteration
                    .binding_storage_names
                    .iter()
                    .find(|binding| {
                        binding.starts_with("$forin.lex.") && binding.ends_with(".property")
                    })
                    .expect("for-in iteration should own its physical binding");
                let value_binding = value_iteration
                    .binding_storage_names
                    .iter()
                    .find(|binding| {
                        binding.starts_with("$forof.lex.") && binding.ends_with(".value")
                    })
                    .expect("for-of iteration should own its physical binding");
                assert_physical_binding_owner(analysis, property_binding, property_iteration);
                assert_physical_binding_owner(analysis, value_binding, value_iteration);
            },
        );
}

#[test]
fn analysis_stamps_root_and_block_function_definition_cursors() {
    with_script_analysis(
            "\"use strict\"; function root(parameter) { var variable = parameter; } { function nested() {} }",
            |analysis| {
                let script_activation =
                    analysis.owner_plans[SCRIPT_OWNER_ID].activation_environment_id;
                let nested_environment = analysis
                    .environment_plans
                    .values()
                    .find(|environment| {
                        environment.kind == EnvironmentKind::Block
                            && environment
                                .binding_storage_names
                                .iter()
                                .any(|binding| binding.ends_with(".nested"))
                    })
                    .expect("nested function block should be planned");
                let root = function_owner_plan_by_name(analysis, "root");
                let root_activation = &analysis.environment_plans[&root.activation_environment_id];

                assert_eq!(
                    root.definition_environment_cursor.environment_id,
                    script_activation
                );
                assert_eq!(root_activation.kind, EnvironmentKind::Activation);
                assert_eq!(
                    root_activation
                        .parent_cursor
                        .as_ref()
                        .map(|cursor| (cursor.owner_id.as_str(), cursor.environment_id)),
                    Some((SCRIPT_OWNER_ID, script_activation))
                );
                assert!(root_activation.binding_storage_names.contains("parameter"));
                assert!(root_activation.binding_storage_names.contains("variable"));
                assert_physical_binding_owner(analysis, "parameter", root_activation);
                assert_physical_binding_owner(analysis, "variable", root_activation);
                assert_eq!(
                    function_owner_plan_by_name(analysis, "nested")
                        .definition_environment_cursor
                        .environment_id,
                    nested_environment.id
                );
            },
        );
}

#[test]
fn analysis_empty_blocks_never_allocate_eval_visible_environments() {
    for prefix in ["", "'use strict';"] {
        let source = format!("{prefix} eval(0); {}", "{}".repeat(4096));
        with_script_analysis(&source, |analysis| {
            assert!(analysis.block_environment_ids.is_empty());
            assert!(analysis
                .environment_plans
                .values()
                .all(|environment| environment.kind != EnvironmentKind::Block));
        });
    }
}

#[test]
fn analysis_empty_control_and_try_blocks_keep_only_their_enclosing_owners() {
    with_script_analysis(
        "eval(0); empty: {} if (flag) {} else {} while (flag) {} for (;flag;) {} \
         do {} while (flag); try {} catch (error) {} finally {}",
        |analysis| {
            assert!(analysis.block_environment_ids.is_empty());
            assert!(analysis
                .environment_plans
                .values()
                .all(|environment| environment.kind != EnvironmentKind::Block));
            assert_eq!(analysis.catch_parameter_environment_ids.len(), 1);
            assert_eq!(
                environment_with_binding_suffix(analysis, ".error").kind,
                EnvironmentKind::SimpleCatchParameter
            );
        },
    );
}

#[test]
fn analysis_nonempty_blocks_retain_eval_visibility_and_binding_ownership() {
    for source in ["eval(0); { ; }", "{ let retained = 17; eval(0); }"] {
        with_script_analysis(source, |analysis| {
            assert_eq!(analysis.block_environment_ids.len(), 1);
            let environment = &analysis.environment_plans
                [analysis.block_environment_ids.values().next().unwrap()];
            assert_eq!(environment.kind, EnvironmentKind::Block);
            assert!(environment.eval_visible);
            assert!(analysis.environment_has_runtime_storage(environment));
            if source.contains("retained") {
                assert_physical_binding_owner(
                    analysis,
                    binding_with_suffix(environment, ".retained"),
                    environment,
                );
            }
        });
    }
}

#[test]
fn arguments_references_resolve_to_one_binding_without_exposing_any_owner() {
    // Without eval, `with` or an Annex B `function arguments`, an `arguments`
    // reference names one statically known binding. No environment of any
    // owner, including the one whose arguments object it is, becomes
    // eval-visible, so unrelated owners keep their proven cells.
    with_script_analysis(
        "var shared = 1; \
         function outer() { 'use strict'; var n = 0; return function inner() { return n + shared; }; } \
         function counts() { return arguments.length; } \
         function implicit() { return () => arguments; } \
         function declared() { var arguments; return () => arguments; } \
         function parameters(a = () => arguments) { var arguments; return a; } \
         function blocked() { { let arguments = 1; return () => arguments; } } \
         { let arguments = 2; var top = () => arguments; }",
        |analysis| {
            assert!(analysis
                .environment_plans
                .values()
                .all(|environment| !environment.eval_visible));
            let arrow_arguments_capture = |parent: &str| {
                let parent_id = if parent == SCRIPT_OWNER_ID {
                    SCRIPT_OWNER_ID.to_string()
                } else {
                    analysis
                        .function_plans
                        .values()
                        .find(|function| function.name == parent)
                        .expect("parent should be planned")
                        .id
                        .clone()
                };
                let arrow = analysis
                    .function_plans
                    .values()
                    .find(|function| {
                        function.parent_owner_id == parent_id
                            && function.protocol == FunctionProtocolIr::Arrow
                    })
                    .expect("arrow should be planned");
                let mut captures = arrow.captures.iter().filter(|(_, capture)| {
                    capture.source_name == "arguments"
                        || capture.source_name == LEXICAL_ARGUMENTS_NAME
                });
                let (storage, capture) = captures.next().expect("arrow captures arguments");
                assert!(captures.next().is_none());
                (storage.clone(), capture.source_name.clone())
            };
            let implicit = (
                LEXICAL_ARGUMENTS_NAME.to_string(),
                LEXICAL_ARGUMENTS_NAME.to_string(),
            );
            assert_eq!(arrow_arguments_capture("implicit"), implicit);
            assert_eq!(arrow_arguments_capture("parameters"), implicit);
            assert_eq!(
                arrow_arguments_capture("declared"),
                ("arguments".to_string(), "arguments".to_string())
            );
            for parent in ["blocked", SCRIPT_OWNER_ID] {
                let (storage, source) = arrow_arguments_capture(parent);
                assert_eq!(source, "arguments");
                assert!(storage != "arguments" && storage != LEXICAL_ARGUMENTS_NAME);
            }
        },
    );
}
