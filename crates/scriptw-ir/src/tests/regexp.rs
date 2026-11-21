#[test]
fn does_not_fold_static_regexp_literal_exec() {
    let program = lower_script("/]/.exec(' ]{}')[0]; /\\c0/.exec('\\x0f\\x10\\x11');");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(first) = &script.body.statements[0] else {
        panic!("expected expression statement");
    };
    let ExprIr::PropertyRead { target, .. } = &first.expr else {
        panic!(
            "expected static regexp match array read, got {:?}",
            first.expr
        );
    };
    assert_literal_regexp_call(target, "]", "", "exec", " ]{}");
    let StatementIr::Expression(second) = &script.body.statements[1] else {
        panic!("expected expression statement");
    };
    assert_literal_regexp_call(second, "\\c0", "", "exec", "\u{f}\u{10}\u{11}");
}

fn assert_literal_regexp_call(
    expression: &TypedExpr,
    pattern: &str,
    expected_flags: &str,
    method: &str,
    argument: &str,
) {
    let ExprIr::MaterializeBinding { value, .. } = &expression.expr else {
        panic!("literal receiver must be evaluated once: {expression:?}");
    };
    assert!(
        matches!(&value.expr, ExprIr::RegExpLiteral { source, flags, .. }
        if source == pattern && flags == expected_flags)
    );
    let (_, args) = retained_indexed_collection_call(expression, method);
    assert_eq!(args.len(), 1);
    assert!(matches!(&args[0].expr, ExprIr::String(value) if value == argument));
}

#[test]
fn lowers_regexp_literals_as_intrinsic_leaves() {
    let program = lower_script("/t[a-b|q-s]/g; /a|b/;");
    let script = program.script.as_ref().expect("script ir should exist");

    let StatementIr::Expression(supported) = &script.body.statements[0] else {
        panic!("expected supported regexp literal expression");
    };
    let ExprIr::RegExpLiteral {
        source,
        flags,
        static_compilation,
    } = &supported.expr
    else {
        panic!(
            "expected intrinsic regexp literal, got {:?}",
            supported.expr
        );
    };
    assert_eq!(source, "t[a-b|q-s]");
    assert_eq!(flags, "g");
    assert_eq!(
        static_compilation.as_ref(),
        Some(&StaticRegExpCompilation::Program(
            RegExpProgram::compile("t[a-b|q-s]", "g").expect("supported matcher")
        ))
    );
    assert!(!matches!(supported.expr, ExprIr::Construct { .. }));

    let StatementIr::Expression(supported_alternation) = &script.body.statements[1] else {
        panic!("expected supported regexp literal expression");
    };
    let ExprIr::RegExpLiteral {
        static_compilation, ..
    } = &supported_alternation.expr
    else {
        panic!(
            "expected intrinsic regexp literal, got {:?}",
            supported_alternation.expr
        );
    };
    assert!(static_compilation.is_some());
    assert!(!matches!(
        supported_alternation.expr,
        ExprIr::Construct { .. }
    ));
}

#[test]
fn regexp_literal_allocation_preserves_preceding_caller_flow_facts() {
    let program = lower_script("const holder = { value: 1 }; /a/; holder.value + 1;");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let Some(StatementIr::Expression(result)) =
        program.script.as_ref().unwrap().body.statements.last()
    else {
        panic!("the final property read remains an expression");
    };
    assert_eq!(result.kind, ValueKind::Number);
    assert_eq!(result.possible_kinds, KindSet::from_kind(ValueKind::Number));
}

#[test]
fn lowers_large_regexp_literals_with_exact_counted_bounds() {
    // A large legal count is data in the native repeat program. It must not
    // grow the instruction stream in proportion to the repetition count.
    let program = lower_script("/a{40000}/;");
    let script = program.script.as_ref().expect("script ir should exist");

    let StatementIr::Expression(literal) = &script.body.statements[0] else {
        panic!("expected regexp literal expression");
    };
    let ExprIr::RegExpLiteral {
        static_compilation, ..
    } = &literal.expr
    else {
        panic!("expected intrinsic regexp literal, got {:?}", literal.expr);
    };
    let Some(StaticRegExpCompilation::Program(program)) = static_compilation else {
        panic!("large finite counts retain a real native program");
    };
    assert_eq!(program.repeat_bounds.len(), 1);
    assert_eq!(program.repeat_bounds[0].minimum().digits(), b"40000");
    assert!(matches!(program.repeat_bounds[0].maximum(),
        crate::RegExpRepeatMaximum::Finite(maximum) if maximum.digits() == b"40000"));
    assert!(program.instructions.len() < 16);
}

