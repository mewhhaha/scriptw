const DESCRIPTOR_SOURCE: &str = include_str!("../src/property_descriptor.rs");
const NAMESPACE_SOURCE: &str = include_str!("../src/modules/namespace.rs");
const BACKEND_NAMESPACE_SOURCE: &str =
    include_str!("../../scriptw-aot-wasm/src/objects/module_namespace.rs");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn code_without_whitespace(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .flat_map(str::chars)
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn source_descriptor_attribute_selection_has_no_boolean_parameter() {
    let shared_attributes = bounded(
        DESCRIPTOR_SOURCE,
        "impl<S: DescriptorSideMarker> DescriptorSourceText<S> {",
        "impl DescriptorSourceText<DataSide> {",
    );
    let data_attributes = bounded(
        DESCRIPTOR_SOURCE,
        "impl DescriptorSourceText<DataSide> {",
        "impl DescriptorSourceText<AccessorSide> {",
    );

    assert!(!shared_attributes.contains("flag: bool"));
    assert!(!data_attributes.contains("flag: bool"));

    let compact_shared = code_without_whitespace(shared_attributes);
    let compact_data = code_without_whitespace(data_attributes);
    assert!(compact_shared.contains("pubfnenumerable(mutself)->Self{"));
    assert!(compact_shared.contains("pubfnnon_enumerable(mutself)->Self{"));
    assert!(compact_shared.contains("pubfnconfigurable(mutself)->Self{"));
    assert!(compact_shared.contains("pubfnnon_configurable(mutself)->Self{"));
    assert!(compact_data.contains("pubfnwritable(mutself)->Self{"));
    assert!(compact_data.contains("pubfnnon_writable(mutself)->Self{"));
}

#[test]
fn each_named_attribute_method_owns_one_explicit_presence_value() {
    let source = code_without_whitespace(DESCRIPTOR_SOURCE);
    for (method, field, value) in [
        ("enumerable", "enumerable", "true"),
        ("non_enumerable", "enumerable", "false"),
        ("configurable", "configurable", "true"),
        ("non_configurable", "configurable", "false"),
        ("writable", "writable", "true"),
        ("non_writable", "writable", "false"),
    ] {
        let expected = format!(
            "pubfn{method}(mutself)->Self{{self.partial.{field}=Presence::Present({value});self}}"
        );
        assert_eq!(
            source.matches(&expected).count(),
            1,
            "attribute method `{method}` must own exactly one `{field}: {value}` projection"
        );
    }
}

#[test]
fn module_namespace_source_has_no_ordinary_source_object_descriptor() {
    // Canonical namespace exotic objects own their attributes. Source Text
    // Modules reject source imports; no ordinary tag descriptor substitutes.
    assert_eq!(
        NAMESPACE_SOURCE.matches("DescriptorSourceText::").count(),
        0
    );
    // The exotic object's attributes stay labelled at their new owner: one
    // export shape and one `@@toStringTag` shape, each spelled by field name.
    let backend = code_without_whitespace(BACKEND_NAMESPACE_SOURCE);
    assert_eq!(backend.matches("StoredPropertyAttributes::").count(), 2);
    assert_eq!(
        backend
            .matches(
                "StoredPropertyAttributes::Data{writable:true,enumerable:true,configurable:false,}"
            )
            .count(),
        1
    );
    assert_eq!(
        backend
            .matches(
                "StoredPropertyAttributes::Data{writable:false,enumerable:false,configurable:false,}"
            )
            .count(),
        1
    );
    for obsolete in [
        ".enumerable(true)",
        ".enumerable(false)",
        ".configurable(true)",
        ".configurable(false)",
        ".writable(true)",
        ".writable(false)",
    ] {
        assert!(!DESCRIPTOR_SOURCE.contains(obsolete));
        assert!(!NAMESPACE_SOURCE.contains(obsolete));
    }
}
