#[test]
fn inspection_visits_executable_parameter_default_expressions() {
    let program = lower_script("function f(value = this.x) { return value; }");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .find(|function| function.name == "f")
        .unwrap();
    let default = function.params[0].default_init.as_ref().unwrap();
    let ExprIr::SpecOperation {
        operation: SpecOperationIr::GetV,
        operands,
    } = &default.expr
    else {
        panic!("parameter default retains its spec GetV: {default:?}")
    };
    assert_eq!(operands.len(), 2);
    assert!(matches!(operands[0].expr, ExprIr::This));
    assert!(matches!(&operands[1].expr, ExprIr::String(key) if key == "x"));
    let summary = program.ir_summary();
    assert!(
        summary
            .split_whitespace()
            .any(|field| field == "default_params=1"),
        "{summary}"
    );
    assert!(
        summary
            .split_whitespace()
            .any(|field| field == "this_reads=1"),
        "{summary}"
    );
    assert!(
        summary
            .split_whitespace()
            .any(|field| field == "spec_operations=1"),
        "{summary}"
    );
}

#[test]
fn inspection_param_fixture_retains_source_parameters_and_arguments_cell_reference() {
    let program = lower_script(include_str!(
        "../../../scriptw-cli/tests/fixtures/wasm_params.js"
    ));
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().unwrap();
    // Exact helper contexts are call-site copies of these source functions.
    let source_functions = script
        .functions
        .iter()
        .filter(|function| !function.id.contains("$exact_helper_context$"))
        .collect::<Vec<_>>();
    assert_eq!(source_functions.len(), 4);
    assert_eq!(
        source_functions
            .iter()
            .flat_map(|function| &function.params)
            .filter(|parameter| parameter.default_init.is_some())
            .count(),
        2
    );
    assert_eq!(
        source_functions
            .iter()
            .flat_map(|function| &function.params)
            .filter(|parameter| parameter.is_rest)
            .count(),
        1
    );
    let outer = source_functions
        .iter()
        .find(|function| function.name == "outer")
        .unwrap();
    let arrow = script
        .functions
        .iter()
        .find(|function| function.protocol.flavor() == FunctionFlavor::Arrow)
        .unwrap();
    assert!(arrow.captures_lexical_arguments);
    let capture = arrow
        .captured_bindings
        .iter()
        .find(|binding| binding.source_name == LEXICAL_ARGUMENTS_NAME)
        .unwrap();
    assert!(outer
        .owned_env_bindings
        .iter()
        .any(|binding| binding.name == capture.source_name && binding.slot == capture.slot));
    let returned = function_return(arrow).unwrap();
    let ExprIr::PropertyRead { target, .. } = &returned.expr else {
        panic!("arguments index retains its property Get: {returned:?}")
    };
    assert!(
        matches!(&target.expr, ExprIr::Identifier(name) if name == &capture.name),
        "captured arguments reads the enclosing owner's cell: {target:?}"
    );
    let summary = program.ir_summary();
    for expected in [
        "arguments_uses=0",
        "lexical_arguments_captures=1",
        "environment_identifier_reads=0",
        "environment_identifier_calls=0",
    ] {
        assert!(
            summary.split_whitespace().any(|field| field == expected),
            "{expected}: {summary}"
        );
    }
}

#[test]
fn inspection_runtime_arguments_reads_do_not_claim_shadowed_arguments_objects() {
    for (source, has_arguments_parameter) in [
        ("function outer() { return () => arguments[0]; }", false),
        (
            "function outer(arguments) { return () => arguments[0]; }",
            true,
        ),
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let script = program.script.as_ref().unwrap();
        let outer = script
            .functions
            .iter()
            .find(|function| function.name == "outer")
            .unwrap();
        assert_eq!(
            outer
                .params
                .iter()
                .any(|parameter| parameter.name == "arguments"),
            has_arguments_parameter,
            "{source}"
        );
        let arrow = script
            .functions
            .iter()
            .find(|function| function.protocol.flavor() == FunctionFlavor::Arrow)
            .unwrap();
        let returned = function_return(arrow).unwrap();
        let ExprIr::PropertyRead { target, .. } = &returned.expr else {
            panic!("arguments index preserves its Get: {returned:?}")
        };
        let resolved = if has_arguments_parameter {
            "arguments"
        } else {
            LEXICAL_ARGUMENTS_NAME
        };
        assert!(
            matches!(&target.expr, ExprIr::Identifier(name) if name == resolved),
            "lookup must resolve the actual enclosing binding: {target:?}"
        );
        assert!(arrow
            .captured_bindings
            .iter()
            .any(|binding| binding.name == resolved));
        assert_eq!(target.kind, ValueKind::Dynamic, "{source}");
        let summary = program.ir_summary();
        assert!(
            summary
                .split_whitespace()
                .any(|field| field == "arguments_uses=0"),
            "{summary}"
        );
        assert!(
            summary
                .split_whitespace()
                .any(|field| field == "environment_identifier_reads=0"),
            "{summary}"
        );
    }
}

