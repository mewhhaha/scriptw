#[test]
fn transferred_string_ranges_keep_the_original_callee_and_all_arguments() {
    for (method, builtin) in [
        ("substring", StandardBuiltinId::StringPrototypeSubstring),
        ("slice", StandardBuiltinId::StringPrototypeSlice),
    ] {
        for spread in [false, true] {
            let arguments = if spread {
                "1, ...Object.keys({ x: 1 }), 'ignored', 42"
            } else {
                "1, 3, 'ignored', 42"
            };
            let source = format!(
                    "function run() {{ const receiver = {{ saved: String.prototype.{method}, \
                     toString() {{ return 'abcd'; }} }}; return (0, receiver).saved({arguments}); }} run();"
                );
            let program = lower_script(&source);
            assert!(
                program.is_wasm_supported(),
                "{source}: {:?}",
                program.diagnostics
            );
            let run = program
                .script
                .as_ref()
                .expect("script")
                .functions
                .iter()
                .find(|function| function.name == "run")
                .expect("run");
            let result = function_return(run).expect("range result");
            let (callee, args) = retained_indexed_collection_call(result, "saved");
            assert_eq!(callee.function_targets.exact_single_target(), Some(&builtin.function_id()));
            assert_eq!(result.kind, ValueKind::String);
            assert!(result.heap_shape.is_none());
            let [start, end, ignored, trailing] = args else {
                panic!("{args:?}")
            };
            assert!(matches!(start.expr, ExprIr::Number(value) if f64::from_bits(value) == 1.0));
            if spread {
                assert!(matches!(&end.expr, ExprIr::SpreadArgument(value)
                        if value.protocol == SpreadArgumentProtocol::ARGUMENT_LIST));
            } else {
                assert!(matches!(end.expr, ExprIr::Number(value) if f64::from_bits(value) == 3.0));
            }
            assert!(matches!(&ignored.expr, ExprIr::String(value) if value == "ignored"));
            assert!(
                matches!(trailing.expr, ExprIr::Number(value) if f64::from_bits(value) == 42.0)
            );
        }
    }
}

#[test]
fn transferred_string_range_coercions_invalidate_captured_flow_facts() {
    for method in ["substring", "slice"] {
        for coercion in [
                "const receiver = { saved: String.prototype.METHOD, \
                 toString() { captured.value = 'changed'; return 'abcd'; } }; receiver.saved(1, 2);",
                "const index = { valueOf() { captured.value = 'changed'; return 1; } }; \
                 const receiver = { saved: String.prototype.METHOD, toString() { return 'abcd'; } }; \
                 receiver.saved(index, 2);",
            ] {
                let source = format!(
                    "let captured = {{ value: 1 }}; {} captured.value + 1;",
                    coercion.replace("METHOD", method)
                );
                assert_caller_flow_invalidation_reaches_final_addition(&source);
            }
    }
}

