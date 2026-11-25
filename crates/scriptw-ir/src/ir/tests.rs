use super::*;
use crate::CompletionKindIr;

#[test]
fn exact_function_target_joins_remain_exhaustive() {
    let targets = FunctionTargetKnowledge::exact("first".to_string())
        .join(FunctionTargetKnowledge::none())
        .join(FunctionTargetKnowledge::exact("second".to_string()));

    assert_eq!(
        targets.exact_targets(),
        Some(&BTreeSet::from(["first".to_string(), "second".to_string()]))
    );
}

#[test]
fn open_function_target_joins_retain_candidates_without_granting_authority() {
    let mut open = FunctionTargetKnowledge::exact("known".to_string());
    open.widen_for_possible_replacement();
    let targets = open.join(FunctionTargetKnowledge::exact("other".to_string()));

    assert!(targets.exact_targets().is_none());
    assert_eq!(
        targets.known_targets(),
        &BTreeSet::from(["known".to_string(), "other".to_string()])
    );
}

#[test]
fn generic_object_target_knowledge_remains_open_for_callable_proxies() {
    let targets = ValueInfo::new(ValueKind::Object).function_targets;

    assert!(targets.exact_targets().is_none());
    assert!(targets.known_targets().is_empty());
}

#[test]
fn bigint_literal_reports_one_limb_signed_magnitude() {
    let maximum =
        BigIntLiteralIr::from_bigint("18446744073709551615".parse().expect("valid BigInt"));
    let negative_maximum =
        BigIntLiteralIr::from_bigint("-18446744073709551615".parse().expect("valid BigInt"));
    let two_limbs =
        BigIntLiteralIr::from_bigint("18446744073709551616".parse().expect("valid BigInt"));

    assert_eq!(maximum.signed_magnitude_u64(), Some((1, u64::MAX)));
    assert_eq!(
        negative_maximum.signed_magnitude_u64(),
        Some((-1, u64::MAX))
    );
    assert_eq!(two_limbs.signed_magnitude_u64(), None);
    assert_eq!(two_limbs.signed_magnitude_limbs(), (1, vec![0, 1]));
}

#[test]
fn value_tags_round_trip_for_runtime_tags() {
    for kind in [
        ValueKind::Undefined,
        ValueKind::Null,
        ValueKind::Boolean,
        ValueKind::Number,
        ValueKind::String,
        ValueKind::Symbol,
        ValueKind::Object,
        ValueKind::Array,
        ValueKind::Function,
        ValueKind::Arguments,
        ValueKind::BigInt,
    ] {
        assert_eq!(ValueKind::from_tag(kind.tag()), Some(kind));
        assert!(!kind.as_str().is_empty());
    }
    assert_eq!(ValueKind::from_tag(ValueKind::Dynamic.tag()), None);
}

#[test]
fn operations_value_kind_classifies_known_ecmascript_type() {
    assert_eq!(
        ValueKind::Undefined.known_ecmascript_type(),
        Some(EcmaLanguageType::Undefined)
    );
    assert_eq!(
        ValueKind::Null.known_ecmascript_type(),
        Some(EcmaLanguageType::Null)
    );
    assert_eq!(
        ValueKind::Boolean.known_ecmascript_type(),
        Some(EcmaLanguageType::Boolean)
    );
    assert_eq!(
        ValueKind::String.known_ecmascript_type(),
        Some(EcmaLanguageType::String)
    );
    assert_eq!(
        ValueKind::Symbol.known_ecmascript_type(),
        Some(EcmaLanguageType::Symbol)
    );
    assert_eq!(
        ValueKind::Number.known_ecmascript_type(),
        Some(EcmaLanguageType::Number)
    );
    assert_eq!(
        ValueKind::BigInt.known_ecmascript_type(),
        Some(EcmaLanguageType::BigInt)
    );

    for kind in [
        ValueKind::Object,
        ValueKind::Array,
        ValueKind::Function,
        ValueKind::Arguments,
    ] {
        assert_eq!(kind.known_ecmascript_type(), Some(EcmaLanguageType::Object));
    }
    assert_eq!(ValueKind::Dynamic.known_ecmascript_type(), None);
}

