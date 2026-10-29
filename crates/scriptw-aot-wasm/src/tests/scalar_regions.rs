//! Inspect real encoded source bodies, independently of emitter scratch APIs.
use super::*;

fn scalar_traffic(source: &str, name: &str) -> (u32, u32, usize) {
    let artifact = emit_script(source).expect("scalar traffic control emits");
    expect_valid_module(&artifact, 1);
    let target = function_names(&artifact)
        .into_iter()
        .find_map(|(index, actual)| actual.starts_with(&format!("js::{name}#")).then_some(index))
        .expect("source body has a Wasm name");
    let mut index = imported_function_count(&artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("module parses") else {
            continue;
        };
        if index == target {
            let mut tags = 0;
            let mut roots = 0;
            for local in body.get_locals_reader().expect("locals parse") {
                let (count, kind) = local.expect("local parses");
                match kind {
                    wasmparser::ValType::I32 => tags += count,
                    wasmparser::ValType::Ref(kind) if kind == wasmparser::RefType::EQREF => {
                        roots += count;
                    }
                    _ => {}
                }
            }
            let arithmetic = body
                .get_operators_reader()
                .expect("operators parse")
                .into_iter()
                .filter(|operator| {
                    matches!(
                        operator.as_ref().expect("operator parses"),
                        Operator::F64Add | Operator::F64Mul | Operator::F64Sub
                    )
                })
                .count();
            return (tags, roots, arithmetic);
        }
        index += 1;
    }
    panic!("named source body is absent");
}

#[test]
fn numeric_expression_depth_does_not_grow_tag_or_reference_scratch() {
    fn source(depth: usize) -> String {
        let mut expression = "+input".to_string();
        for _ in 0..depth {
            expression = format!("({expression} + +input) * 1.5");
        }
        format!("function calculate(input) {{ return {expression}; }} calculate(globalThis.input);")
    }
    let shallow = scalar_traffic(&source(1), "calculate");
    let deep = scalar_traffic(&source(8), "calculate");
    assert!(
        deep.2 >= 16,
        "the arithmetic must survive lowering: {deep:?}"
    );
    assert!(
        deep.2 > shallow.2,
        "the deeper body must perform more arithmetic"
    );
    assert!(
        deep.0 <= shallow.0,
        "numeric intermediates add tags: {shallow:?} -> {deep:?}"
    );
    assert!(
        deep.1 <= shallow.1,
        "numeric intermediates add roots: {shallow:?} -> {deep:?}"
    );
}

#[test]
fn fixed_argument_count_does_not_grow_evaluation_tag_or_eqref_scratch() {
    fn source(count: usize) -> String {
        let arguments = vec!["input"; count].join(",");
        format!(
            "function invoke(input) {{ return globalThis.consume({arguments}); }} invoke(globalThis.input);"
        )
    }
    let one = scalar_traffic(&source(1), "invoke");
    let sixteen = scalar_traffic(&source(16), "invoke");
    assert!(
        sixteen.0 <= one.0,
        "fixed arguments add scratch tags: {one:?} -> {sixteen:?}"
    );
    assert!(
        sixteen.1 <= one.1,
        "fixed arguments add scratch roots: {one:?} -> {sixteen:?}"
    );
}