#[test]
fn annotates_only_supported_constant_regexp_construction() {
    let program = lower_script(
        r#"new RegExp("(.|\r|\n)*", ""); new RegExp("World"); new RegExp(); let pattern = "a"; new RegExp(pattern, ""); let expression = /a/; new RegExp(expression); new RegExp("(?=a)", ""); new RegExp("[", ""); new Date("2020-01-01");"#,
    );
    let script = program.script.as_ref().expect("script ir should exist");

    let StatementIr::Expression(constructed) = &script.body.statements[0] else {
        panic!("expected constructed regexp");
    };
    let ExprIr::Construct {
        static_regexp_compilation: Some(StaticRegExpCompilation::Program(static_program)),
        ..
    } = &constructed.expr
    else {
        panic!("expected supported constant constructed regexp annotation");
    };
    assert_eq!(
        static_program,
        &RegExpProgram::compile("(.|\r|\n)*", "").expect("program should compile")
    );
    assert!(static_program
        .instructions
        .iter()
        .any(|instruction| instruction.opcode == REGEXP_OPCODE_DOT));
    assert_eq!(static_program.capture_count, 1);

    // The earlier invocation invalidates the mutable global constructor fact.
    // Later source spelling alone cannot supply static compilation authority.
    for index in [1, 2, 4, 6, 7, 8, 9] {
        let StatementIr::Expression(expr) = &script.body.statements[index] else {
            panic!("expected construct expression at {index}");
        };
        assert!(matches!(
            expr.expr,
            ExprIr::Construct {
                static_regexp_compilation: None,
                ..
            }
        ));
    }
    for pattern in ["World", "(?=a)", "["] {
        let isolated = lower_script(&format!("new RegExp({pattern:?});"));
        let StatementIr::Expression(TypedExpr {
            expr:
                ExprIr::Construct {
                    static_regexp_compilation: Some(compilation),
                    ..
                },
            ..
        }) = &isolated.script.as_ref().unwrap().body.statements[0]
        else {
            panic!("a proven constructor retains its constant pattern annotation: {pattern}");
        };
        match RegExpProgram::compile(pattern, "") {
            Ok(expected) => assert_eq!(compilation, &StaticRegExpCompilation::Program(expected)),
            Err(error) => {
                assert_eq!(error.kind, RegExpCompileErrorKind::InvalidSyntax);
                assert!(matches!(
                    compilation,
                    StaticRegExpCompilation::InvalidSyntax { .. }
                ));
            }
        }
    }
}

