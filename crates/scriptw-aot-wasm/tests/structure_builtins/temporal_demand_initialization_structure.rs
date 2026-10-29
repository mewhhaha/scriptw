use std::collections::BTreeSet;

const BOOTSTRAP: &str = include_str!("../../src/builtins/bootstrap.rs");
const TEMPORAL: &str = include_str!("../../src/builtins/bootstrap/temporal_initialization.rs");
const FUNCTIONS: &str = include_str!("../../src/functions.rs");
const LOADER: &str = include_str!("../../src/functions/realm_intrinsic.rs");
const FAMILIES: &str = include_str!("../../src/intrinsics/temporal/members.rs");
const INTRINSICS: &str = include_str!("../../src/intrinsics/mod.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing {end}"))
        .0
}

fn names_after(source: &str, prefix: &str) -> BTreeSet<String> {
    source
        .split(prefix)
        .skip(1)
        .map(|tail| {
            tail.chars()
                .take_while(|character| character.is_ascii_alphanumeric())
                .collect()
        })
        .collect()
}

fn ordered(source: &str, markers: &[&str]) {
    let mut remaining = source;
    for marker in markers {
        remaining = remaining
            .split_once(marker)
            .unwrap_or_else(|| panic!("missing or reordered {marker}"))
            .1;
    }
}

#[test]
fn initial_realm_bootstrap_excludes_the_complete_temporal_allocation_graph() {
    let initial = bounded(
        BOOTSTRAP,
        "    fn emit_initialize_realm_intrinsics_inner(",
        "    pub(crate) fn init_runtime_roots(",
    );
    for forbidden in [
        "TemporalIntrinsicFamily",
        "StandardBuiltinId::Temporal",
        "init_temporal",
        "init_building_temporal",
        "emit_build_temporal_cluster",
        "RealmEnsureTemporal",
        "temporal_namespace_members",
    ] {
        assert!(
            !initial.contains(forbidden),
            "eager Temporal path: {forbidden}"
        );
    }
    assert!(TEMPORAL.contains("for family in TemporalIntrinsicFamily::ALL"));
    assert!(TEMPORAL.contains("family.constructor()"));
    assert!(TEMPORAL.contains("members.now_members_in_installation_order()"));
    assert!(TEMPORAL.contains("members.constructors_in_installation_order()"));
}

#[test]
fn every_temporal_family_identity_crosses_the_external_ensure_boundary() {
    let slots = bounded(
        FUNCTIONS,
        "pub(crate) enum NonArrayRealmIntrinsicSlot {",
        "\n}",
    );
    let all_temporal_slots: BTreeSet<_> = slots
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("Temporal"))
        .map(|name| name.trim_end_matches(',').to_owned())
        .collect();
    let classification = bounded(
        FUNCTIONS,
        "pub(crate) const fn is_temporal(self) -> bool {",
        "pub(crate) const fn typed_array_constructor_identity",
    );
    assert_eq!(
        names_after(classification, "Self::Temporal"),
        all_temporal_slots
    );
    let all = bounded(
        FAMILIES,
        "pub(crate) const ALL:",
        "pub(crate) const fn constructor",
    );
    for family in names_after(all, "Self::") {
        for suffix in ["Prototype", "Constructor"] {
            assert!(
                all_temporal_slots.contains(&format!("{family}{suffix}")),
                "{family} {suffix} bypasses Temporal ensure"
            );
        }
    }
    let external = bounded(
        LOADER,
        "pub(crate) fn emit_load_non_array_realm_intrinsic(",
        "    /// Only the Temporal cluster builder",
    );
    ordered(
        external,
        &[
            "if slot.is_temporal()",
            "RealmEnsureTemporalArguments::new(realm)",
            "self.emit_load_realm_intrinsic_at_index",
        ],
    );
    let installation = bounded(
        INTRINSICS,
        "pub(crate) fn emit_intrinsic_callable(",
        "pub(crate) fn emit_intrinsic_canonical_host_callable(",
    );
    ordered(
        installation,
        &[
            "if slot.is_temporal()",
            "self.emit_load_non_array_realm_intrinsic",
            "self.emit_load_intrinsic_for_installation",
        ],
    );
}

#[test]
fn temporal_build_reads_require_the_private_defining_realm_capability() {
    assert!(TEMPORAL
        .contains("pub(crate) struct TemporalIntrinsicBuild<'a>(&'a GcLocal<RealmRecord>);"));
    assert_eq!(
        TEMPORAL
            .matches("TemporalIntrinsicBuild(&parameters.realm)")
            .count(),
        1
    );
    let raw = bounded(
        LOADER,
        "pub(crate) fn emit_load_building_temporal_intrinsic(",
        "    fn emit_load_realm_intrinsic_at_index(",
    );
    assert!(raw.contains("building: &TemporalIntrinsicBuild<'_>"));
    assert!(raw.contains("building.realm()"));
    assert!(!raw.contains("realm: &GcLocal<RealmRecord>"));
    let build = bounded(
        TEMPORAL,
        "    fn emit_build_temporal_cluster(",
        "    fn init_temporal_now_object(",
    );
    for required in [
        "let realm = building.realm()",
        "NonArrayRealmIntrinsicSlot::ObjectPrototype",
        "emit_realm_function_materialization_context_from_realm(realm, function)",
        "self.emit_load_building_temporal_intrinsic(",
    ] {
        assert!(
            build.contains(required),
            "missing canonical owner: {required}"
        );
    }
    for forbidden in [
        "load_current_realm",
        "current_environment",
        "should_initialize_standard_builtin",
        "emit_indirect_call",
        "emit_dynamic_property_read",
    ] {
        assert!(
            !build.contains(forbidden),
            "unsafe building path: {forbidden}"
        );
    }
}

#[test]
fn canonical_namespace_publishes_only_after_complete_build_and_never_retries() {
    let ensure = bounded(
        TEMPORAL,
        "pub(crate) fn compile_realm_ensure_temporal_helper(",
        "    fn emit_build_temporal_cluster(",
    );
    ordered(
        ensure,
        &[
            "TemporalInitializationState::Ready.word()",
            "RealmRecordSchema::TEMPORAL_NAMESPACE",
            "Instruction::Return",
            "TemporalInitializationState::Dormant.word()",
            "Instruction::Unreachable",
            "GcOperand::constant(TemporalInitializationState::Building)",
            "TemporalIntrinsicBuild(&parameters.realm)",
            "self.emit_build_temporal_cluster",
            ".from_value(&namespace",
            "RealmRecordSchema::TEMPORAL_NAMESPACE).write",
            "GcOperand::constant(TemporalInitializationState::Ready)",
        ],
    );
    assert!(!ensure.contains("GcOperand::constant(TemporalInitializationState::Dormant)"));
}
