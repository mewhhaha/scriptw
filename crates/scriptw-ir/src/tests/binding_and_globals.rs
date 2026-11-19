#[test]
fn carries_dynamic_function_apply_argument_list_to_runtime() {
    let program = lower_script(
        "function target() {} function call(args) { return target.apply(null, args); }",
    );
    assert!(program.is_wasm_supported());
}

#[test]
fn carries_dynamic_method_receiver_to_runtime() {
    let program =
        lower_script("function call(receiver, name, args) { return receiver[name](...args); }");
    assert!(program.is_wasm_supported());
}

#[test]
fn keeps_top_level_lexicals_out_of_script_global_bindings() {
    let program = lower_script("let x = 1; const y = 2; var z = 3; function f() {}");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    assert!(!script
        .global_bindings
        .iter()
        .any(|binding| binding.name == "x" || binding.name == "y"));
    for name in ["x", "y"] {
        assert!(script
            .owned_env_bindings
            .iter()
            .any(|binding| binding.name == name));
    }
    assert!(script.global_bindings.iter().any(|binding| {
        binding.name == "z" && binding.declarations == GlobalDeclarationSetIr::Var
    }));
    assert!(script.global_bindings.iter().any(|binding| {
        binding.name == "f" && binding.declarations == GlobalDeclarationSetIr::Function
    }));
}

#[test]
fn global_binding_plan_resolves_predefined_var_and_duplicate_function_collisions_once() {
    let program = lower_script(
        "var Infinity; var NaN; var undefined; var duplicate; function duplicate() { return 1; } function duplicate() { return 2; } function Array() { return 3; }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");

    assert_eq!(
        script
            .global_bindings
            .iter()
            .map(|binding| binding.name.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        script.global_bindings.len(),
        "a global binding plan cannot contain duplicate names"
    );
    for (name, initializer) in [
        ("Infinity", GlobalPropertyInitializerIr::Infinity),
        ("NaN", GlobalPropertyInitializerIr::NaN),
        ("undefined", GlobalPropertyInitializerIr::Undefined),
    ] {
        let binding = script.global_bindings.get(name).expect("predefined global");
        assert_eq!(binding.initializer, initializer);
        assert_eq!(binding.declarations, GlobalDeclarationSetIr::Var);
    }

    let duplicate = script
        .global_bindings
        .get("duplicate")
        .expect("duplicate function binding");
    assert_eq!(
        duplicate.declarations,
        GlobalDeclarationSetIr::FunctionAndVar
    );
    let GlobalPropertyInitializerIr::SourceFunction(function_id) = &duplicate.initializer else {
        panic!("duplicate function binding must carry an exact FunctionId");
    };
    assert_eq!(
        function_id,
        &script
            .functions
            .iter()
            .rev()
            .find(|function| function.name == "duplicate")
            .expect("last duplicate function")
            .id
    );

    assert!(matches!(
        script
            .global_bindings
            .get("Array")
            .expect("configurable builtin collision")
            .initializer,
        GlobalPropertyInitializerIr::SourceFunction(_)
    ));
}

#[test]
fn global_binding_plan_keeps_predefined_properties_beneath_global_lexicals() {
    let program = lower_script("let Infinity = 1; Infinity;");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let plan = &program
        .script
        .as_ref()
        .expect("script ir should exist")
        .global_bindings;
    assert!(plan.lexical_names().any(|name| name == "Infinity"));
    assert!(matches!(
        plan.get("Infinity").map(|binding| &binding.initializer),
        Some(GlobalPropertyInitializerIr::Infinity)
    ));
}

#[test]
fn annex_b_script_copy_carries_a_script_global_target() {
    let program = lower_script("if (true) { function selected() { return 1; } }");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let copy = script
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::If { then_branch, .. } => match then_branch.as_ref() {
                StatementIr::Block(block) => {
                    block
                        .statements
                        .iter()
                        .find_map(|statement| match statement {
                            StatementIr::AnnexBFunctionCopy { target, .. } => Some(target),
                            _ => None,
                        })
                }
                _ => None,
            },
            _ => None,
        })
        .expect("Annex B copy");
    assert_eq!(
        copy,
        &AnnexBFunctionCopyTargetIr::ScriptGlobal {
            name: "selected".to_string()
        }
    );
}

#[test]
fn lowers_assignment_to_const_as_runtime_type_error_after_rhs_evaluation() {
    let program = lower_script("const x = 1; x = 2;");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expression) = &script.body.statements[1] else {
        panic!("expected assignment expression");
    };
    let ExprIr::Comma { lhs, rhs } = &expression.expr else {
        panic!("expected evaluated RHS before immutable-binding throw");
    };
    assert!(matches!(lhs.expr, ExprIr::Number(_)));
    assert!(matches!(
        rhs.expr,
        ExprIr::RuntimeThrow {
            name: NativeErrorKind::TypeError,
            ..
        }
    ));
}

