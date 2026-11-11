const OPERATIONS_SOURCE: &str = include_str!("../../src/operations.rs");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker after {start}: {end}"))
        .0
}

fn normalized(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn positions_in_order(source: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let offset = source[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing marker after byte {cursor}: {marker}"));
        cursor += offset + marker.len();
    }
}

#[test]
fn typeof_evaluates_its_whole_operand_once_before_observing_the_result() {
    let body = bounded(
        OPERATIONS_SOURCE,
        "pub(crate) fn compile_typeof_payload(",
        "pub(crate) fn emit_typeof_value(",
    );
    assert_eq!(body.matches("self.compile_expr_to_value(").count(), 1);
    positions_in_order(
        body,
        &[
            "let input = self.runtime_schema().reserve_value_local(function);",
            "self.compile_expr_to_value(expr, &input, function)?;",
            "self.emit_typeof_value(&input, output, function)?;",
            "input.clear(function);",
        ],
    );
    assert!(body.contains("output: &ValueLocals"));
    assert!(!body.contains("ValueKind::"));
    assert!(!body.contains("expr.kind"));
    assert!(!body.contains("compile_expr_payload"));
}

#[test]
fn runtime_typeof_preserves_exact_primitive_spellings_and_the_object_default() {
    let body = bounded(
        OPERATIONS_SOURCE,
        "pub(crate) fn emit_typeof_value(",
        "pub(crate) fn emit_is_callable_i32(",
    );
    let compact = normalized(body);
    for (tag, spelling) in [
        ("Undefined", "undefined"),
        ("Boolean", "boolean"),
        ("Number", "number"),
        ("BigInt", "bigint"),
        ("Symbol", "symbol"),
        ("String", "string"),
    ] {
        assert_eq!(
            body.matches(&format!("WasmRuntimeValueTag::{tag}")).count(),
            1
        );
        assert!(compact.contains(&format!("(WasmRuntimeValueTag::{tag},\"{spelling}\")")));
    }
    positions_in_order(
        body,
        &[
            "self.emit_interned_string_reference(\"object\", function)?",
            "output.set_reference(&object, schema, function);",
            "for (tag, spelling) in [",
            "input.tag().load(function);",
            "Instruction::I32Eq",
            "self.emit_interned_string_reference(spelling, function)?",
        ],
    );
    assert!(!body.contains("unreachable!"));
    assert!(!body.contains("ValueKind::"));
}

#[test]
fn callable_objects_and_htmldda_override_the_default_in_spec_order() {
    let body = bounded(
        OPERATIONS_SOURCE,
        "pub(crate) fn emit_typeof_value(",
        "pub(crate) fn emit_is_callable_i32(",
    );
    positions_in_order(
        body,
        &[
            "output.set_reference(&object, schema, function);",
            "self.emit_is_callable_i32(input, function)?;",
            "self.emit_interned_string_reference(\"function\", function)?",
            "output.set_reference(&string, schema, function);",
            "self.emit_is_htmldda_function_i32(input, function)?;",
            "self.emit_interned_string_reference(\"undefined\", function)?",
            "output.set_reference(&string, schema, function);",
        ],
    );
    let callable = bounded(
        OPERATIONS_SOURCE,
        "pub(crate) fn emit_is_callable_i32(",
        "pub(crate) fn compile_string_concat_payload(",
    );
    for capability in [
        "reference_type::<FunctionObject>",
        "reference_type::<BoundFunction>",
        "reference_type::<ProxyObject>",
        ".field(ProxyObjectSchema::CALL_CAPABILITY)",
        "ProxyCallCapability::ObjectOnly",
    ] {
        assert!(
            callable.contains(capability),
            "callability lost {capability}"
        );
    }
}
