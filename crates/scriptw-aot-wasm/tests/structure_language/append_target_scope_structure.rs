//! GC composites carry an explicit ordinary header. Appending an intrinsic
//! property projects that header once; it does not inline an Array identity
//! dispatch at each script or bootstrap append.
const OBJECTS: &str = include_str!("../../src/objects.rs");
const INTRINSICS: &str = include_str!("../../src/intrinsics/mod.rs");
const ARRAY: &str = include_str!("../../src/functions/created_realm_array_prototype.rs");
const BOOTSTRAP: &str = include_str!("../../src/builtins/bootstrap.rs");
const PROJECTION: &str = include_str!("../../src/gc_types/value/object_header_projection.rs");
const LAYOUTS: &str = include_str!("../../src/gc_types/layouts.rs");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing {end}"))
        .0
}

#[test]
fn realm_array_bootstrap_publishes_only_the_initialized_exotic() {
    assert!(ARRAY
        .contains("pub(crate) struct ReservedRealmArrayPrototypeLocal(GcLocalSlot<ArrayObject>);"));
    assert!(ARRAY.contains("pub(crate) struct RealmArrayPrototypeLocal(GcLocal<ArrayObject>);"));
    let initialization = bounded(
        ARRAY,
        "pub(crate) fn emit_initialize_realm_array_prototype(",
        "pub(crate) fn emit_define_realm_array_prototype_data_with_flags(",
    );
    assert!(initialization.contains("reserved: ReservedRealmArrayPrototypeLocal,"));
    assert!(initialization
        .contains("self.emit_alloc_empty_array_with_prototype(object_prototype, function)?"));
    let reserved = BOOTSTRAP
        .find("let reserved = self.reserve_realm_array_prototype_local(function);")
        .unwrap();
    let initialized = BOOTSTRAP
        .find("self.emit_initialize_realm_array_prototype(reserved, &object_prototype, function)?")
        .unwrap();
    let published = BOOTSTRAP
        .find("self.emit_store_realm_array_prototype(realm, array.array(), function);")
        .unwrap();
    assert!(reserved < initialized && initialized < published);
    let append = bounded(
        ARRAY,
        "pub(crate) fn emit_define_realm_array_prototype_data_with_flags(",
        "pub(crate) fn emit_bind_realm_array_constructor_prototype(",
    );
    assert!(append.contains("prototype: &RealmArrayPrototypeLocal,"));
    let header = append.find(".field(ArrayObjectSchema::OBJECT)").unwrap();
    let write = append
        .find("self.emit_object_append_data_property_with_flags(")
        .unwrap();
    assert!(header < write);
}

#[test]
fn append_helpers_accept_only_the_ordinary_header_and_share_one_projection() {
    for name in ["data", "accessor"] {
        let start = format!("pub(crate) fn emit_object_append_{name}_property_with_flags(");
        let body = bounded(OBJECTS, &start, "\n    }");
        assert!(body.contains("object: &GcLocal<OrdinaryObject>,"));
        assert_eq!(
            body.matches("self.emit_ordinary_append_property_entry(")
                .count(),
            1
        );
        for forbidden in [
            "is_main()",
            "AppendTargetScope",
            "ARRAY_PROTOTYPE_GLOBAL_INDEX",
            "Instruction::If",
        ] {
            assert!(
                !body.contains(forbidden),
                "{name} append contains {forbidden}"
            );
        }
    }
    for name in ["data", "accessor_values"] {
        let start = format!("pub(crate) fn emit_install_intrinsic_{name}(");
        let body = bounded(INTRINSICS, &start, "\n    }");
        assert_eq!(
            body.matches("self.emit_object_header_projection(target, function)")
                .count(),
            1
        );
    }
    let array = bounded(
        LAYOUTS,
        "struct ArrayObject: JavaScriptObject => ArrayObjectSchema {",
        "        }",
    );
    assert!(array.contains("OBJECT: GcRef<OrdinaryObject>, Immutable, NonNullable;"));
    assert!(PROJECTION.contains("for layout in GcLayout::ALL"));
    assert!(PROJECTION.contains("layout.object_projection()"));
    assert!(PROJECTION.contains("ObjectHeaderProjection::Own => {}"));
    assert!(PROJECTION.contains("ObjectHeaderProjection::Field(field) =>"));
    assert!(PROJECTION.contains("field_index: field.raw()"));
    assert_eq!(PROJECTION.matches("Instruction::Unreachable").count(), 1);
}
