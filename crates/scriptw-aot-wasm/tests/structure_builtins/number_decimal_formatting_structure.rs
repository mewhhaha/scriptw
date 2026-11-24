const OPERATIONS_SOURCE: &str = include_str!("../../src/operations.rs");
const DATA_SOURCE: &str = include_str!("../../src/data.rs");
const NUMBER_TO_STRING_SOURCE: &str = include_str!("../../src/operations/number_to_string.rs");
const DECIMAL_FORMAT_SOURCE: &str =
    include_str!("../../src/operations/number_to_string/decimal_format.rs");
const DECIMAL_FORMATTING_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_number_decimal_formatting.js");
const NUMBER_BUILTIN_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_number_builtin_family.js");
const NUMERICS_CLI_TESTS: &str =
    include_str!("../../../scriptw-cli/tests/cli/language_numerics.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}`"))
        .0
}

fn attributes_before<'a>(source: &'a str, declaration: &str) -> &'a str {
    let prefix = source
        .split_once(declaration)
        .unwrap_or_else(|| panic!("missing declaration `{declaration}`"))
        .0;
    prefix
        .rsplit_once("\n\n")
        .map_or(prefix, |(_, attributes)| attributes)
}

#[test]
fn decimal_format_domain_is_private_closed_and_exhaustive() {
    let decimal_domain = bounded(
        DECIMAL_FORMAT_SOURCE,
        "pub(in crate::operations) enum NumberDecimalFormat {",
        "\n}\n",
    );
    assert_eq!(
        decimal_domain
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>(),
        [
            "Fixed { fraction_digits_local: I64Local },",
            "Exponential(NumberExponentialFormat),",
            "Precision { significant_digits_local: I64Local },",
        ]
    );
    let exponential_domain = bounded(
        DECIMAL_FORMAT_SOURCE,
        "pub(in crate::operations) enum NumberExponentialFormat {",
        "\n}\n",
    );
    assert_eq!(
        exponential_domain
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>(),
        [
            "Shortest,",
            "FractionDigits { fraction_digits_local: I64Local },"
        ]
    );
    assert_eq!(
        NUMBER_TO_STRING_SOURCE
            .matches("mod decimal_format;")
            .count(),
        1
    );
    assert!(!NUMBER_TO_STRING_SOURCE.contains("pub mod decimal_format;"));
    assert_eq!(
        NUMBER_TO_STRING_SOURCE
            .matches(
                "pub(super) use decimal_format::{NumberDecimalFormat, NumberExponentialFormat};",
            )
            .count(),
        1
    );
    for declaration in [
        "pub(in crate::operations) enum NumberDecimalFormat {",
        "pub(in crate::operations) enum NumberExponentialFormat {",
    ] {
        assert!(!attributes_before(DECIMAL_FORMAT_SOURCE, declaration).contains("#[derive("));
    }

    let core = bounded(
        DECIMAL_FORMAT_SOURCE,
        "pub(in crate::operations) fn emit_number_decimal_format_payload(",
        "    #[allow(clippy::too_many_arguments)]",
    );
    assert!(core.contains("format: NumberDecimalFormat,"));
    assert_eq!(core.matches("match format").count(), 1);
    for variant in [
        "NumberDecimalFormat::Fixed {",
        "NumberDecimalFormat::Exponential(",
        "NumberDecimalFormat::Precision {",
    ] {
        assert_eq!(core.matches(variant).count(), 1, "format arm `{variant}`");
    }
    assert_eq!(core.matches("match exponential_format").count(), 1);
    for variant in [
        "NumberExponentialFormat::Shortest",
        "NumberExponentialFormat::FractionDigits {",
    ] {
        assert_eq!(
            core.matches(variant).count(),
            1,
            "exponential format arm `{variant}`"
        );
    }
    for escape in [
        "_ =>",
        "format ==",
        "format !=",
        "unreachable!",
        "debug_assert!",
    ] {
        assert!(
            !core.contains(escape),
            "decimal format core contains `{escape}`"
        );
    }
}

