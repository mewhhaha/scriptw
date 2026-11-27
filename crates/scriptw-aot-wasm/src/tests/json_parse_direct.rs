//! JSON.parse builds containers and numbers directly and keeps exact fallbacks.
use super::*;

fn runtime_body<'a>(bytes: &'a [u8], requested: &str) -> (u32, Vec<Operator<'a>>) {
    let names = function_names_in_bytes(bytes)
        .into_iter()
        .filter(|(_, name)| name.as_str() == requested)
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 1, "one body for {requested}: {names:?}");
    let named_index = names[0].0;
    let mut index = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("runtime decodes") {
            if index == named_index {
                return (
                    index,
                    body.get_operators_reader()
                        .expect("body opens")
                        .into_iter()
                        .map(|operator| operator.expect("operator decodes"))
                        .collect(),
                );
            }
            index += 1;
        }
    }
    panic!("{requested} has no body");
}

fn calls(body: &[Operator<'_>], target: u32) -> bool {
    body.iter().any(|operator| {
        matches!(operator,
            Operator::Call { function_index } | Operator::ReturnCall { function_index }
                if *function_index == target)
    })
}

#[test]
fn plain_string_scans_call_packed_code_unit_helpers() {
    let artifact = emit_script(r#"JSON.parse('"a long JSON string"');"#).expect("JSON.parse emits");
    expect_valid_module(&artifact, 1);
    let bytes = artifact.runtime().expect("JSON.parse links R").bytes();

    let (_, parse) = runtime_body(bytes, "builtin::JSON.parse");
    let (parse_run, _) = runtime_body(bytes, "helper::json_parse_plain_run");
    assert!(calls(&parse, parse_run));

    let (_, equality) = runtime_body(bytes, "helper::string_equality");
    let (equal_units, _) = runtime_body(bytes, "helper::code_unit_equal");
    assert!(calls(&equality, equal_units));
}

#[test]
fn json_parse_grows_compact_objects_and_converts_short_numbers_inline() {
    let artifact = emit_script("JSON.parse('{\"a\":[1.5,\"b\"]}');").expect("JSON.parse emits");
    expect_valid_module(&artifact, 1);
    let runtime = artifact.runtime().expect("JSON.parse links R");
    let bytes = runtime.bytes();
    let (_, parse) = runtime_body(bytes, "builtin::JSON.parse");
    let helper = |name: &str| runtime_body(bytes, &format!("helper::{name}")).0;
    // Fresh objects start on the shared empty compact storage and take each
    // member through the shape-transition kernel.
    assert!(calls(&parse, helper("compact_object_allocate")));
    assert!(calls(&parse, helper("compact_data_define")));
    // A key the kernel declines (an array index, a bound) keeps the general
    // CreateDataProperty definition and its dictionary promotion.
    assert!(calls(&parse, helper("object_define_data")));
    // Numbers come from code units: one exact IEEE operation inside the
    // Clinger window, the exact decimal converter outside it, and never a
    // token String through StringToNumber.
    assert!(calls(&parse, helper("decimal_to_binary64")));
    assert!(!calls(&parse, helper("string_to_number")));
    for wanted in ["f64.mul", "f64.div", "f64.convert_i64_u"] {
        assert!(
            parse.iter().any(|operator| match operator {
                Operator::F64Mul => wanted == "f64.mul",
                Operator::F64Div => wanted == "f64.div",
                Operator::F64ConvertI64U => wanted == "f64.convert_i64_u",
                _ => false,
            }),
            "JSON.parse emits {wanted}"
        );
    }
    // Unescaped runs are copied, not stored unit by unit.
    assert!(parse
        .iter()
        .any(|operator| matches!(operator, Operator::ArrayCopy { .. })));
}
