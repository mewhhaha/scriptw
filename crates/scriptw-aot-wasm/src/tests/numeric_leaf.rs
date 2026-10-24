use super::*;

fn main_number_operations(source: &str) -> (usize, usize) {
    let artifact = emit_script(source).expect("numeric leaf control emits");
    expect_valid_module(&artifact, 1);
    let main = Parser::new(0)
        .parse_all(&artifact.bytes)
        .find_map(|payload| {
            let Payload::ExportSection(exports) = payload.expect("module parses") else {
                return None;
            };
            exports.into_iter().find_map(|export| {
                let export = export.expect("export parses");
                (export.name == "main").then_some(export.index)
            })
        })
        .expect("main export");
    let mut index = imported_function_count(&artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("module parses") {
            if index == main {
                let arithmetic = body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .filter(|op| {
                        matches!(
                            op.as_ref().unwrap(),
                            Operator::F64Add
                                | Operator::F64Mul
                                | Operator::F64Sub
                                | Operator::F64Div
                        )
                    })
                    .count();
                return (arithmetic, body.range().len());
            }
            index += 1;
        }
    }
    panic!("main body missing");
}

#[test]
fn actual_indirect_number_leaf_calls_have_an_allocation_free_arithmetic_route() {
    let (operations, _) = main_number_operations(
        "function leaf(a,b) { return a*b+1; } leaf(globalThis.left,globalThis.right);",
    );
    assert_eq!(
        operations, 2,
        "the guarded route must reach the real call IR"
    );
    for source in [
        "function leaf(a,b) { return a*b+1; } leaf(globalThis.left);",
        "function leaf(a,b) { return a*b+1; } leaf(globalThis.left,globalThis.right,3);",
        "function leaf(a,b=a) { return a*b+1; } leaf(globalThis.left,globalThis.right);",
        "function leaf(a,b) { return arguments[0]*b; } leaf(globalThis.left,globalThis.right);",
        "function leaf(a,b) { return a*b+1; } leaf('3',globalThis.right);",
    ] {
        assert_eq!(main_number_operations(source).0, 0, "{source}");
    }
}

#[test]
fn numeric_leaf_operation_budget_bounds_the_extra_program_body() {
    let source = |count| {
        let expression = std::iter::repeat_n("a", count)
            .collect::<Vec<_>>()
            .join("*");
        format!("function leaf(a) {{return {expression};}} leaf(globalThis.value);")
    };
    // Eight parameter leaves plus seven operators fit; the ninth does not.
    let admitted = main_number_operations(&source(8));
    let rejected = main_number_operations(&source(9));
    assert_eq!(admitted.0, 7);
    assert_eq!(rejected.0, 0);
    assert!(
        admitted.1 <= rejected.1 + 2048,
        "bounded leaf grew P by too much: {admitted:?}, {rejected:?}"
    );
}
