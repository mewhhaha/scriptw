const LOWERING_SOURCE: &str = include_str!("../src/lowering.rs");
const CONDITIONAL_FLOW_SOURCE: &str = include_str!("../src/lowering/conditional_flow.rs");
const SIGNATURE_EVIDENCE_SOURCE: &str = include_str!("../src/lowering/signature_evidence.rs");
const VAR_DECLARATION_SOURCE: &str = include_str!("../src/lowering/var_declaration.rs");
const FUNCTION_DECLARATION_SOURCE: &str = include_str!("../src/lowering/function_declaration.rs");
const LEXICAL_DECLARATION_SOURCE: &str = include_str!("../src/lowering/lexical_declaration.rs");
const LOWERING_HELPERS_SOURCE: &str = include_str!("../src/lowering_helpers.rs");
const LIB_SOURCE: &str = include_str!("../src/lib.rs");
const REGEXP_SOURCE: &str = include_str!("../src/regexp.rs");
const CLASS_DEFINITION_SOURCE: &str = include_str!("../src/lowering/class_definition.rs");
fn generated_function_output_source() -> &'static str {
    let start = LOWERING_SOURCE
        .find("pub(crate) struct GeneratedFunctionOutput {")
        .expect("generated-function output declaration");
    let end = LOWERING_SOURCE[start..]
        .find("\n}")
        .map(|offset| start + offset + 2)
        .expect("generated-function output end");
    &LOWERING_SOURCE[start..end]
}

#[test]
fn disconnected_lowering_specializations_are_absent() {
    for name in [
        "target_has_private_brand",
        "lower_generated_iterator_function_expression",
        "lower_generated_iterator_function",
        "lower_this_range_generator_function_body",
        "single_lexical_number_binding",
        "single_lexical_expression_binding",
        "expression_is_this_unsigned_right_shift_zero",
        "while_body_yields_and_increments",
        "alloc_generated_iterator_values_name",
        "lower_generator_body_as_array_iterator",
        "lower_yield_star_generator_iife",
        "delegate_method_returns_non_object",
        "static_generator_declaration_elements",
        "static_generator_statement_list_elements",
        "static_generator_yield_string_element",
        "static_generator_for_loop_string_elements",
        "static_generator_string_for_loop_initializer",
        "static_generator_string_for_loop_body",
        "static_string_from_char_code_yield_name",
        "static_generator_yield_identifier_name",
        "static_string_from_char_code_arg_is_named",
        "static_string_from_char_code_arg_name",
        "static_negated_string_match_regex",
        "static_string_from_char_code_value",
        "static_generator_declaration_elements_by_name",
        "merge_operand_shapes",
        "lower_static_yield_star_generator_method_call",
        "static_generator_declaration_values",
        "static_generator_for_loop_condition",
    ] {
        assert!(
            [
                LOWERING_SOURCE,
                SIGNATURE_EVIDENCE_SOURCE,
                VAR_DECLARATION_SOURCE,
                FUNCTION_DECLARATION_SOURCE,
                LEXICAL_DECLARATION_SOURCE,
                CONDITIONAL_FLOW_SOURCE,
            ]
            .iter()
            .all(|source| !source.contains(name)),
            "`{name}`"
        );
    }
    assert!(!LOWERING_HELPERS_SOURCE.contains("StaticStringGeneratorLoopBody"));
    assert!(!LIB_SOURCE.contains("use regress::Regex;"));
}

#[test]
fn live_lowering_authorities_and_output_fields_remain() {
    for name in ["lower_generator_expression", "lower_generated_ast_function"] {
        assert!(LOWERING_SOURCE.contains(name), "`{name}`");
    }
    assert!(LOWERING_SOURCE.contains("mod conditional_flow;"));
    assert!(CONDITIONAL_FLOW_SOURCE.contains("fn merge_heap_shapes("));
    assert!(CONDITIONAL_FLOW_SOURCE.contains("self.merge_heap_shapes("));
    assert!(!LOWERING_SOURCE.contains("fn merge_heap_shapes("));

    let fields = generated_function_output_source()
        .lines()
        .filter(|line| line.trim_start().starts_with("pub(crate) "))
        .collect::<Vec<_>>();
    assert_eq!(
        fields,
        [
            "pub(crate) struct GeneratedFunctionOutput {",
            "    pub(crate) return_info: ValueInfo,",
            "    pub(crate) construct_this_info: Option<ValueInfo>,",
        ]
    );
    assert!(CLASS_DEFINITION_SOURCE.contains(".return_info"));
    assert!(CLASS_DEFINITION_SOURCE.contains("output.construct_this_info"));
    assert!(REGEXP_SOURCE.contains("use regress::{"));
}