#[test]
fn every_identifier_put_value_consumer_throws_the_same_immutable_binding_error() {
    // The PutValue consumers that write through an *identifier* Reference —
    // plain assignment, compound arithmetic assignment, compound bitwise
    // assignment and update — all route through
    // `immutable_binding_write`, so the class and the message are asserted
    // once here, over every shape, rather than end to end in four places
    // that can drift. Three of the four used to be `unsupported_expr`
    // refusals of programs the spec says must run, which is why
    // `is_wasm_supported` is part of the assertion and not an aside.
    for source in [
        "const x = 1; x = 2;",
        "const x = 1; x += 2;",
        "const x = 1; x &= 2;",
        "const x = 1; x++;",
        "const x = 1; --x;",
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let script = program.script.as_ref().expect("script ir should exist");
        let StatementIr::Expression(expression) = &script.body.statements[1] else {
            panic!("{source}: expected an expression statement");
        };
        let ExprIr::Comma { lhs, rhs } = &expression.expr else {
            panic!(
                "{source}: expected the already-evaluated operand ahead of the throw, got {:?}",
                expression.expr
            );
        };
        assert!(
            !matches!(lhs.expr, ExprIr::RuntimeThrow { .. }),
            "{source}: the operand must still be evaluated, not replaced by the throw"
        );
        let ExprIr::RuntimeThrow { name, message } = &rhs.expr else {
            panic!("{source}: expected a runtime throw, got {:?}", rhs.expr);
        };
        assert_eq!(*name, NativeErrorKind::TypeError, "{source}");
        assert_eq!(*message, "assignment to immutable binding", "{source}");
    }
}

#[test]
fn lowers_const_update_operand_through_to_numeric_before_the_throw() {
    // 13.4.4.1 step 2 coerces the old value with ToNumeric *before* the
    // step-4 PutValue fails, so `const s = Symbol(); s++` must report
    // ToNumeric's TypeError and `const o = { valueOf() { … } }; o++` must
    // call `valueOf`. A bare identifier read on the `Comma`'s lhs would
    // satisfy every test262 case in this family and do neither.
    let program = lower_script("const x = 1; x++;");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expression) = &script.body.statements[1] else {
        panic!("expected an update expression statement");
    };
    let ExprIr::Comma { lhs, .. } = &expression.expr else {
        panic!("expected the coerced old value ahead of the throw");
    };
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = &lhs.expr
    else {
        panic!(
            "expected ToNumeric on the update operand, got {:?}",
            lhs.expr
        );
    };
    assert_eq!(*operation, SpecOperationIr::ToNumeric);
    assert_eq!(operands.len(), 1);
    assert!(
        matches!(&operands[0].expr, ExprIr::Identifier(_)),
        "{:?}",
        operands[0].expr
    );
}

#[test]
fn lowers_nullish_for_in_head_as_an_evaluated_head_with_zero_iterations() {
    // 14.7.5.6 step 3.a. `StatementIr::Empty` would pass every test262 case
    // in this family — none of the four has an effectful head — and would
    // silently drop the head expression, so the shape is pinned here.
    for source in [
        "for (var k in null) { k; }",
        "for (var k in undefined) { k; }",
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let script = program.script.as_ref().expect("script ir should exist");
        let head = script
            .body
            .statements
            .iter()
            .find_map(|statement| match statement {
                StatementIr::Expression(expression) => Some(expression),
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "{source}: the head must survive as an evaluated expression statement: {:?}",
                    script.body.statements
                )
            });
        assert!(
            matches!(head.expr, ExprIr::Comma { .. }),
            "{source}: {:?}",
            head.expr
        );
        assert_eq!(
            head.kind,
            ValueKind::Undefined,
            "{source}: a nullish for-in completes with undefined, not with the head's value"
        );
    }
}

#[test]
fn does_not_prove_a_for_in_key_is_a_string() {
    // `infer_var_binding_info_from_statement`'s `ForInLoop` arm used to
    // publish a proven `String` for any `var` named as a `for-in` key,
    // regardless of the head. A loop that runs zero times assigns nothing,
    // so the hoisted `var` is still `undefined`, and `k + 1` must lower to a
    // numeric addition (`NaN`) rather than to `ExprIr::StringConcat`, which
    // stringifies both operands and yields `"undefined1"`.
    //
    // Both heads below run zero times: `null` takes 14.7.5.6 step 3.a's
    // break completion, and `{}` has no enumerable own properties. The
    // second one is the case that was wrong before the nullish head was
    // supported at all, so it is not a regression test for that lane alone.
    for source in [
        "for (var k in null) { k; } k + 1;",
        "for (var k in ({})) { k; } k + 1;",
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let script = program.script.as_ref().expect("script ir should exist");
        let concat = script.body.statements.iter().find(|statement| {
            matches!(
                statement,
                StatementIr::Expression(expression)
                    if matches!(expression.expr, ExprIr::StringConcat { .. })
            )
        });
        assert!(
            concat.is_none(),
            "{source}: `k + 1` must not lower to a string concatenation: {:?}",
            script.body.statements
        );
    }
}

#[test]
fn preserves_sloppy_named_function_binding_assignment_through_a_capture() {
    let program = lower_script(
        "const outer = function named() {
                 return function write() { named = 2; };
             };",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let write = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "write")
        .expect("capturing function should be lowered");
    assert!(write.body.statements.iter().any(|statement| matches!(
        statement,
        StatementIr::Expression(TypedExpr {
            expr: ExprIr::Number(_),
            ..
        })
    )));
}

#[test]
fn lowers_string_compound_assignment() {
    let program = lower_script("let s = \"a\"; s += \"b\";");
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("compound_assigns=1"));
    assert_eq!(
        program
            .script
            .as_ref()
            .expect("script ir should exist")
            .result_kind(),
        ValueKind::String
    );
}

#[test]
fn lowers_lone_surrogate_string_literal_with_internal_marker() {
    let program = lower_script("\"\\uD800\";");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[0] else {
        panic!("expected expression statement");
    };
    let ExprIr::String(value) = &expr.expr else {
        panic!("expected string literal expression");
    };
    assert_eq!(value, &format!("{JS_STRING_SURROGATE_SENTINEL}D800"));
}

#[test]
fn lowers_label_on_expression_statement() {
    let program = lower_script("label: 1;");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    assert!(matches!(
        script.body.statements.first(),
        Some(StatementIr::Labelled { .. })
    ));
}

