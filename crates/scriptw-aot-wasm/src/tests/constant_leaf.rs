//! Validate whole source bodies and inspect their real indirect call sites.
use super::*;
use crate::functions::ConstantLeafPlan;

#[derive(Debug, Default)]
struct CallShape {
    depth_admissions: usize,
    source_equalities: usize,
    reference_equalities: usize,
    literal_reads: usize,
    calls: usize,
}

fn call_shape(source: &str, name: &str) -> CallShape {
    let artifact = emit_script(source).expect("constant leaf control emits");
    expect_valid_module(&artifact, 1);
    let target = if name == "main" {
        Parser::new(0)
            .parse_all(&artifact.bytes)
            .find_map(|payload| {
                let Payload::ExportSection(exports) = payload.expect("P decodes") else {
                    return None;
                };
                exports.into_iter().find_map(|export| {
                    let export = export.expect("P export decodes");
                    (export.name == "main").then_some(export.index)
                })
            })
            .expect("main export")
    } else {
        function_names(&artifact)
            .into_iter()
            .find_map(|(index, actual)| {
                actual.starts_with(&format!("js::{name}#")).then_some(index)
            })
            .expect("named source callable")
    };
    let mut index = imported_function_count(&artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("P decodes") else {
            continue;
        };
        if index == target {
            let operators = body
                .get_operators_reader()
                .expect("source operators open")
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .expect("source operators decode");
            let mut shape = CallShape::default();
            for pair in operators.windows(2) {
                if matches!(pair, [Operator::I32Const { value: 128 }, Operator::I32LtU]) {
                    shape.depth_admissions += 1;
                }
            }
            for operator in operators {
                match operator {
                    Operator::I64Eq => shape.source_equalities += 1,
                    Operator::RefEq => shape.reference_equalities += 1,
                    Operator::ArrayGet { .. } => shape.literal_reads += 1,
                    Operator::Call { .. }
                    | Operator::CallRef { .. }
                    | Operator::CallIndirect { .. } => shape.calls += 1,
                    _ => {}
                }
            }
            return shape;
        }
        index += 1;
    }
    panic!("source body is absent");
}

#[test]
fn immutable_leaf_plans_validate_the_entire_ordinary_zero_arity_body() {
    for (source, expected) in [
        (
            "function leaf(){return 'a\\ud800'+'\\udc00b'+'\\u0000c';}",
            true,
        ),
        ("function leaf(){'use strict';return 17;}", true),
        ("function leaf(){return true;}", true),
        ("function leaf(){return null;}", true),
        ("function leaf(){return;}", true),
        ("function leaf(value){return 'constant';}", false),
        (
            "function leaf(value=globalThis.effect()){return 'constant';}",
            false,
        ),
        ("function leaf(...rest){return 'constant';}", false),
        ("async function leaf(){return 'constant';}", false),
        ("function* leaf(){return 'constant';}", false),
        (
            "function outer(){let value='constant';return function leaf(){return value;};}",
            false,
        ),
        (
            "function leaf(){globalThis.effect();return 'constant';}",
            false,
        ),
        ("function leaf(){return arguments;}", false),
        ("function leaf(){return this;}", false),
        ("function leaf(){return {};}", false),
        ("function leaf(){return 1n;}", false),
        ("var leaf=()=> 'constant';", false),
    ] {
        let parsed = parse(source, ParseOptions::script()).expect("plan control parses");
        let program = lower(&parsed);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.expect("script IR");
        let function = script
            .functions
            .iter()
            .find(|function| function.name == "leaf")
            .or_else(|| (script.functions.len() == 1).then(|| &script.functions[0]))
            .expect("source leaf");
        assert_eq!(
            ConstantLeafPlan::for_function(function).is_some(),
            expected,
            "{source}"
        );
    }
}

#[test]
fn unchanged_folded_string_workload_has_guarded_literal_return_and_call_fallback() {
    let shape = call_shape(
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/folded-string-4096.js"
        ),
        "workload",
    );
    assert!(
        shape.depth_admissions >= 1
            && shape.source_equalities >= 1
            && shape.reference_equalities >= 1,
        "the actual text() call needs depth, source-code and defining-P guards: {shape:?}"
    );
    assert!(
        shape.literal_reads >= 1,
        "the primitive return reads its checked literal pool: {shape:?}"
    );
    assert!(
        shape.calls >= 1,
        "the original indirect-call fallback remains compiled: {shape:?}"
    );
}

#[test]
fn constant_call_specialization_declines_extra_arguments_and_nonliteral_bodies() {
    let admitted = call_shape(
        "function leaf(){return 'private-leaf-literal';} leaf();",
        "main",
    );
    assert_eq!(admitted.depth_admissions, 1, "{admitted:?}");
    for source in [
        "function leaf(){return 'private-leaf-literal';} leaf(globalThis.effect());",
        "function leaf(){return 'private-leaf-literal';} leaf(...globalThis.values);",
        "function leaf(value=globalThis.effect()){return 'private-leaf-literal';} leaf();",
        "function leaf(...rest){return 'private-leaf-literal';} leaf();",
        "function leaf(){return globalThis.value;} leaf();",
        "class Leaf {constructor(){return 'private-leaf-literal';}} Leaf();",
        "async function leaf(){return 'private-leaf-literal';} leaf();",
        "function* leaf(){return 'private-leaf-literal';} leaf();",
    ] {
        let shape = call_shape(source, "main");
        assert_eq!(shape.depth_admissions, 0, "{source}: {shape:?}");
    }
}