#[test]
fn inferred_string_aliases_retain_each_acquired_reference() {
    // Lower independent literal calls so each native target reaches the
    // repaired owner without a runtime method table erasing its identity.
    let cases = [
        (
            StandardBuiltinId::StringPrototypeSubstr,
            "substr",
            "1, 2",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeCharAt,
            "charAt",
            "1",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeCharCodeAt,
            "charCodeAt",
            "1",
            ValueKind::Number,
        ),
        (
            StandardBuiltinId::StringPrototypeCodePointAt,
            "codePointAt",
            "1",
            ValueKind::Dynamic,
        ),
        (
            StandardBuiltinId::StringPrototypeAt,
            "at",
            "-1",
            ValueKind::Dynamic,
        ),
        (
            StandardBuiltinId::StringPrototypePadStart,
            "padStart",
            "6, 'x'",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypePadEnd,
            "padEnd",
            "6, 'x'",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeRepeat,
            "repeat",
            "2",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeNormalize,
            "normalize",
            "'NFC'",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeLocaleCompare,
            "localeCompare",
            "'b'",
            ValueKind::Number,
        ),
        (
            StandardBuiltinId::StringPrototypeToLocaleLowerCase,
            "toLocaleLowerCase",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeToLocaleUpperCase,
            "toLocaleUpperCase",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeToLowerCase,
            "toLowerCase",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeToUpperCase,
            "toUpperCase",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeIsWellFormed,
            "isWellFormed",
            "",
            ValueKind::Boolean,
        ),
        (
            StandardBuiltinId::StringPrototypeToWellFormed,
            "toWellFormed",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeAnchor,
            "anchor",
            "'name'",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeBig,
            "big",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeBlink,
            "blink",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeBold,
            "bold",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeFixed,
            "fixed",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeFontcolor,
            "fontcolor",
            "'red'",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeFontsize,
            "fontsize",
            "'3'",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeItalics,
            "italics",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeLink,
            "link",
            "'url'",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeSmall,
            "small",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeStrike,
            "strike",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeSub,
            "sub",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeSup,
            "sup",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeTrim,
            "trim",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeTrimStart,
            "trimStart",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeTrimEnd,
            "trimEnd",
            "",
            ValueKind::String,
        ),
        (
            StandardBuiltinId::StringPrototypeSplit,
            "split",
            "','",
            ValueKind::Dynamic,
        ),
    ];
    assert_eq!(cases.len(), 33);
    let mut seen = BTreeSet::new();
    for (builtin, method, arguments, expected_kind) in cases {
        assert!(seen.insert(builtin.function_id()));
        let source = format!(
            "function run() {{ const receiver = {{ saved: String.prototype.{method}, \
                 {method}() {{ return 99; }}, toString() {{ return 'abc'; }} }}; \
                 return receiver.saved({arguments}); }} run();"
        );
        let program = lower_script(&source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let run = program
            .script
            .as_ref()
            .expect("script")
            .functions
            .iter()
            .find(|function| function.name == "run")
            .expect("run");
        let result = function_return(run).expect("native alias result");
        let (callee, _) = retained_indexed_collection_call(result, "saved");
        assert_eq!(
            callee.function_targets.exact_single_target(),
            Some(&builtin.function_id()),
            "{builtin:?}: {callee:?}"
        );
        assert_eq!(result.kind, expected_kind, "{builtin:?}: {result:?}");
        assert!(result.heap_shape.is_none(), "{builtin:?}: {result:?}");
        match builtin {
            StandardBuiltinId::StringPrototypeCodePointAt => assert_eq!(
                result.possible_kinds,
                KindSet::from_kind(ValueKind::Number)
                    .union(KindSet::from_kind(ValueKind::Undefined))
            ),
            StandardBuiltinId::StringPrototypeAt => assert_eq!(
                result.possible_kinds,
                KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined))
            ),
            StandardBuiltinId::StringPrototypeSplit => {
                assert_eq!(result.possible_kinds, KindSet::all_runtime_tags())
            }
            _ => assert_eq!(result.possible_kinds, KindSet::from_kind(expected_kind)),
        }
    }
}

