#[test]
fn lowers_switch_labels_and_debugger_ir() {
    let program = lower_script(
            "outer: while (true) { switch (2) { case 1: break; case 2: debugger; break outer; default: break; } }",
        );
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("switches=1"));
    assert!(summary.contains("labels=1"));
    assert!(summary.contains("debuggers=1"));
}

#[test]
fn lowers_hoisted_var_ir() {
    let program = lower_script("x; var x = 1; if (true) { var y = 2; } y;");
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("vars=2"));
    let script = program.script.as_ref().expect("script ir should exist");
    assert!(matches!(script.body.statements[1], StatementIr::Var(_)));
}

#[test]
fn lowers_top_level_functions_and_calls() {
    let program = lower_script("add(1, 2); function add(x, y) { return x + y; }");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    assert_eq!(script.functions.len(), 2);
    assert_eq!(script.functions[0].params.len(), 2);
    let summary = program.ir_summary();
    assert!(summary.contains("functions=2"));
    assert!(summary.contains("calls=1"));
    assert!(summary.contains("returns=2"));
}

#[test]
fn lowers_array_binding_patterns_in_function_parameters() {
    let program = lower_script("function dstr(a, [b]) { return b; } dstr(1, [2]);");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let function = script
        .functions
        .iter()
        .find(|function| function.name == "dstr")
        .expect("dstr should be lowered");
    assert_eq!(function.params[1].name, "$destructured.param.1");
    let StatementIr::ParameterInitialization {
        parameter_index: 1,
        statements,
    } = &function.body.statements[0]
    else {
        panic!("expected parameter initialization marker");
    };
    let StatementIr::DeclarationEvaluation(TypedExpr {
        expr:
            ExprIr::ArrayDestructure {
                pattern,
                evaluation: ArrayDestructuringEvaluationIr::BindingInitialization,
                ..
            },
        ..
    }) = &statements[0]
    else {
        panic!("expected parameter array destructuring inside the initialization marker");
    };
    assert!(matches!(
        pattern.elements.as_slice(),
        [ArrayDestructuringElementIr::Target {
            target: DestructuringTargetIr::Binding { name, .. },
            ..
        }] if name == "b"
    ));
}

#[test]
fn formals_carry_every_bound_name_and_patterns_write_those_names() {
    // FunctionDeclarationInstantiation takes `parameterNames` from the
    // formals, and a body `var id` writes `id`. The enclosing code declares
    // `id` and `rest` too, which must not move the pattern's names to
    // renamed lexical slots.
    let program = lower_script(
        "var id, rest; function dstr(a, {id, n: [b, ...rest], ...others}, ...[c]) { var id = a; return id; } dstr(1, {n: []});",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let function = script
        .functions
        .iter()
        .find(|function| function.name == "dstr")
        .expect("dstr should be lowered");
    let bound_names = function
        .params
        .iter()
        .map(|parameter| parameter.bound_names().join(","))
        .collect::<Vec<_>>();
    assert_eq!(bound_names, ["a", "id,b,rest,others", "c"]);
    assert_eq!(function.params[0].name, "a");
    assert_eq!(
        function.params[1].name,
        FunctionParamIr::pattern_storage_name(1)
    );
    assert!(function.params[1].binds("rest"));
    assert!(!function.params[1].binds("n"));
    assert!(!function.params[1].binds(&function.params[1].name));
    let StatementIr::ParameterInitialization {
        parameter_index: 1,
        statements,
    } = &function.body.statements[0]
    else {
        panic!("expected parameter initialization marker");
    };
    let StatementIr::DeclarationEvaluation(TypedExpr {
        expr: ExprIr::ObjectDestructure { pattern, .. },
        ..
    }) = &statements[0]
    else {
        panic!("expected parameter object destructuring inside the initialization marker");
    };
    assert!(matches!(
        &pattern.properties[0].target,
        DestructuringTargetIr::Binding { name, .. } if name == "id"
    ));
    let DestructuringTargetIr::NestedArray(nested) = &pattern.properties[1].target else {
        panic!("expected the nested array pattern");
    };
    assert!(matches!(
        nested.elements.as_slice(),
        [
            ArrayDestructuringElementIr::Target {
                target: DestructuringTargetIr::Binding { name: first, .. },
                ..
            },
            ArrayDestructuringElementIr::Rest {
                target: DestructuringTargetIr::Binding { name: rest, .. },
                ..
            },
        ] if first == "b" && rest == "rest"
    ));
    assert!(matches!(
        &pattern.rest,
        Some(DestructuringTargetIr::Binding { name, .. }) if name == "others"
    ));
}

#[test]
fn lowers_object_rest_binding_in_generator_parameters() {
    let program =
        lower_script("var f = function* ({ a, ...rest } = { a: 1, b: 2 }) {}; f().next();");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.protocol.execution_kind() == FunctionExecutionKind::Generator)
        .expect("generator expression should be lowered");
    assert!(function.params[0].default_init.is_some());
    let StatementIr::ParameterInitialization { statements, .. } = &function.body.statements[0]
    else {
        panic!("expected parameter initialization marker");
    };
    let StatementIr::DeclarationEvaluation(TypedExpr {
        expr: ExprIr::ObjectDestructure { pattern, .. },
        ..
    }) = &statements[0]
    else {
        panic!("expected object destructuring parameter initialization");
    };
    assert_eq!(pattern.properties.len(), 1);
    assert!(matches!(
        pattern.rest,
        Some(DestructuringTargetIr::Binding { ref name, .. }) if name == "rest"
    ));
}

