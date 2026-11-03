#[test]
fn no_source_eval_preserves_runtime_reference_identity_and_arguments() {
    for (source, expected_arg_count) in [
        ("eval();", 0),
        ("eval(1);", 1),
        ("eval(true);", 1),
        ("eval(null);", 1),
        ("eval({ marker: 1 });", 1),
        ("eval(new String('source'));", 1),
        ("eval(function marker() {});", 1),
        ("eval(1, 2);", 2),
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(expression) = &script.body.statements[0] else {
            panic!("{source}: eval should remain an expression statement");
        };
        let ExprIr::EnvironmentIdentifier(identifier) = &expression.expr else {
            panic!("{source}: eval must retain its runtime Reference");
        };
        assert_eq!(identifier.name, "eval");
        let EnvironmentIdentifierOperationIr::Call {
            args,
            direct_eval: Some(context),
            ..
        } = &identifier.operation
        else {
            panic!("{source}: bare eval needs a guarded direct context");
        };
        assert_eq!(
            context.invocation(),
            scriptw_front::EvalInvocationContext::Script
        );
        assert_eq!(args.len(), expected_arg_count, "{source}: retained args");
    }
}

#[test]
fn later_erased_eval_does_not_widen_an_earlier_callsite_return() {
    let source = r#"
let object = { marker: 1 };
let order = "";
function mark(value, label) { order += label; return value; }
eval(mark(object, "a"), mark(0, "b"));
eval(7);
"#;
    let program = lower_script(source);
    assert!(
        !program.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.unsupported_feature(),
            Some(UnsupportedFeature::DynamicSource(_))
        )),
        "{source}: {:?}",
        program.diagnostics
    );
}

#[test]
fn unknown_effects_keep_known_nested_script_global_function_candidates() {
    let source = r#"
var candidate;
function install(hook) {
    candidate = eval;
    hook();
}
function invoke() {
    candidate("source");
}
install(globalThis.unknownHook);
invoke();
"#;
    let program = lower_script(source);
    assert_prepared_script(&program, PreparedScriptKind::IndirectEval);
}

#[test]
fn exact_mixed_and_open_eval_candidates_keep_dynamic_source_authority() {
    for source in [
        "let candidate = unknown ? eval : undefined; candidate('source');",
        "let holder = { candidate: unknown ? eval : undefined }; holder.candidate('source');",
        "var candidate = eval; globalThis.unknownHook(); candidate('source');",
    ] {
        let program = lower_script(source);
        assert_prepared_script(&program, PreparedScriptKind::IndirectEval);
    }
}

#[test]
fn replaced_eval_keeps_guarded_direct_and_indirect_source_candidates() {
    let source = "function mark() {} eval = mark; globalThis.unknownHook(); eval('source');";
    let program = lower_script(source);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.unwrap();
    assert!(script
        .prepared_scripts
        .iter()
        .any(|source| matches!(source.kind, PreparedScriptKind::DirectEval(_))));
    assert!(script
        .prepared_scripts
        .iter()
        .any(|source| source.kind == PreparedScriptKind::IndirectEval));
}

#[test]
fn spread_eval_candidate_keeps_the_prepared_source_and_runtime_selection() {
    let source = "let candidate = unknown ? eval : undefined; candidate(...['source']);";
    let program = lower_script(source);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.expect("spread call remains executable IR");
    assert!(
        script.prepared_scripts.iter().any(|prepared| {
            prepared.kind == PreparedScriptKind::IndirectEval && prepared.source == "source"
        }),
        "{source}: {:?}",
        script.prepared_scripts
    );
}

#[test]
fn exact_mixed_call_result_excludes_the_throwing_non_function_branch() {
    let source = "function returnsNumber() { return 1; } let candidate = unknown ? returnsNumber : undefined; candidate();";
    let program = lower_script(source);
    assert!(
        program.is_wasm_supported(),
        "{source}: {:?}",
        program.diagnostics
    );
    let script = program.script.as_ref().expect("script IR should exist");
    let call = script
        .body
        .statements
        .iter()
        .rev()
        .find_map(|statement| match statement {
            StatementIr::Expression(expression) => indirect_call_body(expression),
            _ => None,
        })
        .expect("mixed target call should remain in IR");
    assert_eq!(call.possible_kinds, KindSet::from_kind(ValueKind::Number));
}