#[test]
fn operations_statement_throw_completion_record_preserves_value() {
    let value = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("boom".into()),
    );
    let statement = StatementIr::Throw(value.clone());
    let completion = statement
        .abrupt_completion_record()
        .expect("throw should produce an abrupt completion record");

    assert!(statement.is_abrupt_completion_statement());
    assert_eq!(completion.kind(), CompletionKindIr::Throw);
    assert!(completion.is_abrupt());
    assert_eq!(completion.value(), Some(&value));
    assert_eq!(completion.target(), None);
}

#[test]
fn operations_statement_return_completion_record_preserves_value() {
    let value = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Number),
        ExprIr::Number(7.0f64.to_bits()),
    );
    let statement = StatementIr::Return(value.clone());
    let completion = statement
        .abrupt_completion_record()
        .expect("return should produce an abrupt completion record");

    assert!(statement.is_abrupt_completion_statement());
    assert_eq!(completion.kind(), CompletionKindIr::Return);
    assert!(completion.is_abrupt());
    assert_eq!(completion.value(), Some(&value));
    assert_eq!(completion.target(), None);
}

#[test]
fn operations_statement_break_continue_completion_record_preserves_target() {
    let break_statement = StatementIr::Break {
        label: Some("outer".into()),
    };
    let continue_statement = StatementIr::Continue {
        label: Some("loop".into()),
    };
    let empty_statement = StatementIr::Empty;

    let break_completion = break_statement
        .abrupt_completion_record()
        .expect("break should produce an abrupt completion record");
    let continue_completion = continue_statement
        .abrupt_completion_record()
        .expect("continue should produce an abrupt completion record");

    assert_eq!(break_completion.kind(), CompletionKindIr::Break);
    assert!(break_completion.is_abrupt());
    assert_eq!(break_completion.value(), None);
    assert_eq!(break_completion.target(), Some("outer"));
    assert_eq!(continue_completion.kind(), CompletionKindIr::Continue);
    assert!(continue_completion.is_abrupt());
    assert_eq!(continue_completion.value(), None);
    assert_eq!(continue_completion.target(), Some("loop"));
    assert!(!empty_statement.is_abrupt_completion_statement());
    assert_eq!(empty_statement.abrupt_completion_record(), None);
}

