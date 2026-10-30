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
            let mut multiplications = loops
                .iter()
                .filter(|region| scalar_arithmetic_loop(region))
                .map(|region| {
                    region
                        .iter()
                        .filter(|op| matches!(op, Operator::F64Mul))
                        .count()
                })
                .collect::<Vec<_>>();
            multiplications.sort_unstable();
            assert_eq!(multiplications, [1, 5],
                "the original recurrence needs an ordered five-step chunk and one-step tail, without calls, references, tags or completion copies");
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
                2,
                "the unchanged integer recurrence needs a composed chunk and original tail with no property/call/Realm/GC traffic or float/integer conversions"
            );
            assert!(
                primitive_imul
                    .iter()
                    .any(|region| region.windows(5).any(|ops| matches!(
                        ops,
                        [
                            Operator::I32Mul,
                            Operator::I32Const { value: 1013904223 },
                            Operator::I32Add,
                            Operator::I32Const { value: 0 },
                            Operator::I32ShrU
                        ]
                    ))),
                "the exact Number sum is observed only as a residue and must stay in i32"
            );
            assert!(primitive_imul.iter().any(|region| {
                region.iter().any(|op| matches!(op, Operator::F64Const { value } if value.bits() == 8.0_f64.to_bits()))
                    && region.iter().filter(|op| matches!(op, Operator::I32Mul)).count() == 1
                    && !region.iter().any(|op| matches!(op, Operator::I32ShrU))
            }), "eight exact residue steps collapse to one multiply/add and an exact counter advance");
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
fn scalar_chunks_keep_floating_operations_ordered_and_compose_other_affine_coefficients() {
    // Strict parameters use private locals; sloppy mapped arguments correctly
    // retain environment storage and cannot enter this scalar region.
    for (source, signed) in [
        ("function run(state,stop){ \"use strict\";for(var i=0;i<stop;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(1,19);", false),
        ("function run(state,stop){ \"use strict\";for(var i=20;i>=stop;i--)state=(Math.imul(state,5)-7)|0;return state;} run(1,2);", true),
    ] {
        with_source_operators(source, "run", |operators| {
            let loops = loop_regions(operators);
            let chunk = loops.iter().find(|region| {
                region.iter().any(|op| matches!(op, Operator::I32Const { value } if *value == if signed {390625} else {6561}))
                    && !region.iter().any(|op| matches!(op, Operator::Call { .. }))
            }).expect("another affine map uses its mathematically composed eight-step coefficient");
            assert_eq!(chunk.iter().filter(|op| matches!(op, Operator::I32Mul)).count(), 1);
            assert!(chunk.iter().any(|op| matches!(op, Operator::I32Const { value } if *value == if signed {-683592} else {3280})));
            assert_eq!(chunk.iter().filter(|op| matches!(op, Operator::F64Lt | Operator::F64Ge)).count(), 1,
                "the chunk has one monotone last-position test");
        });
    }
    with_source_operators(
        "function run(total,stop){ \"use strict\";for(var i=0;i<stop;i++)total=(total+i*0.1)*1.0000000000000002-i/7;return total;} run(0,19);",
        "run", |operators| {
            let loops = loop_regions(operators);
            let chunk = loops.iter().find(|region| scalar_arithmetic_loop(region)
                && region.iter().filter(|op| matches!(op, Operator::F64Div)).count() == 5)
                .expect("five ordered floating iterations have one backedge");
            let arithmetic = chunk.iter().filter_map(|op| match op {
                Operator::F64Mul => Some('m'), Operator::F64Add => Some('a'),
                Operator::F64Div => Some('d'), Operator::F64Sub => Some('s'), _ => None,
            }).collect::<String>();
            assert_eq!(arithmetic, format!("a{}", "mam dsa".replace(' ', "").repeat(5)),
                "last-position lookahead precedes each unchanged multiply/add/multiply/divide/subtract and unit advance");
        });
}

#[test]
fn scalar_chunk_proofs_keep_written_tests_counters_and_full_values_on_their_owners() {
    for (source, expected_multiplications) in [
        ("function run(state,stop){ \"use strict\";for(var i=0;i<stop;i++){stop--;state=(Math.imul(state,3)+1)>>>0;}return state;} run(1,20);", vec![1]),
        ("function run(state,stop){ \"use strict\";for(var i=0;i<stop;i++){i++;state=(Math.imul(state,3)+1)>>>0;}return state;} run(1,20);", vec![1]),
        ("function run(state,stop){ \"use strict\";var i=0;for(;(i=i+1)<stop;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(1,20);", vec![1]),
        ("function run(state,stop){ \"use strict\";for(var i=0;i<stop;i++)state=(Math.imul(state,state)+1)>>>0;return state;} run(1,20);", vec![1,5]),
        ("function run(state,stop){ \"use strict\";var full=0;for(var i=0;i<stop;i++)state=(full=Math.imul(state,3)+1)>>>0;return full;} run(1,20);", vec![1,5]),
        ("function run(state,stop){ \"use strict\";for(var i=0;i<stop;i++)state=(Math.imul(state,3)+i)>>>0;return state;} run(1,20);", vec![1,5]),
    ] {
        with_source_operators(source, "run", |operators| {
            let mut actual = loop_regions(operators).into_iter()
                .filter(|region| region.iter().any(|op| matches!(op, Operator::I32Mul))
                    && !region.iter().any(|op| matches!(op, Operator::Call { .. })))
                .map(|region| region.iter().filter(|op| matches!(op, Operator::I32Mul)).count())
                .collect::<Vec<_>>();
            actual.sort_unstable();
            assert_eq!(actual, expected_multiplications, "{source}");
        });
    }
}

#[test]
fn integer_residue_consumers_preserve_full_number_assignment_stores() {
    with_source_operators(
        "function run(state) { \"use strict\"; var full=0; for(var i=0;i<2;i++) state=(full=Math.imul(state,1664525)+1013904223)>>>0; return full; } run(123456789);",
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