#[test]
fn primitive_string_spread_calls_keep_numeric_result_domains_and_all_arguments() {
    for (method, builtin, expected_kind, arguments, expected_argument_count) in [
        (
            "charCodeAt",
            StandardBuiltinId::StringPrototypeCharCodeAt,
            ValueKind::Number,
            "...[1]",
            1,
        ),
        (
            "charCodeAt",
            StandardBuiltinId::StringPrototypeCharCodeAt,
            ValueKind::Number,
            "...[1], 2, 3",
            3,
        ),
        (
            "codePointAt",
            StandardBuiltinId::StringPrototypeCodePointAt,
            ValueKind::Dynamic,
            "...[1], 2, 3",
            3,
        ),
    ] {
        let source = format!("function run() {{ return 'abc'.{method}({arguments}); }} run();");
        let program = lower_script(&source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let run = program
            .script
            .as_ref()
            .expect("script")
            .functions
            .iter()
            .find(|function| function.name == "run")
            .expect("run");
        let result = function_return(run).expect("primitive native result");
        let (callee, args) = retained_indexed_collection_call(result, method);
        assert_eq!(
            callee.function_targets.exact_single_target(),
            Some(&builtin.function_id())
        );
        assert_eq!(result.kind, expected_kind, "{source}: {result:?}");
        assert!(result.heap_shape.is_none());
        assert_eq!(args.len(), expected_argument_count);
        assert!(matches!(&args[0].expr, ExprIr::SpreadArgument(value)
                if value.protocol == SpreadArgumentProtocol::ARGUMENT_LIST));
        if expected_argument_count == 3 {
            assert!(matches!(args[1].expr, ExprIr::Number(value) if f64::from_bits(value) == 2.0));
            assert!(matches!(args[2].expr, ExprIr::Number(value) if f64::from_bits(value) == 3.0));
        }
        let expected_kinds = if expected_kind == ValueKind::Number {
            KindSet::from_kind(ValueKind::Number)
        } else {
            KindSet::from_kind(ValueKind::Number).union(KindSet::from_kind(ValueKind::Undefined))
        };
        assert_eq!(result.possible_kinds, expected_kinds);
    }
}

#[test]
fn acquired_string_alias_survives_argument_replacement_and_real_spread() {
    let program = lower_script(
            "function run() { const receiver = { saved: String.prototype.trim, \
             toString() { return ' abc '; } }; \
             return (0, receiver).saved((receiver.saved = function() { return 99; }, 1), ...[2], 3); } run();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let run = program
        .script
        .as_ref()
        .expect("script")
        .functions
        .iter()
        .find(|function| function.name == "run")
        .expect("run");
    let result = function_return(run).expect("saved trim call");
    let (callee, args) = retained_indexed_collection_call(result, "saved");
    assert_eq!(
        callee.function_targets.exact_single_target(),
        Some(&StandardBuiltinId::StringPrototypeTrim.function_id())
    );
    assert_eq!(result.kind, ValueKind::String);
    let [replacement, spread, trailing] = args else {
        panic!("{args:?}")
    };
    let ExprIr::Comma { lhs, rhs } = &replacement.expr else {
        panic!("{replacement:?}")
    };
    assert!(matches!(lhs.expr, ExprIr::OrdinaryPropertyAssignment(_)));
    assert!(matches!(rhs.expr, ExprIr::Number(value) if f64::from_bits(value) == 1.0));
    assert!(matches!(&spread.expr, ExprIr::SpreadArgument(value)
            if value.protocol == SpreadArgumentProtocol::ARGUMENT_LIST));
    assert!(matches!(trailing.expr, ExprIr::Number(value) if f64::from_bits(value) == 3.0));
}

#[test]
fn string_symbol_hooks_have_arbitrary_results_in_direct_and_spread_calls() {
    for (method, builtin, pattern) in [
        (
            "match",
            StandardBuiltinId::StringPrototypeMatch,
            "{ [Symbol.match]() { return 7; } }",
        ),
        (
            "matchAll",
            StandardBuiltinId::StringPrototypeMatchAll,
            "{ [Symbol.match]: false, [Symbol.matchAll]() { return 7; } }",
        ),
        (
            "replace",
            StandardBuiltinId::StringPrototypeReplace,
            "{ [Symbol.replace]() { return 7; } }",
        ),
        (
            "replaceAll",
            StandardBuiltinId::StringPrototypeReplaceAll,
            "{ [Symbol.match]: false, [Symbol.replace]() { return 7; } }",
        ),
        (
            "search",
            StandardBuiltinId::StringPrototypeSearch,
            "{ [Symbol.search]() { return 7; } }",
        ),
        (
            "split",
            StandardBuiltinId::StringPrototypeSplit,
            "{ [Symbol.split]() { return 7; } }",
        ),
    ] {
        for spread in [false, true] {
            let arguments = if spread {
                "...[pattern, 'ignored']"
            } else {
                "pattern, 'ignored'"
            };
            let source = format!(
                    "function run() {{ const pattern = {pattern}; \
                     const receiver = {{ saved: String.prototype.{method}, toString() {{ return 'abc'; }} }}; \
                     return receiver.saved({arguments}); }} run();"
                );
            let program = lower_script(&source);
            assert!(
                program.is_wasm_supported(),
                "{source}: {:?}",
                program.diagnostics
            );
            let run = program
                .script
                .as_ref()
                .expect("script")
                .functions
                .iter()
                .find(|function| function.name == "run")
                .expect("run");
            let result = function_return(run).expect("hook result");
            let (callee, args) = retained_indexed_collection_call(result, "saved");
            // A retained intrinsic proof must identify this exact method. An
            // open callee still uses the original observable property read.
            // In both cases the Symbol hook's result has no native codomain.
            match &callee.function_targets {
                FunctionTargetKnowledge::Exact(_) => assert_eq!(
                    callee.function_targets.exact_single_target(), Some(&builtin.function_id())),
                FunctionTargetKnowledge::Open(_) => {},
                FunctionTargetKnowledge::ClosedLocalCall(_) => {
                    panic!("an observable property Get cannot prove a closed lexical call")
                }
            }
            assert_eq!(result.kind, ValueKind::Dynamic, "{source}: {result:?}");
            assert_eq!(result.possible_kinds, KindSet::all_runtime_tags());
            assert!(result.heap_shape.is_none());
            assert_eq!(result.function_targets, FunctionTargetKnowledge::unknown());
            assert_eq!(args.len(), if spread { 1 } else { 2 });
            if spread {
                assert!(matches!(&args[0].expr, ExprIr::SpreadArgument(value)
                        if value.protocol == SpreadArgumentProtocol::ARGUMENT_LIST));
            }
        }
    }
    let program = lower_script(
        "function run() { return 'abc'.split({ [Symbol.split]() { return 7; } }); } run();",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let run = program
        .script
        .as_ref()
        .expect("script")
        .functions
        .iter()
        .find(|function| function.name == "run")
        .expect("run");
    let result = function_return(run).expect("primitive split hook result");
    let (callee, args) = retained_indexed_collection_call(result, "split");
    assert_eq!(
        callee.function_targets.exact_single_target(),
        Some(&StandardBuiltinId::StringPrototypeSplit.function_id())
    );
    assert_eq!(args.len(), 1);
    assert_eq!(result.kind, ValueKind::Dynamic);
    assert_eq!(result.possible_kinds, KindSet::all_runtime_tags());
    assert!(result.heap_shape.is_none());
}

#[test]
fn generic_string_coercions_and_hooks_invalidate_captured_flow_facts() {
    for invocation in [
        "const receiver = { saved: String.prototype.trim, \
             toString() { captured.value = 'changed'; return ' abc '; } }; receiver.saved();",
        "const index = { valueOf() { captured.value = 'changed'; return 1; } }; \
             'abc'.charCodeAt(index);",
        "const receiver = { saved: String.prototype.concat, \
             toString() { captured.value = 'changed'; return 'abc'; } }; receiver.saved('d');",
        "const pattern = { [Symbol.split]() { captured.value = 'changed'; return 7; } }; \
             'abc'.split(pattern);",
        "const pattern = { [Symbol.match]: false, \
             [Symbol.matchAll]() { captured.value = 'changed'; return 7; } }; \
             const receiver = { saved: String.prototype.matchAll }; receiver.saved(pattern);",
    ] {
        let source = format!("let captured = {{ value: 1 }}; {invocation} captured.value + 1;");
        assert_caller_flow_invalidation_reaches_final_addition(&source);
    }
}

#[test]
fn copied_number_string_hooks_keep_the_acquired_callee_and_raw_receiver() {
    for (method, arguments, expected_argument_count) in [
        ("match", "{ [Symbol.match]() { return 7; } }", 1),
        ("split", "{ [Symbol.split]() { return 7; } }", 1),
        ("match", "/2/", 1),
        ("split", "'2', 2", 2),
    ] {
        let source = format!(
            "Number.prototype.{method} = String.prototype.{method}; \
                 (123).{method}({arguments});"
        );
        let program = lower_script(&source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
        let script = program.script.as_ref().expect("script");
        let StatementIr::Expression(result) = script.body.statements.last().expect("call") else {
            panic!("{source}: expected the Number call");
        };
        let (callee, args) = retained_indexed_collection_call(result, method);
        let ExprIr::MaterializeBinding { value, .. } = &result.expr else {
            unreachable!()
        };
        assert!(matches!(value.expr, ExprIr::Number(bits) if f64::from_bits(bits) == 123.0));
        // Number has no catalogued match/split identity. A source copy is
        // an observable property value, not a proof of a native target.
        assert_eq!(callee.function_targets, FunctionTargetKnowledge::unknown());
        assert_eq!(args.len(), expected_argument_count);
        assert_eq!(result.kind, ValueKind::Dynamic);
        assert_eq!(result.possible_kinds, KindSet::all_runtime_tags());
        assert!(result.heap_shape.is_none());
        assert_eq!(result.function_targets, FunctionTargetKnowledge::unknown());
    }
}

#[test]
fn number_hook_reference_precedes_prototype_replacement_and_full_arguments() {
    for method in ["match", "split"] {
        for form in ["direct", "spread", "extra"] {
            let replacement = format!(
                "(Number.prototype.{method} = function replacement() {{ return 99; }}, pattern)"
            );
            let arguments = match form {
                "direct" => replacement,
                "spread" => format!("...[{replacement}]"),
                "extra" => format!("{replacement}, ...[2], 3"),
                _ => unreachable!(),
            };
            let source = format!(
                "const pattern = {{ [Symbol.{method}]() {{ return 7; }} }}; \
                     Number.prototype.{method} = String.prototype.{method}; \
                     (123).{method}({arguments});"
            );
            let program = lower_script(&source);
            assert!(
                program.is_wasm_supported(),
                "{source}: {:?}",
                program.diagnostics
            );
            let script = program.script.as_ref().expect("script");
            let StatementIr::Expression(result) = script.body.statements.last().expect("call")
            else {
                panic!("expected the Number property call");
            };
            let (_, args) = retained_indexed_collection_call(result, method);
            assert_eq!(args.len(), if form == "extra" { 3 } else { 1 });
            let first = if form == "spread" {
                let ExprIr::SpreadArgument(spread) = &args[0].expr else {
                    panic!("{args:?}")
                };
                assert_eq!(spread.protocol, SpreadArgumentProtocol::ARGUMENT_LIST);
                let ExprIr::ArrayLiteral(elements) = &spread.value.expr else {
                    panic!("{spread:?}")
                };
                let [element] = elements.as_slice() else {
                    panic!("{elements:?}")
                };
                element
            } else {
                &args[0]
            };
            let ExprIr::Comma { lhs, rhs } = &first.expr else {
                panic!("{first:?}")
            };
            assert!(matches!(
                lhs.expr,
                ExprIr::OrdinaryPropertyAssignment(_) | ExprIr::PropertyWrite { .. }
            ));
            assert!(matches!(rhs.expr, ExprIr::Identifier(_)));
            if form == "extra" {
                assert!(matches!(&args[1].expr, ExprIr::SpreadArgument(spread)
                        if spread.protocol == SpreadArgumentProtocol::ARGUMENT_LIST));
                assert!(
                    matches!(args[2].expr, ExprIr::Number(bits) if f64::from_bits(bits) == 3.0)
                );
            }
            assert_eq!(result.kind, ValueKind::Dynamic);
            assert_eq!(result.possible_kinds, KindSet::all_runtime_tags());
            assert!(result.heap_shape.is_none());
        }
    }
}

#[test]
fn number_split_static_separator_retains_operands_and_real_hook_dispatch() {
    let program = lower_script(
        "var separator = { toString: function() { return /x/; }, \
             [Symbol.split]() { return 7; } }; \
             let effect = 0; Number.prototype.split = String.prototype.split; \
             (123).split(separator, (effect = 1, 2));",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script");
    let StatementIr::Expression(result) = script.body.statements.last().expect("call") else {
        panic!("expected the retained split call");
    };
    let (_, args) = retained_indexed_collection_call(result, "split");
    let [separator, limit] = args else {
        panic!("{args:?}")
    };
    assert!(matches!(&separator.expr,
            ExprIr::GlobalIdentifierRead { name } if name == "separator"));
    let ExprIr::Comma { lhs, rhs } = &limit.expr else {
        panic!("{limit:?}")
    };
    assert!(matches!(lhs.expr, ExprIr::AssignIdentifier { .. }));
    assert!(matches!(rhs.expr, ExprIr::Number(bits) if f64::from_bits(bits) == 2.0));
    assert_eq!(result.kind, ValueKind::Dynamic);
    assert_eq!(result.possible_kinds, KindSet::all_runtime_tags());
    assert!(result.heap_shape.is_none());
}

#[test]
fn copied_number_hook_calls_invalidate_captured_flow_facts() {
    for method in ["match", "split"] {
        let source = format!(
                "let captured = {{ value: 1 }}; \
                 const pattern = {{ [Symbol.{method}]() {{ captured.value = 'changed'; return 7; }} }}; \
                 Number.prototype.{method} = String.prototype.{method}; \
                 (123).{method}(pattern); captured.value + 1;"
            );
        assert_caller_flow_invalidation_reaches_final_addition(&source);
    }
}