#[test]
fn operations_spec_is_callable_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Function),
        ExprIr::FunctionValue("callable".to_string()),
    );
    let expr = TypedExpr::spec_is_callable(operand.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Boolean));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::IsCallable);
    assert_eq!(operation.name(), "IsCallable");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_is_constructor_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Function),
        ExprIr::FunctionValue("ctor".to_string()),
    );
    let expr = TypedExpr::spec_is_constructor(operand.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Boolean));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::IsConstructor);
    assert_eq!(operation.name(), "IsConstructor");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_is_property_key_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Symbol),
        ExprIr::Symbol { description: None },
    );
    let expr = TypedExpr::spec_is_property_key(operand.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Boolean));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::IsPropertyKey);
    assert_eq!(operation.name(), "IsPropertyKey");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_to_boolean_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Number),
        ExprIr::Number(1.0f64.to_bits()),
    );
    let expr = TypedExpr::spec_to_boolean(operand.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Boolean));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::ToBoolean);
    assert_eq!(operation.name(), "ToBoolean");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_to_primitive_expr_records_hint_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Object),
        ExprIr::ExecutionGlobalObject,
    );
    let expr = TypedExpr::spec_to_primitive(operand.clone(), ToPrimitiveHint::String);

    assert_eq!(expr.kind, ValueKind::Dynamic);
    assert_eq!(expr.possible_kinds, KindSet::PRIMITIVE_ONLY);
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(
        operation,
        SpecOperationIr::ToPrimitive(ToPrimitiveHint::String)
    );
    assert_eq!(operation.name(), "ToPrimitive");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_to_number_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("1".to_string()),
    );
    let expr = TypedExpr::spec_to_number(operand.clone());

    assert_eq!(expr.kind, ValueKind::Number);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Number));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::ToNumber);
    assert_eq!(operation.name(), "ToNumber");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_to_numeric_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::BigInt),
        ExprIr::BigInt(BigIntLiteralIr::from_i64(1)),
    );
    let expr = TypedExpr::spec_to_numeric(operand.clone());

    assert_eq!(expr.kind, ValueKind::Dynamic);
    assert_eq!(
        expr.possible_kinds,
        KindSet::from_kind(ValueKind::Number).union(KindSet::from_kind(ValueKind::BigInt))
    );
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::ToNumeric);
    assert_eq!(operation.name(), "ToNumeric");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_to_bigint_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(ValueInfo::new(ValueKind::Boolean), ExprIr::Boolean(true));
    let expr = TypedExpr::spec_to_bigint(operand.clone());

    assert_eq!(expr.kind, ValueKind::BigInt);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::BigInt));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::ToBigInt);
    assert_eq!(operation.name(), "ToBigInt");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_to_string_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Number),
        ExprIr::Number(1.0f64.to_bits()),
    );
    let expr = TypedExpr::spec_to_string(operand.clone());

    assert_eq!(expr.kind, ValueKind::String);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::String));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::ToString);
    assert_eq!(operation.name(), "ToString");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_to_object_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("boxed".to_string()),
    );
    let expr = TypedExpr::spec_to_object(operand.clone());
    let object_like = KindSet::from_kind(ValueKind::Object)
        .union(KindSet::from_kind(ValueKind::Array))
        .union(KindSet::from_kind(ValueKind::Function))
        .union(KindSet::from_kind(ValueKind::Arguments));

    assert_eq!(expr.kind, ValueKind::Dynamic);
    assert_eq!(expr.possible_kinds, object_like);
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::ToObject);
    assert_eq!(operation.name(), "ToObject");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_to_property_key_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Number),
        ExprIr::Number(1.0f64.to_bits()),
    );
    let expr = TypedExpr::spec_to_property_key(operand.clone());

    assert_eq!(expr.kind, ValueKind::Dynamic);
    assert_eq!(
        expr.possible_kinds,
        KindSet::from_kind(ValueKind::String).union(KindSet::from_kind(ValueKind::Symbol))
    );
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::ToPropertyKey);
    assert_eq!(operation.name(), "ToPropertyKey");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_to_integer_or_infinity_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("-3.7".to_string()),
    );
    let expr = TypedExpr::spec_to_integer_or_infinity(operand.clone());

    assert_eq!(expr.kind, ValueKind::Number);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Number));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::ToIntegerOrInfinity);
    assert_eq!(operation.name(), "ToIntegerOrInfinity");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_to_length_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("3".to_string()),
    );
    let expr = TypedExpr::spec_to_length(operand.clone());

    assert_eq!(expr.kind, ValueKind::Number);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Number));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::ToLength);
    assert_eq!(operation.name(), "ToLength");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_to_index_expr_records_operation_and_operand() {
    let operand = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("3".to_string()),
    );
    let expr = TypedExpr::spec_to_index(operand.clone());

    assert_eq!(expr.kind, ValueKind::Number);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Number));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::ToIndex);
    assert_eq!(operation.name(), "ToIndex");
    assert_eq!(operands, vec![operand]);
}

#[test]
fn operations_spec_strict_equality_expr_records_operands() {
    let lhs = TypedExpr::from_info(ValueInfo::new(ValueKind::Number), ExprIr::Number(1));
    let rhs = TypedExpr::from_info(ValueInfo::new(ValueKind::Number), ExprIr::Number(1));
    let expr = TypedExpr::spec_strict_equality_comparison(lhs.clone(), rhs.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Boolean));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::StrictEqualityComparison);
    assert_eq!(operation.name(), "StrictEqualityComparison");
    assert_eq!(operands, vec![lhs, rhs]);
}

