#[test]
fn closed_call_requires_stable_binding_and_the_same_capture_cell() {
    for (consumer, change, expected) in [
        ("function use() { return build(2); }", "", true),
        ("function use(tag) { return build(2); }", "", false),
        (
            "function use() { return build(2); }",
            "[build] = [replacement];",
            false,
        ),
    ] {
        let program = lower_script(&format!(
            "'use strict';
            function outer(tag, replacement) {{
                function build(value) {{ return {{tag, value}}; }}
                {consumer}
                {change}
                return use(1);
            }}
            outer('original', () => null);"
        ));
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR");
        let function = script
            .functions
            .iter()
            .find(|function| function.name == "use")
            .unwrap();
        let Some(StatementIr::Return(TypedExpr {
            expr: ExprIr::CallIndirect { callee, .. },
            ..
        })) = function.body.statements.last()
        else {
            panic!("expected the original indirect call and its fallback");
        };
        assert_eq!(callee.function_targets.proves_closed_local_call(), expected);
    }
}

#[test]
fn bounded_inline_body_distinguishes_pure_and_observable_work() {
    let program = lower_script(
        "'use strict';
        function outer() {
            const pack = value => { const ok = value !== void 0; return {value, ok}; };
            function read({id}) { return id; }
            function escape(value) { return arguments[0]; }
            return [pack(1), read({id: 2}), escape(3)];
        }
        outer();",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR");
    let plan = |name| {
        let function = script
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap();
        SmallInlineBodyIr::for_function(function).map(|body| body.effects())
    };
    assert_eq!(plan("pack"), Some(SmallInlineEffects::Closed));
    assert_eq!(plan("read"), Some(SmallInlineEffects::StrictActivation));
    assert_eq!(plan("escape"), None);
}

#[test]
fn sloppy_mapped_parameter_function_declaration_is_not_a_closed_local_call() {
    let program = lower_script(
        "function make(tag, f, previous) {
            function f(x) { 'use strict'; return tag; }
            if (previous === undefined) return f;
            arguments[1] = previous;
            return f(0);
        }
        make('old', null, undefined);",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script IR");
    let make = script
        .functions
        .iter()
        .find(|function| function.name == "make")
        .unwrap();
    let Some(StatementIr::Return(TypedExpr {
        expr: ExprIr::CallIndirect { callee, .. },
        ..
    })) = make.body.statements.last()
    else {
        panic!("expected return call in make");
    };
    assert!(
        !callee.function_targets.proves_closed_local_call(),
        "mapped arguments[1] can replace parameter-aliased function declaration f"
    );
}
