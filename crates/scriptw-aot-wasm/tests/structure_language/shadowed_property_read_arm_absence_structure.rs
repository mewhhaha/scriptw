const OBJECTS_SOURCE: &str = include_str!("../../src/objects.rs");
const LIB_SOURCE: &str = include_str!("../../src/lib.rs");
const OPERATIONS_SOURCE: &str = include_str!("../../src/operations.rs");
const GC_LAYOUTS: &str = include_str!("../../src/gc_types/layouts.rs");
const FUNCTIONS_SOURCE: &str = include_str!("../../src/functions.rs");
const HOST_BUILTINS_SOURCE: &str = include_str!("../../src/builtins/host.rs");
const READ_HELPER_SOURCE: &str = include_str!("../../src/emit/runtime_operations.rs");
fn property_read_source() -> &'static str {
    let start = OBJECTS_SOURCE
        .find("    pub(crate) fn compile_property_read_from_locals(")
        .expect("property-read entry");
    let end = OBJECTS_SOURCE[start..]
        .find("    pub(crate) fn emit_dynamic_property_read_with_key_locals(")
        .map(|offset| start + offset)
        .expect("dynamic property-read entry");
    &OBJECTS_SOURCE[start..end]
}

#[test]
fn property_read_dispatch_has_one_whole_value_route() {
    let source = property_read_source();
    assert_eq!(
        source
            .matches("crate::runtime_helpers::DynamicPropertyReadArguments::new(")
            .count(),
        1
    );
    assert!(!source.contains("ValueKind::"));
    assert!(source.contains("receiver: &ValueLocals,"));
    let key = source.find("self.compile_property_key_operand(").unwrap();
    let lookup = source
        .find("crate::runtime_helpers::DynamicPropertyReadArguments::new(")
        .unwrap();
    assert!(key < lookup);
    assert!(!source.contains("self.emit_value_to_property_key_completion("));
    let helper_start = READ_HELPER_SOURCE
        .find("pub(super) fn compile_dynamic_property_read_helper(")
        .unwrap();
    let helper_end = READ_HELPER_SOURCE[helper_start..]
        .find("pub(super) fn compile_proxy_call_helper(")
        .unwrap()
        + helper_start;
    let helper = &READ_HELPER_SOURCE[helper_start..helper_end];
    let coercible = helper.find("self.compile_nullish_tagged_i32(").unwrap();
    let convert = helper
        .find("self.emit_value_to_property_key_locals_or_branch(")
        .unwrap();
    let lookup = helper
        .find("self.emit_dynamic_property_read_with_key_locals(")
        .unwrap();
    assert!(coercible < convert && convert < lookup);
    assert!(!helper.contains("self.emit_value_to_property_key_locals("));
    assert!(!helper.contains("self.emit_value_to_property_key_completion("));
}

#[test]
fn private_types_and_imports_have_direct_owners() {
    for source in [LIB_SOURCE, FUNCTIONS_SOURCE, HOST_BUILTINS_SOURCE] {
        assert!(!source.contains("RealmRecordLocal"));
    }
    assert!(GC_LAYOUTS.contains("struct RealmRecord => RealmRecordSchema {"));
    assert!(HOST_BUILTINS_SOURCE.contains("use crate::gc_types::*;"));
    assert!(OPERATIONS_SOURCE.contains("use scriptw_ir::NativeErrorKind;"));
    assert!(OPERATIONS_SOURCE.contains("use scriptw_ir::StaticRegExpCompilation;"));
    assert!(OPERATIONS_SOURCE.contains("NativeErrorKind::TypeError"));
}
