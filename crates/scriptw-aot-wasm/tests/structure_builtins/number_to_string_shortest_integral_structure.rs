const OPERATIONS_SOURCE: &str = include_str!("../../src/operations.rs");
const NUMBER_TO_STRING_SOURCE: &str = include_str!("../../src/operations/number_to_string.rs");
const RYU_SOURCE: &str = include_str!("../../src/operations/number_to_string/ryu.rs");
const LOWERING_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering.rs");
const SCRIPTW_IR_MANIFEST: &str = include_str!("../../../scriptw-ir/Cargo.toml");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}`"))
        .0
}

#[test]
fn number_to_string_has_one_private_ryu_owner() {
    assert_eq!(
        OPERATIONS_SOURCE.matches("mod number_to_string;").count(),
        1
    );
    assert!(!OPERATIONS_SOURCE.contains("pub mod number_to_string;"));
    assert_eq!(NUMBER_TO_STRING_SOURCE.matches("mod ryu;").count(), 1);
    assert!(!NUMBER_TO_STRING_SOURCE.contains("pub mod ryu;"));
    assert_eq!(
        NUMBER_TO_STRING_SOURCE
            .matches("pub(crate) fn emit_number_to_string_payload(")
            .count(),
        1
    );
    assert_eq!(
        RYU_SOURCE
            .matches("pub(super) fn emit_ryu_number_to_string_payload(")
            .count(),
        1
    );
}

#[test]
fn ryu_power_domain_is_closed_and_traps_if_its_proof_is_violated() {
    let domain = bounded(
        RYU_SOURCE,
        "enum RyuPowerTable {",
        "\n}\n\nimpl RyuPowerTable",
    );
    assert_eq!(domain.matches("\n    InversePow5,").count(), 1);
    assert_eq!(domain.matches("\n    Pow5,").count(), 1);
    let projections = bounded(
        RYU_SOURCE,
        "impl RyuPowerTable {",
        "\n}\n\nconst fn pow5_bits",
    );
    for arm in [
        "Self::InversePow5 => 291",
        "Self::Pow5 => 325",
        "Self::InversePow5 => inverse_pow5(index)",
        "Self::Pow5 => pow5(index)",
    ] {
        assert!(projections.contains(arm), "missing exhaustive arm `{arm}`");
    }
    assert!(!projections.contains("_ =>"));

    let lookup = bounded(
        RYU_SOURCE,
        "fn emit_ryu_power_lookup(",
        "\n    fn emit_unsigned_multiply_128(",
    );
    assert!(lookup.contains("for index in 0..=table.last_index()"));
    assert!(lookup.contains("high_local.load(function)"));
    assert!(lookup.contains("Instruction::I64Eqz"));
    assert!(lookup.contains("Instruction::Unreachable"));
}

#[test]
fn dynamic_formatter_uses_shortest_decimal_and_ecmascript_spelling_projections() {
    for operation in [
        "fn emit_unsigned_multiply_128(",
        "fn emit_ryu_mul_shift(",
        "fn emit_ryu_shortest_decimal(",
        "fn emit_ryu_common_digit_removal(",
        "fn emit_ryu_trailing_zero_digit_removal(",
        "fn emit_ecmascript_decimal_string(",
        "fn emit_decimal_digit_count(",
        "fn emit_decimal_digit_at(",
    ] {
        assert!(RYU_SOURCE.contains(operation), "missing `{operation}`");
    }
    for threshold in ["I64Const(21)", "I64Const(-6)"] {
        assert!(RYU_SOURCE.contains(threshold), "missing `{threshold}`");
    }
    let formatter = bounded(RYU_SOURCE, "fn emit_ecmascript_decimal_string(", "\n}\n");
    for marker in [
        "StringConstruction::allocate(",
        "construction.write(index, unit, schema, function)",
        "scientific.store(function)",
        "magnitude.store(function)",
        "Instruction::I32Const(i32::from(b'e'))",
        "Instruction::I32Const(i32::from(b'+'))",
        "self.emit_decimal_digit_at(magnitude, exponent_digits, digit_index, function)",
    ] {
        assert!(
            formatter.contains(marker),
            "GC decimal/scientific owner lost {marker}"
        );
    }
    let entry = bounded(
        RYU_SOURCE,
        "pub(super) fn emit_ryu_number_to_string_payload(",
        "fn emit_decimal_digit_count(",
    );
    let shortest = entry.find("self.emit_ryu_shortest_decimal(").unwrap();
    let spelling = entry.find("self.emit_ecmascript_decimal_string(").unwrap();
    assert!(shortest < spelling);
    for removed_heuristic in [
        "1_000_000.0",
        "frac_scaled_local",
        "emit_fraction_width_local",
        "shortest_int_u_local",
    ] {
        assert!(
            !NUMBER_TO_STRING_SOURCE.contains(removed_heuristic)
                && !RYU_SOURCE.contains(removed_heuristic)
                && !OPERATIONS_SOURCE.contains(removed_heuristic),
            "obsolete formatter path remains: `{removed_heuristic}`"
        );
    }
}

#[test]
fn static_number_spelling_uses_the_pinned_ecmascript_authority() {
    assert!(SCRIPTW_IR_MANIFEST.contains("ryu-js = \"=1.0.2\""));
    let formatter = bounded(
        LOWERING_SOURCE,
        "fn js_number_to_string(value: f64) -> String {",
        "\n    }\n\n    fn read_object_shape",
    );
    assert!(formatter.contains("ryu_js::Buffer::new().format(value).to_string()"));
    assert!(!formatter.contains("value.to_string()"));

    let property_key = bounded(
        LOWERING_SOURCE,
        "fn static_number_property_key_from_value(value: f64) -> String {",
        "\n    }\n\n    fn static_number_index_expr",
    );
    assert!(property_key.contains("Self::js_number_to_string(value)"));
    assert!(!property_key.contains("format!("));
}