#[test]
fn lowers_computed_object_keys_in_generator_parameters() {
    let program = lower_script(
        "var key = 'value'; var f = function* ({ [key]: value = 9 }) {}; f({}).next();",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.protocol.execution_kind() == FunctionExecutionKind::Generator)
        .expect("generator expression should be lowered");
    let StatementIr::ParameterInitialization { statements, .. } = &function.body.statements[0]
    else {
        panic!("expected parameter initialization marker");
    };
    let StatementIr::DeclarationEvaluation(TypedExpr {
        expr: ExprIr::ObjectDestructure { pattern, .. },
        ..
    }) = &statements[0]
    else {
        panic!("expected object destructuring parameter initialization");
    };
    assert!(matches!(
        pattern.properties.as_slice(),
        [ObjectDestructuringPropertyIr {
            key: DestructuringPropertyKeyIr::Computed(_),
            default: Some(_),
            ..
        }]
    ));
}

#[test]
fn lowers_nested_object_and_array_generator_parameters() {
    let program = lower_script(
        "var f = function* ([{ x }], { values: [y] }) {}; f([{ x: 1 }], { values: [2] }).next();",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.protocol.execution_kind() == FunctionExecutionKind::Generator)
        .expect("generator expression should be lowered");
    let StatementIr::ParameterInitialization {
        statements: first, ..
    } = &function.body.statements[0]
    else {
        panic!("expected first parameter initialization marker");
    };
    let StatementIr::DeclarationEvaluation(TypedExpr {
        expr: ExprIr::ArrayDestructure { pattern: first, .. },
        ..
    }) = &first[0]
    else {
        panic!("expected nested object in array destructuring");
    };
    assert!(matches!(
        first.elements.as_slice(),
        [ArrayDestructuringElementIr::Target {
            target: DestructuringTargetIr::NestedObject(_),
            ..
        }]
    ));

    let StatementIr::ParameterInitialization {
        statements: second, ..
    } = &function.body.statements[1]
    else {
        panic!("expected second parameter initialization marker");
    };
    let StatementIr::DeclarationEvaluation(TypedExpr {
        expr: ExprIr::ObjectDestructure {
            pattern: second, ..
        },
        ..
    }) = &second[0]
    else {
        panic!("expected nested array in object destructuring");
    };
    assert!(matches!(
        second.properties.as_slice(),
        [ObjectDestructuringPropertyIr {
            target: DestructuringTargetIr::NestedArray(_),
            ..
        }]
    ));
}

#[test]
fn marks_function_body_use_strict_directive() {
    let program = lower_script(r#"function f() { "use strict"; return this; } f();"#);
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    let function = script
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("function should be lowered");
    assert!(function.strict);
}

#[test]
fn marks_script_use_strict_directive() {
    let program = lower_script(r#""use strict"; function f() { return this; } f();"#);
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    assert!(script.strict);
    let function = script
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("function should be lowered");
    assert!(function.strict);
}

#[test]
fn lowers_block_function_declarations_as_hoisted_bindings() {
    let program = lower_script(
        "let value = 0; if (true) { value = nested(); function nested() { return 7; } } value;",
    );
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("functions=2"));
    assert!(summary.contains("calls=1"));
}

#[test]
fn lowers_root_function_constructor_reference_inside_function_body() {
    let program = lower_script(
            "function Box(message) { this.message = message; } function make(message) { return new Box(message); } make(\"ok\");",
        );
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("constructs=1"));
}

#[test]
fn exposes_gc_as_noop_host_builtin() {
    let program = lower_script("if (typeof gc === \"function\") { gc(); }");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    assert!(script.host_builtins.contains(&HostBuiltinId::Gc));
}

#[test]
fn lowering_requires_explicit_test262_host_surface_authority() {
    let source = "gc; parseInt; __scriptwCreateRealm;";
    let product = lower_script(source);
    let product_hosts = &product
        .script
        .expect("script ir should exist")
        .host_builtins;
    assert!(product_hosts.contains(&HostBuiltinId::Gc));
    assert!(product_hosts.contains(&HostBuiltinId::ParseInt));
    assert!(!product_hosts.contains(&HostBuiltinId::CreateRealm));

    let test262 = lower_test262_script(source);
    let test262_hosts = &test262
        .script
        .expect("script ir should exist")
        .host_builtins;
    assert!(test262_hosts.contains(&HostBuiltinId::Gc));
    assert!(test262_hosts.contains(&HostBuiltinId::ParseInt));
    assert!(test262_hosts.contains(&HostBuiltinId::CreateRealm));
}

#[test]
fn prunes_unresolved_typeof_function_guard() {
    let program =
        lower_script("if (typeof __missingHostHook === \"function\") { __missingHostHook(); }");
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("calls=0"));
}

#[test]
fn prunes_unresolved_typeof_and_guard_rhs() {
    let program =
        lower_script("if (typeof Symbol !== \"undefined\" && Symbol.iterator) { missingCall(); }");
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("calls=0"));
}
