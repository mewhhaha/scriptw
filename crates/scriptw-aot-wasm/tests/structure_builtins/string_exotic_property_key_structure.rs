const LOWERING_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering.rs");
const DESCRIPTOR_SOURCE: &str = include_str!("../../src/objects/define_property.rs");
const OBJECTS_SOURCE: &str = include_str!("../../src/objects.rs");

fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker after: {start}"))
        .0
}

#[test]
fn computed_string_keys_have_one_closed_lowering_classification() {
    let declaration = between(
        LOWERING_SOURCE,
        "enum StringExoticComputedKey {",
        "}\n\nimpl StringExoticComputedKey",
    );
    let variants = declaration
        .lines()
        .map(str::trim)
        .filter(|line| line.ends_with(','))
        .collect::<Vec<_>>();
    assert_eq!(
        variants,
        [
            "CanonicalIndex(Box<TypedExpr>),",
            "OrdinaryPropertyKey(PropertyKeyIr),",
        ]
    );

    let conversion = between(
        LOWERING_SOURCE,
        "impl StringExoticComputedKey {",
        "\n}\n\n#[derive(Debug, Clone, PartialEq, Eq)]",
    );
    assert!(conversion.contains("match self {"));
    assert!(conversion.contains("Self::CanonicalIndex(index) => PropertyKeyIr::ArrayIndex(index),"));
    assert!(conversion.contains("Self::OrdinaryPropertyKey(key) => key,"));
    assert!(!conversion.contains("_ =>"));
    assert!(!conversion.contains("unreachable!"));
}

#[test]
fn failure_to_prove_a_string_index_preserves_the_property_key() {
    let lowering = between(
        LOWERING_SOURCE,
        "fn lower_string_index_key(",
        "fn lower_arguments_index_key(",
    );
    assert_eq!(
        lowering
            .matches(".classify_string_exotic_computed_key(expr)")
            .count(),
        1
    );
    assert!(!lowering.contains("string index must be number"));

    let classifier = between(
        lowering,
        "fn classify_string_exotic_computed_key(",
        "fn static_string_exotic_index(",
    );
    assert!(classifier.contains("self.static_array_numeric_property_key(expr)"));
    assert!(classifier.contains("self.lower_static_property_key(expr)"));
    assert!(classifier.contains(
        "StringExoticComputedKey::OrdinaryPropertyKey(PropertyKeyIr::StringExpr(Box::new(key)))"
    ));
    assert!(!classifier.contains("unsupported_expr"));
}

#[test]
fn backend_classifies_dynamic_keys_and_preserves_prototype_fallback() {
    let index = between(
        OBJECTS_SOURCE,
        "pub(crate) fn emit_property_key_array_index(",
        "pub(crate) fn emit_delete_ordinary_by_tag(",
    );
    assert!(index.contains("key: &PropertyKeyLocals"));
    assert!(index.contains("key.value().tag().load(function)"));
    assert!(index.contains("WasmRuntimeValueTag::String"));
    // The Array-index domain is bounded decimal, unlike a typed array's
    // broader CanonicalNumericIndexString domain (which includes -0).
    assert!(index.contains("u32::MAX as i64"));
    assert!(!index.contains("emit_canonical_numeric_index_string("));
    assert!(!index.contains("emit_number_to_string_payload("));
    let descriptor = between(
        DESCRIPTOR_SOURCE,
        "fn emit_string_index_descriptor(",
        "    pub(crate) fn emit_non_proxy_own_descriptor(",
    );
    assert!(
        descriptor.contains("self.emit_property_key_array_index(key, index, valid, function)?;")
    );
    assert!(descriptor.contains("Instruction::I32LtU"));
    assert!(descriptor.contains("StringValueSchema::CODE_UNITS"));
    assert!(descriptor.contains("writable: false,"));
    assert!(descriptor.contains("enumerable: true,"));
    assert!(descriptor.contains("configurable: false,"));
    let read = between(
        OBJECTS_SOURCE,
        "fn emit_non_proxy_object_get(",
        "pub(crate) fn emit_canonical_numeric_property_key_i32(",
    );
    let probe = read
        .find("self.emit_ordinary_get_probe(target, key, function)?;")
        .unwrap();
    let own = read
        .find("self.emit_proxy_target_own_descriptor(target, key, function)?;")
        .unwrap();
    let prototype = read
        .find("self.emit_ordinary_get_prototype_of(target, value, function);")
        .unwrap();
    let inherited = read
        .find("crate::runtime_helpers::ObjectReadArguments::new(")
        .unwrap();
    assert!(probe < own && own < prototype && prototype < inherited);
    assert!(read.contains("receiver,"));
    assert!(!read.contains("ValueKind::String =>"));
    assert!(!read.contains("emit_string_index_0_to_4_or_minus_one("));
}
