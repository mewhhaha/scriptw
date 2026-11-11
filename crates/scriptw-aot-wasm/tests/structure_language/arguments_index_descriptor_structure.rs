const FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_arguments_mapped_descriptors.js");
#[test]
fn dynamic_arguments_named_writes_never_enter_ordinary_object_storage() {
    for witness in [
        "honorsOwnNamedSetter",
        "honorsInheritedNamedSetter",
        "honorsNonWritableNamedProperty",
        "rejectsAbsentIndexOnNonExtensibleArguments",
        "honorsInheritedIndexSetterAfterDelete",
        "rejectsAbsentIndexAssignmentOnNonExtensibleArguments",
        "honorsArgumentsPrototypeIndexedDescriptors",
    ] {
        assert!(FIXTURE.contains(witness));
    }
}