#[test]
fn lowers_string_or_number_binding_addition_as_coercive_add() {
    let program = lower_script("var x; if (true) { x = 1; } else { x = \"a\"; } x + 1;");
    assert!(program.is_wasm_supported());
    assert!(program.ir_summary().contains("heap_coercions=1"));
}

#[test]
fn lowers_dynamic_plus_proven_string_as_coercive_add() {
    let program = lower_script(
        "function choose(flag) { if (flag) return 1; return {}; } function format(message) { return message + \" suffix\"; } format(choose(true));",
    );
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("heap_coercions=2"));
}

#[test]
fn lowers_maybe_string_plus_as_coercive_add() {
    let program = lower_script(
        "function choose(flag) { if (flag) return \"name\"; return 1; } choose(true) + 1;",
    );
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("heap_coercions=1"));
}

#[test]
fn lowers_nested_maybe_string_plus_as_coercive_add() {
    let program = lower_script(
        "function choose(flag) { if (flag) return \"name\"; return 1; } choose(true) + 1 + \" suffix\";",
    );
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("heap_coercions=1"));
    assert!(summary.contains("string_concats=1"));
}

#[test]
fn function_var_shadows_same_named_script_global_var() {
    let program = lower_script(
        "function helper() { var index = 0; index = index + 1; return index; } var index; for (var index in []) {} helper();",
    );
    assert!(program.is_wasm_supported());
}

