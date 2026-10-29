//! Read the encoded loop edge rather than a source-shaped emitter assertion.
use super::*;

#[test]
fn number_loop_comparison_reaches_control_without_boolean_materialization() {
    let artifact = emit_script(
        "function hot() { var index=0; while(+index<32) { index=index+1; } return index; } hot();",
    )
    .expect("numeric loop emits");
    expect_valid_module(&artifact, 1);
    let target = function_names(&artifact)
        .into_iter()
        .find_map(|(index, name)| name.starts_with("js::hot#").then_some(index))
        .expect("numeric source body has a Wasm name");
    let mut index = imported_function_count(&artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("module parses") else {
            continue;
        };
        if index == target {
            let operators = body
                .get_operators_reader()
                .expect("source body operators parse")
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .expect("source operators decode");
            let comparisons = operators
                .iter()
                .enumerate()
                .filter_map(|(offset, operator)| {
                    matches!(operator, Operator::F64Lt).then_some(offset)
                })
                .collect::<Vec<_>>();
            assert!(
                !comparisons.is_empty(),
                "the runtime loop comparison survives"
            );
            // A runtime Number guard and its coercive fallback may each encode
            // a comparison. Every path must feed control without boxing it.
            for comparison in comparisons {
                let next = &operators[comparison + 1..];
                let edge = next
                    .windows(2)
                    .position(|pair| matches!(pair, [Operator::I32Eqz, Operator::BrIf { .. }]))
                    .expect("the Number predicate reaches the loop edge");
                assert!(
                    !next[..edge]
                        .iter()
                        .any(|operator| { matches!(operator, Operator::I64ExtendI32U) }),
                    "a predicate must not materialize a tagged Boolean before its loop edge: {:?}",
                    &next[..edge]
                );
            }
            return;
        }
        index += 1;
    }
    panic!("named numeric source body is absent");
}
