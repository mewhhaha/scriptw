//! Reservation and completion remain private states after shared GC bootstrap.
const FUNCTIONS_SOURCE: &str = include_str!("../../src/functions.rs");
const OWNER_SOURCE: &str = include_str!("../../src/functions/created_realm_array_prototype.rs");

#[test]
fn created_realm_array_prototype_lifecycle_has_one_private_owner() {
    assert_eq!(
        FUNCTIONS_SOURCE
            .matches("\nmod created_realm_array_prototype;\n")
            .count(),
        1
    );
    assert!(!FUNCTIONS_SOURCE.contains("pub mod created_realm_array_prototype;"));
    for state in [
        "ReservedRealmArrayPrototypeLocal",
        "RealmArrayPrototypeLocal",
    ] {
        let declaration = format!("pub(crate) struct {state}");
        assert_eq!(OWNER_SOURCE.matches(&declaration).count(), 1);
        assert!(!FUNCTIONS_SOURCE.contains(&declaration));
        for capability in ["Clone", "Copy"] {
            assert!(!OWNER_SOURCE.contains(&format!("impl {capability} for {state}")));
        }
    }
    assert!(!OWNER_SOURCE.contains("#[derive("));
}

#[test]
fn initialization_and_constructor_linking_consume_only_typed_states() {
    assert!(
        OWNER_SOURCE.contains("struct ReservedRealmArrayPrototypeLocal(GcLocalSlot<ArrayObject>)")
    );
    assert!(OWNER_SOURCE.contains("struct RealmArrayPrototypeLocal(GcLocal<ArrayObject>)"));
    assert!(OWNER_SOURCE.contains("reserved: ReservedRealmArrayPrototypeLocal,"));
    assert!(OWNER_SOURCE.contains("Result<RealmArrayPrototypeLocal, EmitError>"));
    assert!(OWNER_SOURCE.contains("fn array(&self) -> &GcLocal<ArrayObject>"));
    assert!(OWNER_SOURCE.contains("prototype: RealmArrayPrototypeLocal,"));
    assert!(!OWNER_SOURCE.contains("pub(crate) struct ReservedRealmArrayPrototypeLocal(pub"));
    assert!(!OWNER_SOURCE.contains("pub(crate) struct RealmArrayPrototypeLocal(pub"));
}
