const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/ir.rs");
const OPERATION_SOURCE: &str = include_str!("../../../scriptw-ir/src/operations.rs");
const LOWERING_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering/operator_values.rs");
const CLI_NUMERIC_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/language_numerics.rs");
const CLI_BITWISE_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_bigint_bitwise_core.js");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/numeric-conversion-codomains.md");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end after {start}: {end}"))
        .0
}

#[test]
fn contract_and_behavior_witness_cover_both_unary_numeric_kinds() {
    assert!(CONTRACT.contains(
        "cargo test -p scriptw-aot-wasm --test structure_language -- unary_numeric_ir_structure::"
    ));
    assert!(CLI_NUMERIC_TESTS.contains("fn run_wasm_backend_succeeds_for_bigint_bitwise_fixture()"));
    for marker in [
        "~0n",
        "~(-a)",
        "~0",
        "~Infinity",
        "complementTrace",
        "throwingComplement",
    ] {
        assert!(
            CLI_BITWISE_FIXTURE.contains(marker),
            "missing unary complement witness `{marker}`"
        );
    }
}

#[test]
fn unary_plus_and_minus_have_distinct_ir_states() {
    let unary_plus = bounded(IR_SOURCE, "    UnaryPlus {", "    UnaryMinusNumeric {");
    let unary_minus = bounded(
        IR_SOURCE,
        "    UnaryMinusNumeric {",
        "    UnaryBitwiseNumeric {",
    );
    assert_eq!(unary_plus.trim(), "expr: Box<TypedExpr>,\n    },");
    assert_eq!(unary_minus.trim(), "expr: Box<TypedExpr>,\n    },");
    assert!(!IR_SOURCE.contains("UnaryNumber"));
    assert!(!IR_SOURCE.contains("UnaryBigInt"));
    assert!(!OPERATION_SOURCE.contains("UnaryNumericOp"));
}

#[test]
fn lowering_keeps_to_number_and_to_numeric_domains_separate() {
    let unary_lowering = bounded(
        LOWERING_SOURCE,
        "            UnaryOp::Plus => {",
        "            UnaryOp::Not =>",
    );
    let (plus, minus) = unary_lowering
        .split_once("UnaryOp::Minus => {")
        .expect("unary-minus lowering");
    assert_eq!(plus.matches("ExprIr::UnaryPlus").count(), 1);
    assert!(
        plus.contains("self.record_possible_to_primitive_effects(&lowered_target.value_info())")
    );
    assert!(plus.contains("kind: ValueKind::Number,"));
    assert!(plus.contains("possible_kinds: KindSet::from_kind(ValueKind::Number),"));
    assert!(!plus.contains("numeric_domain"));

    assert_eq!(minus.matches("ExprIr::UnaryMinusNumeric").count(), 1);
    assert_eq!(
        minus.matches("numeric_domain(primitive.as_ref())").count(),
        1
    );
    assert!(minus.contains("ValueKind::Number"));
    assert!(minus.contains("ValueKind::BigInt"));
    assert!(!minus.contains("ExprIr::UnaryPlus"));
}
