//! These source witnesses protect connected compiler ownership, not conformance.
const BODY: &str = include_str!("../../src/builtins/intl_supported_values.rs");
const POOL: &str = include_str!("../../src/data/intl_supported_values.rs");
const DATA: &str = include_str!("../../src/data.rs");
const EMIT: &str = include_str!("../../src/emit/module_assembly.rs");
const INTL_EMIT: &str = include_str!("../../src/emit/module_assembly/intl.rs");
const EMISSION: &str = include_str!("../../src/emit.rs");
const NAMES: &str = include_str!("../../../scriptw-ir/src/names.rs");
const SHAPE: &str = include_str!("../../../scriptw-ir/src/lowering/builtin_shapes.rs");
const BOOTSTRAP: &str = include_str!("../../src/builtins/bootstrap.rs");
const PLANNING: &str = include_str!("../../src/planning.rs");
fn ordered(source: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        cursor += source[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing ordered step {marker}"))
            + marker.len();
    }
}
#[test]
fn enumeration_coerces_once_before_closed_dispatch_and_called_realm_allocation() {
    ordered(
        BODY,
        &[
            "emit_builtin_arg_to_value(0",
            "emit_intl_number_to_string(&argument",
            "for selected in SupportedValuesKey::ALL",
            "intl_supported_values_table(selected)?",
            "emit_string_payload_equality_i32",
            "emit_array_from_argument_list",
            "emit_intl_number_range_error",
        ],
    );
    assert_eq!(BODY.matches("emit_intl_number_to_string(").count(), 1);
    assert!(!BODY.contains("emit_intl_provider_call"));
}
#[test]
fn primitive_tables_check_producer_identity_before_host_image_emission() {
    for key in [
        "Calendar",
        "Collation",
        "Currency",
        "NumberingSystem",
        "TimeZone",
        "Unit",
    ] {
        assert!(POOL.contains(&format!("SupportedValuesKey::{key}")));
    }
    ordered(
        POOL,
        &[
            "selection",
            ".compilation_data()",
            "selected.identity().artifact_identity()",
            ".supported_values(key)",
            "source.provider_identity().artifact_identity().as_bytes() != identity",
            "source.values()",
            "pool.intern_string",
            "CompiledSupportedValuesTable",
        ],
    );
    assert!(POOL.contains("Available(Arc<SupportedValuesList>)"));
    assert!(POOL.contains("ProviderIdentityMismatch"));
    assert!(POOL.contains("self.checked_intl_supported_values()?"));
    assert!(DATA.contains("pool.collect_intl_supported_values(intl_selection)"));
    let emission = EMISSION
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    ordered(
        &emission,
        &[
            "letintl_selection=inputs.intl_selection(intl_profile)?",
            "emit_script(script,promise_rejection_policy,&intl_selection,snapshot,inputs,)",
            "fnemit_script(",
            "runtime_artifact_with_inputs(intl_selection,inputs)",
            "module_assembly::emit_script_module(",
            "intl_selection,",
        ],
    );
    ordered(
        INTL_EMIT,
        &[
            "string_pool.check_intl_supported_values()?",
            "if kind.owns_globals() && (uses_intl_host || uses_system_time_zone || uses_intl_catalogue) {",
            "intl_selection.compilation_data()",
            "selected.identity().artifact_identity()",
            "INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION",
            "for (name, data) in selected.component_sections()",
        ],
    );
    // The assembly caller passes the same checked pool and selection into the
    // frame owner, which checks catalogue admission before publishing identity
    // or component sections. Lazy data selection does not bypass that boundary.
    ordered(
        EMIT,
        &[
            "intl::append_sections(",
            "&mut module,",
            "kind,",
            "&string_pool,",
            "intl_selection,",
        ],
    );
    assert!(!INTL_EMIT.contains("embedded_intl_data_identity"));
    assert!(!POOL.contains("embedded_supported_values"));
    assert!(!INTL_EMIT.contains(
        "uses_intl_host || uses_system_time_zone || supported_values_identity.is_some()"
    ));
    assert!(!POOL.contains("pub(crate) fn new"));
}
#[test]
fn namespace_methods_share_shape_entry_and_created_realm_roots() {
    assert!(NAMES.contains("pub const INTL_NAMESPACE_METHODS:"));
    assert!(NAMES.contains("StandardBuiltinId::IntlSupportedValuesOf"));
    assert!(SHAPE.contains("for (name, builtin) in INTL_NAMESPACE_METHODS"));
    assert!(BOOTSTRAP.contains("members.methods_in_installation_order()"));
    assert!(BOOTSTRAP.contains("members.in_installation_order()"));
    assert!(PLANNING.contains("while method < INTL_NAMESPACE_METHODS.len()"));
}
