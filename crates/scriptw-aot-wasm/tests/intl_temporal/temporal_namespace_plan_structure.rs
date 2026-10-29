const NAMES_SOURCE: &str = include_str!("../../../scriptw-ir/src/names.rs");
const SHAPES_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering/builtin_shapes.rs");
const PLANNING_SOURCE: &str = include_str!("../../src/planning.rs");
const TEMPORAL_PLAN_SOURCE: &str = include_str!("../../src/planning/temporal_namespace.rs");
const BOOTSTRAP_SOURCE: &str = include_str!("../../src/builtins/bootstrap.rs");
const TEMPORAL_INITIALIZATION_SOURCE: &str =
    include_str!("../../src/builtins/bootstrap/temporal_initialization.rs");
const ENGINE_TEST_SOURCE: &str = include_str!("../../../scriptw-engine/src/tests/intl.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end after {start}: {end}"))
        .0
}

fn ordered(source: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let next = source[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing marker after byte {cursor}: {marker}"));
        cursor += next + marker.len();
    }
}

#[test]
fn temporal_namespace_shapes_and_installers_share_two_ordered_member_lists() {
    let constructors = bounded(
        NAMES_SOURCE,
        "pub const TEMPORAL_NAMESPACE_CONSTRUCTORS:",
        "pub const TEMPORAL_NOW_NAMESPACE_MEMBERS:",
    );
    assert_eq!(
        constructors.matches("StandardBuiltinId::Temporal").count(),
        8
    );
    for name in [
        "TEMPORAL_INSTANT_NAME",
        "TEMPORAL_PLAIN_DATE_NAME",
        "TEMPORAL_ZONED_DATE_TIME_NAME",
        "TEMPORAL_PLAIN_TIME_NAME",
        "TEMPORAL_PLAIN_DATE_TIME_NAME",
        "TEMPORAL_PLAIN_YEAR_MONTH_NAME",
        "TEMPORAL_PLAIN_MONTH_DAY_NAME",
        "TEMPORAL_DURATION_NAME",
    ] {
        assert_eq!(
            constructors.matches(name).count(),
            1,
            "constructor `{name}`"
        );
    }

    let now_members = bounded(
        NAMES_SOURCE,
        "pub const TEMPORAL_NOW_NAMESPACE_MEMBERS:",
        "pub const TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_METHODS:",
    );
    assert_eq!(
        now_members
            .matches("StandardBuiltinId::TemporalNow")
            .count(),
        6
    );
    ordered(
        now_members,
        &[
            "timeZoneId",
            "TemporalNowTimeZoneId",
            "instant",
            "zonedDateTimeISO",
            "plainDateTimeISO",
            "plainDateISO",
            "plainTimeISO",
        ],
    );

    let now_shape = bounded(
        SHAPES_SOURCE,
        "pub(super) fn temporal_now_object_value_info()",
        "pub(super) fn temporal_object_value_info()",
    );
    assert_eq!(
        now_shape.matches("TEMPORAL_NOW_NAMESPACE_MEMBERS").count(),
        1
    );
    assert!(!now_shape.contains("StandardBuiltinId::TemporalNow"));

    let temporal_shape = bounded(
        SHAPES_SOURCE,
        "pub(super) fn temporal_object_value_info()",
        "pub(super) fn intl_object_value_info()",
    );
    assert_eq!(
        temporal_shape
            .matches("TEMPORAL_NAMESPACE_CONSTRUCTORS")
            .count(),
        1
    );
    assert!(!temporal_shape.contains("StandardBuiltinId::TemporalInstantConstructor"));

    assert_eq!(
        TEMPORAL_INITIALIZATION_SOURCE
            .matches("members.now_members_in_installation_order()")
            .count(),
        1
    );
    assert_eq!(
        TEMPORAL_INITIALIZATION_SOURCE
            .matches("members.constructors_in_installation_order()")
            .count(),
        1
    );
}

