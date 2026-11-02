const FUNCTIONS_SOURCE: &str = include_str!("../../src/functions.rs");
const FUNCTION_ALLOCATION_SOURCE: &str =
    include_str!("../../src/functions/function_object_allocation.rs");
const BOOTSTRAP_SOURCE: &str = include_str!("../../src/builtins/bootstrap.rs");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/function-prototype-materialization.md");

fn normalized(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn materialization_policy_is_the_exact_non_capability_domain() {
    let declaration_marker = "pub(crate) enum FunctionPrototypeMaterialization {";
    let declaration_offset = FUNCTIONS_SOURCE
        .find(declaration_marker)
        .expect("materialization-policy declaration");
    let preceding_item_end = FUNCTIONS_SOURCE[..declaration_offset]
        .rfind('}')
        .expect("item before materialization-policy declaration");
    let declaration_end = FUNCTIONS_SOURCE[declaration_offset..]
        .find("\n}")
        .map(|offset| declaration_offset + offset + 2)
        .expect("materialization-policy declaration end");
    let declaration_region = &FUNCTIONS_SOURCE[preceding_item_end + 1..declaration_end];
    let expected_declaration = r#"

/// Whether function allocation also creates the default own `prototype`
/// property. This policy is deliberately separate from semantic
/// constructability; realm bootstrap supplies a few intrinsic prototypes.
pub(crate) enum FunctionPrototypeMaterialization {
    Automatic,
    BootstrapSupplied,
}
"#;
    assert_eq!(
        normalized(declaration_region),
        normalized(expected_declaration),
        "the exact adjacent declaration must remain attribute-free"
    );

    for forbidden in [
        "impl FunctionPrototypeMaterialization",
        "for FunctionPrototypeMaterialization",
    ] {
        assert!(
            !FUNCTIONS_SOURCE.contains(forbidden) && !BOOTSTRAP_SOURCE.contains(forbidden),
            "found `{forbidden}`"
        );
    }
}

#[test]
fn contract_and_t09_record_the_exhaustive_source_equivalence() {
    for marker in [
        "seven producer sites",
        "exhaustive two-arm projection",
        "changes no emitted instruction",
    ] {
        assert!(
            CONTRACT.contains(marker),
            "missing contract marker `{marker}`"
        );
    }
    assert!(CONTRACT.contains("FunctionPrototypeMaterialization::{Automatic, BootstrapSupplied}"));
}

#[test]
fn function_prototype_callable_is_completed_from_its_birth_layout() {
    let allocation = FUNCTION_ALLOCATION_SOURCE
        .split_once("pub(super) fn emit_complete_function_record(")
        .expect("complete function record")
        .1
        .split_once("let code = schema")
        .expect("function record code allocation")
        .0;
    assert!(
        allocation.contains("meta.standard_builtin == Some(StandardBuiltinId::FunctionPrototype)")
    );
    assert!(allocation.contains("IntrinsicPropertyLayoutId::FunctionPrototype"));
    assert!(allocation.contains("layout.validate_native_metadata(meta)?"));
    assert!(allocation.contains("self.emit_alloc_intrinsic_layout_header("));
    assert!(!allocation.contains("emit_install_intrinsic_"));

    let shell = BOOTSTRAP_SOURCE
        .split_once("fn emit_builtin_constructor_shell_with_parent(")
        .expect("canonical constructor shell")
        .1
        .split_once("fn emit_bootstrap_namespace(")
        .expect("canonical constructor shell end")
        .0;
    assert!(shell.contains("IntrinsicPropertyLayoutId::for_constructor(&meta)?.is_some()"));
    assert!(shell.contains("if !covered {\n                self.emit_install_intrinsic_data("));
    assert!(shell.contains("FunctionObjectSchema::PUBLIC_PROTOTYPE_CACHE"));
    assert!(shell.contains("emit_store_non_array_realm_intrinsic("));
}
