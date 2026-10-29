use std::collections::BTreeSet;

const BOOTSTRAP: &str = include_str!("../../src/builtins/bootstrap.rs");
const BINARY: &str = include_str!("../../src/builtins/bootstrap/binary_data_initialization.rs");
const FUNCTIONS: &str = include_str!("../../src/functions.rs");
const LOADER: &str = include_str!("../../src/functions/realm_intrinsic.rs");
const INTRINSICS: &str = include_str!("../../src/intrinsics/mod.rs");
const RECIPES: &str = include_str!("../../src/intrinsics/deferred_methods.rs");
const HEAP: &str = include_str!("../../src/heap.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing {end}"))
        .0
}

fn names(source: &str, prefix: &str) -> BTreeSet<String> {
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

#[test]
fn every_binary_data_identity_has_one_demand_owner_and_shared_array_to_string_stays_core() {
    let classification = bounded(
        FUNCTIONS,
        "pub(crate) const fn is_binary_data(self)",
        "/// Every externally observable identity owned by the Temporal",
    );
    let actual = names(classification, "Self::");
    let mut expected = BTreeSet::new();
    for family in [
        "ArrayBuffer",
        "SharedArrayBuffer",
        "DataView",
        "TypedArray",
        "Float64Array",
        "Float32Array",
        "Float16Array",
        "Int32Array",
        "Int16Array",
        "Int8Array",
        "Uint32Array",
        "Uint16Array",
        "Uint8Array",
        "Uint8ClampedArray",
        "BigInt64Array",
        "BigUint64Array",
    ] {
        for suffix in ["Constructor", "Prototype"] {
            expected.insert(format!("{family}{suffix}"));
        }
    }
    assert_eq!(actual, expected);
    assert!(!classification.contains("TypedArrayPrototypeToString"));
    let eager = bounded(
        BOOTSTRAP,
        "fn emit_initialize_realm_intrinsics_inner(",
        "pub(crate) fn init_runtime_roots(",
    );
    for forbidden in [
        "ArrayBufferPrototype",
        "DataViewPrototype",
        "TypedArrayPrototype",
        "ArrayBufferConstructor",
        "DataViewConstructor",
        "init_typed_array",
        "RealmEnsureBinaryData",
    ] {
        assert!(
            !eager.contains(forbidden),
            "binary graph leaked into eager bootstrap: {forbidden}"
        );
    }
    assert!(BINARY.contains("for kind in TypedArrayElementKind::ALL"));
    assert!(BINARY.contains("NonArrayRealmIntrinsicSlot::TypedArrayPrototypeToString"));
    assert!(!BINARY.contains("StandardBuiltinId::TypedArrayPrototypeToString,"));
}

#[test]
fn external_and_installer_reads_ensure_the_defining_realm_while_build_reads_require_capability() {
    let external = bounded(
        LOADER,
        "pub(crate) fn emit_load_non_array_realm_intrinsic(",
        "/// Only the binary-data builder",
    );
    assert!(external.contains("if slot.is_binary_data()"));
    assert!(external.contains("RealmEnsureBinaryDataArguments::new(realm)"));
    assert!(
        external.find("RealmEnsureBinaryDataArguments").unwrap()
            < external
                .find("self.emit_load_realm_intrinsic_stored_at_index")
                .unwrap()
    );
    let installer = bounded(
        INTRINSICS,
        "pub(crate) fn emit_intrinsic_callable(",
        "pub(crate) fn emit_intrinsic_canonical_host_callable(",
    );
    assert!(installer.contains("slot.is_binary_data()"));
    assert!(
        installer
            .find("self.emit_load_non_array_realm_intrinsic")
            .unwrap()
            < installer
                .find("self.emit_load_intrinsic_for_installation")
                .unwrap()
    );
    assert!(BINARY
        .contains("pub(crate) struct BinaryDataIntrinsicBuild<'a>(&'a GcLocal<RealmRecord>);"));
    assert_eq!(
        BINARY
            .matches("BinaryDataIntrinsicBuild(&parameters.realm)")
            .count(),
        1
    );
    let private = bounded(
        LOADER,
        "pub(crate) fn emit_load_building_binary_data_intrinsic(",
        "/// Only the Temporal cluster builder",
    );
    assert!(private.contains("building: &BinaryDataIntrinsicBuild<'_>"));
    assert!(private.contains("slot.is_binary_data()"));
    assert!(private.contains("building.realm()"));
    let build = bounded(
        BINARY,
        "fn emit_build_binary_data_cluster(",
        "pub(super) fn emit_install_deferred_binary_data_constructor(",
    );
    assert!(build.contains("Some(&constructor_parent)"));
    assert!(!build.contains("self.init_builtin_constructor_object("));
    assert!(!build.contains("should_initialize_standard_builtin"));
    assert!(!build.contains("current_environment"));
}

#[test]
fn ordinary_global_properties_retain_closed_defining_realm_recipes_and_never_resurrect() {
    let global = bounded(
        BOOTSTRAP,
        "GlobalPropertyInitializerIr::BuiltinFunction(builtin) => {",
        "GlobalPropertyInitializerIr::HostFunction",
    );
    assert!(
        global.find("BinaryDataConstructor::from_builtin").unwrap()
            < global.find("emit_load_non_array_realm_intrinsic").unwrap()
    );
    assert!(global.contains("self.emit_install_deferred_binary_data_constructor("));
    assert!(global.contains("continue;"));
    let publication = bounded(
        BINARY,
        "pub(super) fn emit_install_deferred_binary_data_constructor(",
        "fn init_building_typed_array_intrinsic(",
    );
    assert!(publication.contains("meta.standard_builtin != Some(builtin)"));
    assert!(publication.contains("GcOperand::reference(realm.realm(), schema)"));
    assert!(publication.contains("GcOperand::constant(constructor)"));
    assert!(publication.contains("IntrinsicDataPublishArguments::new("));
    let resolver = bounded(
        RECIPES,
        ".is_subtype::<DeferredBinaryDataConstructor>",
        ".is_subtype::<DeferredIntlNamespace>",
    );
    assert!(resolver.contains("DeferredBinaryDataConstructorSchema::REALM"));
    assert!(resolver.contains("for identity in crate::heap::BinaryDataConstructor::ALL"));
    assert!(resolver.contains("self.emit_load_non_array_realm_intrinsic_stored("));
    let domain = bounded(
        HEAP,
        "runtime_scalar_domain!(BinaryDataConstructor,",
        "impl BinaryDataConstructor",
    );
    assert_eq!(
        domain.lines().filter(|line| line.contains(" = ")).count(),
        15
    );
    assert!(!domain.contains("TypedArray ="));
    let ensure = bounded(
        BINARY,
        "pub(crate) fn compile_realm_ensure_binary_data_helper(",
        "fn emit_build_binary_data_cluster(",
    );
    assert!(
        ensure
            .find("BinaryDataInitializationState::Building")
            .unwrap()
            < ensure.find("self.emit_build_binary_data_cluster").unwrap()
    );
    assert!(
        ensure.find("self.emit_build_binary_data_cluster").unwrap()
            < ensure
                .find("GcOperand::constant(BinaryDataInitializationState::Ready)")
                .unwrap()
    );
    assert!(!ensure.contains("emit_install_intrinsic_data"));
    assert!(!ensure.contains("GLOBAL_OBJECT"));
}
