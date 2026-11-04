//! Layout sharing must not acquire mutable object state or read-site identity.
const EMIT: &str = include_str!("../../src/emit.rs");
const PLANNING: &str = include_str!("../../src/planning.rs");
const SHAPE: &str = include_str!("../../src/objects/compact_data/shape.rs");
const LITERAL: &str = include_str!("../../src/objects/plain_data_literal.rs");
const READ: &str = include_str!("../../src/objects/property_read_plan.rs");
const CACHE: &str = include_str!("../../src/objects/compact_data/cache.rs");

fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing {end}"))
        .0
}

fn normalized(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn literal_layout_identity_is_ordered_keys_and_complete_attributes_only() {
    assert_eq!(
        normalized(between(EMIT, "    CompactShape {", "    },")),
        "names:&'a[&'astr],attributes:crate::heap::DescriptorWord,"
    );
    let registration = between(
        PLANNING,
        "crate::emit::ProgramSiteKind::CompactShape { names, attributes } => {",
        "crate::emit::ProgramSiteKind::OwnDataRead =>",
    );
    assert!(normalized(registration).contains(concat!(
        "letkey=(names.iter().map(|name|(*name).to_owned()).collect(),",
        "attributes.bits(),);"
    )));
    assert!(registration.contains("if let Some(&site) = shapes.get(&key)"));
    assert!(registration.contains("u32::try_from(shapes.len())"));
    assert!(registration.contains("shapes.insert(key, site)"));

    // One word owns both the interning identity and the immutable GC field.
    let birth = between(
        SHAPE,
        "pub(in crate::objects) fn emit_compact_literal_shape(",
        "pub(crate) fn compile_compact_literal_allocate_helper(",
    );
    assert_eq!(birth.matches("StoredPropertyAttributes::Data").count(), 1);
    assert!(normalized(birth).contains("ProgramSiteKind::CompactShape{names,attributes}"));
    assert!(birth.contains("GcOperand::descriptor_word(attributes)"));
}

#[test]
fn shared_layouts_keep_physical_instance_roots_and_live_object_reads() {
    // A fresh compilation registry cannot alias an unrelated P's numeric IDs.
    assert!(PLANNING.contains("compact_shape_sites: std::cell::RefCell::new(BTreeMap::new())"));
    assert!(SHAPE.contains("self.current_program().compact_shapes(id, schema, f)"));
    assert!(LITERAL.contains("builder.has_source_execution_environment()"));
    assert!(LITERAL.contains("builder.emit_source_literal_prototype_to_value("));
    assert!(LITERAL.contains("CompactLiteralAllocateArguments::new(&prototype, &shape)"));
    assert!(LITERAL.contains("builder.emit_compact_store_at(&storage, slot, &value, f)"));

    // Read caches stay distinct per source site and prove current storage,
    // shape and key before loading this receiver's current value.
    assert!(PLANNING
        .contains("crate::emit::ProgramSiteKind::OwnDataRead => &self.next_own_data_read_site"));
    assert!(READ.contains("self.next_program_site(crate::emit::ProgramSiteKind::OwnDataRead)"));
    assert!(READ.contains("crate::runtime_helpers::CachedPropertyReadArguments::new("));
    assert!(!READ.contains("OwnDataReadCacheSchema"));
    assert!(CACHE.contains(".field(OrdinaryObjectSchema::PROPERTIES)"));
    assert!(CACHE.contains(".field(CompactDataStorageSchema::SHAPE)"));
    assert!(CACHE.contains(".field(OwnDataReadCacheSchema::KEY)"));
    assert!(CACHE.contains("self.emit_compact_value_at(&storage, slot, &value, &mut f)"));
}
