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

#[test]
fn number_remainder_guards_integer_conversion_and_retains_exact_significand_fallback() {
    let artifact = emit_script(
        "function remainder(a,b){return +a % +b;} print(remainder(globalThis.left,globalThis.right));",
    )
    .expect("dynamic numeric remainder emits");
    expect_valid_module(&artifact, 1);
    let mut inspected = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("P parses") else {
            continue;
        };
        let operators = body
            .get_operators_reader()
            .unwrap()
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for (remainder, _) in operators
            .iter()
            .enumerate()
            .filter(|(_, op)| matches!(op, Operator::I64RemS))
        {
            let (numerator, denominator, sign) = match &operators[remainder - 6..=remainder + 7] {
                [Operator::LocalGet {
                    local_index: numerator,
                }, Operator::F64ReinterpretI64, Operator::I64TruncF64S, Operator::LocalGet {
                    local_index: denominator,
                }, Operator::F64ReinterpretI64, Operator::I64TruncF64S, Operator::I64RemS, Operator::F64ConvertI64S, Operator::I64ReinterpretF64, Operator::LocalGet { local_index: sign }, Operator::I64Or, Operator::LocalSet { .. }, Operator::Br { relative_depth: 1 }, Operator::End] => {
                    (*numerator, *denominator, *sign)
                }
                _ => panic!(
                    "integer residual converts exactly and preserves numerator sign before exiting"
                ),
            };
            let fast_if = operators[..remainder - 6]
                .iter()
                .rposition(|op| matches!(op, Operator::If { .. }))
                .expect("integer conversions are guarded");
            let block = operators[..fast_if]
                .iter()
                .rposition(|op| matches!(op, Operator::Block { .. }))
                .expect("the fast branch exits the complete remainder block");
            let guard = &operators[block + 1..fast_if];
            let bounds = guard
                .windows(4)
                .filter_map(|ops| match ops {
                    [Operator::LocalGet { local_index }, Operator::I64Const { value },
                     Operator::I64LeU, Operator::I32And]
                        if *value == 9_007_199_254_740_991.0_f64.to_bits() as i64 => Some(*local_index),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                bounds.len(),
                2,
                "both finite magnitudes are bounded before conversion"
            );
            assert!(
                matches!(&guard[..3],
                [Operator::LocalGet { local_index }, Operator::I64Eqz, Operator::I32Eqz]
                if *local_index == bounds[1]),
                "the divisor's sign-cleared magnitude must be nonzero"
            );
            for (input, magnitude) in [(numerator, bounds[0]), (denominator, bounds[1])] {
                assert!(operators[..block].windows(4).any(|ops| matches!(ops,
                    [Operator::LocalGet { local_index: source }, Operator::I64Const { value: i64::MAX },
                     Operator::I64And, Operator::LocalSet { local_index: target }]
                    if *source == input && *target == magnitude)), "the bound uses the actual input's absolute bits");
                assert!(
                    guard.windows(7).any(|ops| matches!(ops,
                    [Operator::LocalGet { local_index: first }, Operator::F64ReinterpretI64,
                     Operator::F64Trunc, Operator::LocalGet { local_index: second },
                     Operator::F64ReinterpretI64, Operator::F64Eq, Operator::I32And]
                    if *first == input && *second == input)),
                    "each converted input must equal its truncation"
                );
            }
            assert!(
                operators[..block].windows(4).any(|ops| matches!(ops,
                [Operator::LocalGet { local_index: source }, Operator::I64Const { value: i64::MIN },
                 Operator::I64And, Operator::LocalSet { local_index: target }]
                if *source == numerator && *target == sign)),
                "signed zero comes from the original numerator"
            );
            assert!(
                !guard.iter().any(|op| matches!(
                    op,
                    Operator::I64TruncF64S
                        | Operator::Block { .. }
                        | Operator::Loop { .. }
                        | Operator::If { .. }
                )),
                "all nontrapping checks dominate the only fast-arm branch"
            );
            let mut depth = 1;
            let mut block_end = None;
            for (index, op) in operators.iter().enumerate().skip(block + 1) {
                match op {
                    Operator::Block { .. } | Operator::Loop { .. } | Operator::If { .. } => {
                        depth += 1
                    }
                    Operator::End => {
                        depth -= 1;
                        if depth == 0 {
                            block_end = Some(index);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let fallback = &operators[remainder + 8..block_end.unwrap()];
            assert_eq!(
                fallback
                    .iter()
                    .filter(|op| matches!(op, Operator::Loop { .. }))
                    .count(),
                1,
                "a guard miss retains the exact significand long-division loop"
            );
            assert!(
                fallback.iter().any(|op| matches!(op, Operator::I64Clz))
                    && fallback.iter().any(|op| matches!(op, Operator::I64Shl))
                    && fallback.iter().any(|op| matches!(op, Operator::I64ShrU)),
                "the exact fallback still normalizes and reconstructs binary64 significands"
            );
            assert!(!fallback.iter().any(|op| matches!(op, Operator::F64Div | Operator::I64RemS)),
                "unsupported values retain exact division rather than the integer arm or rounded quotient");
            inspected += 1;
        }
    }
    assert!(
        inspected > 0,
        "the actual dynamic P body contains the guarded integer arm"
    );
}
