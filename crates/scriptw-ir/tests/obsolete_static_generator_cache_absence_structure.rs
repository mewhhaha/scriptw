const LOWERING_SOURCE: &str = include_str!("../src/lowering.rs");
const SIGNATURE_EVIDENCE_SOURCE: &str = include_str!("../src/lowering/signature_evidence.rs");
const VAR_DECLARATION_SOURCE: &str = include_str!("../src/lowering/var_declaration.rs");
const FUNCTION_DECLARATION_SOURCE: &str = include_str!("../src/lowering/function_declaration.rs");
const LEXICAL_DECLARATION_SOURCE: &str = include_str!("../src/lowering/lexical_declaration.rs");
const ASSIGNMENT_SOURCE: &str = include_str!("../src/lowering/assignment.rs");
const CALL_EXPRESSION_SOURCE: &str = include_str!("../src/lowering/call_expression.rs");
const FOR_OF_SOURCE: &str = include_str!("../src/lowering/for_of.rs");
#[test]
fn write_never_static_generator_cache_surface_is_absent() {
    let product_sources = [
        LOWERING_SOURCE,
        SIGNATURE_EVIDENCE_SOURCE,
        VAR_DECLARATION_SOURCE,
        FUNCTION_DECLARATION_SOURCE,
        LEXICAL_DECLARATION_SOURCE,
        ASSIGNMENT_SOURCE,
        CALL_EXPRESSION_SOURCE,
        FOR_OF_SOURCE,
    ];

    for name in [
        "static_generator_sum_values",
        "static_generator_element_values",
        "prepare_static_generator_declarations",
        "is_static_generator_declaration",
        "static_generator_call_values",
        "static_generator_call_values_owned",
        "static_generator_call_elements_owned",
        "static_generator_call_name",
        "static_generator_call_is_known",
        "array_iterator_from_static_generator_values",
        "array_iterator_from_lowered_elements",
        "static_generator_call_overrides",
        "static_iterator_binding_values",
        "static_generator_declaration_values",
        "static_generator_declaration_values_by_name",
        "static_object_iterator_literal_values",
        "static_object_iterator_iife_values",
        "static_iterator_values_expr",
        "array_literal_from_static_generator_values",
        "lower_generator_iife_as_array",
        "lower_static_iterator_from_wrapper_method_call",
    ] {
        assert!(
            product_sources.iter().all(|source| !source.contains(name)),
            "`{name}`"
        );
    }
}

#[test]
fn ordinary_call_assignment_and_generator_owners_remain() {
    assert!(LOWERING_SOURCE.contains("fn lower_generator_expression("));
    assert!(LOWERING_SOURCE.contains("fn lower_indirect_method_call("));
    assert!(CALL_EXPRESSION_SOURCE.contains("self.lower_property_target(access.target())"));
    assert!(ASSIGNMENT_SOURCE.contains("let value = self.lower_expression(rhs);"));
    assert!(FOR_OF_SOURCE.contains("let element_info = ValueInfo {"));
}
