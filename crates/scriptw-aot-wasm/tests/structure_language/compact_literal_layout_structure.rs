//! Layout sharing must not acquire mutable object state or read-site identity.
const EMIT: &str = include_str!("../../src/emit.rs");
const PLANNING: &str = include_str!("../../src/planning.rs");
const SHAPE: &str = include_str!("../../src/objects/compact_data/shape.rs");
const LITERAL: &str = include_str!("../../src/objects/plain_data_literal.rs");
const READ: &str = include_str!("../../src/objects/property_read_plan.rs");
const CACHE: &str = include_str!("../../src/objects/compact_data/cache.rs");
const HIT: &str = include_str!("../../src/objects/compact_data/cache/hit.rs");
const SITE: &str = include_str!("../../src/objects/compact_data/cache/site.rs");

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
        "crate::emit::ProgramSiteKind::InstanceofPrototype =>",
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

    // Read records stay distinct per source site, in the site's own P
    // global, and prove current storage, shape and key before loading this
    // receiver's current value.
    assert!(PLANNING.contains("site_records: std::cell::RefCell::new(Vec::new()),"));
    assert!(EMIT.contains("let position = self.functions.next_site_record(T::KIND)?;"));
    assert!(EMIT.contains("self.runtime_schema().site_record_global(position)"));
    assert!(READ.contains("PropertyReadPlan::CachedStaticString => self.next_site_record().map(Some)"));
    assert!(READ.contains("let may_be_ordinary = target.possible_kinds.contains(ValueKind::Object);"));
    assert!(READ.contains("self.emit_static_read_site(name, may_be_ordinary, base, result, f)"));
    assert!(SITE.contains("let global = self.next_site_record()?;"));
    assert!(SITE.contains("crate::runtime_helpers::CachedPropertyReadArguments::new("));
    assert!(!READ.contains("OwnDataReadCacheSchema"));
    // The site reads only the record's shape and slot; R fills records.
    assert!(!SITE.contains(".write(record"));
    assert!(SITE.contains("crate::runtime_helpers::CachedPropertyRecordArguments::new(&name)"));
    assert!(CACHE.contains(".field(OrdinaryObjectSchema::PROPERTIES)"));
    assert!(HIT.contains(".field(CompactDataStorageSchema::SHAPE)"));
    assert!(HIT.contains(".field(OwnDataReadCacheSchema::KEY)"));
    assert!(HIT.contains("self.emit_compact_value_at(storage, slot, &value, f)"));
    assert!(CACHE.contains("self.emit_cached_compact_read_hit("));
    assert!(HIT.contains("self.emit_cached_compact_identity(key, storage, cache, f)"));
}
