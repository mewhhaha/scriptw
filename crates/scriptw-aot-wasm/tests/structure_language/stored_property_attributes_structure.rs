const HEAP_SOURCE: &str = include_str!("../../src/heap.rs");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/stored-property-attributes.md");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn normalized(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn stored_property_attributes_is_the_exact_capability_free_domain() {
    let domain = bounded(
        HEAP_SOURCE,
        "/// Static 6.2.6.6 attributes for a complete GC descriptor.",
        "/// A **test** against a descriptor word.",
    );
    let declaration = bounded(
        HEAP_SOURCE,
        "pub(crate) enum StoredPropertyAttributes {",
        "\n}\n\nimpl StoredPropertyAttributes",
    );
    let declaration = normalized(declaration);
    assert_eq!(
        declaration,
        concat!(
            "Data{writable:bool,enumerable:bool,configurable:bool,},",
            "Accessor{enumerable:bool,configurable:bool,},"
        )
    );
    assert!(!domain.contains("#[derive"));
    assert!(!domain.lines().any(|line| {
        line.trim_start().starts_with("impl ") && line.contains(" for StoredPropertyAttributes")
    }));
}

#[test]
fn one_exhaustive_projection_owns_both_descriptor_kinds() {
    let implementation = bounded(
        HEAP_SOURCE,
        "impl StoredPropertyAttributes {",
        "/// A **test** against a descriptor word.",
    );
    let compact = normalized(implementation);
    assert_eq!(implementation.matches("match self {").count(), 1);
    assert!(!implementation.contains("_ =>"));
    assert!(!implementation.contains("unreachable!"));
    assert!(compact.contains(concat!(
        "Self::Data{writable,enumerable,configurable,}=>",
        "DescriptorWord::of_data(writable,enumerable,configurable),"
    )));
    assert!(compact.contains(concat!(
        "Self::Accessor{enumerable,configurable,}=>",
        "DescriptorWord::of_accessor(enumerable,configurable),"
    )));
    assert!(compact.contains("pub(crate)constfndescriptor_kind_bits(self)->u32{"));
    assert!(compact.contains("self.descriptor_word().bits()"));
}

#[test]
fn task_and_contract_record_the_stored_attribute_boundary() {
    for evidence in [CONTRACT] {
        assert!(evidence.contains("StoredPropertyAttributes"));
        assert!(evidence.contains("positional boolean"));
        assert!(evidence.contains("Accessor"));
    }
}