#[test]
fn annotates_only_direct_constant_regexp_calls() {
    let static_program_for = |source: &str| {
        let program = lower_script(source);
        let script = program.script.as_ref().expect("script ir should exist");
        let StatementIr::Expression(TypedExpr {
            expr:
                ExprIr::CallIndirect {
                    static_regexp_compilation,
                    ..
                },
            ..
        }) = script.body.statements.last().expect("expected RegExp call")
        else {
            panic!("expected indirect RegExp call");
        };
        static_regexp_compilation.clone()
    };

    assert_eq!(
        static_program_for(r#"RegExp("(?<name>a)");"#),
        Some(StaticRegExpCompilation::Program(
            RegExpProgram::compile("(?<name>a)", "").expect("program should compile")
        ))
    );
    assert_eq!(
        static_program_for(r#"RegExp("(?<π>a)", "u");"#),
        Some(StaticRegExpCompilation::Program(
            RegExpProgram::compile("(?<π>a)", "u").expect("program should compile")
        ))
    );
    assert!(
        static_program_for(r#"var pattern = "a"; RegExp(pattern);"#).is_none(),
        "dynamic patterns must not be annotated"
    );
    assert!(
        static_program_for(r#"var expression = /a/; RegExp(expression);"#).is_none(),
        "RegExp object identity calls must not be annotated"
    );
    assert!(
        static_program_for(r#"function RegExp() {} RegExp("a");"#).is_none(),
        "shadowed RegExp calls must not be annotated"
    );
    assert!(
        static_program_for(r#"RegExp = function () {}; RegExp("a");"#).is_none(),
        "reassigned RegExp calls must not be annotated"
    );
}

#[test]
fn annotates_constant_regexp_prototype_compile_calls() {
    let program =
        lower_script(r#"let subject = /original/; subject.compile("[\ud834\udf06]", "u");"#);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expression) = script
        .body
        .statements
        .last()
        .expect("expected compile call")
    else {
        panic!("expected compile expression");
    };
    let Some(TypedExpr {
        expr:
            ExprIr::CallIndirect {
                static_regexp_compilation: Some(StaticRegExpCompilation::Program(static_program)),
                ..
            },
        ..
    }) = indirect_call_body(expression)
    else {
        panic!(
            "expected annotated indirect RegExp.prototype.compile call, got {:#?}",
            script.body.statements.last()
        );
    };
    assert_eq!(
        static_program.instructions[0],
        RegExpInstruction::literal_code_point(0x1d306)
    );
}

#[test]
fn annotates_invalid_constant_regexp_prototype_compile_calls() {
    let program = lower_script(r#"let subject = /original/; subject.compile(".{2,1}");"#);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expression) = script
        .body
        .statements
        .last()
        .expect("expected compile call")
    else {
        panic!("expected compile expression");
    };
    let Some(TypedExpr {
        expr:
            ExprIr::CallIndirect {
                static_regexp_compilation: Some(StaticRegExpCompilation::InvalidSyntax { message }),
                ..
            },
        ..
    }) = indirect_call_body(expression)
    else {
        panic!("expected invalid static RegExp compilation annotation");
    };
    assert!(message.contains("regular-expression quantifier bounds are reversed"));
}

#[test]
fn lowers_numbered_capture_programs_with_alternation() {
    let program = lower_script(r"/(\d+)/g; /(a|b)/;");
    let script = program.script.as_ref().expect("script ir should exist");

    let StatementIr::Expression(supported) = &script.body.statements[0] else {
        panic!("expected supported regexp literal expression");
    };
    let ExprIr::RegExpLiteral {
        static_compilation: Some(StaticRegExpCompilation::Program(capture_program)),
        ..
    } = &supported.expr
    else {
        panic!("expected first-slice capture matcher program");
    };
    assert_eq!(capture_program.capture_count, 1);

    let StatementIr::Expression(supported_alternation) = &script.body.statements[1] else {
        panic!("expected supported regexp literal expression");
    };
    let ExprIr::RegExpLiteral {
        static_compilation, ..
    } = &supported_alternation.expr
    else {
        panic!("expected intrinsic regexp literal");
    };
    assert!(static_compilation.is_some());
}

#[test]
fn does_not_fold_stateful_regexp_literal_exec_or_test() {
    let program = lower_script("/b/y.exec('ab'); /a/g.test('a');");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");

    let StatementIr::Expression(exec) = &script.body.statements[0] else {
        panic!("expected exec expression statement");
    };
    assert_literal_regexp_call(exec, "b", "y", "exec", "ab");

    let StatementIr::Expression(test) = &script.body.statements[1] else {
        panic!("expected test expression statement");
    };
    assert_literal_regexp_call(test, "a", "g", "test", "a");
}

// The pattern parser recurses once per group and, with the `v` flag, once per
// nested class. Every pattern it is given is scanned first, so one nesting
// deeper than the literal limit is refused, as unsupported here and left to
// the runtime compiler, whatever its source: a literal (already a SyntaxError
// in the front end), a `RegExp` call's string argument or a string literal of
// the runtime candidate table.
#[test]
fn regexp_patterns_nesting_past_the_limit_are_refused_before_parsing() {
    let nested = |open: &str, close: &str, depth: usize| {
        format!("{}a{}", open.repeat(depth), close.repeat(depth))
    };
    for (pattern, flags) in [
        (nested("[", "]", 100_000), "v"),
        (nested("(", ")", 100_000), ""),
        (nested("(?:", ")", 100_000), "u"),
        (nested("[(", ")]", 60_000), "v"),
    ] {
        let error = RegExpProgram::compile(&pattern, flags).unwrap_err();
        assert_eq!(error.kind, RegExpCompileErrorKind::UnsupportedFeature, "{flags}");
        assert!(error.message.contains("nest more than"), "{}", error.message);
    }
    RegExpProgram::compile(&nested("[", "]", 200), "v").expect("nested classes within the limit");
    RegExpProgram::compile(&nested("(", ")", 200), "").expect("nested groups within the limit");
    // Without the `v` flag a `[` inside a class is an ordinary character.
    RegExpProgram::compile(&format!("[{}]", "[".repeat(100_000)), "u")
        .expect("a class of `[` characters");
}
