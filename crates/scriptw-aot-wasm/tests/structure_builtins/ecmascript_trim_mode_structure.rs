//! Protect the closed TrimString policy, rather than retired byte transports.
const OPERATIONS_SOURCE: &str = include_str!("../../src/operations.rs");
const STRING_BUILTINS_SOURCE: &str = include_str!("../../src/builtins/string.rs");
const STRING_TRIM_SOURCE: &str = include_str!("../../src/operations/string_trim.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .expect(start)
        .1
        .split_once(end)
        .expect(end)
        .0
}

fn compact(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[test]
fn ecmascript_trim_mode_is_crate_internal_and_closed() {
    assert_eq!(OPERATIONS_SOURCE.matches("mod string_trim;").count(), 1);
    assert!(!OPERATIONS_SOURCE.contains("pub mod string_trim;"));
    assert!(!OPERATIONS_SOURCE.contains("pub(crate) mod string_trim;"));
    assert!(!OPERATIONS_SOURCE.contains("enum EcmaTrimMode"));
    assert_eq!(
        OPERATIONS_SOURCE
            .matches("pub(crate) use string_trim::EcmaTrimMode;")
            .count(),
        1
    );
    assert!(!OPERATIONS_SOURCE.contains("pub use string_trim::EcmaTrimMode;"));
    assert!(
        STRING_BUILTINS_SOURCE
            .contains("use crate::operations::EcmaTrimMode as StringTrimOperation;")
    );
    assert!(!STRING_BUILTINS_SOURCE.contains("enum StringTrimOperation"));
    assert_eq!(
        compact(bounded(
            STRING_TRIM_SOURCE,
            "\npub(crate) enum EcmaTrimMode {",
            "\n}"
        )),
        "Start,End,Both,"
    );
    for visibility in ["pub", "pub(super)"] {
        assert!(!STRING_TRIM_SOURCE.contains(&format!("{visibility} enum EcmaTrimMode")));
    }
    for visibility in ["pub", "pub(crate)", "pub(super)"] {
        assert!(!STRING_TRIM_SOURCE.contains(&format!(
            "{visibility} fn emit_ecmascript_trim_payload_from_locals"
        )));
    }
}

#[test]
fn ecmascript_trim_whitespace_table_is_complete_and_single_owned() {
    let predicate = bounded(
        OPERATIONS_SOURCE,
        "pub(crate) fn emit_ecmascript_whitespace_i32(",
        "\n    fn emit_numeric_ascii_digit_i64(",
    );
    let domain = compact(bounded(predicate, "for code in [", "] {"));
    assert_eq!(
        domain,
        "0x0009,0x000a,0x000b,0x000c,0x000d,0x0020,0x00a0,0x1680,0x2000,0x2001,0x2002,0x2003,0x2004,0x2005,0x2006,0x2007,0x2008,0x2009,0x200a,0x2028,0x2029,0x202f,0x205f,0x3000,0xfeff,"
    );
    assert_eq!(
        OPERATIONS_SOURCE
            .matches("fn emit_ecmascript_whitespace_i32(")
            .count(),
        1
    );
    assert!(!STRING_TRIM_SOURCE.contains("ECMASCRIPT_NON_ASCII_WHITESPACE_UTF8"));
}
