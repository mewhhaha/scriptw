//! Exercise the real lowered call form before relying on numeric-call timings.
use super::*;

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