#[test]
fn operations_spec_is_loosely_equal_expr_records_operands() {
    let lhs = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("1".to_string()),
    );
    let rhs = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Number),
        ExprIr::Number(1.0f64.to_bits()),
    );
    let expr = TypedExpr::spec_is_loosely_equal(lhs.clone(), rhs.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Boolean));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::IsLooselyEqual);
    assert_eq!(operation.name(), "IsLooselyEqual");
    assert_eq!(operands, vec![lhs, rhs]);
}

#[test]
fn operations_spec_same_value_expr_records_operands() {
    let lhs = TypedExpr::from_info(ValueInfo::new(ValueKind::Number), ExprIr::Number(1));
    let rhs = TypedExpr::from_info(ValueInfo::new(ValueKind::Number), ExprIr::Number(1));
    let expr = TypedExpr::spec_same_value(lhs.clone(), rhs.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Boolean));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::SameValue);
    assert_eq!(operation.name(), "SameValue");
    assert_eq!(operands, vec![lhs, rhs]);
}

#[test]
fn operations_spec_same_value_zero_expr_records_operands() {
    let lhs = TypedExpr::from_info(ValueInfo::new(ValueKind::Number), ExprIr::Number(0));
    let rhs = TypedExpr::from_info(ValueInfo::new(ValueKind::Number), ExprIr::Number(0));
    let expr = TypedExpr::spec_same_value_zero(lhs.clone(), rhs.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Boolean));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::SameValueZero);
    assert_eq!(operation.name(), "SameValueZero");
    assert_eq!(operands, vec![lhs, rhs]);
}

#[test]
fn operations_spec_get_v_expr_records_target_and_key_operands() {
    let target = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Object),
        ExprIr::ExecutionGlobalObject,
    );
    let key = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("answer".to_string()),
    );
    let expr = TypedExpr::spec_get_v(target.clone(), key.clone());

    assert_eq!(expr.kind, ValueKind::Dynamic);
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::GetV);
    assert_eq!(operation.name(), "GetV");
    assert_eq!(operands, vec![target, key]);
}

#[test]
fn operations_spec_get_expr_records_target_and_key_operands() {
    let target = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Object),
        ExprIr::ExecutionGlobalObject,
    );
    let key = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("answer".to_string()),
    );
    let expr = TypedExpr::spec_get(target.clone(), key.clone());

    assert_eq!(expr.kind, ValueKind::Dynamic);
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::Get);
    assert_eq!(operation.name(), "Get");
    assert_eq!(operands, vec![target, key]);
}

#[test]
fn operations_spec_has_property_expr_records_target_and_key_operands() {
    let target = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Object),
        ExprIr::ExecutionGlobalObject,
    );
    let key = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("answer".to_string()),
    );
    let expr = TypedExpr::spec_has_property(target.clone(), key.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    assert_eq!(expr.possible_kinds, KindSet::from_kind(ValueKind::Boolean));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::HasProperty);
    assert_eq!(operation.name(), "HasProperty");
    assert_eq!(operands, vec![target, key]);
}

#[test]
fn operations_spec_create_data_property_or_throw_expr_records_operands() {
    let target = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Object),
        ExprIr::ExecutionGlobalObject,
    );
    let key = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("answer".to_string()),
    );
    let value = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Number),
        ExprIr::Number(42.0f64.to_bits()),
    );
    let expr =
        TypedExpr::spec_create_data_property_or_throw(target.clone(), key.clone(), value.clone());

    assert_eq!(expr.kind, ValueKind::Undefined);
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::CreateDataPropertyOrThrow);
    assert_eq!(operation.name(), "CreateDataPropertyOrThrow");
    assert_eq!(operands, vec![target, key, value]);
}

