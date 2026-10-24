#[test]
fn lowers_for_multi_binding_lexical_init_ir() {
    let program = lower_script("for (let i = 0, j = 1; i < 1; i = i + 1) { j; }");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::For {
        init: Some(ForInitIr::LexicalBlock(bindings)),
        ..
    } = &script.body.statements[0]
    else {
        panic!("expected multi-binding lexical for initializer");
    };
    assert_eq!(bindings.len(), 2);
    assert!(program.ir_summary().contains("lets=2"));
}

#[test]
fn lowers_update_and_compound_ir() {
    let program = lower_script("let i = 2; let x = i++; x += ++i; x;");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    assert_eq!(script.result_kind(), ValueKind::Number);
    let summary = program.ir_summary();
    assert!(summary.contains("postfix_updates=1"));
    assert!(summary.contains("prefix_updates=1"));
    assert!(summary.contains("compound_assigns=1"));
}

#[test]
fn lowers_numeric_update_for_dynamically_typed_binding() {
    let program = lower_script(
        "function visitRange(start, end) { for (let codePoint = start; codePoint <= end; codePoint++) {} }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    assert!(program.ir_summary().contains("postfix_updates=1"));
}

#[test]
fn lowers_property_update_after_generic_get_v() {
    let program = lower_script(
        "var count = -1; function increment() { this.count++; } Array.from([0], increment, this);",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    assert!(program.ir_summary().contains("postfix_updates=1"));
}

#[test]
fn lowers_string_compound_add_with_dynamic_rhs_ir() {
    for (source, expected) in [
        ("let result = 'A'; result += unknown; result;", ValueKind::String),
        // Reading a mutable .apply property does not prove the call returns
        // String. The compound operation must retain runtime coercion.
        ("let result = String.fromCodePoint.apply(null, [65]); result += String.fromCodePoint.apply(null, [66]); result;", ValueKind::Dynamic),
    ] {
        let program = lower_script(source);
        assert!(program.is_wasm_supported());
        let script = program.script.as_ref().expect("script ir should exist");
        assert_eq!(script.result_kind(), expected, "{source}");
        let StatementIr::Expression(assignment) = &script.body.statements[1] else {
            panic!("expected the retained compound assignment");
        };
        assert_eq!(assignment.kind, expected);
        assert!(matches!(assignment.expr, ExprIr::CompoundAssignIdentifier { .. }
            | ExprIr::AssignIdentifier { .. }), "{assignment:?}");
    }
}

#[test]
fn lowers_coercive_compound_add_in_reduce_callback() {
    let program = lower_script(
        "function callback(accumulator, value) { accumulator += value; return accumulator; } [11, 9].reduceRight(callback, 0);",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    assert!(program.ir_summary().contains("heap_coercions=1"));
}

#[test]
fn lowers_dynamic_primitive_ir() {
    let program =
        lower_script("let x = 0; x || \"fallback\"; null ?? 3; typeof missingName; \"a\" + x;");
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("string_concats=1"));
    assert!(summary.contains("typeof_uses=1"));
    assert!(summary.contains("nullish_ops=1"));
}

#[test]
fn literal_string_addition_folds_semantic_utf16_and_canonicalizes_surrogate_pairs() {
    for (source, expected) in [
        (r#""left" + "right";"#, "leftright".encode_utf16().collect()),
        (r#""" + "\0";"#, vec![0]),
        (r#""\ud83d" + "\ude00";"#, vec![0xd83d, 0xde00]),
        (r#""\ud800" + "x";"#, vec![0xd800, 0x78]),
        (
            r#""\u{F0000}" + "D800";"#,
            "\u{f0000}D800".encode_utf16().collect(),
        ),
        (
            r#"("a" + "\ud83d") + ("\ude00" + "b");"#,
            vec![0x61, 0xd83d, 0xde00, 0x62],
        ),
    ] {
        let program = lower_script(source);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR");
        let StatementIr::Expression(expression) = &script.body.statements[0] else {
            panic!("expression statement: {source}");
        };
        let ExprIr::String(text) = &expression.expr else {
            panic!(
                "expected literal string, got {:?}: {source}",
                expression.expr
            );
        };
        assert_eq!(decode_js_string_utf16(text), expected, "{source}");
        assert_eq!(*text, encode_js_string_utf16(&expected), "{source}");
    }
}

#[test]
fn string_addition_fold_keeps_a_bounded_copy_budget_and_effectful_operands() {
    let left = "a".repeat(2048);
    for (right, folded) in [("b".repeat(2048), true), ("b".repeat(2049), false)] {
        let source = format!("'{left}' + '{right}';");
        let program = lower_script(&source);
        let script = program.script.as_ref().expect("script IR");
        let StatementIr::Expression(expression) = &script.body.statements[0] else {
            panic!("expression statement");
        };
        assert_eq!(matches!(expression.expr, ExprIr::String(_)), folded);
        assert_eq!(
            matches!(expression.expr, ExprIr::StringConcat { .. }),
            !folded
        );
    }
    for source in [
        "function value(){return 'x';} value() + 'y';",
        "const object={get value(){return 'x';}}; object.value + 'y';",
        "let value='x'; (value='z', value) + 'y';",
        "'x' + Symbol('y');",
        "'x' + {toString(){return 'y';}};",
    ] {
        let program = lower_script(source);
        let script = program.script.as_ref().expect("script IR");
        let StatementIr::Expression(expression) = script.body.statements.last().unwrap() else {
            panic!("expression statement: {source}");
        };
        assert!(!matches!(expression.expr, ExprIr::String(_)), "{source}");
    }
}

#[test]
fn operations_lowers_boolean_call_to_to_boolean_spec_operation() {
    let program = lower_script("Boolean(globalThis.flag);");
    assert!(program.is_wasm_supported());
    assert!(program.ir_summary().contains("spec_operations=2"));
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[0] else {
        panic!("expected expression statement");
    };
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = &expr.expr
    else {
        panic!("expected Boolean call to lower to a spec operation");
    };
    assert_eq!(*operation, SpecOperationIr::ToBoolean);
    assert_eq!(operands.len(), 1);
    assert!(matches!(
        operands[0].expr,
        ExprIr::SpecOperation {
            operation: SpecOperationIr::GetV,
            ..
        }
    ));
}

#[test]
fn operations_lowers_host_is_constructor_call_to_spec_operation() {
    let program =
        lower_test262_script("let value = function C() {}; __scriptwIsConstructor(value);");
    assert!(program.is_wasm_supported());
    assert!(program.ir_summary().contains("spec_operations=1"));
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[1] else {
        panic!("expected expression statement");
    };
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = &expr.expr
    else {
        panic!("expected __scriptwIsConstructor call to lower to a spec operation");
    };
    assert_eq!(*operation, SpecOperationIr::IsConstructor);
    assert_eq!(operands.len(), 1);
}

#[test]
fn operations_lowers_number_call_to_to_number_spec_operation() {
    let program = lower_script("let value = \"42\"; Number(value);");
    assert!(program.is_wasm_supported());
    assert!(program.ir_summary().contains("spec_operations=1"));
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[1] else {
        panic!("expected expression statement");
    };
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = &expr.expr
    else {
        panic!("expected spec operation");
    };
    assert_eq!(*operation, SpecOperationIr::ToNumber);
    assert_eq!(operands.len(), 1);
}

#[test]
fn operations_keeps_bigint_number_call_off_to_number_spec_operation() {
    let program = lower_script("let value = 1n; Number(value);");
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[1] else {
        panic!("expected expression statement");
    };
    assert!(!matches!(
        expr.expr,
        ExprIr::SpecOperation {
            operation: SpecOperationIr::ToNumber,
            ..
        }
    ));
}

#[test]
fn operations_keeps_number_call_with_reassigned_parameter_off_to_number_spec_operation() {
    let program = lower_script(
        "function convert(value) { value = Number(value); return value; } convert(1); convert(1n);",
    );
    let script = program.script.as_ref().expect("script ir should exist");
    let function = script.functions.first().expect("convert function");
    let StatementIr::Expression(expr) = &function.body.statements[0] else {
        panic!("expected assignment expression statement");
    };
    let ExprIr::AssignIdentifier { value, .. } = &expr.expr else {
        panic!("expected identifier assignment");
    };
    assert!(!matches!(
        value.expr,
        ExprIr::SpecOperation {
            operation: SpecOperationIr::ToNumber,
            ..
        }
    ));
}

#[test]
fn array_sort_call_keeps_comparator_parameters_dynamic() {
    let program = lower_script(
        "function compare(a, b) { a = Number(a); b = Number(b); return a - b; } const values = new BigInt64Array([2n, 1n]); Array.prototype.sort.call(values, compare);",
    );
    let script = program.script.as_ref().expect("script ir should exist");
    let compare = script
        .functions
        .iter()
        .find(|function| function.name == "compare")
        .expect("compare function");

    assert!(compare
        .params
        .iter()
        .all(|param| param.kind == ValueKind::Dynamic));
}

#[test]
fn arithmetic_with_a_bigint_operand_preserves_its_only_normal_result_kind() {
    let program = lower_script(
        "function decrement(value) { return value - 1n; } decrement(globalThis.value);",
    );
    let script = program.script.as_ref().expect("script ir should exist");
    let decrement = script
        .functions
        .iter()
        .find(|function| function.name == "decrement")
        .expect("decrement function");
    let StatementIr::Return(expr) = &decrement.body.statements[0] else {
        panic!("expected return statement");
    };

    assert_eq!(expr.kind, ValueKind::BigInt);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::BigInt));
    assert!(matches!(expr.expr, ExprIr::CoerciveBinaryNumber { .. }));
}

#[test]
fn bigint_literal_ir_preserves_arbitrary_precision_decimal() {
    let program = lower_script("184467440737095516161234567890n;");
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[0] else {
        panic!("expected expression statement");
    };
    let ExprIr::BigInt(value) = &expr.expr else {
        panic!("expected BigInt literal");
    };

    assert_eq!(value.decimal, "184467440737095516161234567890");
    assert!(value.requires_arbitrary_precision_storage);
    assert_eq!(
        value.wrapping_payload(),
        184467440737095516161234567890_u128 as u64
    );
}

#[test]
fn bigint_literal_ir_keeps_signed_minimum_in_immediate_storage() {
    let program = lower_script("-0x8000000000000000n;");
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[0] else {
        panic!("expected expression statement");
    };
    let ExprIr::BigInt(value) = &expr.expr else {
        panic!("expected BigInt literal");
    };

    assert_eq!(value.decimal, i64::MIN.to_string());
    assert!(!value.requires_arbitrary_precision_storage);
    assert_eq!(value.wrapping_payload(), i64::MIN as u64);
}

#[test]
fn bigint_constant_fold_uses_arbitrary_precision_decimal() {
    let program = lower_script("184467440737095516161234567890n + 10n;");
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[0] else {
        panic!("expected expression statement");
    };
    let ExprIr::BigInt(value) = &expr.expr else {
        panic!("expected folded BigInt literal");
    };

    assert_eq!(value.decimal, "184467440737095516161234567900");
    assert!(value.requires_arbitrary_precision_storage);
}

#[test]
fn bigint_complement_constant_fold_uses_arbitrary_precision_decimal() {
    let program = lower_script("~184467440737095516161234567890n;");
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[0] else {
        panic!("expected expression statement");
    };
    let ExprIr::BigInt(value) = &expr.expr else {
        panic!("expected folded BigInt literal");
    };

    assert_eq!(value.decimal, "-184467440737095516161234567891");
    assert!(value.requires_arbitrary_precision_storage);
}

#[test]
fn dynamic_complement_retains_the_closed_number_or_bigint_domain() {
    let program = lower_script("function complement(value) { return ~value; }");
    let script = program.script.as_ref().expect("script ir should exist");
    let complement = script.functions.first().expect("complement function");
    let StatementIr::Return(expr) = &complement.body.statements[0] else {
        panic!("expected return statement");
    };

    let expected =
        KindSet::from_kind(ValueKind::Number).union(KindSet::from_kind(ValueKind::BigInt));
    assert_eq!(expr.kind, ValueKind::Dynamic);
    assert_eq!(expr.possible_kinds, expected);
    assert!(matches!(
        expr.expr,
        ExprIr::UnaryBitwiseNumeric {
            op: UnaryBitwiseOp::Complement,
            ..
        }
    ));
}

#[test]
fn operations_lowers_string_call_to_to_string_spec_operation() {
    let program = lower_script("let value = 42; String(value);");
    assert!(program.is_wasm_supported());
    assert!(program.ir_summary().contains("spec_operations=1"));
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[1] else {
        panic!("expected expression statement");
    };
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = &expr.expr
    else {
        panic!("expected spec operation");
    };
    assert_eq!(*operation, SpecOperationIr::ToString);
    assert_eq!(operands.len(), 1);
}

#[test]
fn operations_lowers_strict_equality_to_spec_operation() {
    let program = lower_script("let value = 1; value === 1;");
    assert!(program.is_wasm_supported());
    assert!(program.ir_summary().contains("spec_operations=1"));
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[1] else {
        panic!("expected expression statement");
    };
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = &expr.expr
    else {
        panic!("expected strict equality to lower to a spec operation");
    };
    assert_eq!(*operation, SpecOperationIr::StrictEqualityComparison);
    assert_eq!(operands.len(), 2);
}

#[test]
fn operations_lowers_strict_not_equal_to_logical_not_spec_operation() {
    let program = lower_script("let value = 1; value !== 2;");
    assert!(program.is_wasm_supported());
    assert!(program.ir_summary().contains("spec_operations=1"));
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[1] else {
        panic!("expected expression statement");
    };
    let ExprIr::LogicalNot { expr } = &expr.expr else {
        panic!("expected strict not equal to lower to logical not");
    };
    assert!(matches!(
        expr.expr,
        ExprIr::SpecOperation {
            operation: SpecOperationIr::StrictEqualityComparison,
            ..
        }
    ));
}

#[test]
fn operations_lowers_loose_equality_to_spec_operation() {
    let program = lower_script("let value = 1; value == \"1\";");
    assert!(program.is_wasm_supported());
    assert!(program.ir_summary().contains("spec_operations=1"));
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[1] else {
        panic!("expected expression statement");
    };
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = &expr.expr
    else {
        panic!("expected loose equality to lower to a spec operation");
    };
    assert_eq!(*operation, SpecOperationIr::IsLooselyEqual);
    assert_eq!(operands.len(), 2);
}

#[test]
fn operations_lowers_loose_not_equal_to_logical_not_spec_operation() {
    let program = lower_script("let value = 1; value != \"2\";");
    assert!(program.is_wasm_supported());
    assert!(program.ir_summary().contains("spec_operations=1"));
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[1] else {
        panic!("expected expression statement");
    };
    let ExprIr::LogicalNot { expr } = &expr.expr else {
        panic!("expected loose not equal to lower to logical not");
    };
    assert!(matches!(
        expr.expr,
        ExprIr::SpecOperation {
            operation: SpecOperationIr::IsLooselyEqual,
            ..
        }
    ));
}

#[test]
fn operations_preserves_object_is_property_acquisition() {
    let program = lower_script("Object.is(NaN, NaN);");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[0] else {
        panic!("expected expression statement");
    };
    let ExprIr::CallIndirect { callee, args, .. } = &indirect_call_body(expr)
        .expect("acquired Object.is call")
        .expr
    else {
        unreachable!("indirect_call_body only accepts call IR");
    };
    assert!(matches!(
        callee.expr,
        ExprIr::PropertyRead { .. }
            | ExprIr::SpecOperation {
                operation: SpecOperationIr::GetV,
                ..
            }
    ));
    assert_eq!(args.len(), 2);
    assert_eq!(expr.kind, ValueKind::Boolean);
}

#[test]
fn operations_lowers_generic_object_property_read_to_get_v_spec_operation() {
    let program = lower_script("let object = {}; object.missing;");
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("spec_operations=1"));
    assert!(summary.contains("property_reads=1"));
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[1] else {
        panic!("expected expression statement");
    };
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = &expr.expr
    else {
        panic!("expected generic property read to lower to GetV");
    };
    assert_eq!(*operation, SpecOperationIr::GetV);
    assert_eq!(operands.len(), 2);
}

#[test]
fn operations_lowers_in_operator_to_has_property_spec_operation() {
    let program = lower_script("let object = {}; \"missing\" in object;");
    assert!(program.is_wasm_supported());
    let summary = program.ir_summary();
    assert!(summary.contains("spec_operations=1"));
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(expr) = &script.body.statements[1] else {
        panic!("expected expression statement");
    };
    let ExprIr::MaterializeBinding { name, value, body } = &expr.expr else {
        panic!("expected in operator to retain the key before evaluating the object");
    };
    assert!(matches!(&value.expr, ExprIr::String(value) if value == "missing"));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = &body.expr
    else {
        panic!("expected in operator to lower to HasProperty");
    };
    assert_eq!(*operation, SpecOperationIr::HasProperty);
    assert_eq!(operands.len(), 2);
    assert!(matches!(&operands[0].expr, ExprIr::Identifier(name) if name == "object"));
    assert!(matches!(&operands[1].expr, ExprIr::Identifier(key) if key == name));
}

#[test]
fn preserves_function_or_for_htmldda_truthiness() {
    let program = lower_script("function f() {} f || 2;");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    let Some(StatementIr::Expression(expr)) = script.body.statements.last() else {
        panic!("expected expression statement");
    };
    assert!(matches!(
        expr.expr,
        ExprIr::LogicalShortCircuit {
            op: LogicalBinaryOp::Or,
            ..
        }
    ));
}

#[test]
fn lowers_identifier_logical_assignment_ir() {
    let program = lower_script("let value = 0; value ||= 2; value &&= 3; value ??= 4;");
    assert!(program.is_wasm_supported());
    let script = program.script.as_ref().expect("script ir should exist");
    for (statement, expected_op) in script.body.statements[1..].iter().zip([
        LogicalBinaryOp::Or,
        LogicalBinaryOp::And,
        LogicalBinaryOp::Coalesce,
    ]) {
        let StatementIr::Expression(expr) = statement else {
            panic!("expected logical assignment expression statement");
        };
        let ExprIr::LogicalShortCircuit { op, lhs, rhs } = &expr.expr else {
            panic!("expected logical short circuit, got {:?}", expr.expr);
        };
        assert_eq!(*op, expected_op);
        assert!(matches!(lhs.expr, ExprIr::Identifier(_)));
        assert!(matches!(rhs.expr, ExprIr::AssignIdentifier { .. }));
    }
}