fn with_source_operators(source: &str, name: &str, check: impl FnOnce(&[Operator<'_>])) {
    let artifact = emit_script(source).expect("loop control emits");
    expect_valid_module(&artifact, 1);
    let target = function_names(&artifact)
        .into_iter()
        .find_map(|(index, actual)| actual.starts_with(&format!("js::{name}#")).then_some(index))
        .expect("loop source body has a Wasm name");
    let mut index = imported_function_count(&artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("module parses") else {
            continue;
        };
        if index == target {
            let operators = body
                .get_operators_reader()
                .expect("source operators parse")
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .expect("source operators decode");
            check(&operators);
            return;
        }
        index += 1;
    }
    panic!("named source body is absent");
}

fn loop_regions<'a>(operators: &'a [Operator<'a>]) -> Vec<&'a [Operator<'a>]> {
    let mut frames = Vec::new();
    let mut loops = Vec::new();
    for (index, operator) in operators.iter().enumerate() {
        match operator {
            Operator::Loop { .. } => frames.push(Some(index)),
            Operator::Block { .. } | Operator::If { .. } | Operator::TryTable { .. } => {
                frames.push(None)
            }
            Operator::End => {
                if let Some(Some(start)) = frames.pop() {
                    loops.push(&operators[start + 1..index]);
                }
            }
            _ => {}
        }
    }
    loops
}

fn scalar_arithmetic_loop(operators: &[Operator<'_>]) -> bool {
    operators.iter().any(|op| matches!(op, Operator::F64Mul))
        && operators.iter().any(|op| matches!(op, Operator::F64Lt))
        && operators.iter().all(|op| {
            matches!(
                op,
                Operator::LocalGet { .. }
                    | Operator::LocalSet { .. }
                    | Operator::LocalTee { .. }
                    | Operator::F64Const { .. }
                    | Operator::F64Add
                    | Operator::F64Sub
                    | Operator::F64Mul
                    | Operator::F64Div
                    | Operator::F64Lt
                    | Operator::I32Const { .. }
                    | Operator::I32Eqz
                    | Operator::Br { .. }
                    | Operator::BrIf { .. }
                    | Operator::Drop
            )
        })
}

#[test]
fn original_numeric_workloads_contain_a_primitive_loop_and_the_generic_fallback() {
    for source in [
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/numeric-loop-10000.js"
        ),
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/numeric-loop-50000.js"
        ),
    ] {
        with_source_operators(source, "workload", |operators| {
            let loops = loop_regions(operators);
            assert_eq!(
                loops
                    .iter()
                    .filter(|region| scalar_arithmetic_loop(region))
                    .count(),
                1,
                "the original recurrence needs one scalar loop, without calls, references, tags or completion copies"
            );
            assert!(
                loops.iter().any(
                    |region| region.iter().any(|op| matches!(op, Operator::F64Mul))
                        && region
                            .iter()
                            .any(|op| matches!(op, Operator::I64ReinterpretF64))
                ),
                "a failed Number guard must retain ordinary numeric/coercion dispatch"
            );
        });
    }
}

#[test]
fn numeric_loop_admission_uses_operations_and_binding_identity() {
    for (source, expected) in [
        (
            "function varied(total, stop) { \"use strict\"; for(var step=0;(step+0)<stop;step++) total+=step*2+0.5; return total; } varied(1,7);",
            true,
        ),
        (
            "function varied(total, stop) { \"use strict\"; function capture(){return total;} for(var step=0;(step+0)<stop;step++) total+=step*2+0.5; return capture(); } varied(1,7);",
            false,
        ),
        (
            "function varied(total, stop) { \"use strict\"; for(var step=0;(step+0)<stop;step++) { total+=step*2+0.5; globalThis.observe(total); } return total; } varied(1,7);",
            false,
        ),
        (
            "function varied(total, stop) { \"use strict\"; for(var step=0;(step+0)<stop;step++) { total+=step*2+0.5; if(step) break; } return total; } varied(1,7);",
            false,
        ),
        // Uncaptured `let` heads need no materialized per-iteration cells.
        // Admission follows actual local storage and IR environment ownership.
        (
            "function varied(total, stop) { \"use strict\"; for(let step=0;(step+0)<stop;step++) total+=step*2+0.5; return total; } varied(1,7);",
            true,
        ),
        (
            "function varied() { const total=0; for(var step=0;(step+0)<3;step++) total+=step*2+0.5; } varied();",
            false,
        ),
    ] {
        with_source_operators(source, "varied", |operators| {
            assert_eq!(
                loop_regions(operators)
                    .iter()
                    .any(|region| scalar_arithmetic_loop(region)),
                expected,
                "{source}"
            );
        });
    }
}

fn scalar_integer_loop(operators: &[Operator<'_>]) -> bool {
    operators.iter().any(|op| matches!(op, Operator::I32Mul))
        && operators.iter().any(|op| matches!(op, Operator::F64Lt))
        && operators.iter().all(|op| {
            matches!(
                op,
                Operator::LocalGet { .. }
                    | Operator::LocalSet { .. }
                    | Operator::F64Const { .. }
                    | Operator::I32Const { .. }
                    | Operator::F64Add
                    | Operator::F64Lt
                    | Operator::I32Mul
                    | Operator::I32Add
                    | Operator::I32ShrU
                    | Operator::I32Eqz
                    | Operator::Br { .. }
                    | Operator::BrIf { .. }
                    | Operator::Drop
            )
        })
}

#[test]
fn original_integer_workload_contains_a_scalar_loop_with_invariant_native_guard() {
    with_source_operators(
        include_str!("../../../scriptw-cli/tests/fixtures/v8_baseline/integer.js"),
        "workload",
        |operators| {
            let loops = loop_regions(operators);
            let primitive_imul = loops
                .iter()
                .filter(|region| scalar_integer_loop(region))
                .collect::<Vec<_>>();
            assert_eq!(
                primitive_imul.len(),
                1,
                "the unchanged integer recurrence needs one loop with no per-iteration property/call/Realm/GC traffic or float/integer conversions"
            );
            assert!(
                primitive_imul[0].windows(5).any(|ops| matches!(
                    ops,
                    [
                        Operator::I32Mul,
                        Operator::I32Const { value: 1013904223 },
                        Operator::I32Add,
                        Operator::I32Const { value: 0 },
                        Operator::I32ShrU
                    ]
                )),
                "the exact Number sum is observed only as a residue and must stay in i32"
            );
            assert!(
                loops.iter().any(|region| region
                    .iter()
                    .any(|op| matches!(op, Operator::Call { .. }))
                    && region.iter().any(|op| matches!(op, Operator::I32Mul))),
                "failed invariant/native guards retain the existing ordered call path"
            );
        },
    );
}

#[test]
fn integer_residue_consumers_preserve_full_number_assignment_stores() {
    with_source_operators(
        "function run(state) { var full=0; for(var i=0;i<2;i++) state=(full=Math.imul(state,1664525)+1013904223)>>>0; return full; } run(123456789);",
        "run",
        |operators| {
            let loops = loop_regions(operators);
            let scalar = loops
                .iter()
                .find(|region| {
                    region.iter().any(|op| matches!(op, Operator::I32Mul))
                        && !region.iter().any(|op| matches!(op, Operator::Call { .. }))
                })
                .expect("the assignment remains a scalar region");
            assert!(
                scalar.windows(5).any(|ops| matches!(
                    ops,
                    [
                        Operator::I32Mul,
                        Operator::F64ConvertI32S,
                        Operator::F64Const { .. },
                        Operator::F64Add,
                        Operator::LocalSet { .. }
                    ]
                )),
                "an enclosing ToUint32 cannot truncate the nested assignment's full Number"
            );
            assert!(
                scalar
                    .iter()
                    .any(|op| matches!(op, Operator::I64TruncSatF64S)),
                "the stored full exact sum is converted only after its assignment"
            );
        },
    );
}
