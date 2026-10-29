use std::collections::BTreeMap;
use std::path::Path;

const DOMAIN: &str = include_str!("../../src/objects/accessor_descriptor.rs");
const OBJECTS: &str = include_str!("../../src/objects.rs");
const DEFINE: &str = include_str!("../../src/objects/define_property.rs");
const INTRINSICS: &str = include_str!("../../src/intrinsics/mod.rs");
const OBJECT_CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/object.rs");
const FUNCTION_CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/functions.rs");
const TYPED_ARRAY_CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/typed_array.rs");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/accessor-descriptor-local-roles.md");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing `{end}` after `{start}`"))
        .0
}

fn normalized(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

fn product_sources(directory: &Path, root: &Path, output: &mut BTreeMap<String, String>) {
    for entry in std::fs::read_dir(directory).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            product_sources(&path, root, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.insert(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                std::fs::read_to_string(path).expect("source file"),
            );
        }
    }
}

#[test]
fn accessor_descriptor_roles_form_one_exact_nonempty_domain() {
    let source = normalized(DOMAIN);
    for role in ["AccessorGetter", "AccessorSetter"] {
        assert!(source.contains(&format!("pub(crate)struct{role}<T>(T);")));
        assert!(source.contains(&format!(
            "impl<T>{role}<T>{{pub(crate)constfnnew(value:T)->Self{{Self(value)}}}}"
        )));
    }
    assert_eq!(normalized(bounded(DOMAIN, "pub(crate) enum AccessorDescriptor<T> {", "pub(crate) type AccessorGetterLocals")),
        "Getter(AccessorGetter<T>),Setter(AccessorSetter<T>),GetterAndSetter{getter:AccessorGetter<T>,setter:AccessorSetter<T>,},}");
    for (alias, generic) in [
        ("AccessorGetterLocals", "AccessorGetter"),
        ("AccessorSetterLocals", "AccessorSetter"),
        ("AccessorDescriptorLocals", "AccessorDescriptor"),
    ] {
        assert!(source.contains(&format!(
            "pub(crate)type{alias}<'v>={generic}<&'vValueLocals>;"
        )));
    }
    assert!(!source.contains("#[derive"));
    assert!(!source.contains("Option<"));
    assert!(!source.contains("_=>"));
    for capability in ["Clone", "Copy", "Debug", "PartialEq", "Eq", "Default"] {
        assert!(!source.contains(&format!("impl{capability}")));
    }
}

#[test]
fn three_definition_boundaries_consume_the_typed_descriptor() {
    for (source, method, next) in [
        (
            OBJECTS,
            "pub(crate) fn emit_object_append_accessor_property_with_flags(",
            "/// ArrayCreate",
        ),
        (
            DEFINE,
            "pub(crate) fn emit_object_define_accessor_with_flag_local(",
            "fn emit_define_property_or_throw(",
        ),
        (
            INTRINSICS,
            "pub(crate) fn emit_install_intrinsic_accessor_values(",
            "pub(crate) fn emit_install_intrinsic_string(",
        ),
    ] {
        let body = normalized(bounded(source, method, next));
        assert!(
            body.contains("accessors:AccessorDescriptorLocals<'_>,"),
            "{method}"
        );
        assert!(!body.contains("getter:Option<"));
        assert!(!body.contains("setter:Option<"));
    }
    let projection = normalized(bounded(
        DOMAIN,
        "pub(super) fn into_fields<R>(",
        "impl AccessorDescriptor<ValueLocals>",
    ));
    assert!(projection.contains(
        "Self::Getter(AccessorGetter(getter))=>(Presence::Present(getter),Presence::Absent)"
    ));
    assert!(projection.contains(
        "Self::Setter(AccessorSetter(setter))=>(Presence::Absent,Presence::Present(setter))"
    ));
    assert!(projection.contains("Self::GetterAndSetter{getter:AccessorGetter(getter),setter:AccessorSetter(setter),}=>(Presence::Present(getter),Presence::Present(setter))"));
    assert_eq!(projection.matches("Self::").count(), 3);
    assert!(!projection.contains("_=>"));
    assert!(OBJECTS.contains("accessors.into_fields::<core::convert::Infallible>()"));
    assert!(DEFINE.contains("let (get, set) = accessors.into_fields();"));
    let storage = normalized(bounded(
        DOMAIN,
        "pub(crate) fn emit_intrinsic_accessor_property_publication(",
        "\n    }\n}",
    ));
    assert!(storage.contains("accessors:AccessorDescriptorLocals<'_>,"));
    assert!(storage.contains("accessors.into_fields::<core::convert::Infallible>()"));
    assert_eq!(storage.matches("None=>s.stored_undefined(f)").count(), 2);
    assert!(storage.contains("std::ptr::eq(getter,setter)"));
    assert!(storage.contains("IntrinsicAccessorPublishArguments::new("));
    assert!(!storage.contains("cast_subtype::<FunctionObject>"));
    let installation = normalized(bounded(
        INTRINSICS,
        "pub(crate) fn emit_install_intrinsic_accessor_values(",
        "pub(crate) fn emit_install_intrinsic_string(",
    ));
    assert!(installation.contains("self.emit_intrinsic_accessor_property_publication("));
}

