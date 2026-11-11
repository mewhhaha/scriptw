const PLANNING_SOURCE: &str = include_str!("../../src/planning.rs");
const DATA_SOURCE: &str = include_str!("../../src/data.rs");
const MODULE_SOURCE: &str = include_str!("../../src/module.rs");
#[test]
fn unreachable_planning_analysis_is_absent() {
    for name in [
        "is_large_deferred_standard_builtin",
        "script_uses_env",
        "script_uses_calls",
        "script_uses_function_heap",
        "script_uses_function_table",
        "block_uses_function_table",
        "block_uses_calls",
        "statement_uses_calls",
        "for_init_uses_calls",
        "statement_uses_function_table",
        "for_init_uses_function_table",
        "expr_uses_function_table",
        "expr_uses_calls",
    ] {
        assert!(!PLANNING_SOURCE.contains(name), "`{name}`");
    }
}

#[test]
fn live_planning_and_ir_authorities_remain() {
    for declaration in [
        "pub(crate) struct RuntimeBootstrapPlan {",
        "pub(crate) fn collect_hoisted_vars_block_root(",
        "pub(crate) fn block_references_function(",
        "fn require_standard_builtin(&mut self, builtin: StandardBuiltinId)",
        "pub(crate) fn build_function_metas<",
        "pub(crate) fn values(&self)",
        "pub(crate) fn metas(&self)",
    ] {
        assert!(PLANNING_SOURCE.contains(declaration), "`{declaration}`");
    }
    for retired in [
        "fn count_param_binding_locals(",
        "fn count_block_lexicals(",
        "fn should_stub_standard_builtin(",
    ] {
        assert!(
            !PLANNING_SOURCE.contains(retired),
            "retired raw budget/stub entry {retired}"
        );
    }
    assert!(!PLANNING_SOURCE.contains("super_constructor_target"));
    assert!(!PLANNING_SOURCE.contains("pub(crate) fn iter(&self)"));
    assert!(MODULE_SOURCE.contains("pub(crate) fn is_typed_array_constructor("));
    assert!(DATA_SOURCE.contains(
        "function.super_constructor_target.as_deref() == Some(BUILTIN_REGEXP_FUNCTION_ID)"
    ));
}
