//! Exercise the real lowered call form before relying on numeric-call timings.
use super::*;
use std::collections::BTreeSet;

#[test]
fn numeric_imul_specialization_reaches_real_indirect_call_forms() {
    for (source, expected) in [
        ("Math.imul(globalThis.left, 7);", true),
        ("const imul = Math.imul; imul(globalThis.left, 7);", true),
        ("Math.imul(globalThis.left, 7, 0);", false),
    ] {
        let artifact = emit_script(source).expect("numeric builtin call emits");
        expect_valid_module(&artifact, 0);
        let main = Parser::new(0)
            .parse_all(&artifact.bytes)
            .find_map(|payload| match payload.expect("module parses") {
                Payload::ExportSection(exports) => exports
                    .into_iter()
                    .map(|export| export.expect("export parses"))
                    .find(|export| export.name == "main")
                    .map(|export| export.index),
                _ => None,
            })
            .expect("script entry is exported");
        let mut index = imported_function_count(&artifact);
        let mut has_numeric_multiply = false;
        for payload in Parser::new(0).parse_all(&artifact.bytes) {
            if let Payload::CodeSectionEntry(body) = payload.expect("module parses") {
                if index == main {
                    let operators = body
                        .get_operators_reader()
                        .expect("function operators parse")
                        .into_iter()
                        .map(|operator| operator.expect("operator parses"))
                        .collect::<Vec<_>>();
                    has_numeric_multiply = operators.windows(7).any(|ops| {
                        matches!(
                            ops,
                            [
                                Operator::LocalGet { .. },
                                Operator::I32WrapI64,
                                Operator::LocalGet { .. },
                                Operator::I32WrapI64,
                                Operator::I32Mul,
                                Operator::F64ConvertI32S,
                                Operator::I64ReinterpretF64
                            ]
                        )
                    });
                }
                index += 1;
            }
        }
        assert_eq!(has_numeric_multiply, expected, "{source}");
    }
}

/// Each source body named `js::{name}#…`: whether it multiplies the two
/// ToUint32 operands inline, and whether it leaves through a runtime-helper
/// proper tail call.
fn named_body_routes(artifact: &WasmArtifact, name: &str) -> Vec<(bool, bool)> {
    let imports = imported_function_count(artifact);
    let targets = function_names(artifact)
        .into_iter()
        .filter(|(_, actual)| actual.starts_with(&format!("js::{name}#")))
        .map(|(index, _)| index)
        .collect::<BTreeSet<_>>();
    assert!(!targets.is_empty(), "source body {name} is named");
    let mut index = imports;
    let mut routes = Vec::new();
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("module parses") {
            if targets.contains(&index) {
                let operators = crate::tests::emitted_operators(&body);
                let multiplies = operators.windows(5).any(|ops| {
                    matches!(
                        ops,
                        [
                            Operator::I32WrapI64,
                            Operator::LocalGet { .. },
                            Operator::I32WrapI64,
                            Operator::I32Mul,
                            Operator::F64ConvertI32S
                        ]
                    )
                });
                let tail_calls = operators.iter().any(|operator| {
                    matches!(operator, Operator::ReturnCall { function_index } if *function_index < imports)
                });
                routes.push((multiplies, tail_calls));
            }
            index += 1;
        }
    }
    routes
}

#[test]
fn numeric_imul_specialization_reaches_strict_return_position() {
    let script = emit_script(
        r#"function f(a, b) { "use strict"; return Math.imul(a, b); }
           function g(a, b) { return Math.imul(a, b); }
           f(globalThis.left, 7); g(globalThis.left, 7);"#,
    )
    .expect("strict return-position imul emits");
    expect_valid_module(&script, 2);
    let module = emit(&lower(
        &parse(
            "export function f(a, b) { return Math.imul(a, b); }",
            ParseOptions::module(),
        )
        .expect("module parses"),
    ))
    .expect("module return-position imul emits");
    expect_valid_module(&module, 1);
    for (goal, artifact) in [("strict script", &script), ("module", &module)] {
        // The guarded inline product and the guard-miss tail call share one body.
        assert!(
            named_body_routes(artifact, "f").contains(&(true, true)),
            "{goal}: {:?}",
            named_body_routes(artifact, "f")
        );
    }
    // Sloppy code has no tail continuation: the inline route continues.
    assert!(
        named_body_routes(&script, "g")
            .iter()
            .any(|&(multiplies, _)| multiplies),
        "{:?}",
        named_body_routes(&script, "g")
    );
}