#[test]
fn intrinsic_materialization_preserves_nonempty_roles_and_order() {
    let materialize = normalized(bounded(
        INTRINSICS,
        "pub(crate) fn emit_install_intrinsic_accessor(",
        "pub(crate) fn emit_install_intrinsic_accessor_values(",
    ));
    assert!(materialize.contains("accessors:AccessorDescriptor<StandardBuiltinId>,"));
    assert!(materialize.contains("letaccessors=accessors.try_map(materialize)?;"));
    assert!(materialize.contains("accessors.borrowed(),"));
    assert!(materialize.contains("accessors.clear(function);"));
    assert!(!materialize.contains("Option<"));
    let mapping = normalized(bounded(
        DOMAIN,
        "pub(crate) fn try_map<U, E>(",
        "/// Only the GC object owner",
    ));
    assert!(mapping.contains("getter:AccessorGetter::new(materialize(getter)?),setter:AccessorSetter::new(materialize(setter)?),"));
    let release = normalized(bounded(DOMAIN, "pub(crate) fn clear(", "\n    }\n}"));
    assert!(release.contains("setter.clear(function);getter.clear(function);"));
}

#[test]
fn every_definition_producer_names_getter_and_setter_roles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = BTreeMap::new();
    product_sources(&root, &root, &mut sources);
    let methods = [
        "emit_install_intrinsic_accessor",
        "emit_install_intrinsic_accessor_values",
        "emit_object_append_accessor_property_with_flags",
        "emit_object_define_accessor_with_flag_local",
        "emit_intrinsic_accessor_property_publication",
    ];
    let actual = sources
        .iter()
        .filter_map(|(path, source)| {
            let counts = methods.map(|method| source.matches(&format!("self.{method}(")).count());
            counts
                .iter()
                .any(|count| *count != 0)
                .then_some((path.as_str(), counts))
        })
        .collect::<BTreeMap<_, _>>();
    let expected = BTreeMap::from([
        ("builtins/bootstrap.rs", [0, 1, 0, 0, 0]),
        (
            "builtins/bootstrap/binary_data_initialization.rs",
            [3, 0, 0, 0, 0],
        ),
        ("builtins/string/regexp_legacy.rs", [0, 1, 0, 0, 0]),
        ("functions/arguments_object.rs", [0, 0, 1, 0, 0]),
        ("functions/throw_type_error.rs", [0, 0, 1, 0, 0]),
        ("intrinsics/abstract_module_source.rs", [1, 0, 0, 0, 0]),
        ("intrinsics/array.rs", [1, 0, 0, 0, 0]),
        ("intrinsics/binary_data.rs", [5, 0, 0, 0, 0]),
        ("intrinsics/collections.rs", [5, 0, 0, 0, 0]),
        ("intrinsics/errors.rs", [1, 0, 0, 0, 0]),
        ("intrinsics/intl.rs", [0, 1, 0, 0, 0]),
        ("intrinsics/iterator.rs", [2, 0, 0, 0, 0]),
        ("intrinsics/mod.rs", [0, 1, 0, 0, 1]),
        ("intrinsics/object.rs", [1, 0, 0, 0, 0]),
        ("intrinsics/promise.rs", [1, 0, 0, 0, 0]),
        ("intrinsics/regexp.rs", [2, 0, 0, 0, 0]),
        ("intrinsics/resource_management.rs", [1, 0, 0, 0, 0]),
        ("intrinsics/symbol.rs", [1, 0, 0, 0, 0]),
        ("intrinsics/temporal/members.rs", [1, 0, 0, 0, 0]),
        ("objects/object_literal_property.rs", [0, 0, 0, 2, 0]),
    ]);
    assert_eq!(actual, expected);
    let producers = sources
        .iter()
        .filter(|(path, _)| path.as_str() != "objects/accessor_descriptor.rs")
        .map(|(_, source)| source.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for (role, count) in [
        ("AccessorGetter::new(", 25),
        ("AccessorSetter::new(", 4),
        ("AccessorGetterLocals::new(", 6),
        ("AccessorSetterLocals::new(", 5),
        ("AccessorDescriptor::", 25),
        ("AccessorDescriptorLocals::", 7),
    ] {
        assert_eq!(producers.matches(role).count(), count, "{role}");
    }
}

#[test]
fn focused_accessor_behavior_and_evidence_remain_in_inventory() {
    assert!(OBJECT_CLI_TESTS
        .contains("fn run_wasm_backend_succeeds_for_supported_object_form_fixture()"));
    assert!(FUNCTION_CLI_TESTS.contains("fn run_wasm_class_auto_accessor_fixture()"));
    assert!(TYPED_ARRAY_CLI_TESTS
        .contains("fn run_wasm_backend_succeeds_for_typedarray_accessors_fixture()"));
    for evidence in [CONTRACT] {
        assert!(evidence.contains("`AccessorDescriptorLocals::{Getter, Setter, GetterAndSetter}`"));
        assert!(evidence.contains("`AccessorGetterLocals`"));
        assert!(evidence.contains("`AccessorSetterLocals`"));
    }
}
