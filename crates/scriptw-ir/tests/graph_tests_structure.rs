const GRAPH_SOURCE: &str = include_str!("../src/modules/graph.rs");
const OWNER_SOURCE: &str = include_str!("../src/modules/graph_tests.rs");
const REQUEST_OWNER_SOURCE: &str = include_str!("../src/modules/graph_request_identity_tests.rs");
const INTEGRATION_SOURCE: &str = include_str!("integration_cases.rs");

#[test]
fn graph_tests_have_one_private_adjacent_owner_with_the_existing_namespace() {
    let test_only_items = [
        "#[cfg(test)]\nuse super::loaded_sources::{ModuleGraphSources, ModuleParse, ModuleSourceIr};",
        "#[cfg(test)]\n#[path = \"graph_tests.rs\"]\nmod tests;",
        "#[cfg(test)]\n#[path = \"graph_request_identity_tests.rs\"]\nmod request_identity_tests;",
        "#[cfg(test)]\npub(crate) fn link(graph: &mut ModuleGraphIr) {",
    ];
    assert_eq!(
        GRAPH_SOURCE.matches("\n#[cfg(test)]\n").count(),
        test_only_items.len()
    );
    for item in test_only_items {
        assert_eq!(GRAPH_SOURCE.matches(item).count(), 1, "{item}");
    }
    assert_eq!(
        INTEGRATION_SOURCE
            .matches("#[path = \"graph_tests_structure.rs\"]\nmod graph_tests_structure;")
            .count(),
        1
    );
    assert!(!GRAPH_SOURCE.contains("mod tests {"));
    assert!(!GRAPH_SOURCE.contains("#[test]"));
    assert!(!OWNER_SOURCE.contains("mod tests"));
    assert!(!GRAPH_SOURCE.contains("mod request_identity_tests {"));
    assert!(!REQUEST_OWNER_SOURCE.contains("mod request_identity_tests"));
}

#[test]
fn graph_tests_keep_the_inherited_graph_namespace_and_exact_test_census() {
    assert!(OWNER_SOURCE.starts_with("use super::*;\n\n"));
    assert_eq!(
        OWNER_SOURCE
            .matches("use super::super::graph_build::build_graph;")
            .count(),
        1
    );
    assert_eq!(OWNER_SOURCE.matches("#[test]").count(), 60);
    assert_eq!(REQUEST_OWNER_SOURCE.matches("#[test]").count(), 2);
    for (owner, other, names) in [
        (
            OWNER_SOURCE,
            REQUEST_OWNER_SOURCE,
            [
                "a_public_host_resolution_row_matches_canonical_request_attributes",
                "an_attributed_reexport_uses_the_matching_public_host_resolution_row",
            ],
        ),
        (
            REQUEST_OWNER_SOURCE,
            OWNER_SOURCE,
            [
                "public_host_request_identity_keeps_custom_attribute_values_exact",
                "public_host_text_rows_require_exact_type_attributes_and_actual_text_records",
            ],
        ),
    ] {
        for name in names {
            let definition = format!("#[test]\nfn {name}(");
            assert_eq!(owner.matches(&definition).count(), 1, "{name}");
            assert!(!other.contains(&definition), "duplicate owner for {name}");
        }
    }
    assert_eq!(OWNER_SOURCE.matches("fn sources_of(").count(), 1);
    assert_eq!(OWNER_SOURCE.matches("fn linked(").count(), 1);
    assert_eq!(OWNER_SOURCE.matches("fn unit_of(").count(), 1);
    assert_eq!(OWNER_SOURCE.matches("fn components(").count(), 1);
}