#[test]
fn merges_nested_function_arguments_into_script_global_value_info() {
    let program = lower_script(
        "var args = null; var close = function() { args = arguments; }; close(); args.length;",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
}

#[test]
fn lowers_arguments_reads_with_computed_string_keys() {
    let program = lower_script(
        "function readArgument(propertyKey) { return arguments[propertyKey]; } readArgument(\"0\");",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
}

#[test]
fn lowers_arguments_symbol_iterator_reads() {
    let program = lower_script(
        "function readArgument(propertyKey) { return arguments[propertyKey]; } readArgument(\"0\"); readArgument(Symbol.iterator) === Array.prototype.values;",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
}

#[test]
fn lowers_arguments_object_to_string_reads() {
    let program = lower_script(
        "function describeArguments() { return arguments.toString(); } describeArguments();",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
}

#[test]
fn root_update_replaces_an_earlier_nested_script_global_value() {
    let program = lower_script(
        "var args = null; var close = function() { args = arguments; }; close(); args = 1; args;",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expression) = script.body.statements.last().unwrap() else {
        panic!("expected final expression");
    };
    let ExprIr::GlobalIdentifierRead { name } = &expression.expr else {
        panic!("the final read must observe the live updated global binding");
    };
    assert_eq!(name, "args");
    assert_eq!(expression.possible_kinds, KindSet::from_kind(ValueKind::Number));
    let StatementIr::Expression(update) = &script.body.statements[script.body.statements.len() - 2]
    else {
        panic!("the original root update must precede the final read");
    };
    let ExprIr::GlobalPropertyWrite { name, value, .. } = &update.expr else {
        panic!("the root update must remain a single actual assignment");
    };
    assert_eq!(name, "args");
    assert_eq!(value.kind, ValueKind::Number);
    assert_eq!(value.possible_kinds, KindSet::from_kind(ValueKind::Number));
    assert!(matches!(value.expr, ExprIr::Number(bits) if bits == 1f64.to_bits()));
}

#[test]
fn nested_arrow_writes_script_global_before_property_read() {
    let program =
        lower_script("var args = null; var close = () => { args = []; }; close(); args.length;");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
}

#[test]
fn nested_script_global_write_is_known_before_later_root_function_lowering() {
    let program = lower_script(
        "var args = null; function reader() { args.length; } function writer() { args = arguments; } reader(); writer(); args.length;",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
}

#[test]
fn script_global_compound_assignments_remain_runtime_read_modify_writes_at_every_owner() {
    let program = lower_script(
        "var trace = \"\"; function append() { trace += \"nested;\"; } append(); trace += \"root;\"; trace;",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR should exist");
    let append = script
        .functions
        .iter()
        .find(|function| function.name == "append")
        .expect("append function should be lowered");
    for (body, suffix) in [(&append.body, "nested;"), (&script.body, "root;")] {
        let compounds = body
            .statements
            .iter()
            .filter_map(|statement| match statement {
                StatementIr::Expression(TypedExpr {
                    expr: ExprIr::EnvironmentIdentifier(reference),
                    ..
                }) => Some(reference),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            compounds.len(),
            1,
            "each owner must retain its compound write"
        );
        let reference = compounds[0];
        assert_eq!(reference.name, "trace");
        assert_eq!(
            reference.resolution_start(),
            EnvironmentIdentifierResolutionStart::GlobalEnvironment
        );
        assert_eq!(reference.strictness, Strictness::Sloppy);
        let EnvironmentIdentifierOperationIr::EagerCompound { operation, rhs } =
            &reference.operation
        else {
            panic!("global Get/RHS/Put must share one Reference: {reference:?}");
        };
        assert_eq!(*operation, EnvironmentCompoundOperationIr::Add);
        assert!(matches!(&rhs.expr, ExprIr::String(value) if value == suffix));
    }
}

#[test]
fn nested_writer_in_later_root_function_is_known_to_earlier_reader() {
    let program = lower_script(
        "var args = null; function reader() { args.length; } function container() { var writer = function() { args = arguments; }; writer(); } reader(); container(); args.length;",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
}

#[test]
fn read_only_nested_script_global_does_not_widen_later_root_update() {
    let program = lower_script(
        "var value = null; function reader() { value; } reader(); value = 1; value + 1;",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expression) = script.body.statements.last().unwrap() else {
        panic!("expected final expression");
    };
    assert_eq!(
        expression.possible_kinds,
        KindSet::from_kind(ValueKind::Number)
    );
}

#[test]
fn lowers_unbound_identifier_read_as_runtime_global_resolution() {
    let program = lower_script("try { missingName; } catch (e) {}");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::TryCatch { try_block, .. } = &script.body.statements[0] else {
        panic!("expected try/catch statement");
    };
    let StatementIr::Expression(expr) = &try_block.statements[0] else {
        panic!("expected try expression statement");
    };
    assert!(matches!(
        expr.expr,
        ExprIr::GlobalIdentifierRead { ref name } if name == "missingName"
    ));
}

#[test]
fn lowers_catch_parameter_source_alias_for_redeclared_var() {
    let program =
        lower_script("foo = \"prior\"; try { throw 1; } catch (foo) { var foo = \"init\"; }");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let (catch_name, catch_source_name, catch_block) = script
        .body
        .statements
        .iter()
        .find_map(|statement| {
            if let StatementIr::TryCatch {
                catch_name,
                catch_source_name,
                catch_block,
                ..
            } = statement
            {
                Some((catch_name, catch_source_name, catch_block))
            } else {
                None
            }
        })
        .expect("expected try/catch statement");

    assert_eq!(catch_source_name, "foo");
    assert_ne!(catch_name, catch_source_name);
    let StatementIr::Var(declarators) = &catch_block.statements[0] else {
        panic!("expected var declaration in catch block");
    };
    assert_eq!(declarators[0].name, "foo");
}

#[test]
fn lowers_static_class_field_as_static_with_initializer_function() {
    let program = lower_script("class C { static x = 1; } C.x;");
    let script = program.script.as_ref().expect("script ir should exist");
    assert_eq!(script.functions.len(), 2);
    let class_expr = match &script.body.statements[0] {
        StatementIr::Lexical { init, .. } => &init.expr,
        other => panic!("expected class lexical statement, got {other:?}"),
    };
    let ExprIr::ClassDefinition(class) = class_expr else {
        panic!("expected class definition");
    };
    let ClassStaticElementIr::Field(field) = &class.element_plan.static_elements[0] else {
        panic!("expected static field");
    };
    assert!(matches!(&field.key, ClassFieldKeyIr::Public(key) if key == "x"));
    assert!(field.init_function_id.is_some());
}

#[test]
fn number_wrapper_to_string_retains_construction_and_method_acquisition() {
    let program = lower_script("throw (new Number()).toString();");
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Throw(value) = &script.body.statements[0] else {
        panic!("expected throw statement");
    };
    let ExprIr::MaterializeBinding {
        value: receiver,
        body,
        ..
    } = &value.expr
    else {
        panic!("expected the Number wrapper to be evaluated once: {value:?}");
    };
    assert!(matches!(receiver.expr, ExprIr::Construct { .. }));
    assert!(matches!(body.expr, ExprIr::CallIndirect { .. }));
}

#[test]
fn public_global_spellings_preserve_parameter_and_capture_references() {
    let assert_nan_read_from_binding = |value: &TypedExpr, binding: &str| {
        let target = match &value.expr {
            ExprIr::PropertyRead {
                target,
                key: PropertyKeyIr::StaticString(key),
            } if key == "NaN" => target.as_ref(),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::GetV,
                operands,
            } if operands.len() == 2
                && matches!(&operands[1].expr, ExprIr::String(key) if key == "NaN") =>
            {
                &operands[0]
            }
            _ => panic!("expected the actual NaN property Get: {value:?}"),
        };
        assert!(
            matches!(&target.expr, ExprIr::Identifier(name) if name == binding),
            "property receiver must read owned binding {binding}: {target:?}"
        );
    };
    for directive in ["", "'use strict';"] {
        let program = lower_script(&format!(
            "{directive}
             function shadow(globalThis, Infinity, NaN, undefined, print) {{
                 return [globalThis.NaN, Infinity, NaN, undefined, print];
             }}
             function capture(globalThis) {{
                 return function capturedReader() {{ return globalThis.NaN; }};
             }}
             function readPublicGlobal() {{ return globalThis; }}
             function readPublicType() {{ return typeof globalThis; }}
             shadow({{NaN: 'local'}}, 1, 2, 3, 4);
             capture({{NaN: 'captured'}})();
             readPublicGlobal();"
        ));
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR");
        let shadow = script
            .functions
            .iter()
            .find(|function| function.name == "shadow")
            .expect("parameter owner");
        let values = shadow
            .body
            .statements
            .iter()
            .find_map(|statement| match statement {
                StatementIr::Return(TypedExpr {
                    expr: ExprIr::ArrayLiteral(values),
                    ..
                }) => Some(values),
                _ => None,
            })
            .expect("returned parameter values");
        assert_eq!(shadow.params[0].name, "globalThis");
        assert_nan_read_from_binding(&values[0], &shadow.params[0].name);
        for (value, expected) in values[1..]
            .iter()
            .zip(["Infinity", "NaN", "undefined", "print"])
        {
            assert!(
                matches!(&value.expr, ExprIr::Identifier(name) if name == expected),
                "{value:?}"
            );
        }
        let captured = script
            .functions
            .iter()
            .find(|function| function.name == "capturedReader")
            .expect("capture reader");
        let capture_owner = script
            .functions
            .iter()
            .find(|function| function.name == "capture")
            .expect("captured parameter owner");
        let parameter_cell = capture_owner
            .owned_env_bindings
            .iter()
            .find(|binding| binding.name == capture_owner.params[0].name)
            .expect("captured parameter has its own environment cell");
        let captured_binding = captured
            .captured_bindings
            .iter()
            .find(|binding| binding.source_name == "globalThis")
            .expect("source globalThis resolves to the enclosing parameter");
        assert_eq!(captured_binding.name, parameter_cell.name);
        assert_eq!(captured_binding.slot, parameter_cell.slot);
        let captured_read = captured
            .body
            .statements
            .iter()
            .find_map(|statement| match statement {
                StatementIr::Return(value) => Some(value),
                _ => None,
            })
            .expect("captured property result");
        assert_nan_read_from_binding(captured_read, &captured_binding.name);
        let public = script
            .functions
            .iter()
            .find(|function| function.name == "readPublicGlobal")
            .expect("source global reader");
        assert!(
            public.body.statements.iter().any(|statement| {
                matches!(statement, StatementIr::Return(TypedExpr {
                expr: ExprIr::GlobalIdentifierRead { name }, kind: ValueKind::Dynamic, ..
            }) if name == "globalThis")
            }),
            "a dormant source function must read the live public binding"
        );
        let public_type = script
            .functions
            .iter()
            .find(|function| function.name == "readPublicType")
            .expect("source global typeof owner");
        assert!(
            public_type.body.statements.iter().any(|statement| {
                matches!(statement, StatementIr::Return(TypedExpr {
                expr: ExprIr::TypeOfUnresolvedIdentifier { name }, ..
            }) if name == "globalThis")
            }),
            "typeof in a dormant function must tolerate a later deletion"
        );
    }
}

#[test]
fn global_reference_objects_do_not_alias_a_same_named_source_parameter() {
    let program = lower_script(
        "var counter = 1; function update(globalThis) { counter += 2; return globalThis.NaN; } update({NaN: 'parameter'});",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let update = program
        .script
        .as_ref()
        .expect("script IR")
        .functions
        .iter()
        .find(|function| function.name == "update")
        .expect("update owner");
    let reference = update
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Expression(TypedExpr {
                expr: ExprIr::EnvironmentIdentifier(reference),
                ..
            }) => Some(reference),
            _ => None,
        })
        .expect("global compound Reference resolution");
    assert_eq!(reference.name, "counter");
    assert_eq!(
        reference.resolution_start(),
        EnvironmentIdentifierResolutionStart::GlobalEnvironment
    );
    assert!(matches!(&reference.operation,
        EnvironmentIdentifierOperationIr::EagerCompound { operation: EnvironmentCompoundOperationIr::Add, rhs }
        if matches!(&rhs.expr, ExprIr::Number(value) if *value == 2.0f64.to_bits())));
    assert_eq!(update.params[0].name, "globalThis");
    let property_read = update
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Return(value) => Some(value),
            _ => None,
        })
        .expect("source parameter property read");
    let target = match &property_read.expr {
        ExprIr::PropertyRead {
            target,
            key: PropertyKeyIr::StaticString(key),
        } if key == "NaN" => target.as_ref(),
        ExprIr::SpecOperation {
            operation: SpecOperationIr::GetV,
            operands,
        } if operands.len() == 2
            && matches!(&operands[1].expr, ExprIr::String(key) if key == "NaN") =>
        {
            &operands[0]
        }
        _ => panic!("expected the source NaN property Get: {property_read:?}"),
    };
    assert!(
        matches!(&target.expr, ExprIr::Identifier(name) if name == &update.params[0].name),
        "the property receiver must remain the parameter beside Global Reference resolution: {target:?}"
    );
}

#[test]
fn global_this_loses_initial_property_authority_after_unknown_effects() {
    let program = lower_script(
        "globalThis; unknownHook(); globalThis; this.globalThis; typeof globalThis; delete globalThis;",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let statements = &program.script.as_ref().expect("script IR").body.statements;
    let expression = |index: usize| {
        let StatementIr::Expression(expression) = &statements[index] else {
            panic!("expected source expression at {index}");
        };
        expression
    };
    assert!(
        matches!(&expression(0).expr, ExprIr::ExecutionGlobalObject),
        "the initial own data-property fact proves the root identity"
    );
    assert!(
        matches!(&expression(2).expr, ExprIr::GlobalIdentifierRead { name } if name == "globalThis")
    );
    assert_eq!(expression(2).possible_kinds, KindSet::all_runtime_tags());
    assert_eq!(
        expression(3).possible_kinds,
        KindSet::all_runtime_tags(),
        "the real global object's shape must not invent an own globalThis data property"
    );
    assert!(
        matches!(&expression(4).expr, ExprIr::TypeOfUnresolvedIdentifier { name }
        if name == "globalThis"),
        "typeof must tolerate deletion and retain accessor evaluation"
    );
    assert!(
        matches!(&expression(5).expr, ExprIr::DeleteGlobalProperty { name, .. } if name == "globalThis"),
        "DeleteBinding must consult the configurable descriptor"
    );
}

#[test]
fn dormant_declared_global_this_typeof_retains_global_resolution() {
    let program = lower_script(
        "var globalThis; function readType() { return typeof globalThis; } readType();",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let read = program
        .script
        .as_ref()
        .expect("script IR")
        .functions
        .iter()
        .find(|function| function.name == "readType")
        .expect("global typeof reader");
    assert!(read.body.statements.iter().any(|statement| {
        matches!(statement, StatementIr::Return(TypedExpr {
            expr: ExprIr::TypeOfUnresolvedIdentifier { name }, ..
        }) if name == "globalThis")
    }));
}

#[test]
fn property_delete_preserves_its_receiver_beside_a_same_named_global_lexical() {
    for directive in ["", "'use strict';"] {
        let program = lower_script(&format!(
            "{directive} let Math; delete globalThis.Math; Math;"
        ));
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let statements = &program.script.as_ref().expect("script IR").body.statements;
        assert!(
            statements.iter().any(|statement| {
                matches!(statement, StatementIr::Expression(TypedExpr {
                expr: ExprIr::DeleteProperty { target, key, .. }, ..
            }) if matches!(&target.expr, ExprIr::ExecutionGlobalObject)
                && matches!(key, PropertyKeyIr::StaticString(name) if name == "Math"))
            }),
            "property deletion must not resolve the lexical Math binding"
        );
    }
    let deleted = lower_script("delete globalThis.globalThis; globalThis;");
    assert!(deleted.is_wasm_supported(), "{:?}", deleted.diagnostics);
    assert!(
        matches!(deleted.script.as_ref().expect("script IR").body.statements.last(),
        Some(StatementIr::Expression(TypedExpr {
            expr: ExprIr::GlobalIdentifierRead { name }, ..
        })) if name == "globalThis"),
        "property deletion must discard initial globalThis authority"
    );
}

#[test]
fn replaced_global_this_retains_open_dynamic_source_candidates() {
    let program = lower_script("globalThis = Function; globalThis; globalThis('return 1');");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR");
    let StatementIr::Expression(read) = &script.body.statements[1] else {
        panic!("replaced public binding must remain an evaluated read");
    };
    assert!(matches!(&read.expr, ExprIr::GlobalIdentifierRead { name } if name == "globalThis"));
    assert_eq!(read.possible_kinds, KindSet::all_runtime_tags());
    assert!(read.heap_shape.is_none());
    assert!(
        matches!(&read.function_targets, FunctionTargetKnowledge::Open(targets)
        if targets.contains(&StandardBuiltinId::FunctionConstructor.function_id()))
    );
    assert!(script.prepared_dynamic_functions.iter().any(|prepared| {
        prepared.arguments == ["return 1"]
            && matches!(
                prepared.outcome,
                PreparedDynamicFunctionOutcome::Compiled { .. }
            )
    }));

    let dynamic = lower_script("globalThis = Function; globalThis(unknownSource);");
    assert!(dynamic.is_wasm_supported(), "{:?}", dynamic.diagnostics);
    let script = dynamic.script.as_ref().expect("script IR");
    let Some(StatementIr::Expression(TypedExpr {
        expr:
            ExprIr::CallIndirect {
                callee,
                args,
                direct_eval: None,
                this_arg: None,
                ..
            },
        ..
    })) = script.body.statements.last()
    else {
        panic!("the public alias must retain its actual callee and argument evaluation");
    };
    assert!(matches!(&callee.expr, ExprIr::GlobalIdentifierRead { name } if name == "globalThis"));
    assert!(
        matches!(&callee.function_targets, FunctionTargetKnowledge::Open(targets)
        if targets.contains(&StandardBuiltinId::FunctionConstructor.function_id()))
    );
    assert_eq!(args.len(), 1);
    assert!(
        matches!(&args[0].expr, ExprIr::GlobalIdentifierRead { name } if name == "unknownSource")
    );
    // Function arguments retain runtime evaluation and ToString. An actual
    // intrinsic call with an unmatched tuple reaches its typed runtime source
    // boundary; a missing argument binding can first throw ReferenceError.
    assert!(script.prepared_dynamic_functions.is_empty());
}

#[test]
fn declared_global_plain_assignments_keep_runtime_references_at_each_owner() {
    for (prelude, expected_strictness) in [
        ("", Strictness::Sloppy),
        ("'use strict';", Strictness::Strict),
    ] {
        let program = lower_script(&format!(
            "{prelude} var parseInt; parseInt = (delete globalThis.parseInt, 17); function assignGlobal() {{ parseInt = (delete globalThis.parseInt, 19); }}"
        ));
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR");
        assert!(script
            .global_bindings
            .main_frame_write_bindings()
            .any(|binding| binding.name == "parseInt"));
        let nested = script
            .functions
            .iter()
            .find(|function| function.name == "assignGlobal")
            .expect("nested global writer");
        for (body, expected_rhs) in [(&script.body, 17.0_f64), (&nested.body, 19.0_f64)] {
            let write = body
                .statements
                .iter()
                .find_map(|statement| match statement {
                    StatementIr::Expression(value)
                        if matches!(
                            &value.expr,
                            ExprIr::AssignIdentifier { name, .. }
                                | ExprIr::GlobalPropertyWrite { name, .. }
                                if name == "parseInt"
                        ) =>
                    {
                        Some(value)
                    }
                    _ => None,
                })
                .expect("source assignment");
            let ExprIr::GlobalPropertyWrite {
                name,
                value,
                strictness,
                ..
            } = &write.expr
            else {
                panic!("declared global source write must retain a Global Reference: {write:?}");
            };
            assert_eq!(name, "parseInt");
            assert_eq!(*strictness, expected_strictness);
            let ExprIr::Comma { lhs, rhs } = &value.expr else {
                panic!("the original deletion must precede the assigned value");
            };
            assert!(matches!(
                &lhs.expr,
                ExprIr::DeleteProperty { .. } | ExprIr::DeleteGlobalProperty { .. }
            ));
            assert!(matches!(&rhs.expr, ExprIr::Number(value) if *value == expected_rhs.to_bits()));
        }
    }
}

#[test]
fn with_global_plain_assignment_orders_selection_and_rhs_with_source_strictness() {
    for (directive, strictness) in [
        ("", Strictness::Sloppy),
        ("'use strict';", Strictness::Strict),
    ] {
        let program = lower_script(&format!(
            "var declared; var writer; with ({{}}) writer = function writer() {{ {directive} missing = 11; declared = 13; }};"
        ));
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR");
        let writer = script
            .functions
            .iter()
            .find(|function| function.name == "writer")
            .expect("with-captured writer");
        let writes: Vec<_> = writer
            .body
            .statements
            .iter()
            .filter_map(|statement| match statement {
                StatementIr::Expression(TypedExpr {
                    expr: ExprIr::EnvironmentIdentifier(identifier),
                    ..
                }) => Some(identifier),
                _ => None,
            })
            .collect();
        assert_eq!(writes.len(), 2);
        for (write, (name, number)) in writes
            .into_iter()
            .zip([("missing", 11.0_f64), ("declared", 13.0_f64)])
        {
            assert_eq!(write.name, name);
            assert_eq!(write.strictness, strictness);
            assert_eq!(
                write.resolution_start(),
                EnvironmentIdentifierResolutionStart::GlobalEnvironment
            );
            let EnvironmentIdentifierOperationIr::AssignWithGlobalFallback { selection, value } =
                &write.operation
            else {
                panic!("ordered with/global resolution must own the complete RHS");
            };
            assert!(
                matches!(&selection.expr, ExprIr::Conditional { condition, .. }
                if matches!(&condition.expr, ExprIr::SpecOperation { operation: SpecOperationIr::WithEnvironmentHasBinding, .. }))
            );
            assert!(matches!(&value.expr, ExprIr::Number(value) if *value == number.to_bits()));
            let operands: Vec<_> = write.operation.operands().collect();
            assert_eq!(operands, [selection.as_ref(), value.as_ref()]);
            assert_eq!(
                carried_put_value_failure(&ExprIr::EnvironmentIdentifier(write.clone())),
                Some((strictness, PutValueFailure::TypeErrorOrReferenceError))
            );
        }
    }
}

#[test]
fn global_var_metadata_does_not_reclassify_same_spelled_owned_bindings() {
    let program = lower_script(
        "var parseInt; function parameter(parseInt) { parseInt = 1; } function local() { var parseInt = 0; parseInt = 2; } function lexical() { let parseInt = 0; parseInt = 3; } function make() { let parseInt = 0; function captured() { parseInt = 3; } return captured; }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR");
    for name in ["parameter", "local", "lexical", "captured"] {
        let function = script
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("shadowing function");
        let writes = function
            .body
            .statements
            .iter()
            .filter_map(|statement| match statement {
                StatementIr::Expression(value) => Some(value),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(writes.len(), 1, "{name} retains its source assignment");
        assert!(
            matches!(&writes[0].expr, ExprIr::AssignIdentifier { .. }),
            "{name} must write its owned binding: {:?}",
            writes[0]
        );
    }
}

#[test]
fn declared_global_sibling_updates_retain_one_reference_across_get_and_put() {
    for source in [
        "var parseInt = 1; parseInt += 2;",
        "var parseInt = 1; parseInt |= 2;",
        "var parseInt = 1; parseInt &&= 2;",
        "var parseInt = 1; parseInt++;",
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let script = program.script.as_ref().expect("script IR");
        let StatementIr::Expression(update) = script.body.statements.last().expect("source update")
        else {
            panic!("{source}: expected source expression");
        };
        let ExprIr::EnvironmentIdentifier(reference) = &update.expr else {
            panic!("{source}: GetValue and PutValue must retain one Global Reference: {update:?}");
        };
        assert_eq!(reference.name, "parseInt");
        assert_eq!(reference.strictness, Strictness::Sloppy);
        assert_eq!(
            reference.resolution_start(),
            EnvironmentIdentifierResolutionStart::GlobalEnvironment
        );
        match &reference.operation {
            EnvironmentIdentifierOperationIr::EagerCompound { operation, rhs } => {
                let expected = if source.contains("+=") {
                    EnvironmentCompoundOperationIr::Add
                } else {
                    EnvironmentCompoundOperationIr::Bitwise(BitwiseBinaryOp::Or)
                };
                assert_eq!(*operation, expected);
                assert!(matches!(&rhs.expr, ExprIr::Number(value) if *value == 2.0_f64.to_bits()));
            }
            EnvironmentIdentifierOperationIr::LogicalCompound { operation, rhs } => {
                assert!(source.contains("&&="));
                assert_eq!(*operation, LogicalBinaryOp::And);
                assert!(matches!(&rhs.expr, ExprIr::Number(value) if *value == 2.0_f64.to_bits()));
            }
            EnvironmentIdentifierOperationIr::Update {
                operation,
                return_mode,
            } => {
                assert!(source.contains("++"));
                assert_eq!(*operation, NumericUpdateOp::Increment);
                assert_eq!(*return_mode, UpdateReturnMode::Postfix);
            }
            _ => panic!("{source}: expected the original read-modify-write operation"),
        }
    }
}

#[test]
fn logical_global_start_preserves_local_and_captured_binding_ownership() {
    let program = lower_script(
        "var parseInt; parseInt ||= 1; function global() { parseInt ||= 2; } function parameter(parseInt) { parseInt ||= 3; } function local() { let parseInt = 0; parseInt ||= 4; } function make() { let parseInt = 0; function captured() { parseInt ||= 5; } return captured; }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR");
    for name in ["global", "parameter", "local", "captured"] {
        let function = script
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap();
        let expression = function
            .body
            .statements
            .iter()
            .find_map(|statement| match statement {
                StatementIr::Expression(value) => Some(value),
                _ => None,
            })
            .expect("source logical assignment");
        if name == "global" {
            assert!(
                matches!(&expression.expr, ExprIr::EnvironmentIdentifier(reference)
                if reference.resolution_start() == EnvironmentIdentifierResolutionStart::GlobalEnvironment
                    && matches!(reference.operation, EnvironmentIdentifierOperationIr::LogicalCompound { .. }))
            );
        } else {
            assert!(
                matches!(&expression.expr, ExprIr::LogicalShortCircuit { lhs, rhs, .. }
                if matches!(&lhs.expr, ExprIr::Identifier(_))
                    && matches!(&rhs.expr, ExprIr::AssignIdentifier { .. })),
                "{name} must keep its actual declarative binding: {expression:?}"
            );
        }
    }
}

#[test]
fn global_eager_get_effects_precede_rhs_facts_and_retain_function_candidates() {
    let program =
        lower_script("var target = Function; var probe = 0; probe += target('return 1');");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR");
    let StatementIr::Expression(TypedExpr {
        expr: ExprIr::EnvironmentIdentifier(reference),
        ..
    }) = script.body.statements.last().expect("compound expression")
    else {
        panic!("global compound must retain its Reference");
    };
    let EnvironmentIdentifierOperationIr::EagerCompound { rhs, .. } = &reference.operation else {
        panic!("eager operation must retain the source RHS");
    };
    let ExprIr::CallIndirect { callee, .. } = &rhs.expr else {
        panic!("possible Get hooks require actual runtime callee acquisition: {rhs:?}");
    };
    assert!(callee.function_targets.exact_targets().is_none());
    assert!(callee
        .function_targets
        .known_targets()
        .contains(&StandardBuiltinId::FunctionConstructor.function_id()));
    assert!(matches!(&callee.expr, ExprIr::GlobalIdentifierRead { name } if name == "target"));
}

#[test]
fn eager_arithmetic_tdz_reads_reject_before_rhs_evaluation() {
    for operator in ["+=", "-=", "*=", "/=", "%=", "**="] {
        for declaration in ["let value;", "const value = 1;"] {
            let source = format!("function fail() {{ value {operator} rhs(); {declaration} }}");
            let program = lower_script(&source);
            assert!(
                program.is_wasm_supported(),
                "{source}: {:?}",
                program.diagnostics
            );
            let function = program
                .script
                .as_ref()
                .expect("script IR")
                .functions
                .iter()
                .find(|function| function.name == "fail")
                .expect("declarative owner");
            assert!(
                matches!(
                    function.body.statements.first(),
                    Some(StatementIr::Expression(TypedExpr {
                        expr: ExprIr::RuntimeThrow {
                            name: NativeErrorKind::ReferenceError,
                            ..
                        },
                        ..
                    }))
                ),
                "GetValue must reject before emitting a RHS call: {source}: {:?}",
                function.body
            );
        }
    }
}

#[test]
fn typeof_of_an_unbound_host_global_installs_that_host_builtin() {
    let program = lower_script("print(1); typeof gc;");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    assert!(
        script.host_builtins.contains(&HostBuiltinId::Gc),
        "typeof must resolve the host global in the realm: {:?}",
        script.host_builtins
    );
}

#[test]
fn with_plain_write_does_not_prove_a_global_fallback_exists_after_scope_exit() {
    for source in [
        "var o = {foo:1}; with(o) { foo = 42; } foo;",
        "var o = Object.create({foo:1}); with(o) { foo = 42; } foo;",
        "var o = {foo:1, [Symbol.unscopables]:{foo:true}}; with(o) { foo = 42; } foo;",
        "try { with({set foo(value) {throw 1;}}) { foo = 42; } } catch(error) {} foo;",
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let script = program.script.as_ref().unwrap();
        let StatementIr::Expression(read) = script.body.statements.last().unwrap() else {
            panic!("source identifier read")
        };
        assert!(
            matches!(&read.expr, ExprIr::GlobalIdentifierRead { name } if name == "foo"),
            "{source}: {read:?}"
        );
        assert_eq!(read.kind, ValueKind::Dynamic);
    }
    // A definite ordinary write retains the existing presence/value proof.
    let program = lower_script("outside = 7; outside;");
    let StatementIr::Expression(read) = program
        .script
        .as_ref()
        .unwrap()
        .body
        .statements
        .last()
        .unwrap()
    else {
        panic!("ordinary source read")
    };
    assert!(matches!(&read.expr, ExprIr::GlobalPropertyRead {name} if name == "outside"));
    assert_eq!(read.kind, ValueKind::Number);
}

#[test]
fn with_plain_write_joins_the_skipped_mutable_declarative_fallback() {
    let program = lower_script("function local(object) { var fallback = 'kept'; with(object) { fallback = 42; } return fallback; }");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .find(|function| function.name == "local")
        .unwrap();
    let read = function
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Return(read) => Some(read),
            _ => None,
        })
        .unwrap();
    assert!(matches!(&read.expr, ExprIr::Identifier(_)), "{read:?}");
    assert!(
        read.possible_kinds.contains(ValueKind::String),
        "the selected Object ER leaves the old String: {read:?}"
    );
    assert!(
        read.possible_kinds.contains(ValueKind::Number),
        "the unselected Object ER writes the Number: {read:?}"
    );
}

#[test]
fn an_arguments_reference_keeps_unrelated_captured_reads_static() {
    // The implicit binding is one owner's storage. A capture elsewhere in the
    // Script still reads its cell, not a named runtime environment lookup.
    let program = lower_script(
        "var make = function () { 'use strict'; var n = 2; \
           function read() { return n; } \
           function count() { return arguments.length; } \
           return read; }; make()();",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    assert!(script.eval_environment.is_none());
    for function in &script.functions {
        assert!(function.eval_environment.is_none(), "{}", function.name);
    }
    let read = script
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("read should be lowered");
    let body = format!("{:?}", read.body);
    assert!(!body.contains("EnvironmentIdentifier"), "{body}");
}