#[test]
fn operations_spec_set_expr_records_operands() {
    let target = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Object),
        ExprIr::ExecutionGlobalObject,
    );
    let key = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("answer".to_string()),
    );
    let value = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Number),
        ExprIr::Number(42.0f64.to_bits()),
    );
    let expr = TypedExpr::spec_set(target.clone(), key.clone(), value.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::Set);
    assert_eq!(operation.name(), "Set");
    assert_eq!(operands, vec![target, key, value]);
}

#[test]
fn operations_spec_delete_property_or_throw_expr_records_operands() {
    let target = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Object),
        ExprIr::ExecutionGlobalObject,
    );
    let key = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("answer".to_string()),
    );
    let expr = TypedExpr::spec_delete_property_or_throw(target.clone(), key.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::DeletePropertyOrThrow);
    assert_eq!(operation.name(), "DeletePropertyOrThrow");
    assert_eq!(operands, vec![target, key]);
}

#[test]
fn operations_spec_has_own_property_expr_records_target_and_key_operands() {
    let target = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Object),
        ExprIr::ExecutionGlobalObject,
    );
    let key = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("answer".to_string()),
    );
    let expr = TypedExpr::spec_has_own_property(target.clone(), key.clone());

    assert_eq!(expr.kind, ValueKind::Boolean);
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::HasOwnProperty);
    assert_eq!(operation.name(), "HasOwnProperty");
    assert_eq!(operands, vec![target, key]);
}

#[test]
fn operations_spec_get_method_expr_records_target_and_key_operands() {
    let target = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Object),
        ExprIr::ExecutionGlobalObject,
    );
    let key = TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("answer".to_string()),
    );
    let expr = TypedExpr::spec_get_method(target.clone(), key.clone());

    assert_eq!(expr.kind, ValueKind::Dynamic);
    assert_eq!(
        expr.possible_kinds,
        KindSet::from_kind(ValueKind::Undefined).union(KindSet::from_kind(ValueKind::Function))
    );
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::GetMethod);
    assert_eq!(operation.name(), "GetMethod");
    assert_eq!(operands, vec![target, key]);
}

#[test]
fn operations_spec_call_expr_records_callee_this_and_args() {
    let callee = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Function),
        ExprIr::FunctionValue(StandardBuiltinId::MathMax.function_id()),
    );
    let this_arg = TypedExpr::from_info(ValueInfo::new(ValueKind::Undefined), ExprIr::Undefined);
    let arg = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Number),
        ExprIr::Number(1.0f64.to_bits()),
    );
    let expr = TypedExpr::spec_call(callee.clone(), this_arg.clone(), vec![arg.clone()]);

    assert_eq!(expr.kind, ValueKind::Dynamic);
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::Call);
    assert_eq!(operation.name(), "Call");
    assert_eq!(operands, vec![callee, this_arg, arg]);
}

#[test]
fn operations_spec_construct_expr_records_callee_and_args() {
    let callee = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Function),
        ExprIr::FunctionValue(StandardBuiltinId::ArrayConstructor.function_id()),
    );
    let arg = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Number),
        ExprIr::Number(1.0f64.to_bits()),
    );
    let expr = TypedExpr::spec_construct(callee.clone(), vec![arg.clone()]);

    assert_eq!(expr.kind, ValueKind::Dynamic);
    assert!(expr.possible_kinds.contains(ValueKind::Array));
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = expr.expr
    else {
        panic!("expected spec operation expression");
    };
    assert_eq!(operation, SpecOperationIr::Construct);
    assert_eq!(operation.name(), "Construct");
    assert_eq!(operands, vec![callee, arg]);
}

#[test]
fn heap_shape_property_read_follows_prototype_chain() {
    let proto = HeapShape::Object(ObjectShape {
        properties: BTreeMap::from([(
            "x".to_string(),
            ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Number)),
        )]),
        ..ObjectShape::default()
    });
    let shape = HeapShape::Object(ObjectShape {
        prototype: Some(std::sync::Arc::new(proto)),
        ..ObjectShape::default()
    });
    assert_eq!(
        read_heap_shape_property(&shape, "x"),
        Some(ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Number)))
    );
}
