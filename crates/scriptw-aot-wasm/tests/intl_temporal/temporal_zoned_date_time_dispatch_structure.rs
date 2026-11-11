const METHODS_SOURCE: &str = include_str!("../../src/builtins/temporal_zoned_date_time_methods.rs");
const MOD_SOURCE: &str = include_str!("../../src/builtins/mod.rs");
const STANDARD_SOURCE: &str = include_str!("../../src/builtins/standard.rs");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

#[test]
fn zoned_date_time_catalog_routes_use_four_fixed_family_entries() {
    for (method, domain, variant, raw_emitter) in [
        (
            "add",
            "TemporalZonedArithmeticOperation",
            "Add",
            "emit_temporal_zoned_date_time_add_or_subtract",
        ),
        (
            "subtract",
            "TemporalZonedArithmeticOperation",
            "Subtract",
            "emit_temporal_zoned_date_time_add_or_subtract",
        ),
        (
            "until",
            "TemporalZonedDifferenceOperation",
            "Until",
            "emit_temporal_zoned_date_time_until_or_since",
        ),
        (
            "since",
            "TemporalZonedDifferenceOperation",
            "Since",
            "emit_temporal_zoned_date_time_until_or_since",
        ),
    ] {
        let fixed_entry = format!("pub(super) fn emit_temporal_zoned_date_time_{method}_builtin(");
        let fixed_entry_body = METHODS_SOURCE
            .split_once(&fixed_entry)
            .unwrap_or_else(|| panic!("missing fixed {method} entry"))
            .1
            .split_once("\n    }")
            .expect("fixed entry end")
            .0;
        let standard_route =
            format!("self.emit_temporal_zoned_date_time_{method}_builtin(function)?;");
        assert_eq!(METHODS_SOURCE.matches(&fixed_entry).count(), 1);
        assert_eq!(
            fixed_entry_body
                .matches(&format!("{domain}::{variant}"))
                .count(),
            1
        );
        assert_eq!(STANDARD_SOURCE.matches(&standard_route).count(), 1);
        assert!(!STANDARD_SOURCE.contains(raw_emitter));
    }

    assert_eq!(
        METHODS_SOURCE
            .matches("fn emit_temporal_zoned_date_time_add_or_subtract(")
            .count(),
        1
    );
    assert_eq!(
        METHODS_SOURCE
            .matches("fn emit_temporal_zoned_date_time_until_or_since(")
            .count(),
        1
    );
    assert!(
        !METHODS_SOURCE.contains("pub(crate) fn emit_temporal_zoned_date_time_add_or_subtract(")
    );
    assert!(!METHODS_SOURCE.contains("pub(crate) fn emit_temporal_zoned_date_time_until_or_since("));
    assert!(!STANDARD_SOURCE.contains("TemporalZonedArithmeticOperation"));
    assert!(!STANDARD_SOURCE.contains("TemporalZonedDifferenceOperation"));
    assert!(!MOD_SOURCE.contains("pub(crate) use temporal_zoned_date_time_methods"));
}
