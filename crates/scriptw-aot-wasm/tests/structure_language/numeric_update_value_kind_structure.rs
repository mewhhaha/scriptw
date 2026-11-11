const OPERATIONS_SOURCE: &str = include_str!("../../../scriptw-ir/src/operations.rs");
const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/ir.rs");
const REFERENCE_SOURCE: &str = include_str!("../../../scriptw-ir/src/reference.rs");
const BACKEND_OPERATIONS_SOURCE: &str = include_str!("../../src/operations.rs");
const EXPRESSIONS_SOURCE: &str = include_str!("../../src/expressions.rs");
const SUPER_EXPRESSIONS_SOURCE: &str =
    include_str!("../../src/expressions/super_property_mutation.rs");
const PLANNING_SOURCE: &str = include_str!("../../src/planning.rs");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source
        .find(start)
        .unwrap_or_else(|| panic!("missing start marker: {start}"));
    let rest = &source[start..];
    let end = rest
        .find(end)
        .unwrap_or_else(|| panic!("missing end marker: {end}"));
    &rest[..end]
}

#[test]
fn numeric_update_kind_is_one_closed_domain_with_total_value_kind_projection() {
    let domain = bounded(
        OPERATIONS_SOURCE,
        "pub enum NumericUpdateValueKind {",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum UpdateReturnMode",
    );
    for variant in ["Number", "BigInt", "Dynamic"] {
        assert_eq!(domain.matches(&format!("    {variant},")).count(), 1);
        assert!(domain.contains(&format!("Self::{variant} => ValueKind::{variant}")));
    }
    assert_eq!(
        domain
            .lines()
            .filter(|line| matches!(line.trim(), "Number," | "BigInt," | "Dynamic,"))
            .count(),
        3
    );
    assert!(domain.contains("pub const fn value_kind(self) -> ValueKind"));
    assert!(!domain.contains("_ =>"));
}

#[test]
fn every_numeric_update_ir_carrier_stores_the_closed_kind() {
    assert_eq!(
        IR_SOURCE
            .matches("value_kind: NumericUpdateValueKind,")
            .count(),
        1
    );
    assert_eq!(
        REFERENCE_SOURCE
            .matches("value_kind: NumericUpdateValueKind,")
            .count(),
        3
    );
    // The remaining direct ExprIr carrier is UpdateIdentifier; ordinary and
    // Super references own their fields in reference.rs. The third occurrence
    // there is the checked Super constructor parameter, not another carrier.
    assert!(bounded(
        IR_SOURCE,
        "    UpdateIdentifier {",
        "    CompoundAssignIdentifier {"
    )
    .contains("value_kind: NumericUpdateValueKind,"));
    assert!(bounded(
        REFERENCE_SOURCE,
        "pub struct OrdinaryPropertyNumericUpdateIr {",
        "impl OrdinaryPropertyNumericUpdateIr {"
    )
    .contains("value_kind: NumericUpdateValueKind,"));
    assert!(bounded(
        REFERENCE_SOURCE,
        "pub enum SuperPropertyMutationOperationIr {",
        "impl SuperPropertyMutationIr {"
    )
    .contains("value_kind: NumericUpdateValueKind,"));
    assert!(!IR_SOURCE.contains("value_kind: ValueKind"));
    assert!(!REFERENCE_SOURCE.contains("value_kind: ValueKind"));
}

#[test]
fn backend_consumers_are_exhaustive_and_have_no_impossible_kind_branch() {
    let delta = bounded(
        BACKEND_OPERATIONS_SOURCE,
        "pub(crate) fn emit_numeric_update_to_locals(",
        "pub(crate) fn compile_unary_minus_numeric_to_locals(",
    );
    for variant in ["Number", "BigInt", "Dynamic"] {
        assert_eq!(
            delta
                .matches(&format!("NumericUpdateValueKind::{variant} =>"))
                .count(),
            1
        );
    }
    assert!(!delta.contains("unreachable!"));
    assert!(!delta.contains("_ =>"));
    assert!(delta.contains("self.emit_is_bigint_tag_i32(old.tag(), function)"));
    assert_eq!(
        delta.matches("self.emit_numeric_bigint_operation(").count(),
        1
    );
    assert!(delta.contains("NumericUpdateOp::Increment => BigIntHelperOp::Add"));
    assert!(delta.contains("NumericUpdateOp::Decrement => BigIntHelperOp::Sub"));
    assert!(delta.contains("old: &ValueLocals,"));
    assert!(delta.contains("output: &ValueLocals,"));
    assert!(!delta.contains("_value_kind"));
    assert!(delta.contains("output.set_number(output.scalar(), function)"));
    let dynamic = delta
        .split_once("NumericUpdateValueKind::Dynamic => {")
        .unwrap()
        .1;
    assert_eq!(
        dynamic
            .matches("self.emit_is_bigint_tag_i32(old.tag(), function)")
            .count(),
        1
    );
    assert!(!delta.contains("Instruction::I64Add"));
    assert!(!delta.contains("Instruction::I64Sub"));
    assert!(!BACKEND_OPERATIONS_SOURCE.contains("fn emit_update_delta_from_locals("));
    assert!(!BACKEND_OPERATIONS_SOURCE.contains("fn emit_update_delta("));

    for source in [
        EXPRESSIONS_SOURCE,
        SUPER_EXPRESSIONS_SOURCE,
        PLANNING_SOURCE,
    ] {
        assert!(!source.contains("numeric update requires Number, BigInt, or Dynamic"));
        assert!(!source.contains("ordinary property numeric update kind is closed"));
    }
}