#[test]
fn inspection_builtin_counters_keep_resolved_intrinsics_distinct_from_runtime_references() {
    let resolved = lower_script("AggregateError([], 'x');");
    let script = resolved.script.as_ref().unwrap();
    assert_eq!(script.aggregate_errors, 1);
    let StatementIr::Expression(expression) = &script.body.statements[0] else {
        panic!("resolved call remains an expression")
    };
    let call = indirect_call_body(expression).unwrap();
    let ExprIr::CallIndirect { callee, .. } = &call.expr else {
        unreachable!()
    };
    assert_eq!(
        callee.function_targets.exact_single_target(),
        Some(&StandardBuiltinId::AggregateErrorConstructor.function_id())
    );

    // Direct eval keeps every Script identifier a runtime Reference.
    let runtime = lower_script(
        "function pick() { return eval('1'); } AggregateError([], 'x'); new AggregateError([], 'x');",
    );
    assert!(runtime.is_wasm_supported(), "{:?}", runtime.diagnostics);
    let script = runtime.script.as_ref().unwrap();
    assert_eq!(script.aggregate_errors, 0);
    let StatementIr::Expression(TypedExpr {
        expr: ExprIr::EnvironmentIdentifier(call),
        ..
    }) = &script.body.statements[0]
    else {
        panic!(
            "runtime call preserves ResolveBinding and GetValue: {:?}",
            script.body
        )
    };
    assert_eq!(call.name, "AggregateError");
    assert_eq!(
        call.resolution_start(),
        EnvironmentIdentifierResolutionStart::CurrentEnvironment
    );
    assert!(matches!(
        call.operation,
        EnvironmentIdentifierOperationIr::Call { .. }
    ));
    let StatementIr::Expression(TypedExpr {
        expr: ExprIr::Construct { callee, .. },
        ..
    }) = &script.body.statements[1]
    else {
        panic!("runtime constructor remains emitted: {:?}", script.body)
    };
    assert!(callee.function_targets.exact_single_target().is_none());
    let ExprIr::EnvironmentIdentifier(reference) = &callee.expr else {
        panic!("constructor retains its runtime callee: {callee:?}")
    };
    assert_eq!(reference.name, "AggregateError");
    assert!(matches!(
        reference.operation,
        EnvironmentIdentifierOperationIr::Read
    ));
    let summary = runtime.ir_summary();
    for expected in [
        "calls=3",
        "constructs=1",
        "environment_identifier_reads=1",
        "environment_identifier_calls=2",
        "aggregate_errors=0",
    ] {
        assert!(
            summary.split_whitespace().any(|field| field == expected),
            "{expected}: {summary}"
        );
    }

    let shadowed = lower_script("function AggregateError() {} AggregateError([], 'x');");
    assert_eq!(shadowed.script.as_ref().unwrap().aggregate_errors, 0);
}

#[test]
fn inspection_named_environment_reads_include_parameter_initializers() {
    // Direct eval makes the invocation records named; `arguments` alone does not.
    let program = lower_script("eval(''); function f(value = arguments) { return value; }");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .find(|function| function.name == "f")
        .unwrap();
    let default = function.params[0].default_init.as_ref().unwrap();
    let ExprIr::EnvironmentIdentifier(reference) = &default.expr else {
        panic!("initializer reads the invocation environment: {default:?}")
    };
    assert_eq!(reference.name, "arguments");
    assert!(matches!(
        reference.operation,
        EnvironmentIdentifierOperationIr::Read
    ));
    let summary = program.ir_summary();
    assert!(
        summary
            .split_whitespace()
            .any(|field| field == "environment_identifier_reads=2"),
        "{summary}"
    );
    assert!(
        summary
            .split_whitespace()
            .any(|field| field == "arguments_uses=0"),
        "{summary}"
    );
}

#[test]
fn inspection_parameter_traversal_preserves_scalar_exponentiation_dependency() {
    for (source, requires_number_pow) in [
        ("function f(value = this.x ** 2) { return value; }", true),
        ("function f(value = 2n ** 3n) { return value; }", false),
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        assert_eq!(
            program.script.as_ref().unwrap().has_scalar_exponentiation(),
            requires_number_pow,
            "{source}"
        );
    }
}