#[test]
fn pure_exact_source_candidates_preserve_a_later_receiver_shape() {
    let source = "class A extends Array { join() { return 'custom join'; } } const left = function () { return 1; }; const right = function () { return 2; }; const candidate = unknown ? left : right; const a = new A(); candidate(); a.join();";
    let program = lower_script(source);
    assert!(
        program.is_wasm_supported(),
        "{source}: {:?}",
        program.diagnostics
    );
    let script = program.script.as_ref().expect("script IR should exist");
    let StatementIr::Expression(call) = script.body.statements.last().unwrap() else {
        panic!("expected the call after the pure candidates");
    };

    assert_eq!(call.kind, ValueKind::String, "{call:?}");
}

#[test]
fn mutable_global_candidates_invalidate_a_later_receiver_shape() {
    let source = "class A extends Array { join() { return 'custom join'; } } const a = new A(); function left() { return 1; } function right() { return 2; } const candidate = unknown ? left : right; candidate(); a.join();";
    let program = lower_script(source);
    assert!(
        program.is_wasm_supported(),
        "{source}: {:?}",
        program.diagnostics
    );
    let script = program.script.as_ref().expect("script IR should exist");
    let StatementIr::Expression(call) = script.body.statements.last().unwrap() else {
        panic!("expected the call after the mutable global candidates");
    };

    assert_eq!(call.kind, ValueKind::Dynamic, "{call:?}");
}

#[test]
fn a_mutating_exact_source_candidate_invalidates_a_later_receiver_shape() {
    let source = "class A extends Array { join() { return 'custom join'; } } let a = new A(); function keep() { return 0; } function replace() { a = { join() { return 1; } }; return 0; } const candidate = unknown ? keep : replace; candidate(); a.join();";
    let program = lower_script(source);
    assert!(
        program.is_wasm_supported(),
        "{source}: {:?}",
        program.diagnostics
    );
    let script = program.script.as_ref().expect("script IR should exist");
    let StatementIr::Expression(call) = script.body.statements.last().unwrap() else {
        panic!("expected the call after the mixed candidates");
    };

    assert_eq!(call.kind, ValueKind::Dynamic, "{call:?}");
}

#[test]
fn a_later_argument_invalidates_an_earlier_descriptor_subject_shape() {
    assert_caller_flow_invalidation_reaches_final_addition(
            "let object = { value: 1 }; const descriptor = Object.getOwnPropertyDescriptor(object, 'value', (object.value = 's')); descriptor.value + 1;",
        );
}

#[test]
fn a_later_argument_invalidates_a_captured_method_receiver_shape() {
    assert_caller_flow_invalidation_reaches_final_addition(
            "let object = { value: 1, read() { return this.value; } }; object.read(object.value = 's') + 1;",
        );
}

#[test]
fn a_later_argument_invalidates_the_default_this_shape() {
    let source = "var value = 1; function read() { return this.value; } read(value = 's') + 1;";
    let program = lower_script(source);
    assert!(
        program.is_wasm_supported(),
        "{source}: {:?}",
        program.diagnostics
    );
    let script = program.script.as_ref().expect("script IR should exist");
    let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
        panic!("expected the final addition: {source}");
    };
    let (lhs, rhs) = match &result.expr {
        ExprIr::CoerciveAdd { lhs, rhs } | ExprIr::StringConcat { lhs, rhs } => (lhs, rhs),
        _ => panic!("default-this Get must not reuse the earlier numeric global: {result:?}"),
    };
    assert!(matches!(rhs.expr, ExprIr::Number(bits) if bits == 1.0_f64.to_bits()));
    let ExprIr::CallIndirect { this_arg, args, .. } = &lhs.expr else {
        panic!("the call must own the argument's assignment: {lhs:?}");
    };
    assert!(this_arg.is_none());
    assert!(matches!(args.as_slice(), [TypedExpr {
        expr: ExprIr::GlobalPropertyWrite { name, value, implicit: false, .. }, ..
    }] if name == "value" && matches!(&value.expr, ExprIr::String(value) if value == "s")));
}