#[test]
fn temporal_namespace_plan_can_publish_members_only_after_complete_rooting() {
    assert_eq!(
        PLANNING_SOURCE
            .matches("\nmod temporal_namespace;\n")
            .count(),
        1
    );
    assert!(!PLANNING_SOURCE.contains("\npub mod temporal_namespace;\n"));
    assert!(!PLANNING_SOURCE.contains("\nmod temporal_namespace {\n"));
    assert_eq!(
        PLANNING_SOURCE.matches("TemporalNamespaceMembers").count(),
        2
    );
    assert_eq!(PLANNING_SOURCE.matches("TemporalNamespacePlan").count(), 3);
    assert!(PLANNING_SOURCE.contains("pub(crate) use temporal_namespace::{"));
    assert_eq!(
        PLANNING_SOURCE
            .matches("temporal: TemporalNamespacePlan,")
            .count(),
        1
    );
    assert!(!PLANNING_SOURCE.contains("temporal_object: bool"));
    assert!(TEMPORAL_PLAN_SOURCE.starts_with("use super::*;\n\n"));

    for declaration in [
        "pub(crate) struct TemporalRootsSeeded(());",
        "pub(crate) enum TemporalNamespacePlan {",
        "pub(crate) struct TemporalNamespaceMembers {",
    ] {
        assert_eq!(TEMPORAL_PLAN_SOURCE.matches(declaration).count(), 1);
        assert!(!PLANNING_SOURCE.contains(declaration));
    }
    assert!(!TEMPORAL_PLAN_SOURCE.contains("pub(crate) struct TemporalRootsSeeded(pub"));
    assert!(!TEMPORAL_PLAN_SOURCE.contains("struct TemporalRooting"));

    let root = bounded(
        TEMPORAL_PLAN_SOURCE,
        "pub(crate) fn root(plan: &mut RuntimeBootstrapPlan)",
        "pub(crate) fn members(",
    );
    ordered(
        root,
        &[
            "plan.temporal = Self::Rooting;",
            "TEMPORAL_NAMESPACE_CONSTRUCTORS",
            ".chain(TEMPORAL_NOW_NAMESPACE_MEMBERS)",
            "plan.require_standard_builtin(*builtin);",
            "plan.temporal = Self::Rooted(TemporalRootsSeeded(()));",
        ],
    );

    let members = bounded(
        TEMPORAL_PLAN_SOURCE,
        "pub(crate) fn members(",
        "pub(crate) struct TemporalNamespaceMembers {",
    );
    assert!(members.contains("full_standard_globals || matches!(self, Self::Rooted(_))"));
    assert!(!members.contains("Self::Rooting"));

    assert!(!PLANNING_SOURCE.contains(
        "#[ignore = \"known planning gap: bare `Temporal` roots only the namespace shell\"]"
    ));
    let full = bounded(
        PLANNING_SOURCE,
        "pub(crate) fn full() -> Self {",
        "pub(crate) fn should_initialize_standard_builtin(",
    );
    ordered(
        full,
        &[
            "plan.full_standard_globals = true;",
            "StandardBuiltinId::all_functions()",
            "plan.require_standard_builtin(*builtin);",
        ],
    );
    let assembly = include_str!("../../src/emit/module_assembly.rs")
        .split_whitespace()
        .collect::<String>();
    assert!(assembly.contains(
        "letruntime_bootstrap_plan=ifuses_heap{RuntimeBootstrapPlan::full()}else{RuntimeBootstrapPlan::default()};"
    ));
}

#[test]
fn temporal_bootstrap_requires_the_witness_and_installs_without_partial_guards() {
    let now_installer = bounded(
        TEMPORAL_INITIALIZATION_SOURCE,
        "fn init_temporal_now_object(",
        "fn init_building_temporal_object(",
    );
    assert!(now_installer.contains("members: TemporalNamespaceMembers,"));
    assert!(now_installer.contains("members.now_members_in_installation_order()"));
    assert!(!now_installer.contains("should_initialize_standard_builtin"));
    assert!(!now_installer.contains("continue;"));

    let temporal_installer = bounded(
        TEMPORAL_INITIALIZATION_SOURCE,
        "fn init_building_temporal_object(",
        "\n    }\n}",
    );
    assert!(temporal_installer.contains("building: &TemporalIntrinsicBuild<'_>,"));
    assert!(temporal_installer.contains("members: TemporalNamespaceMembers,"));
    ordered(
        temporal_installer,
        &[
            "self.init_temporal_now_object(members,",
            "IntrinsicKey::Name(TEMPORAL_NOW_NAME)",
            "members.constructors_in_installation_order()",
            "self.emit_load_building_temporal_intrinsic(building, slot,",
        ],
    );
    assert!(!temporal_installer.contains("should_initialize_standard_builtin"));
    assert!(!temporal_installer.contains("continue;"));

    let demand_gate = bounded(
        TEMPORAL_INITIALIZATION_SOURCE,
        "fn emit_build_temporal_cluster(",
        "fn init_temporal_now_object(",
    );
    ordered(
        demand_gate,
        &[
            ".temporal_namespace_members()",
            "self.init_building_temporal_object(",
            "building,",
            "members,",
            "&context,",
            "&object_prototype,",
        ],
    );
    assert!(!demand_gate.contains("runtime_bootstrap_plan.temporal_object"));
    assert!(!demand_gate.contains("should_initialize_standard_builtin"));
    assert!(!demand_gate.contains("continue;"));

    let initial_bootstrap = bounded(
        BOOTSTRAP_SOURCE,
        "fn emit_initialize_realm_intrinsics_inner(",
        "pub(crate) fn init_runtime_roots(",
    );
    assert!(!initial_bootstrap.contains("temporal_namespace_members()"));
    assert!(!initial_bootstrap.contains("init_building_temporal_object"));
    assert!(!initial_bootstrap.contains("RealmEnsureTemporal"));

    assert_eq!(
        ENGINE_TEST_SOURCE
            .matches("fn wasm_backend_exposes_complete_temporal_namespace_from_a_bare_reference()",)
            .count(),
        1
    );
    let runtime_witness = bounded(
        ENGINE_TEST_SOURCE,
        "fn wasm_backend_exposes_complete_temporal_namespace_from_a_bare_reference()",
        "fn wasm_backend_reads_the_wall_clock_through_temporal_now()",
    );
    assert!(runtime_witness.contains("var namespace = Temporal;"));
    assert!(runtime_witness.contains("Object.getOwnPropertyDescriptor(namespace, constructorName)"));
    assert!(runtime_witness.contains("Object.getOwnPropertyDescriptor(namespace, \"Now\")"));
}
