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

fn normalized(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[test]
fn realm_array_bootstrap_publishes_only_the_initialized_exotic() {
    assert!(ARRAY
        .contains("pub(crate) struct ReservedRealmArrayPrototypeLocal(GcLocalSlot<ArrayObject>);"));
    assert!(ARRAY.contains("pub(crate) struct RealmArrayPrototypeLocal(GcLocal<ArrayObject>);"));
    let initialization = normalized(bounded(
        ARRAY,
        "pub(crate) fn emit_initialize_realm_array_prototype(",
        "pub(crate) fn release_realm_array_prototype_local(",
    ));
    assert!(initialization.contains("reserved:ReservedRealmArrayPrototypeLocal,"));
    assert!(initialization.contains("IntrinsicPropertyLayoutId::ArrayPrototype"));
    assert!(initialization.contains("self.emit_alloc_intrinsic_layout_header("));
    assert!(initialization.contains("self.runtime_bootstrap_plan.full_standard_globals"));
    assert!(initialization.contains("self.strings.has_intrinsic_property_layout(layout)?"));
    assert!(initialization
        .contains("full_standard_globals||self.strings.has_intrinsic_property_layout(layout)?"));
    assert!(initialization.contains("with_empty_index_storage("));
    let reserved = BOOTSTRAP
        .find("let reserved = self.reserve_realm_array_prototype_local(function);")
        .unwrap();
    let initialized = BOOTSTRAP
        .find("self.emit_initialize_realm_array_prototype(")
        .unwrap();
    let published = BOOTSTRAP
        .find("self.emit_store_realm_array_prototype(realm, array.array(), function);")
        .unwrap();
    assert!(reserved < initialized && initialized < published);
    assert!(!ARRAY.contains("emit_define_realm_array_prototype_data_with_flags"));
    assert!(!ARRAY.contains("emit_bind_realm_array_constructor_prototype"));
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
    assert!(PROJECTION.contains("FREQUENT_OBJECT_LAYOUTS.iter().chain("));
    assert!(PROJECTION.contains("GcLayout::ALL"));
    assert!(PROJECTION.contains("for layout in ordered"));
    assert!(PROJECTION.contains("layout.object_projection()"));
    assert!(PROJECTION.contains("ObjectHeaderProjection::Own => {}"));
    assert!(PROJECTION.contains("ObjectHeaderProjection::Field(field) =>"));
    assert!(PROJECTION.contains("field_index: field.raw()"));
    assert_eq!(PROJECTION.matches("Instruction::Unreachable").count(), 1);
    let object_prototype = include_str!("../../src/objects/allocation.rs");
    let object_birth = normalized(bounded(
        object_prototype,
        "pub(crate) fn emit_alloc_immutable_prototype_object(",
        "fn emit_allocate_object_header(",
    ));
    assert!(object_birth.contains("!self.runtime_bootstrap_plan.full_standard_globals"));
    assert!(object_birth.contains("self.strings.has_intrinsic_property_layout(layout)?"));
    assert!(object_birth.contains("self.emit_alloc_intrinsic_layout_header_with_mutability("));
    assert!(object_birth.contains("!self.runtime_bootstrap_plan.full_standard_globals&&!self.strings.has_intrinsic_property_layout(layout)?"));
    let plain_prototype = normalized(bounded(
        BOOTSTRAP,
        "fn emit_bootstrap_plain_prototype(",
        "fn emit_bootstrap_boxed_prototype(",
    ));
    assert!(plain_prototype.contains("self.runtime_bootstrap_plan.full_standard_globals"));
    assert!(plain_prototype.contains("self.strings.has_intrinsic_property_layout(layout)?"));
    assert!(plain_prototype.contains("self.runtime_bootstrap_plan.full_standard_globals||self.strings.has_intrinsic_property_layout(layout)?"));
    assert!(plain_prototype
        .contains("self.emit_bootstrap_intrinsic_namespace(layout,realm,parent,function)?"));
    assert!(!plain_prototype.contains("emit_install_intrinsic_"));
    assert!(!plain_prototype.contains("emit_object_append_"));
    let layout_namespace = normalized(bounded(
        BOOTSTRAP,
        "fn emit_bootstrap_intrinsic_namespace(",
        "pub(crate) fn init_reflect_object(",
    ));
    assert!(layout_namespace.contains("self.emit_alloc_intrinsic_layout_header("));
    let boxed_prototype = normalized(bounded(
        BOOTSTRAP,
        "fn emit_bootstrap_boxed_prototype(",
        "fn emit_bootstrap_constructor_from_slot(",
    ));
    assert!(boxed_prototype.contains("self.runtime_bootstrap_plan.full_standard_globals"));
    assert!(boxed_prototype.contains("self.strings.has_intrinsic_property_layout(layout)?"));
    assert!(boxed_prototype.contains("self.runtime_bootstrap_plan.full_standard_globals||self.strings.has_intrinsic_property_layout(layout)?"));
    assert!(boxed_prototype.contains("self.emit_alloc_intrinsic_layout_header("));
    assert!(boxed_prototype.contains("schema.struct_type::<PrimitiveBox>().construct("));
    assert!(!boxed_prototype.contains("emit_install_intrinsic_property_layout"));
    let layout_header = include_str!("../../src/objects/intrinsic_properties.rs");
    let allocation = bounded(
        layout_header,
        "pub(crate) fn emit_alloc_intrinsic_layout_header(",
        "pub(super) fn emit_intrinsic_property_row(",
    );
    assert!(allocation.contains("self.strings.intrinsic_property_layout_span(id)?"));
    assert!(!allocation.contains("Option<"));
}