#[test]
fn a_later_argument_invalidates_a_constructor_prototype_shape() {
    assert_caller_flow_invalidation_reaches_final_addition(
            "function Constructor() {} Constructor.prototype = { value: 1 }; const instance = new Constructor(Constructor.prototype = {}); instance.value + 1;",
        );
}

#[test]
fn function_apply_array_like_access_invalidates_caller_flow() {
    assert_caller_flow_invalidation_reaches_final_addition(
            "let holder = { value: 1 }; function inspect() {} const argumentsList = new Proxy([], { get(target, key, receiver) { holder = {}; return Reflect.get(target, key, receiver); } }); inspect.apply(null, argumentsList); holder.value + 1;",
        );
}

#[test]
fn mutating_source_forwarded_with_call_invalidates_later_element_analysis() {
    assert_caller_flow_invalidation_reaches_final_addition(
            "let values = [1]; function replace() { values = {}; } replace.call(undefined); values[0] + 1;",
        );
}

#[test]
fn mutating_source_forwarded_with_apply_invalidates_later_element_analysis() {
    assert_caller_flow_invalidation_reaches_final_addition(
            "let values = [1]; function replace() { values = {}; } replace.apply(undefined, []); values[0] + 1;",
        );
}

#[test]
fn mixed_source_targets_forwarded_with_call_invalidate_later_property_analysis() {
    assert_caller_flow_invalidation_reaches_final_addition(
            "const holder = { value: 1 }; function inspect() { return 0; } function remove() { delete holder.value; } const candidate = unknown ? inspect : remove; candidate.call(undefined); holder.value + 1;",
        );
}

#[test]
fn mixed_source_targets_forwarded_with_apply_invalidate_later_property_analysis() {
    assert_caller_flow_invalidation_reaches_final_addition(
            "const holder = { value: 1 }; function inspect() { return 0; } function remove() { delete holder.value; } const candidate = unknown ? inspect : remove; candidate.apply(undefined, []); holder.value + 1;",
        );
}

#[test]
fn erased_target_forwarded_with_call_invalidates_later_element_analysis() {
    assert_caller_flow_invalidation_reaches_final_addition(
        "const values = [1]; unknown.call(undefined); values[0] + 1;",
    );
}

#[test]
fn mixed_array_fill_call_candidate_invalidates_later_property_analysis() {
    assert_caller_flow_invalidation_reaches_final_addition(
            "const candidate = unknown ? Array.prototype.at : Array.prototype.fill; (function readAfterCall() { const holder = { value: 1 }; candidate('x'); return holder.value; })() + 1;",
        );
}

#[test]
fn unguarded_call_property_does_not_inherit_the_source_function_purity() {
    let source = "const holder = { value: 1 }; (function inspect() { return 1; }).call(undefined); holder.value + 1;";
    let program = lower_script(source);
    assert!(
        program.is_wasm_supported(),
        "{source}: {:?}",
        program.diagnostics
    );
    let script = program.script.as_ref().expect("script IR should exist");
    let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
        panic!("expected the call after the pure forwarded source");
    };

    assert!(
        matches!(result.expr, ExprIr::CoerciveAdd { .. }),
        "{result:?}"
    );
}

#[test]
fn captured_array_fill_fast_path_invalidates_later_element_analysis() {
    assert_caller_flow_invalidation_reaches_final_addition(
        "let values = [1]; function mutate() { values.fill('x'); } mutate(); values[0] + 1;",
    );
}

#[test]
fn captured_array_pop_fast_path_invalidates_later_element_analysis() {
    assert_caller_flow_invalidation_reaches_final_addition(
        "let values = [1]; function mutate() { values.pop(); } mutate(); values[0] + 1;",
    );
}