#[test]
fn exactly_three_number_methods_call_the_shared_decimal_core() {
    let fixed = bounded(
        OPERATIONS_SOURCE,
        "pub(crate) fn emit_number_to_fixed_payload(",
        "pub(crate) fn emit_number_to_exponential_payload(",
    );
    let exponential = bounded(
        OPERATIONS_SOURCE,
        "pub(crate) fn emit_number_to_exponential_payload(",
        "pub(crate) fn emit_number_to_precision_payload(",
    );
    let precision = bounded(
        OPERATIONS_SOURCE,
        "pub(crate) fn emit_number_to_precision_payload(",
        "pub(crate) fn emit_count_decimal_digits_u64(",
    );

    assert_eq!(
        fixed.matches("emit_number_decimal_format_payload(").count(),
        1
    );
    assert_eq!(
        exponential
            .matches("emit_number_decimal_format_payload(")
            .count(),
        2
    );
    assert_eq!(
        precision
            .matches("emit_number_decimal_format_payload(")
            .count(),
        1
    );
    assert_eq!(
        OPERATIONS_SOURCE
            .matches("emit_number_decimal_format_payload(")
            .count(),
        4,
        "four calls owned by exactly three wrappers"
    );
    assert_eq!(
        DECIMAL_FORMAT_SOURCE
            .matches("fn emit_number_decimal_format_payload(")
            .count(),
        1
    );
}

#[test]
fn decimal_formatting_has_no_value_table_empty_sentinel_or_magic_integer_case() {
    for source in [OPERATIONS_SOURCE, DECIMAL_FORMAT_SOURCE, DATA_SOURCE] {
        assert!(!source.contains("NUMBER_TO_PRECISION_CASES"));
        assert!(!source.contains("emit_number_precision_case"));
        assert!(!source.contains("1000000000000000128.0"));
    }
}

#[test]
fn decimal_fixture_covers_each_format_and_keeps_the_existing_regression() {
    assert_eq!(
        NUMERICS_CLI_TESTS
            .matches("fn run_wasm_backend_formats_dynamic_numbers_with_decimal_rounding()")
            .count(),
        1
    );
    assert_eq!(
        NUMERICS_CLI_TESTS
            .matches("fixture_path(\"wasm_number_decimal_formatting.js\")")
            .count(),
        1
    );
    for witness in [
        "function checkFixed(",
        "function checkExponential(",
        "function checkPrecision(",
        "exact halfway rounds upward",
        "rounding carries into the integer part",
        "negative zero suppresses its sign",
        "large-value shortest threshold",
        "not-a-number spelling",
        "positive infinity spelling",
        "negative infinity spelling",
        "exact integer digits differ from shortest spelling",
        "rounding carries into the exponent",
        "exact binary64 digits beyond shortest spelling",
        "minimum subnormal",
        "maximum finite value",
        "omitted fraction digits use shortest mantissa",
        "upper scientific threshold",
        "lower scientific threshold",
        "lower fixed threshold",
        "carry selects fixed notation after rounding",
        "rounding changes notation",
        "exact binary64 significant digits beyond shortest spelling",
        "omitted precision uses shortest spelling",
        "number-decimal-formatting:ok",
    ] {
        assert!(
            DECIMAL_FORMATTING_FIXTURE.contains(witness),
            "missing decimal-format witness `{witness}`"
        );
    }

    assert_eq!(
        NUMERICS_CLI_TESTS
            .matches("fn run_wasm_backend_succeeds_for_number_builtin_family_fixture()")
            .count(),
        1
    );
    assert_eq!(
        NUMERICS_CLI_TESTS
            .matches("fixture_path(\"wasm_number_builtin_family.js\")")
            .count(),
        1
    );
    for method in [".toFixed(", ".toExponential(", ".toPrecision("] {
        assert!(NUMBER_BUILTIN_FIXTURE.contains(method));
    }
}
