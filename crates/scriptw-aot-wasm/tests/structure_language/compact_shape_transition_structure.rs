//! Only a complete default data add of a new non-index String key on an
//! extensible all-default compact object may take a shape transition.
const TRANSITION: &str = include_str!("../../src/objects/compact_data/transition.rs");
const DEFINE: &str = include_str!("../../src/objects/define_property.rs");
const ALLOCATION: &str = include_str!("../../src/objects/allocation.rs");
const LAYOUTS: &str = include_str!("../../src/gc_types/layouts.rs");
const SHAPES: &str = include_str!("../../src/gc_types/compact_shapes.rs");
const OBJECTS: &str = include_str!("../../src/objects.rs");
const LITERAL: &str = include_str!("../../src/objects/plain_data_literal.rs");
const CONSTRUCT: &str = include_str!("../../src/functions/call_dispatch.rs");
const REST: &str = include_str!("../../src/control_flow/object_destructuring.rs");

fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing {end}"))
        .0
}

fn ordered(source: &str, needles: &[&str]) {
    let mut rest = source;
    for needle in needles {
        let (_, after) = rest
            .split_once(needle)
            .unwrap_or_else(|| panic!("missing or out of order: {needle}"));
        rest = after;
    }
}

#[test]
fn only_complete_default_data_definitions_reach_the_transition_kernel() {
    let offer = between(
        TRANSITION,
        "pub(in crate::objects) fn emit_try_compact_data_definition(",
        "pub(crate) fn compile_compact_data_define_helper(",
    );
    ordered(
        offer,
        &[
            "let Presence::Present(value) = &fields.value else",
            "return Ok(None);",
            "matches!(fields.get, Presence::Absent)",
            "matches!(fields.set, Presence::Absent)",
            "Presence::Present(DescriptorFlag::Known(true)) => {}",
            "Presence::Present(DescriptorFlag::BooleanPayload(local)) => runtime.push(*local)",
            "_ => return Ok(None)",
            "CompactDataDefineArguments::new(object, key, value)",
        ],
    );
    // The general algorithm runs, unchanged, whenever the kernel declines.
    let define = between(
        DEFINE,
        "pub(super) fn emit_ordinary_define_own_property(",
        "fn emit_general_ordinary_define_own_property(",
    );
    ordered(
        define,
        &[
            "self.emit_try_compact_data_definition(header, key, descriptor, function)?",
            "return self",
            ".emit_general_ordinary_define_own_property(",
            "handled.load(function);",
            "self.emit_object_boolean_result(handled, result, function);",
            "function.instruction(&Instruction::Else);",
            "self.emit_general_ordinary_define_own_property(",
        ],
    );
}

#[test]
fn transition_kernel_guards_precede_every_mutation() {
    let kernel = between(
        TRANSITION,
        "pub(crate) fn compile_compact_data_define_helper(",
        "fn emit_compact_shape_child(",
    );
    ordered(
        kernel,
        &[
            "WasmRuntimeValueTag::String as i32",
            "base.is_subtype::<CompactDataStorage>(s, f);",
            "CompactDataShapeSchema::ATTRIBUTES",
            "default_data_attributes().as_i32()",
            "self.emit_compact_slot_probe_prepared(&compact, &name, hash, slot, found, f);",
            "self.emit_compact_store_at(&compact, slot, &p.value, f);",
            "OrdinaryObjectSchema::EXTENSIBLE",
            "self.emit_compact_shape_child(",
            "self.emit_compact_storage_extend(",
        ],
    );
    let child = between(
        TRANSITION,
        "fn emit_compact_shape_child(",
        "pub(super) fn emit_compact_transition_lookup(",
    );
    ordered(
        child,
        &[
            "self.emit_compact_transition_scan(&table, count, name, hash, &child, used, capacity, f);",
            "MAX_COMPACT_SHAPE_KEYS",
            "MAX_COMPACT_SHAPE_TRANSITIONS",
            "s.compact_shape_budget(f);",
            "self.emit_property_key_array_index(key, index, canonical, f)?;",
            "s.compact_shape_keys_extended(",
            "GcOperand::descriptor_word(default_data_attributes())",
            "s.spend_compact_shape_budget(f);",
            "CompactDataShapeSchema::TRANSITIONS",
        ],
    );
    // An existing child is found by key, and a lookup alone creates nothing.
    let scan = between(
        TRANSITION,
        "fn emit_compact_transition_scan(",
        "fn emit_compact_storage_extend(",
    );
    assert!(scan.contains("self.emit_compact_key_matches_i32(&candidate_key, name, hash, f);"));
    let lookup = between(
        TRANSITION,
        "pub(super) fn emit_compact_transition_lookup(",
        "fn emit_compact_transition_scan(",
    );
    assert!(lookup.contains("self.emit_compact_transition_scan("));
    for creation in ["construct(", "spend_compact_shape_budget", ".write("] {
        assert!(!lookup.contains(creation) && !scan.contains(creation));
    }
    // Storage is published only after the replacement owns the new value.
    let extend = between(
        TRANSITION,
        "fn emit_compact_storage_extend(",
        "fn emit_alloc_compact_array_storage(",
    );
    ordered(
        extend,
        &[
            "self.emit_compact_store_at(&next, count, value, f);",
            "OrdinaryObjectSchema::PROPERTIES",
        ],
    );
    assert_eq!(extend.matches("OrdinaryObjectSchema::PROPERTIES").count(), 1);
}

#[test]
fn shapes_hold_only_strings_and_further_shapes() {
    let shape = between(
        LAYOUTS,
        "struct CompactDataShape => CompactDataShapeSchema {",
        "}",
    );
    assert!(shape.contains("KEYS: GcRef<CompactShapeKeys>, Immutable, NonNullable;"));
    assert!(shape.contains("TRANSITIONS: GcRef<CompactShapeTable>, Mutable, Nullable;"));
    assert!(shape.contains("INDEX: GcRef<OrdinaryPropertyIndex>, Mutable, Nullable;"));
    assert_eq!(shape.matches("GcRef<").count(), 3);
    assert!(LAYOUTS.contains(
        "array CompactShapeTable => CompactShapeTableSchema { ELEMENT: GcRef<CompactDataShape>, Mutable, Nullable; }"
    ));
    // Shape keys are written only by their private construction.
    assert!(LAYOUTS.contains("(@writable_array CompactShapeKeys, $mutability:ident) => {};"));
    assert_eq!(SHAPES.matches("Instruction::ArrayCopy").count(), 1);
    assert!(!SHAPES.contains("ArraySet"));
}

#[test]
fn only_fresh_plain_objects_start_on_the_empty_shape() {
    let allocator = "emit_alloc_compact_object_with_prototype(";
    assert_eq!(ALLOCATION.matches(allocator).count(), 1);
    assert!(ALLOCATION.contains("schema\n                .empty_compact_storage(&mut function)"));
    // Object literals, ordinary [[Construct]] receivers and rest objects.
    assert_eq!(LITERAL.matches(allocator).count(), 1);
    assert!(LITERAL.contains("if self.fields.is_empty() {"));
    assert_eq!(OBJECTS.matches(allocator).count(), 1);
    assert_eq!(CONSTRUCT.matches(allocator).count(), 1);
    assert_eq!(REST.matches(allocator).count(), 1);
}
